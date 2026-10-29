//! Markdown block parser for the transcript.
//!
//! [`pulldown_cmark`] owns the CommonMark and GFM grammar. This module turns
//! that event stream into the block tree the transcript paints. It does not
//! know about GPUI: theme colors, selection, code folding, and images stay in
//! `transcript_view`.
//!
//! Bare `http://` and `https://` URLs are not links in CommonMark, and
//! emphasis markers inside a URL split it into several events. Those URLs are
//! recovered from the source span with [`autolink_end`].

use std::ops::Range;

use pulldown_cmark::{
    Alignment, BlockQuoteKind, CodeBlockKind, Event, LinkType, Options, Parser, Tag, TagEnd,
};

/// One styled run of inline text. Adjacent runs with the same style are merged
/// so a single link is a single click target.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct InlineSpan {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub strikethrough: bool,
    pub code: bool,
    pub link: Option<String>,
    /// A bare URL or `<autolink>`, as opposed to a `[label](url)` link.
    /// Autolinks get an external-link mark in the rendered text.
    pub autolink: bool,
}

/// Inline content of a paragraph, heading, list item, or table cell.
#[derive(Clone, Debug, Default)]
pub(crate) struct RichText {
    pub spans: Vec<InlineSpan>,
}

impl RichText {
    pub(crate) fn from_plain(text: impl Into<String>) -> Self {
        let text = text.into();
        if text.is_empty() {
            Self::default()
        } else {
            Self {
                spans: vec![InlineSpan {
                    text,
                    ..InlineSpan::default()
                }],
            }
        }
    }

    pub(crate) fn plain(&self) -> String {
        let mut out = String::new();
        for span in &self.spans {
            out.push_str(&span.text);
        }
        out
    }

    fn is_blank(&self) -> bool {
        self.plain().trim().is_empty()
    }

    fn push_span(&mut self, span: InlineSpan) {
        if span.text.is_empty() {
            return;
        }
        if let Some(last) = self.spans.last_mut() {
            if same_style(last, &span) {
                last.text.push_str(&span.text);
                return;
            }
        }
        self.spans.push(span);
    }

    fn push_styled(&mut self, text: &str, style: &Style) {
        if text.is_empty() {
            return;
        }
        self.push_span(InlineSpan {
            text: text.to_string(),
            bold: style.bold > 0,
            italic: style.italic > 0,
            strikethrough: style.strikethrough > 0,
            code: style.code,
            link: style.link.clone(),
            autolink: style.autolink,
        });
    }

    /// Append `other`, separated by a space when both sides have text.
    fn join_space(&mut self, other: RichText) {
        if other.is_blank() {
            return;
        }
        if self.is_blank() {
            *self = other;
            return;
        }
        self.push_styled(" ", &Style::default());
        for span in other.spans {
            self.push_span(span);
        }
    }
}

fn same_style(left: &InlineSpan, right: &InlineSpan) -> bool {
    left.bold == right.bold
        && left.italic == right.italic
        && left.strikethrough == right.strikethrough
        && left.code == right.code
        && left.link == right.link
        && left.autolink == right.autolink
}

/// Emphasis is counted, not toggled. `*a _b_ c*` emits an inner end that must
/// leave the outer italic in place; a bool would clear it for the trailing `c`.
#[derive(Clone, Debug, Default)]
struct Style {
    bold: u8,
    italic: u8,
    strikethrough: u8,
    code: bool,
    link: Option<String>,
    autolink: bool,
}

/// A block plus the preorder index used for selection ids and code-fold keys.
///
/// The index is assigned before a container's children, and every nested block
/// gets its own, so a code fence inside a quote does not share an id with the
/// quote or with a later sibling.
#[derive(Clone, Debug)]
pub(crate) struct MdBlock {
    pub index: u16,
    pub kind: Block,
}

#[derive(Clone, Debug)]
pub(crate) enum Block {
    Paragraph(RichText),
    Heading(u8, RichText),
    Code(Option<String>, Vec<String>),
    List(Vec<ListItem>),
    Quote(Vec<MdBlock>),
    /// GitHub-style alert (`> [!NOTE]` …). The marker line is not part of the body.
    Alert(AlertKind, Vec<MdBlock>),
    Rule,
    Table {
        header: Vec<RichText>,
        rows: Vec<Vec<RichText>>,
        aligns: Vec<TableAlign>,
    },
    Image {
        alt: String,
        url: String,
    },
}

/// GFM column alignment. `Alignment::None` from the parser is [`Left`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TableAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// Kinds of GitHub alert blockquote (`> [!WARNING]` …).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AlertKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

#[derive(Clone, Debug)]
pub(crate) struct ListItem {
    pub depth: usize,
    pub ordered: bool,
    pub number: u64,
    /// `Some(true)` checked, `Some(false)` open, `None` a plain item.
    pub checked: Option<bool>,
    pub text: RichText,
}

/// Quotes nested deeper than this are flattened into the fourth quote so a
/// pasted diff cannot grow an unbounded container chain.
const MAX_QUOTE_NESTING: usize = 4;

/// Parse `text` into painted blocks.
///
/// Enabled extensions are tables, strikethrough, task lists, and GitHub alerts.
/// Smart punctuation, footnotes, and metadata blocks stay off: a chat line of
/// `---` must stay a rule, and `a -- b` must stay ASCII.
pub(crate) fn parse_blocks(text: &str) -> Vec<MdBlock> {
    let mut walker = Walker::new(text);
    walker.read_blocks(0, false)
}

/// Spans of the first paragraph in `text`. Tests and the inline-run checks
/// use this in place of the old line scanner.
pub(crate) fn inline_spans(text: &str) -> Vec<InlineSpan> {
    parse_blocks(text)
        .into_iter()
        .find_map(|block| match block.kind {
            Block::Paragraph(rich) => Some(rich.spans),
            _ => None,
        })
        .unwrap_or_default()
}

fn parser_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM
}

struct Walker<'a> {
    source: &'a str,
    events: Vec<(Event<'a>, Range<usize>)>,
    cursor: usize,
    next_index: u16,
    /// Source offset through which text is dropped because it belongs to a
    /// bare URL that started in an earlier event. Style start/end events in
    /// that span still update the style stack.
    suppress_until: usize,
    pending_checked: Option<bool>,
}

impl<'a> Walker<'a> {
    fn new(source: &'a str) -> Self {
        let events = Parser::new_ext(source, parser_options())
            .into_offset_iter()
            .collect();
        Self {
            source,
            events,
            cursor: 0,
            next_index: 0,
            suppress_until: 0,
            pending_checked: None,
        }
    }

    fn alloc(&mut self) -> u16 {
        let index = self.next_index;
        self.next_index = self.next_index.saturating_add(1);
        index
    }

    fn block(&mut self, kind: Block) -> MdBlock {
        MdBlock {
            index: self.alloc(),
            kind,
        }
    }

