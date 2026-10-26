//! Composer state that belongs to a *target*, not to the app: the unsent
//! draft a session carries while the user is elsewhere, and the long-paste
//! pills that hold text compactly until it is sent.

use super::*;

/// Long pastes collapse into a pill; anything a person would type inline
/// stays inline. The gate is the whole feature — a false positive swallows
/// text the user was still arranging, a false negative wrecks the box.
#[gpui::test]
fn long_pastes_pill_and_short_ones_stay_inline(_cx: &mut gpui::TestAppContext) {
    // Short, ordinary pastes ride the caret.
    assert!(!crate::composer::should_pill_paste("hello"));
    assert!(!crate::composer::should_pill_paste("a line\nand another"));
    assert!(!crate::composer::should_pill_paste(
        &"x".repeat(crate::composer::PASTE_PILL_MIN_CHARS - 1)
    ));

    // Many lines, or one very long line, becomes a pill.
    let many = (0..crate::composer::PASTE_PILL_MIN_LINES)
        .map(|ix| format!("line {ix}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(crate::composer::should_pill_paste(&many));
    assert!(crate::composer::should_pill_paste(
        &"x".repeat(crate::composer::PASTE_PILL_MIN_CHARS)
    ));
}

/// A draft is keyed to its session file, so switching away and back restores
/// exactly what was being written — and switching to a *different* session
/// never shows the first one's text.
#[gpui::test]
fn drafts_follow_their_session(cx: &mut gpui::TestAppContext) {
    use crate::theme::{Theme, ThemeId};

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));

    let first = PathBuf::from("/tmp/orbit-draft-a.jsonl");
    let second = PathBuf::from("/tmp/orbit-draft-b.jsonl");

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            // Writing in the first session.
            app.current_session_path = Some(first.clone());
            app.input
                .update(cx, |input, cx| input.set_text("half a thought", cx));
            app.pasted_texts.push(PastedText::new("pasted".into()));
            app.files.push(AttachedFile {
                path: PathBuf::from("/tmp/notes.md"),
                name: "notes.md".into(),
                size: 12,
            });
            app.save_composer_draft(cx);

            // Leaving parks it and leaves nothing behind.
            app.restore_composer_draft(Some(&second), cx);
            assert_eq!(app.input.read(cx).text(), "");
            assert!(app.pasted_texts.is_empty());
            assert!(app.files.is_empty());

            // Coming back brings it, attachments included.
            app.restore_composer_draft(Some(&first), cx);
            assert_eq!(app.input.read(cx).text(), "half a thought");
            assert_eq!(app.pasted_texts.len(), 1);
            assert_eq!(app.pasted_texts[0].content, "pasted");
            assert_eq!(app.files.len(), 1);
            assert_eq!(app.files[0].name, "notes.md");
        });
    });
}

/// An empty composer parks no draft, so an unedited session does not leave a
/// blank entry behind (and a stale draft can never resurrect old text).
#[gpui::test]
fn empty_composers_park_no_draft(cx: &mut gpui::TestAppContext) {
    use crate::theme::{Theme, ThemeId};

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    let path = PathBuf::from("/tmp/orbit-draft-empty.jsonl");

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.current_session_path = Some(path.clone());
            app.input
                .update(cx, |input, cx| input.set_text("   \n  ", cx));
            app.save_composer_draft(cx);
            assert!(app.drafts.is_empty(), "whitespace is not a draft");
        });
    });
}

/// The New Task page has no session file yet, so its draft lives in its own
/// slot — typing there then opening a session must not leak it.
#[gpui::test]
fn the_new_task_draft_has_its_own_slot(cx: &mut gpui::TestAppContext) {
    use crate::theme::{Theme, ThemeId};

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    let path = PathBuf::from("/tmp/orbit-draft-task.jsonl");

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.current_session_path = None;
            app.input
                .update(cx, |input, cx| input.set_text("brand new task", cx));
            app.save_composer_draft(cx);
            assert!(app.new_task_draft.is_some());

            app.restore_composer_draft(Some(&path), cx);
            assert_eq!(app.input.read(cx).text(), "");

            app.restore_composer_draft(None, cx);
            assert_eq!(app.input.read(cx).text(), "brand new task");
        });
    });
}

