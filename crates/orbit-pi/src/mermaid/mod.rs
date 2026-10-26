//! Native Mermaid rendering for the transcript (INTENT.md D2).
//!
//! Two halves: [`flowchart`] parses the fence and lays the graph out in
//! logical pixels, [`render`] paints that layout with gpui. Nothing here runs
//! JavaScript, spawns a webview, or shells out — the drawing is GPUI paths and
//! text, so a diagram is the app's own render and costs one cached layout.
//!
//! Only a subset of Mermaid is covered on purpose. [`diagram`] returns `None`
//! for anything outside it, and the transcript then keeps the copyable source
//! block: a diagram that silently dropped half the graph would be worse than
//! the source, so the fallback is always the honest option.

mod flowchart;
mod measure;
mod render;

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;

use crate::theme;

pub use flowchart::Flowchart;
pub use measure::install;
pub use render::render_diagram;

/// Diagrams cached before the map is cleared. A transcript re-renders its
/// visible rows on every heartbeat, and layout is pure over the source plus
/// the label face, so each distinct fence lays out once.
const CACHE_LIMIT: usize = 48;

thread_local! {
    static CACHE: RefCell<HashMap<u64, Option<Rc<Flowchart>>>> = RefCell::new(HashMap::new());
}

/// Lay out a `mermaid` fence, or `None` when it is not a diagram this build
/// can draw.
pub fn diagram(source: &str) -> Option<Rc<Flowchart>> {
    let key = {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        source.hash(&mut hasher);
        // Measured widths change with the interface face, so the face is part
        // of the identity — otherwise a Settings font change would keep the
        // old geometry until the process restarted.
        theme::ui_font_family().hash(&mut hasher);
        std::hash::Hasher::finish(&hasher)
    };
    if let Some(hit) = CACHE.with(|cache| cache.borrow().get(&key).cloned()) {
        return hit;
    }
    let laid_out = flowchart::parse(source).map(|parsed| {
        Rc::new(flowchart::layout(&parsed, &|line: &str, size: f32| {
            measure::line_width(line, size)
        }))
    });
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(key, laid_out.clone());
    });
    laid_out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caches_by_source() {
        let source = "graph TD\n A[one] --> B[two]";
        let first = diagram(source).expect("lays out");
        let second = diagram(source).expect("lays out");
        assert!(Rc::ptr_eq(&first, &second));
    }

    #[test]
    fn unknown_diagram_types_have_no_layout() {
        assert!(diagram("pie title Pets\n \"Dogs\" : 3").is_none());
    }
}