    fn peek(&self) -> Option<Event<'a>> {
        self.events.get(self.cursor).map(|(event, _)| event.clone())
    }

    fn peek_range(&self) -> Range<usize> {
        self.events
            .get(self.cursor)
            .map(|(_, range)| range.clone())
            .unwrap_or(0..0)
    }

    fn bump(&mut self) -> Option<Event<'a>> {
        let event = self.peek()?;
        self.cursor += 1;
        Some(event)
    }

    fn read_blocks(&mut self, quote_depth: usize, stop_on_quote_end: bool) -> Vec<MdBlock> {
        let mut out = Vec::new();
        while let Some(event) = self.peek() {
            if stop_on_quote_end && matches!(event, Event::End(TagEnd::BlockQuote(_))) {
                break;
            }
            // A stray end at the top level used to stop the walk and drop the
            // rest of the document. Inside a quote, an end still closes it.
            if matches!(event, Event::End(_)) {
                if quote_depth == 0 && !stop_on_quote_end {
                    self.bump();
                    continue;
                }
                break;
            }
            let before = self.cursor;
            out.extend(self.read_one(quote_depth));
            if self.cursor == before {
                self.bump();
            }
        }
        out
    }

    fn read_one(&mut self, quote_depth: usize) -> Vec<MdBlock> {
        match self.peek() {
            Some(Event::Start(Tag::Paragraph)) => {
                let chunks = self.read_paragraph();
                self.chunks_to_blocks(chunks)
            }
            Some(Event::Start(Tag::Heading { .. })) => self.read_heading(),
            Some(Event::Start(Tag::CodeBlock(_))) => vec![self.read_code()],
            Some(Event::Start(Tag::List(_))) => self.read_list(0, quote_depth),
            Some(Event::Start(Tag::BlockQuote(_))) => self.read_quote(quote_depth),
            Some(Event::Start(Tag::Table(_))) => vec![self.read_table()],
            Some(Event::Start(Tag::HtmlBlock)) => self.read_html(),
            Some(Event::Rule) => {
                self.bump();
                vec![self.block(Block::Rule)]
            }
            Some(Event::Start(
                Tag::FootnoteDefinition(_)
                | Tag::MetadataBlock(_)
                | Tag::DefinitionList
                | Tag::DefinitionListTitle
                | Tag::DefinitionListDefinition,
            )) => {
                self.skip_container();
                Vec::new()
            }
            _ => {
                self.bump();
                Vec::new()
            }
        }
    }

    fn chunks_to_blocks(&mut self, chunks: Vec<Chunk>) -> Vec<MdBlock> {
        let mut out = Vec::new();
        for chunk in chunks {
            match chunk {
                Chunk::Text(text) if !text.is_blank() => {
                    out.push(self.block(Block::Paragraph(text)))
                }
                Chunk::Image { alt, url } => out.push(self.block(Block::Image { alt, url })),
                Chunk::Text(_) => {}
            }
        }
        out
    }

    fn read_heading(&mut self) -> Vec<MdBlock> {
        let level = match self.bump() {
            Some(Event::Start(Tag::Heading { level, .. })) => level,
            _ => return Vec::new(),
        };
        let text = self.read_inlines_until(TagEnd::Heading(level));
        if text.is_blank() {
            Vec::new()
        } else {
            vec![self.block(Block::Heading(level as u8, text))]
        }
    }

    fn read_code(&mut self) -> MdBlock {
        let language = match self.bump() {
            Some(Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info)))) => {
                fence_language(&info)
            }
            _ => None,
        };
        let mut body = String::new();
        while !matches!(self.peek(), Some(Event::End(TagEnd::CodeBlock)) | None) {
            if let Some(Event::Text(text)) = self.peek() {
                body.push_str(&text);
            }
            self.bump();
        }
        if matches!(self.peek(), Some(Event::End(TagEnd::CodeBlock))) {
            self.bump();
        }
        let lines = code_lines(&body);
        self.block(Block::Code(language, lines))
    }

    fn read_quote(&mut self, depth: usize) -> Vec<MdBlock> {
        let kind = match self.bump() {
            Some(Event::Start(Tag::BlockQuote(kind))) => kind,
            _ => return Vec::new(),
        };
        if depth >= MAX_QUOTE_NESTING {
            let children = self.read_blocks(depth, true);
            self.bump_quote_end();
            return children;
        }
        let index = self.alloc();
        let children = self.read_blocks(depth + 1, true);
        self.bump_quote_end();
        let kind = match kind {
            Some(kind) => Block::Alert(alert_kind(kind), children),
            None => Block::Quote(children),
        };
        vec![MdBlock { index, kind }]
    }

    fn bump_quote_end(&mut self) {
        if matches!(self.peek(), Some(Event::End(TagEnd::BlockQuote(_)))) {
            self.bump();
        }
    }

    fn read_html(&mut self) -> Vec<MdBlock> {
        self.bump();
        let mut literal = String::new();
        let mut out = Vec::new();
        while !matches!(self.peek(), Some(Event::End(TagEnd::HtmlBlock)) | None) {
            match self.bump() {
                Some(Event::Html(html)) => {
                    let pieces = html_pieces(&html);
                    if images_only(&pieces) {
                        self.flush_literal(&mut literal, &mut out);
                        for piece in pieces {
                            if let HtmlPiece::Image { alt, url } = piece {
                                out.push(self.block(Block::Image { alt, url }));
                            }
                        }
                    } else {
                        literal.push_str(&html);
                    }
                }
                Some(_) => {}
                None => break,
            }
        }
        if matches!(self.peek(), Some(Event::End(TagEnd::HtmlBlock))) {
            self.bump();
        }
        self.flush_literal(&mut literal, &mut out);
        out
    }

    fn flush_literal(&mut self, literal: &mut String, out: &mut Vec<MdBlock>) {
        let text = std::mem::take(literal);
        let trimmed = text.trim_end();
        if trimmed.trim().is_empty() {
            return;
        }
        out.push(self.block(Block::Paragraph(RichText::from_plain(trimmed.to_string()))));
    }

    fn read_table(&mut self) -> MdBlock {
        let aligns = match self.bump() {
            Some(Event::Start(Tag::Table(aligns))) => aligns.into_iter().map(table_align).collect(),
            _ => Vec::new(),
        };
        let mut header = Vec::new();
        let mut rows = Vec::new();
        while !matches!(self.peek(), Some(Event::End(TagEnd::Table)) | None) {
            match self.peek() {
                // The header row is a `TableHead` whose cells are direct
                // children. pulldown parses it as a row, then relabels the
                // row itself — there is no `TableRow` inside the head.
                Some(Event::Start(Tag::TableHead)) => {
                    self.bump();
                    header = self.read_cells_until(TagEnd::TableHead);
                }
                Some(Event::Start(Tag::TableRow)) => {
                    self.bump();
                    rows.push(self.read_cells_until(TagEnd::TableRow));
                }
                _ => {
                    self.bump();
                }
            }
        }
        if matches!(self.peek(), Some(Event::End(TagEnd::Table))) {
            self.bump();
        }
        let columns = header.len();
        for row in &mut rows {
            if row.len() > columns {
                row.truncate(columns);
            }
            while row.len() < columns {
                row.push(RichText::default());
            }
        }
        self.block(Block::Table {
            header,
            rows,
            aligns,
        })
    }

    fn read_cells_until(&mut self, end: TagEnd) -> Vec<RichText> {
        let mut cells = Vec::new();
        while !matches!(self.peek(), Some(Event::End(tag)) if tag == end) {
            match self.peek() {
                None => break,
                Some(Event::Start(Tag::TableCell)) => {
                    self.bump();
                    cells.push(self.read_inlines_until(TagEnd::TableCell));
                }
                _ => {
                    self.bump();
                }
            }
        }
        if matches!(self.peek(), Some(Event::End(tag)) if tag == end) {
            self.bump();
        }
        cells
    }

    fn read_list(&mut self, depth: usize, quote_depth: usize) -> Vec<MdBlock> {
        let depth = depth.min(2);
        let start = match self.bump() {
            Some(Event::Start(Tag::List(start))) => start,
            _ => return Vec::new(),
        };
        let ordered = start.is_some();
        let mut number = start.unwrap_or(1);
        let mut items = Vec::new();
        let mut out = Vec::new();
        while !matches!(self.peek(), Some(Event::End(TagEnd::List(_))) | None) {
            if !matches!(self.peek(), Some(Event::Start(Tag::Item))) {
                self.bump();
                continue;
            }
            self.bump();
            self.pending_checked = None;
            let mut checked = None;
            let mut text = RichText::default();
            let mut after_break = false;
            // One source item consumes one ordered number, even when its only
            // content is a fence or an image that leaves the list.
            let mut accounted = false;
            while !matches!(self.peek(), Some(Event::End(TagEnd::Item)) | None) {
                self.apply_checked(&mut checked);
                let before = self.cursor;
                if self.take_item_image(
                    &mut text,
                    &mut items,
                    &mut out,
                    &mut after_break,
                    &mut checked,
                    &mut accounted,
                    depth,
                    ordered,
                    &mut number,
                ) {
                    continue;
                }
                match self.peek() {
                    Some(Event::Start(Tag::Paragraph)) => {
                        let chunks = self.read_paragraph();
                        self.absorb_chunks(
                            &mut text,
                            &mut items,
                            &mut out,
                            &mut after_break,
                            &mut checked,
                            &mut accounted,
                            chunks,
                            depth,
                            ordered,
                            &mut number,
                        );
                    }
                    Some(Event::Start(Tag::List(_))) => {
                        self.apply_checked(&mut checked);
                        self.close_item(
                            &mut items,
                            &mut text,
                            &mut checked,
                            depth,
                            ordered,
                            &mut number,
                            &mut accounted,
                            true,
                        );
                        let nested = self.read_list(depth + 1, quote_depth);
                        self.absorb_nested(&mut items, &mut out, nested);
                        // Trailing text of this item is a sibling paragraph.
                        // Another bullet would renumber the ordered list.
                        after_break = true;
                    }
                    Some(event) if is_breaking(&event) => {
                        self.apply_checked(&mut checked);
                        self.close_item(
                            &mut items,
                            &mut text,
                            &mut checked,
                            depth,
                            ordered,
                            &mut number,
                            &mut accounted,
                            true,
                        );
                        self.flush_items(&mut items, &mut out);
                        after_break = true;
                        out.extend(self.read_one(quote_depth));
                    }
                    Some(Event::TaskListMarker(value)) => {
                        if checked.is_none() {
                            checked = Some(value);
                        }
                        self.bump();
                    }
                    _ => {
                        let mut extra = RichText::default();
                        let mut style = Style::default();
                        while !self.at_item_boundary() {
                            if !self.consume_inline(
                                if after_break { &mut extra } else { &mut text },
                                &mut style,
                            ) {
                                break;
                            }
                        }
                        if after_break && !extra.is_blank() {
                            self.flush_items(&mut items, &mut out);
                            out.push(self.block(Block::Paragraph(extra)));
                        }
                    }
                }
                if self.cursor == before {
                    self.bump();
                }
            }
            self.apply_checked(&mut checked);
            if !after_break {
                self.close_item(
                    &mut items,
                    &mut text,
                    &mut checked,
                    depth,
                    ordered,
                    &mut number,
                    &mut accounted,
                    false,
                );
            } else if !text.is_blank() {
                self.flush_items(&mut items, &mut out);
                out.push(self.block(Block::Paragraph(text)));
            }
            if matches!(self.peek(), Some(Event::End(TagEnd::Item))) {
                self.bump();
            }
        }
        if matches!(self.peek(), Some(Event::End(TagEnd::List(_)))) {
            self.bump();
        }
        self.flush_items(&mut items, &mut out);
        out
    }

    fn take_item_image(
        &mut self,
        text: &mut RichText,
        items: &mut Vec<ListItem>,
        out: &mut Vec<MdBlock>,
        after_break: &mut bool,
        checked: &mut Option<bool>,
        accounted: &mut bool,
        depth: usize,
        ordered: bool,
        number: &mut u64,
    ) -> bool {
        // A tight item has no paragraph, so an image event sits directly on
        // the item. Only an image that owns its line (or is the whole item)
        // leaves the list. `see ![x](url) here` stays alt text.
        let image = matches!(
            self.peek(),
            Some(Event::Start(Tag::Image { dest_url, .. })) if !dest_url.is_empty()
        );
        if !image || !self.block_image_at(self.cursor) {
            return false;
        }
        let (alt, url, _) = self.read_image();
        if url.is_empty() {
            text.push_styled(&alt, &Style::default());
            return true;
        }
        self.close_item(
            items, text, checked, depth, ordered, number, accounted, true,
        );
        self.flush_items(items, out);
        out.push(self.block(Block::Image { alt, url }));
        *after_break = true;
        true
    }

    fn absorb_chunks(
        &mut self,
        text: &mut RichText,
        items: &mut Vec<ListItem>,
        out: &mut Vec<MdBlock>,
        after_break: &mut bool,
        checked: &mut Option<bool>,
        accounted: &mut bool,
        chunks: Vec<Chunk>,
        depth: usize,
        ordered: bool,
        number: &mut u64,
    ) {
        for chunk in chunks {
            match chunk {
                Chunk::Text(rich) if rich.is_blank() => {}
                Chunk::Text(rich) if !*after_break => text.join_space(rich),
                Chunk::Text(rich) => {
                    self.flush_items(items, out);
                    out.push(self.block(Block::Paragraph(rich)));
                }
                Chunk::Image { alt, url } => {
                    self.close_item(
                        items, text, checked, depth, ordered, number, accounted, true,
                    );
                    self.flush_items(items, out);
                    out.push(self.block(Block::Image { alt, url }));
                    *after_break = true;
                }
            }
        }
    }

    fn absorb_nested(
        &mut self,
        items: &mut Vec<ListItem>,
        out: &mut Vec<MdBlock>,
        nested: Vec<MdBlock>,
    ) {
        for block in nested {
            if let Block::List(nested_items) = block.kind {
                items.extend(nested_items);
            } else {
                self.flush_items(items, out);
                out.push(block);
            }
        }
    }

    /// Push the current item text, or consume its ordered number when the item
    /// itself produced no text (a leading fence, a block image).
    ///
    /// `consume_empty` is set when a sibling block replaces the bullet. The
    /// natural end of an empty item does not consume a number.
    fn close_item(
        &mut self,
        items: &mut Vec<ListItem>,
        text: &mut RichText,
        checked: &mut Option<bool>,
        depth: usize,
        ordered: bool,
        number: &mut u64,
        accounted: &mut bool,
        consume_empty: bool,
    ) {
        self.apply_checked(checked);
        if text.is_blank() {
            text.spans.clear();
            if consume_empty && !*accounted {
                if ordered {
                    *number += 1;
                }
                *accounted = true;
            }
            return;
        }
        items.push(ListItem {
            depth,
            ordered,
            number: if ordered { *number } else { 0 },
            checked: checked.take(),
            text: std::mem::take(text),
        });
        if ordered {
            *number += 1;
        }
        *accounted = true;
    }

    fn flush_items(&mut self, items: &mut Vec<ListItem>, out: &mut Vec<MdBlock>) {
        if items.is_empty() {
            return;
        }
        out.push(self.block(Block::List(std::mem::take(items))));
    }

    fn apply_checked(&mut self, checked: &mut Option<bool>) {
        if checked.is_none() {
            if let Some(value) = self.pending_checked.take() {
                *checked = Some(value);
            }
        } else {
            self.pending_checked = None;
        }
    }

    fn at_item_boundary(&self) -> bool {
        match self.peek() {
            None => true,
            Some(Event::End(TagEnd::Item)) => true,
            Some(Event::Start(Tag::Paragraph | Tag::List(_) | Tag::Item)) => true,
            // Stop so a block image can leave the list. An image inside a link
            // or a sentence stays in this inline run and keeps the link style.
            Some(Event::Start(Tag::Image { .. })) => self.block_image_at(self.cursor),
            Some(event) => is_breaking(&event),
        }
    }

    fn read_paragraph(&mut self) -> Vec<Chunk> {
        if !matches!(self.peek(), Some(Event::Start(Tag::Paragraph))) {
            return Vec::new();
        }
        self.bump();
        let mut chunks = Vec::new();
        let mut rich = RichText::default();
        let mut style = Style::default();
        while !matches!(self.peek(), Some(Event::End(TagEnd::Paragraph)) | None) {
            if matches!(self.peek(), Some(Event::SoftBreak | Event::HardBreak))
                && self.next_is_line_image()
            {
                self.bump();
                continue;
            }
            if self.take_line_image(&mut rich, &mut chunks) {
                continue;
            }
            if !self.consume_inline(&mut rich, &mut style) {
                break;
            }
        }
        if matches!(self.peek(), Some(Event::End(TagEnd::Paragraph))) {
            self.bump();
        }
        if !rich.is_blank() {
            chunks.push(Chunk::Text(rich));
        }
        chunks
    }

    fn read_inlines_until(&mut self, end: TagEnd) -> RichText {
        let mut rich = RichText::default();
        let mut style = Style::default();
        while !matches!(self.peek(), Some(Event::End(tag)) if tag == end) {
            if self.peek().is_none() {
                break;
            }
            if matches!(self.peek(), Some(Event::Start(Tag::Paragraph))) {
                let chunks = self.read_paragraph();
                for chunk in chunks {
                    match chunk {
                        Chunk::Text(text) => rich.join_space(text),
                        Chunk::Image { alt, .. } => rich.push_styled(&alt, &Style::default()),
                    }
                }
                continue;
            }
            if !self.consume_inline(&mut rich, &mut style) {
                break;
            }
        }
        if matches!(self.peek(), Some(Event::End(tag)) if tag == end) {
            self.bump();
        }
        rich
    }

    fn take_line_image(&mut self, rich: &mut RichText, chunks: &mut Vec<Chunk>) -> bool {
        let sole = rich.is_blank() && chunks.is_empty() && self.sole_image_fallback();
        if !self.block_image_at(self.cursor) && !sole {
            return false;
        }
        match self.peek() {
            Some(Event::Start(Tag::Image { .. })) => {
                let (alt, url, _) = self.read_image();
                self.flush_rich(rich, chunks);
                if url.is_empty() {
                    rich.push_styled(&alt, &Style::default());
                } else {
                    chunks.push(Chunk::Image { alt, url });
                }
            }
            Some(Event::InlineHtml(_)) => {
                let html = match self.bump() {
                    Some(Event::InlineHtml(html)) => html.to_string(),
                    _ => return true,
                };
                self.flush_rich(rich, chunks);
                for piece in html_pieces(&html) {
                    if let HtmlPiece::Image { alt, url } = piece {
                        chunks.push(Chunk::Image { alt, url });
                    }
                }
            }
            _ => return false,
        }
        if matches!(self.peek(), Some(Event::SoftBreak | Event::HardBreak)) {
            self.bump();
        }
        true
    }

    fn flush_rich(&mut self, rich: &mut RichText, chunks: &mut Vec<Chunk>) {
        if rich.is_blank() {
            rich.spans.clear();
            return;
        }
        chunks.push(Chunk::Text(std::mem::take(rich)));
    }

    fn next_is_line_image(&self) -> bool {
        self.block_image_at(self.cursor + 1)
    }

    /// An image or `<img>` that owns its source line, or whose neighboring
    /// events are line edges. The event check is what sees `> ![x](url)` and
    /// `- ![x](url)`: the marker is not part of the image's source range, so
    /// a raw line test says the image does not fill the line.
    fn block_image_at(&self, index: usize) -> bool {
        let extracted = match self.events.get(index) {
            Some((Event::Start(Tag::Image { dest_url, .. }), range)) => {
                Some((!dest_url.is_empty(), range.clone()))
            }
            Some((Event::InlineHtml(html), range)) => {
                Some((images_only(&html_pieces(html)), range.clone()))
            }
            _ => None,
        };
        let Some((candidate, range)) = extracted else {
            return false;
        };
        candidate && (self.occupies_line(&range) || self.event_on_own_line(index))
    }

    fn sole_image_fallback(&self) -> bool {
        match self.peek() {
            Some(Event::Start(Tag::Image { dest_url, .. })) if !dest_url.is_empty() => {
                self.image_is_sole()
            }
            Some(Event::InlineHtml(html)) if images_only(&html_pieces(&html)) => {
                self.following_is_paragraph_end()
            }
            _ => false,
        }
    }

    fn event_on_own_line(&self, index: usize) -> bool {
        let before_edge = index == 0 || self.is_line_edge(index - 1, true);
        before_edge && self.is_line_edge(self.sibling_after(index), false)
    }

    /// `opening` is true for the event before the image (a start tag or a
    /// break). The event after the image is a break or the container's end.
    fn is_line_edge(&self, index: usize, opening: bool) -> bool {
        match self.events.get(index).map(|(event, _)| event) {
            None => true,
            Some(Event::SoftBreak | Event::HardBreak) => true,
            Some(Event::Start(
                Tag::Paragraph | Tag::Item | Tag::TableCell | Tag::Heading { .. },
            )) if opening => true,
            Some(Event::End(
                TagEnd::Paragraph | TagEnd::Item | TagEnd::TableCell | TagEnd::Heading(_),
            )) if !opening => true,
            _ => false,
        }
    }

    fn sibling_after(&self, index: usize) -> usize {
        let Some((event, _)) = self.events.get(index) else {
            return index + 1;
        };
        if !matches!(event, Event::Start(Tag::Image { .. })) {
            return index + 1;
        }
        let mut depth = 0usize;
        let mut cursor = index;
        while let Some((event, _)) = self.events.get(cursor) {
            match event {
                Event::Start(Tag::Image { .. }) => depth += 1,
                Event::End(TagEnd::Image) => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return cursor + 1;
                    }
                }
                _ => {}
            }
            cursor += 1;
        }
        cursor
    }

    /// The image at the cursor is the only content of its paragraph.
    fn image_is_sole(&self) -> bool {
        let mut depth = 0usize;
        let mut index = 0usize;
        let mut closed = false;
        while let Some((event, _)) = self.events.get(self.cursor + index) {
            match event {
                Event::Start(_) => depth += 1,
                Event::End(TagEnd::Image) => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        closed = true;
                        index += 1;
                        break;
                    }
                }
                Event::End(_) => depth = depth.saturating_sub(1),
                _ => {}
            }
            index += 1;
        }
        if !closed {
            return false;
        }
        while let Some((event, _)) = self.events.get(self.cursor + index) {
            match event {
                Event::End(TagEnd::Paragraph) => return true,
                Event::SoftBreak | Event::HardBreak => index += 1,
                _ => return false,
            }
        }
        true
    }

    fn following_is_paragraph_end(&self) -> bool {
        let mut index = 1usize;
        while let Some((event, _)) = self.events.get(self.cursor + index) {
            match event {
                Event::End(TagEnd::Paragraph) => return true,
                Event::SoftBreak | Event::HardBreak => index += 1,
                _ => return false,
            }
        }
        false
    }

    /// `range`, ignoring surrounding spaces and tabs, fills its source line.
    fn occupies_line(&self, range: &Range<usize>) -> bool {
        let Some(source) = self.source.get(..) else {
            return false;
        };
        if range.end > source.len() || range.start > range.end {
            return false;
        }
        let line_start = source[..range.start]
            .rfind('\n')
            .map(|index| index + 1)
            .unwrap_or(0);
        let line_end = source[range.end..]
            .find('\n')
            .map(|index| range.end + index)
            .unwrap_or(source.len());
        source[line_start..range.start]
            .trim_matches([' ', '\t', '\r'])
            .is_empty()
            && source[range.end..line_end]
                .trim_matches([' ', '\t', '\r'])
                .is_empty()
    }

    fn consume_inline(&mut self, rich: &mut RichText, style: &mut Style) -> bool {
        let Some(event) = self.peek() else {
            return false;
        };
        let range = self.peek_range();
        match event {
            Event::Start(Tag::Emphasis) => {
                style.italic = style.italic.saturating_add(1);
                self.bump();
            }
            Event::End(TagEnd::Emphasis) => {
                style.italic = style.italic.saturating_sub(1);
                self.bump();
            }
            Event::Start(Tag::Strong) => {
                style.bold = style.bold.saturating_add(1);
                self.bump();
            }
            Event::End(TagEnd::Strong) => {
                style.bold = style.bold.saturating_sub(1);
                self.bump();
            }
            Event::Start(Tag::Strikethrough) => {
                style.strikethrough = style.strikethrough.saturating_add(1);
                self.bump();
            }
            Event::End(TagEnd::Strikethrough) => {
                style.strikethrough = style.strikethrough.saturating_sub(1);
                self.bump();
            }
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                ..
            }) => {
                style.link = Some(link_destination(link_type, &dest_url));
                style.autolink = link_type == LinkType::Autolink;
                self.bump();
            }
            Event::End(TagEnd::Link) => {
                style.link = None;
                style.autolink = false;
                self.bump();
            }
            Event::Start(Tag::Image { .. }) => {
                let (alt, _, _) = self.read_image();
                rich.push_styled(&alt, style);
            }
            Event::Text(text) => {
                self.bump();
                self.emit_text(rich, &text, range, style);
            }
            Event::Code(text) => {
                self.bump();
                let mut code_style = style.clone();
                code_style.code = true;
                code_style.autolink = false;
                self.emit_text(rich, &text, range, &code_style);
            }
            Event::SoftBreak => {
                self.bump();
                if self.take_visible(range).is_some() {
                    rich.push_styled(" ", style);
                }
            }
            Event::HardBreak => {
                self.bump();
                if self.take_visible(range).is_some() {
                    rich.push_styled("\n", style);
                }
            }
            Event::InlineHtml(html) => {
                self.bump();
                if self.take_visible(range.clone()).is_none() {
                    return true;
                }
                if is_break_tag(&html) {
                    rich.push_styled("\n", style);
                } else {
                    self.push_html(rich, &html, style);
                }
            }
            Event::TaskListMarker(value) => {
                self.pending_checked = Some(value);
                self.bump();
            }
            Event::Start(_) => {
                self.skip_container();
            }
            _ => {
                self.bump();
            }
        }
        true
    }

    fn push_html(&mut self, rich: &mut RichText, html: &str, style: &Style) {
        let pieces = html_pieces(html);
        if !pieces
            .iter()
            .any(|piece| matches!(piece, HtmlPiece::Image { .. }))
        {
            rich.push_styled(html, style);
            return;
        }
        for piece in pieces {
            match piece {
                HtmlPiece::Image { alt, .. } => rich.push_styled(&alt, style),
                HtmlPiece::Text(text) if text.trim().is_empty() => {}
                HtmlPiece::Text(text) => rich.push_styled(&text, style),
            }
        }
    }

    fn read_image(&mut self) -> (String, String, Range<usize>) {
        let range = self.peek_range();
        let dest = match self.bump() {
            Some(Event::Start(Tag::Image { dest_url, .. })) => dest_url.to_string(),
            _ => return (String::new(), String::new(), range),
        };
        let mut alt = String::new();
        let mut depth = 1usize;
        while depth > 0 {
            let Some(event) = self.bump() else {
                break;
            };
            match event {
                Event::Start(_) => depth += 1,
                Event::End(_) => depth -= 1,
                Event::Text(text) | Event::Code(text) => alt.push_str(&text),
                Event::SoftBreak | Event::HardBreak => alt.push(' '),
                _ => {}
            }
        }
        let url = if dest.is_empty() {
            String::new()
        } else {
            normalize_image_url(&dest)
        };
        (alt, url, range)
    }

    fn skip_container(&mut self) {
        let mut depth = 0usize;
        loop {
            match self.bump() {
                Some(Event::Start(_)) => depth += 1,
                Some(Event::End(_)) => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        break;
                    }
                }
                Some(_) => {}
                None => break,
            }
        }
    }

    fn emit_text(&mut self, rich: &mut RichText, text: &str, range: Range<usize>, style: &Style) {
        let Some(visible) = self.take_visible(range.clone()) else {
            return;
        };
        if style.code || style.link.is_some() {
            if visible == range {
                rich.push_styled(text, style);
            } else if let Some(tail) = self.source.get(visible) {
                rich.push_styled(tail, style);
            }
            return;
        }
        let source_matches = self.source.get(range.clone()) == Some(text);
        if source_matches && visible == range {
            self.scan_source_range(rich, range, style);
        } else if let Some(tail) = self.source.get(visible) {
            self.scan_plain(rich, tail, style);
        } else {
            self.scan_plain(rich, text, style);
        }
    }

    fn scan_source_range(&mut self, rich: &mut RichText, range: Range<usize>, style: &Style) {
        let Some(slice) = self.source.get(range.start..range.end) else {
            return;
        };
        let chars: Vec<char> = slice.chars().collect();
        let mut byte_at = vec![0usize; chars.len() + 1];
        let mut byte = 0usize;
        for (index, ch) in chars.iter().enumerate() {
            byte_at[index] = byte;
            byte += ch.len_utf8();
        }
        byte_at[chars.len()] = byte;

        let mut last = 0usize;
        let mut cursor = 0usize;
        while cursor < chars.len() {
            if let Some(end) = autolink_end(&chars, cursor) {
                if cursor > last {
                    let start = range.start + byte_at[last];
                    let stop = range.start + byte_at[cursor];
                    if let Some(prefix) = self.source.get(start..stop) {
                        rich.push_styled(prefix, style);
                    }
                }
                let src_start = range.start + byte_at[cursor];
                let mut src_end = range.start + byte_at[end];
                if end == chars.len() {
                    if let Some(extended) = autolink_byte_end(self.source, src_start) {
                        if extended > range.end {
                            src_end = extended;
                            self.suppress_until = extended;
                        }
                    }
                }
                if let Some(url) = self.source.get(src_start..src_end) {
                    let mut link_style = style.clone();
                    link_style.link = Some(url.to_string());
                    link_style.autolink = true;
                    rich.push_styled(url, &link_style);
                }
                if src_end > range.end {
                    return;
                }
                cursor = end;
                last = end;
            } else {
                cursor += 1;
            }
        }
        if last < chars.len() {
            let start = range.start + byte_at[last];
            if let Some(tail) = self.source.get(start..range.end) {
                rich.push_styled(tail, style);
            }
        }
    }

    fn scan_plain(&mut self, rich: &mut RichText, text: &str, style: &Style) {
        let chars: Vec<char> = text.chars().collect();
        let mut last = 0usize;
        let mut cursor = 0usize;
        while cursor < chars.len() {
            if let Some(end) = autolink_end(&chars, cursor) {
                if cursor > last {
                    rich.push_styled(&chars[last..cursor].iter().collect::<String>(), style);
                }
                let url: String = chars[cursor..end].iter().collect();
                let mut link_style = style.clone();
                link_style.link = Some(url.clone());
                link_style.autolink = true;
                rich.push_styled(&url, &link_style);
                cursor = end;
                last = end;
            } else {
                cursor += 1;
            }
        }
        if last < chars.len() {
            rich.push_styled(&chars[last..].iter().collect::<String>(), style);
        }
    }

    fn take_visible(&mut self, range: Range<usize>) -> Option<Range<usize>> {
        if self.suppress_until == 0 {
            return Some(range);
        }
        if range.end <= self.suppress_until {
            return None;
        }
        let start = range.start.max(self.suppress_until);
        self.suppress_until = 0;
        (start < range.end).then_some(start..range.end)
    }
}