/// Pills are only a compact way to *hold* text: on submit the paste rejoins
/// the prompt as its own block, so the agent never reads it as a continuation
/// of the sentence above.
#[gpui::test]
fn pills_fold_back_into_the_prompt(_cx: &mut gpui::TestAppContext) {
    let paste = PastedText::new("fn main() {\n    todo!()\n}".into());
    assert_eq!(
        OrbitApp::fold_pasted_texts("explain this", std::slice::from_ref(&paste)),
        "explain this\n\nfn main() {\n    todo!()\n}"
    );
    // A pill on its own is the whole prompt — no leading blank block.
    assert_eq!(
        OrbitApp::fold_pasted_texts("", std::slice::from_ref(&paste)),
        "fn main() {\n    todo!()\n}"
    );
    assert_eq!(OrbitApp::fold_pasted_texts("just text", &[]), "just text");
}

/// The block the composer folds into a prompt must read back out of it, or a
/// sent message keeps showing a raw `@/path` line where the composer showed a
/// chip. Prose that merely mentions a file is left alone.
#[gpui::test]
fn the_folded_file_block_reads_back_out_of_the_sent_message(_cx: &mut gpui::TestAppContext) {
    let attached = |path: &str| AttachedFile {
        path: PathBuf::from(path),
        name: Path::new(path)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default(),
        size: 0,
    };
    let cwd = PathBuf::from("/work/repo");
    let files = [
        attached("/work/repo/src/main.rs"),
        attached("/Users/someone/Downloads/APP-Install.md"),
    ];
    let text =
        OrbitApp::fold_attached_files("can you check the app install process", &files, Some(&cwd));
    assert_eq!(
        OrbitApp::split_attached_files(&text),
        Some((
            "can you check the app install process",
            vec!["src/main.rs", "/Users/someone/Downloads/APP-Install.md"],
        ))
    );

    // A file-only prompt is the whole block: no empty prose bubble above it.
    let only = OrbitApp::fold_attached_files("", &files[..1], Some(&cwd));
    assert_eq!(
        OrbitApp::split_attached_files(&only),
        Some(("", vec!["src/main.rs"]))
    );

    // Whitespace paths round-trip through the backticked form.
    let spaced = [attached("/work/repo/my notes.md")];
    let text = OrbitApp::fold_attached_files("read this", &spaced, Some(&cwd));
    assert_eq!(
        OrbitApp::split_attached_files(&text),
        Some(("read this", vec!["my notes.md"]))
    );

    // Ordinary prose is never mistaken for a file block.
    for prose in [
        "look at src/main.rs",
        "run `npm test`",
        "`src/main.rs` is where it lives",
        "two\n\nparagraphs",
        "```\n@not-a-file\n```",
    ] {
        assert_eq!(
            OrbitApp::split_attached_files(prose),
            None,
            "{prose:?} is prose, not an attachment block"
        );
    }
}

/// Files ride the prompt as path mentions, one per line, workspace-relative
/// when they live inside it. A file-only message is still a sendable prompt.
#[gpui::test]
fn file_attachments_fold_into_the_prompt_as_path_mentions(_cx: &mut gpui::TestAppContext) {
    let attached = |path: &str| AttachedFile {
        path: PathBuf::from(path),
        name: Path::new(path)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default(),
        size: 0,
    };
    let cwd = PathBuf::from("/work/repo");
    let inside = attached("/work/repo/src/main.rs");
    let outside = attached("/tmp/notes.txt");

    assert_eq!(
        OrbitApp::fold_attached_files("explain this", std::slice::from_ref(&inside), Some(&cwd)),
        "explain this\n\n@src/main.rs"
    );
    // A file on its own is the whole prompt.
    assert_eq!(
        OrbitApp::fold_attached_files("", std::slice::from_ref(&outside), Some(&cwd)),
        "@/tmp/notes.txt"
    );
    // Each file gets its own line.
    assert_eq!(
        OrbitApp::fold_attached_files("", &[inside, outside], Some(&cwd)),
        "@src/main.rs\n@/tmp/notes.txt"
    );
    // Whitespace would split an `@` token, so the path becomes a code span.
    let spaced = attached("/work/repo/my notes.md");
    assert_eq!(
        OrbitApp::fold_attached_files("", std::slice::from_ref(&spaced), Some(&cwd)),
        "`my notes.md`"
    );
    // No files leaves the text exactly as typed.
    assert_eq!(
        OrbitApp::fold_attached_files("just text", &[], Some(&cwd)),
        "just text"
    );
}