enum Chunk {
    Text(RichText),
    Image { alt: String, url: String },
}

fn is_breaking(event: &Event<'_>) -> bool {
    match event {
        Event::Rule => true,
        Event::Start(
            Tag::Heading { .. }
            | Tag::CodeBlock(_)
            | Tag::BlockQuote(_)
            | Tag::Table(_)
            | Tag::HtmlBlock
            | Tag::FootnoteDefinition(_)
            | Tag::MetadataBlock(_)
            | Tag::DefinitionList,
        ) => true,
        _ => false,
    }
}

fn fence_language(info: &str) -> Option<String> {
    let token = info.split_whitespace().next().unwrap_or("");
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

fn code_lines(body: &str) -> Vec<String> {
    if body.is_empty() {
        return Vec::new();
    }
    body.lines().map(str::to_string).collect()
}

fn table_align(align: Alignment) -> TableAlign {
    match align {
        Alignment::Center => TableAlign::Center,
        Alignment::Right => TableAlign::Right,
        Alignment::Left | Alignment::None => TableAlign::Left,
    }
}

fn alert_kind(kind: BlockQuoteKind) -> AlertKind {
    match kind {
        BlockQuoteKind::Note => AlertKind::Note,
        BlockQuoteKind::Tip => AlertKind::Tip,
        BlockQuoteKind::Important => AlertKind::Important,
        BlockQuoteKind::Warning => AlertKind::Warning,
        BlockQuoteKind::Caution => AlertKind::Caution,
    }
}

fn link_destination(link_type: LinkType, dest: &str) -> String {
    if link_type == LinkType::Email && !dest.to_ascii_lowercase().starts_with("mailto:") {
        format!("mailto:{dest}")
    } else {
        dest.to_string()
    }
}

/// The end index (exclusive) of a bare `http://` or `https://` URL starting
/// at `start`, or `None` when the text there is not a URL. The scan stops at
/// whitespace, `<`, and `>`; trailing sentence punctuation, emphasis markers,
/// and closing brackets with no opener inside the URL are left for the prose.
pub(crate) fn autolink_end(chars: &[char], start: usize) -> Option<usize> {
    let tail = chars.get(start..)?;
    if !tail.starts_with(&['h', 't', 't', 'p', 's', ':', '/', '/'])
        && !tail.starts_with(&['h', 't', 't', 'p', ':', '/', '/'])
    {
        return None;
    }
    let mut end = start;
    while end < chars.len() && !chars[end].is_whitespace() && chars[end] != '<' && chars[end] != '>'
    {
        end += 1;
    }
    while end > start {
        let last = chars[end - 1];
        let trim = matches!(
            last,
            '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '"' | '*' | '_' | '~'
        ) || (matches!(last, ')' | ']' | '}') && {
            let opener = match last {
                ')' => '(',
                ']' => '[',
                _ => '{',
            };
            let opens = chars[start..end].iter().filter(|&&ch| ch == opener).count();
            let closes = chars[start..end].iter().filter(|&&ch| ch == last).count();
            closes > opens
        });
        if !trim {
            break;
        }
        end -= 1;
    }
    (end > start).then_some(end)
}

fn autolink_byte_end(source: &str, start: usize) -> Option<usize> {
    let tail = source.get(start..)?;
    // A URL never contains whitespace. Stop there instead of copying the
    // rest of a long message on every link that ends a text event.
    let token_len = tail
        .find(|ch: char| ch.is_whitespace() || ch == '<' || ch == '>')
        .unwrap_or(tail.len());
    let token = tail.get(..token_len)?;
    let chars: Vec<char> = token.chars().collect();
    let end = autolink_end(&chars, 0)?;
    let bytes = chars
        .iter()
        .take(end)
        .map(|ch| ch.len_utf8())
        .sum::<usize>();
    Some(start + bytes)
}

enum HtmlPiece {
    Text(String),
    Image { alt: String, url: String },
}

/// Split `raw` into literal text and `<img>` tags. A line of several images
/// is several pieces; `html_image` only sees the first tag.
fn html_pieces(raw: &str) -> Vec<HtmlPiece> {
    let lower = raw.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut pieces = Vec::new();
    let mut last = 0usize;
    let mut index = 0usize;
    while index + 4 <= bytes.len() {
        let starts_tag = bytes[index..].starts_with(b"<img")
            && (index + 4 == bytes.len() || !bytes[index + 4].is_ascii_alphanumeric());
        if starts_tag {
            if let Some(end) = raw[index..].find('>').map(|rel| index + rel + 1) {
                if let Some((alt, url)) = html_image(&raw[index..end]) {
                    if index > last {
                        pieces.push(HtmlPiece::Text(raw[last..index].to_string()));
                    }
                    pieces.push(HtmlPiece::Image { alt, url });
                    last = end;
                    index = end;
                    continue;
                }
            }
        }
        index += 1;
    }
    if last < raw.len() {
        pieces.push(HtmlPiece::Text(raw[last..].to_string()));
    }
    pieces
}

fn images_only(pieces: &[HtmlPiece]) -> bool {
    pieces
        .iter()
        .any(|piece| matches!(piece, HtmlPiece::Image { .. }))
        && pieces.iter().all(|piece| match piece {
            HtmlPiece::Image { .. } => true,
            HtmlPiece::Text(text) => text.trim().is_empty(),
        })
}

/// Parse a string that is solely an HTML `<img>` into `(alt, url)`.
pub(crate) fn html_image(raw: &str) -> Option<(String, String)> {
    let trimmed = raw.trim();
    if !trimmed.to_ascii_lowercase().starts_with("<img") {
        return None;
    }
    let src = html_attr(trimmed, "src")?;
    if src.is_empty() {
        return None;
    }
    Some((
        html_attr(trimmed, "alt").unwrap_or_default(),
        normalize_image_url(&src),
    ))
}

fn is_break_tag(raw: &str) -> bool {
    let trimmed = raw.trim().to_ascii_lowercase();
    let inner = trimmed
        .strip_prefix('<')
        .and_then(|rest| rest.strip_suffix('>'))
        .unwrap_or("");
    let name = inner.trim().trim_end_matches('/').trim();
    name == "br" || name.starts_with("br ") || name.starts_with("br/")
}

/// Rewrite GitHub's signed `private-user-images` CDN URL to the durable
/// `user-attachments` URL for the same asset.
pub(crate) fn normalize_image_url(url: &str) -> String {
    const SIGNED_HOST: &str = "https://private-user-images.githubusercontent.com/";
    let Some(rest) = url.strip_prefix(SIGNED_HOST) else {
        return url.to_string();
    };
    let path = rest.split(['?', '#']).next().unwrap_or(rest);
    let file = path.rsplit('/').next().unwrap_or(path);
    let Some((_, stem)) = file.split_once('-') else {
        return url.to_string();
    };
    let uuid = stem.rsplit_once('.').map_or(stem, |(stem, _)| stem);
    if !is_uuid(uuid) {
        return url.to_string();
    }
    format!("https://github.com/user-attachments/assets/{uuid}")
}

/// `8-4-4-4-12` hex, the shape of GitHub's asset IDs.
fn is_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => *byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}

/// Read `name="value"` (single or double quoted) from a tag's source text.
fn html_attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let key = format!("{name}=");
    let mut search_from = 0usize;
    let start = loop {
        let rel = lower.get(search_from..)?.find(&key)?;
        let at = search_from + rel;
        let boundary = at == 0
            || matches!(
                lower.as_bytes().get(at - 1).copied(),
                Some(b' ' | b'\t' | b'\n' | b'\r' | b'<')
            );
        if boundary {
            break at + key.len();
        }
        search_from = at + 1;
    };
    let rest = tag.get(start..)?;
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = rest.get(quote.len_utf8()..)?;
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(block: &MdBlock) -> String {
        match &block.kind {
            Block::Paragraph(text) | Block::Heading(_, text) => text.plain(),
            Block::List(items) => items
                .iter()
                .map(|item| item.text.plain())
                .collect::<Vec<_>>()
                .join("|"),
            _ => String::new(),
        }
    }

    fn paragraph(text: &str) -> String {
        let blocks = parse_blocks(text);
        plain(blocks.first().expect("block"))
    }

    #[test]
    fn paragraphs_join_soft_breaks_with_a_space() {
        assert_eq!(paragraph("one\ntwo"), "one two");
    }

    #[test]
    fn inline_marks_links_and_escapes() {
        let spans = inline_spans("plain **bold** and `code` and [x](https://a.b)");
        assert_eq!(spans.len(), 6);
        assert_eq!(spans[0].text, "plain ");
        assert!(spans[1].bold);
        assert_eq!(spans[1].text, "bold");
        assert!(spans[3].code);
        assert_eq!(spans[3].text, "code");
        assert_eq!(spans[5].link.as_deref(), Some("https://a.b"));
        assert_eq!(spans[5].text, "x");
        assert!(!spans[5].autolink);

        let literal = inline_spans("a * b ` c [d");
        let joined: String = literal.iter().map(|span| span.text.as_str()).collect();
        assert_eq!(joined, "a * b ` c [d");
        assert!(literal
            .iter()
            .all(|span| !span.bold && !span.code && span.link.is_none()));

        let escaped = inline_spans(r"\*not bold\*");
        let joined: String = escaped.iter().map(|span| span.text.as_str()).collect();
        assert_eq!(joined, "*not bold*");
        assert!(escaped.iter().all(|span| !span.bold));
    }

    #[test]
    fn headings_fences_and_mermaid() {
        let blocks = parse_blocks("one\ntwo\n\n## Title\n\n```rs\nlet a = 1;\n```");
        assert_eq!(blocks.len(), 3);
        assert_eq!(plain(&blocks[0]), "one two");
        assert!(matches!(&blocks[1].kind, Block::Heading(2, title) if title.plain() == "Title"));
        assert!(matches!(&blocks[2].kind, Block::Code(Some(lang), lines)
            if lang == "rs" && lines == &["let a = 1;".to_string()]));

        let bare = parse_blocks("```\nx\n```");
        assert!(matches!(&bare[0].kind, Block::Code(None, lines) if lines == &["x".to_string()]));

        let mermaid = parse_blocks("```mermaid\ngraph TD; A-->B;\n```");
        assert!(matches!(&mermaid[0].kind, Block::Code(Some(lang), lines)
            if lang == "mermaid" && lines == &["graph TD; A-->B;".to_string()]));

        let info = parse_blocks("```rust ignore\nlet a = 1;\n```");
        assert!(matches!(&info[0].kind, Block::Code(Some(lang), _) if lang == "rust"));

        let empty = parse_blocks("```\n```");
        assert!(matches!(&empty[0].kind, Block::Code(None, lines) if lines.is_empty()));

        let unclosed = parse_blocks("```rs\nlet a = 1;");
        assert!(matches!(&unclosed[0].kind, Block::Code(Some(lang), lines)
            if lang == "rs" && lines == &["let a = 1;".to_string()]));
    }

    #[test]
    fn empty_headings_are_dropped_and_setext_is_a_heading() {
        assert!(parse_blocks("#").is_empty());
        assert!(parse_blocks("# ").is_empty());
        let blocks = parse_blocks("text\n---");
        assert_eq!(blocks.len(), 1);
        assert!(matches!(&blocks[0].kind, Block::Heading(2, title) if title.plain() == "text"));
    }

    #[test]
    fn indented_code_needs_a_blank_line() {
        let prose = parse_blocks("text\n    more");
        assert_eq!(prose.len(), 1);
        assert!(plain(&prose[0]).contains("text"));
        assert!(plain(&prose[0]).contains("more"));
        assert!(!matches!(prose[0].kind, Block::Code(..)));

        let blocks = parse_blocks("para\n\n    code");
        assert!(matches!(&blocks[0].kind, Block::Paragraph(_)));
        assert!(
            matches!(&blocks[1].kind, Block::Code(None, lines) if lines == &["code".to_string()])
        );
    }

    #[test]
    fn smart_punctuation_stays_ascii() {
        assert_eq!(paragraph("a -- b"), "a -- b");
    }

    #[test]
    fn bare_urls_survive_emphasis_and_trim_punctuation() {
        for url in [
            "https://x.dev/__init__.py",
            "https://x.dev/_foo_/bar",
            "https://x.dev/a*b*c",
        ] {
            let spans = inline_spans(url);
            let links: Vec<_> = spans
                .iter()
                .filter(|span| span.autolink)
                .map(|span| span.text.as_str())
                .collect();
            assert_eq!(links, vec![url], "{url}");
        }

        let spans = inline_spans("see https://pi.dev/docs. and http://a.b/(x) now");
        let links: Vec<_> = spans
            .iter()
            .filter_map(|span| span.link.as_deref())
            .collect();
        assert_eq!(links, vec!["https://pi.dev/docs", "http://a.b/(x)"]);
        assert!(spans.iter().filter(|span| span.autolink).count() == 2);

        let spans = inline_spans("(see https://x.dev/a)");
        assert_eq!(
            spans
                .iter()
                .filter_map(|span| span.link.as_deref())
                .collect::<Vec<_>>(),
            vec!["https://x.dev/a"]
        );
        let spans = inline_spans("<https://x.dev>");
        assert_eq!(
            spans
                .iter()
                .filter_map(|span| span.link.as_deref())
                .collect::<Vec<_>>(),
            vec!["https://x.dev"]
        );
        assert!(spans.iter().any(|span| span.autolink));

        assert!(inline_spans("[docs](https://pi.dev)")
            .iter()
            .all(|span| !span.autolink));
        assert!(inline_spans("`https://pi.dev`")
            .iter()
            .all(|span| span.link.is_none()));
    }

    #[test]
    fn email_links_gain_a_mailto_prefix() {
        let spans = inline_spans("<user@example.com>");
        assert_eq!(
            spans.iter().find_map(|span| span.link.clone()),
            Some("mailto:user@example.com".to_string())
        );
    }

    #[test]
    fn reference_definitions_resolve_and_disappear() {
        let blocks = parse_blocks("[hi][id]\n\n[id]: https://example.com");
        let spans = match &blocks[0].kind {
            Block::Paragraph(text) => &text.spans,
            other => panic!("expected a paragraph, got {other:?}"),
        };
        assert_eq!(
            spans.iter().find_map(|span| span.link.clone()).as_deref(),
            Some("https://example.com")
        );
        assert!(blocks.len() == 1);
    }

    #[test]
    fn lists_nest_split_around_fences_and_keep_task_state() {
        let blocks = parse_blocks("- a\n- b\n\n  - nested\n\n3. third\n4. fourth");
        assert_eq!(blocks.len(), 2);
        let Block::List(items) = &blocks[0].kind else {
            panic!("expected list");
        };
        assert_eq!(items.len(), 3);
        assert!(!items[0].ordered);
        assert_eq!(items[2].depth, 1);
        assert_eq!(items[2].text.plain(), "nested");
        let Block::List(ordered) = &blocks[1].kind else {
            panic!("expected ordered list");
        };
        assert!(ordered[0].ordered);
        assert_eq!(ordered[0].number, 3);

        let split = parse_blocks("1. Run:\n   ```bash\n   cargo test\n   ```\n2. Next");
        assert!(
            matches!(&split[0].kind, Block::List(items) if items.len() == 1 && items[0].text.plain() == "Run:")
        );
        assert!(matches!(&split[1].kind, Block::Code(Some(lang), lines)
            if lang == "bash" && lines == &["cargo test".to_string()]));
        assert!(
            matches!(&split[2].kind, Block::List(items) if items[0].text.plain() == "Next" && items[0].number == 2)
        );

        let bullets = parse_blocks("* a\n+ b\n- c");
        assert_eq!(bullets.len(), 3);

        let tasks = parse_blocks("- [ ] open\n- [x] done\n- plain");
        let Block::List(items) = &tasks[0].kind else {
            panic!("expected list");
        };
        assert_eq!(items[0].checked, Some(false));
        assert_eq!(items[0].text.plain(), "open");
        assert_eq!(items[1].checked, Some(true));
        assert_eq!(items[1].text.plain(), "done");
        assert_eq!(items[2].checked, None);
        assert_eq!(items[2].text.plain(), "plain");

        let loose = parse_blocks("- [ ] open\n\n- [x] done");
        let Block::List(items) = &loose[0].kind else {
            panic!("expected loose task list");
        };
        assert_eq!(items[0].checked, Some(false));
        assert_eq!(items[1].checked, Some(true));
    }

    #[test]
    fn rules_quotes_alerts_and_tables() {
        let blocks =
            parse_blocks("---\n\n> quoted\n> lines\n\n| a | b |\n| --- | --- |\n| 1 | 2 |");
        assert!(matches!(blocks[0].kind, Block::Rule));
        let Block::Quote(children) = &blocks[1].kind else {
            panic!("expected quote");
        };
        assert_eq!(children.len(), 1);
        assert_eq!(plain(&children[0]), "quoted lines");
        let Block::Table { header, rows, .. } = &blocks[2].kind else {
            panic!("expected table");
        };
        assert_eq!(header[0].plain(), "a");
        assert_eq!(header[1].plain(), "b");
        assert_eq!(rows[0][0].plain(), "1");
        assert_eq!(rows[0][1].plain(), "2");

        let aligned = parse_blocks("| a | b | c |\n| :-- | :-: | --: |\n| 1 | 2 | 3 |");
        let Block::Table { aligns, .. } = &aligned[0].kind else {
            panic!("expected table");
        };
        assert_eq!(
            aligns,
            &[TableAlign::Left, TableAlign::Center, TableAlign::Right]
        );

        let pipes = parse_blocks("| a \\| b | c |\n| --- | --- |\n| 1 | 2 |");
        let Block::Table { header, .. } = &pipes[0].kind else {
            panic!("expected table");
        };
        assert_eq!(header[0].plain(), "a | b");
        assert_eq!(header[1].plain(), "c");

        // Inside a cell, `\|` is a literal pipe and must not open another column.
        let code_pipe = parse_blocks("| `p\\|q` |\n| --- |\n| x |");
        let Block::Table { header, .. } = &code_pipe[0].kind else {
            panic!("expected table");
        };
        assert_eq!(header.len(), 1);
        assert!(header[0]
            .spans
            .iter()
            .any(|span| span.code && span.text == "p|q"));

        let broken = parse_blocks("| x<br>y |\n| --- |\n| z |");
        let Block::Table { header, .. } = &broken[0].kind else {
            panic!("expected table");
        };
        assert_eq!(header[0].plain(), "x\ny");

        let short = parse_blocks("| a | b |\n| --- | --- |\n| 1 |");
        let Block::Table { rows, .. } = &short[0].kind else {
            panic!("expected table");
        };
        assert_eq!(rows[0].len(), 2);
        assert_eq!(rows[0][1].plain(), "");

        let extra = parse_blocks("| a |\n| --- |\n| 1 | 2 |");
        let Block::Table { rows, .. } = &extra[0].kind else {
            panic!("expected table");
        };
        assert_eq!(rows[0].len(), 1);

        let mismatch = parse_blocks("| a | b |\n| --- |\n| 1 | 2 |");
        assert!(mismatch
            .iter()
            .all(|block| !matches!(block.kind, Block::Table { .. })));
    }

    #[test]
    fn alerts_and_plain_quotes() {
        let blocks = parse_blocks("> [!WARNING]\n> This overwrites the file.");
        let Block::Alert(kind, children) = &blocks[0].kind else {
            panic!("expected alert");
        };
        assert_eq!(*kind, AlertKind::Warning);
        assert_eq!(plain(&children[0]), "This overwrites the file.");

        let note = parse_blocks("> [!note]\n> lower");
        assert!(matches!(&note[0].kind, Block::Alert(AlertKind::Note, _)));

        assert!(matches!(
            parse_blocks("> just a quote")[0].kind,
            Block::Quote(_)
        ));
        assert!(matches!(
            parse_blocks("> [!SOMETHING]\n> x")[0].kind,
            Block::Quote(_)
        ));
    }

    #[test]
    fn deep_quotes_flatten_into_the_fourth() {
        let blocks = parse_blocks("> > > > > a");
        fn depth(block: &MdBlock) -> usize {
            match &block.kind {
                Block::Quote(children) => 1 + children.iter().map(depth).max().unwrap_or(0),
                _ => 0,
            }
        }
        assert_eq!(depth(&blocks[0]), 4);
        fn contains_a(block: &MdBlock) -> bool {
            match &block.kind {
                Block::Paragraph(text) => text.plain() == "a",
                Block::Quote(children) | Block::Alert(_, children) => {
                    children.iter().any(contains_a)
                }
                _ => false,
            }
        }
        assert!(contains_a(&blocks[0]));
    }

    #[test]
    fn quote_code_has_its_own_index() {
        let blocks = parse_blocks("> ```\n> code\n> ```");
        let Block::Quote(children) = &blocks[0].kind else {
            panic!("expected quote");
        };
        let code = children
            .iter()
            .find(|child| matches!(child.kind, Block::Code(..)))
            .expect("code");
        assert!(blocks[0].index < code.index);
    }

    #[test]
    fn images_are_line_blocks_and_inline_alt_text() {
        let blocks = parse_blocks(
            "Screenshots or recordings\n\n![shot](<https://example.com/a b.png> \"title\")\n\n<img src=\"https://example.com/c.png\" alt=\"html\" />",
        );
        assert_eq!(plain(&blocks[0]), "Screenshots or recordings");
        assert!(matches!(&blocks[1].kind, Block::Image { alt, url }
            if alt == "shot" && url == "https://example.com/a b.png"));
        assert!(matches!(&blocks[2].kind, Block::Image { alt, url }
            if alt == "html" && url == "https://example.com/c.png"));

        let inline = parse_blocks("see ![inline](https://example.com/x.png) here");
        assert_eq!(plain(&inline[0]), "see inline here");
        assert!(!matches!(inline[0].kind, Block::Image { .. }));

        let item = parse_blocks("- ![shot](https://example.com/x.png)");
        assert!(matches!(&item[0].kind, Block::Image { alt, .. } if alt == "shot"));

        let empty = parse_blocks("![alt]()");
        assert_eq!(plain(&empty[0]), "alt");
        assert!(!matches!(empty[0].kind, Block::Image { .. }));
    }

    #[test]
    fn github_pasted_and_quoted_images() {
        let tag = "<img alt=\"Image\" width=\"1450\" height=\"932\" src=\"https://private-user-images.githubusercontent.com/1/2-a.png?jwt=eyJ0eXAi.eyJpc3Mi.OiJnaXRodWI\">";
        let blocks = parse_blocks(&format!(
            "### What happened?\n\n{tag}\n\n### Steps\n\n1. Open\n"
        ));
        assert!(matches!(&blocks[1].kind, Block::Image { alt, url }
            if alt == "Image" && url.starts_with("https://private-user-images.githubusercontent.com/1/2-a.png?jwt=")));

        let quoted = parse_blocks(
            "> ### What happened?\n> <img alt=\"Image\" width=\"1450\" height=\"932\" src=\"https://private-user-images.githubusercontent.com/1/2-a.png?jwt=eyJ0eXAi.eyJpc3Mi.OiJnaXRodWI\">\n> \n> ### Steps\n> 1. Open\n",
        );
        let Block::Quote(children) = &quoted[0].kind else {
            panic!("expected quote");
        };
        let image = children
            .iter()
            .find_map(|child| match &child.kind {
                Block::Image { url, .. } => Some(url.as_str()),
                _ => None,
            })
            .expect("quoted image");
        assert_eq!(
            image,
            "https://private-user-images.githubusercontent.com/1/2-a.png?jwt=eyJ0eXAi.eyJpc3Mi.OiJnaXRodWI"
        );
    }

    #[test]
    fn signed_cdn_urls_rewrite_to_the_durable_asset_url() {
        let signed = "https://private-user-images.githubusercontent.com/74659438/656345150-69f721c8-e025-44cd-8242-4e87eef54f7e.png?jwt=eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.expired";
        assert_eq!(
            normalize_image_url(signed),
            "https://github.com/user-attachments/assets/69f721c8-e025-44cd-8242-4e87eef54f7e"
        );
        assert_eq!(
            normalize_image_url(
                "https://github.com/user-attachments/assets/69f721c8-e025-44cd-8242-4e87eef54f7e"
            ),
            "https://github.com/user-attachments/assets/69f721c8-e025-44cd-8242-4e87eef54f7e"
        );
        assert_eq!(
            normalize_image_url("https://user-images.githubusercontent.com/1/2-a.png"),
            "https://user-images.githubusercontent.com/1/2-a.png"
        );
        assert_eq!(
            normalize_image_url(
                "https://private-user-images.githubusercontent.com/1/2-a.png?jwt=x"
            ),
            "https://private-user-images.githubusercontent.com/1/2-a.png?jwt=x"
        );
    }

    #[test]
    fn link_around_an_image_keeps_the_alt() {
        let spans = inline_spans("[![badge](https://example.com/b.png)](https://example.com)");
        assert!(spans.iter().any(
            |span| span.text == "badge" && span.link.as_deref() == Some("https://example.com")
        ));
    }

    #[test]
    fn nested_emphasis_keeps_the_outer_style() {
        let italic = inline_spans("*a _b_ c*");
        let joined: String = italic.iter().map(|span| span.text.as_str()).collect();
        assert_eq!(joined, "a b c");
        assert!(italic.iter().all(|span| span.italic));

        let bold = inline_spans("**a __b__ c**");
        let joined: String = bold.iter().map(|span| span.text.as_str()).collect();
        assert_eq!(joined, "a b c");
        assert!(bold.iter().all(|span| span.bold));
    }

    #[test]
    fn tight_list_keeps_an_image_inside_the_sentence() {
        let inline = parse_blocks("- see ![x](https://e.com/a.png) here");
        assert!(matches!(&inline[0].kind, Block::List(items)
                if items.len() == 1 && items[0].text.plain() == "see x here"));
        assert!(inline
            .iter()
            .all(|block| !matches!(block.kind, Block::Image { .. })));

        let linked = parse_blocks("- [![badge](https://e.com/a.png)](https://e.com)");
        let Block::List(items) = &linked[0].kind else {
            panic!("expected list");
        };
        assert!(items[0]
            .text
            .spans
            .iter()
            .any(|span| span.text == "badge" && span.link.as_deref() == Some("https://e.com")));
    }

    #[test]
    fn ordered_numbers_survive_a_leading_fence_and_a_nested_tail() {
        let blocks = parse_blocks("1. ```\n   code\n   ```\n2. Next");
        let next = blocks.iter().find_map(|block| match &block.kind {
            Block::List(items) => items.iter().find(|item| item.text.plain() == "Next"),
            _ => None,
        });
        assert_eq!(next.map(|item| item.number), Some(2));

        let nested = parse_blocks("- a\n\n  - b\n\n  tail");
        let items = nested.iter().find_map(|block| match &block.kind {
            Block::List(items) => Some(items.as_slice()),
            _ => None,
        });
        let texts: Vec<_> = items
            .expect("list")
            .iter()
            .map(|item| item.text.plain())
            .collect();
        assert_eq!(texts, vec!["a".to_string(), "b".to_string()]);
        assert!(nested
            .iter()
            .any(|block| matches!(&block.kind, Block::Paragraph(text) if text.plain() == "tail")));
    }

    #[test]
    fn quoted_images_on_their_own_lines_stay_images() {
        let blocks = parse_blocks("> ![a](https://e.com/a.png)\n> ![b](https://e.com/b.png)");
        let Block::Quote(children) = &blocks[0].kind else {
            panic!("expected quote");
        };
        let alts: Vec<_> = children
            .iter()
            .filter_map(|child| match &child.kind {
                Block::Image { alt, .. } => Some(alt.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(alts, vec!["a", "b"]);

        let mixed = parse_blocks("> text\n> <img alt=\"pic\" src=\"https://e.com/a.png\">");
        let Block::Quote(children) = &mixed[0].kind else {
            panic!("expected quote");
        };
        assert!(children.iter().any(
            |child| matches!(&child.kind, Block::Paragraph(text) if text.plain().contains("text"))
        ));
        assert!(children
            .iter()
            .any(|child| matches!(&child.kind, Block::Image { alt, .. } if alt == "pic")));
    }

    #[test]
    fn inline_html_image_becomes_alt_text() {
        let blocks = parse_blocks("see <img alt=\"pic\" src=\"https://e.com/a.png\"> now");
        assert_eq!(plain(&blocks[0]), "see pic now");
        assert!(!plain(&blocks[0]).contains("<img"));

        // pulldown may emit one event or two. Either way both alts survive
        // and the raw tags do not.
        let pair = parse_blocks(
            "<img alt=\"a\" src=\"https://e.com/a.png\"><img alt=\"b\" src=\"https://e.com/b.png\">",
        );
        let rendered: String = pair
            .iter()
            .map(|block| match &block.kind {
                Block::Paragraph(text) => text.plain(),
                Block::Image { alt, .. } => alt.clone(),
                _ => String::new(),
            })
            .collect();
        assert!(rendered.contains('a') && rendered.contains('b'));
        assert!(!rendered.contains("<img"));
    }
}
