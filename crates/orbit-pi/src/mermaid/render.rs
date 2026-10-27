//! Painting a laid-out flowchart.
//!
//! Two layers: one canvas draws every group frame, edge and node shape (paths,
//! arrowheads, quads), then absolutely positioned text elements draw the group
//! titles and the node and edge labels on top. Text stays real text — it
//! inherits the window's font smoothing and subpixel positioning, which
//! hand-painted glyphs would not.

use gpui::{
    canvas, div, point, prelude::*, px, AnyElement, BorderStyle, Bounds, Corners, Edges, Hsla,
    IntoElement, PaintQuad, PathBuilder, Pixels, SharedString, Window,
};

use super::flowchart::{
    EdgeHead, EdgeStyle, Flowchart, NodeShape, PlacedCluster, PlacedEdge, PlacedLabel, PlacedNode,
    LABEL_SIZE, LINE_H,
};
use crate::theme::Theme;

/// Node fill and edge colors follow the transcript's code-block surface, so a
/// diagram sits in the same visual family as the fence it replaces.
fn node_fill(theme: Theme) -> Hsla {
    theme.overlay
}

fn node_border(theme: Theme) -> Hsla {
    theme.border_strong
}

fn edge_color(theme: Theme) -> Hsla {
    theme.text_muted
}

pub fn render_diagram(chart: &Flowchart, theme: Theme) -> AnyElement {
    let painted = chart.clone();
    let shapes = canvas(
        |_, _, _| (),
        move |bounds, _, window, _| paint(&painted, bounds, theme, window),
    )
    .absolute()
    .inset_0()
    .size_full();

    div()
        .w(px(chart.width))
        .h(px(chart.height))
        .flex_none()
        .relative()
        .debug_selector(|| "mermaid-diagram".to_string())
        .child(shapes)
        .children(
            chart
                .nodes
                .iter()
                .enumerate()
                .map(|(ix, node)| node_label(node, ix, theme)),
        )
        .children(
            chart
                .clusters
                .iter()
                .enumerate()
                .map(|(ix, cluster)| cluster_title(cluster, ix, theme)),
        )
        .children(chart.edges.iter().enumerate().filter_map(|(ix, edge)| {
            edge.label
                .as_ref()
                .map(|label| edge_label(label, ix, theme))
        }))
        .into_any_element()
}

/// One node's text, centred in its shape's bounding box.
fn node_label(node: &PlacedNode, ix: usize, theme: Theme) -> AnyElement {
    div()
        .id(("mermaid-node", ix))
        .absolute()
        .left(px(node.x))
        .top(px(node.y))
        .w(px(node.w))
        .h(px(node.h))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .overflow_hidden()
        .text_size(px(LABEL_SIZE))
        .line_height(px(LINE_H))
        .text_color(theme.text_primary)
        .children(node.lines.iter().map(|line| {
            div()
                .whitespace_nowrap()
                .child(SharedString::from(line.clone()))
        }))
        .into_any_element()
}

/// A `subgraph` heading, inside the top-left of its frame.
fn cluster_title(cluster: &PlacedCluster, ix: usize, theme: Theme) -> AnyElement {
    div()
        .id(("mermaid-cluster", ix))
        .absolute()
        .left(px(cluster.x + 12.))
        .top(px(cluster.y + 8.))
        .flex()
        .flex_col()
        .items_start()
        .text_size(px(LABEL_SIZE - 1.0))
        .line_height(px(LINE_H - 2.0))
        .text_color(theme.text_muted)
        .children(cluster.title.iter().map(|line| {
            div()
                .whitespace_nowrap()
                .child(SharedString::from(line.clone()))
        }))
        .into_any_element()
}

fn edge_label(label: &PlacedLabel, ix: usize, theme: Theme) -> AnyElement {
    div()
        .id(("mermaid-edge-label", ix))
        .absolute()
        .left(px(label.x))
        .top(px(label.y))
        .w(px(label.w))
        .h(px(label.h))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .rounded(px(4.))
        .bg(theme.code_bg)
        .text_size(px(LABEL_SIZE))
        .line_height(px(LINE_H))
        .text_color(theme.text_secondary)
        .children(label.lines.iter().map(|line| {
            div()
                .whitespace_nowrap()
                .child(SharedString::from(line.clone()))
        }))
        .into_any_element()
}

fn paint(chart: &Flowchart, bounds: Bounds<Pixels>, theme: Theme, window: &mut Window) {
    let origin = bounds.origin;
    for cluster in &chart.clusters {
        paint_cluster(cluster, origin, theme, window);
    }
    for edge in &chart.edges {
        paint_edge(edge, origin, theme, window);
    }
    for node in &chart.nodes {
        paint_node(node, origin, theme, window);
    }
}

/// A `subgraph` frame: a quiet wash behind its members with a hairline border.
/// Mermaid draws these solid; the fill stays translucent so node boxes read as
/// sitting above the group rather than in it.
fn paint_cluster(
    cluster: &PlacedCluster,
    origin: gpui::Point<Pixels>,
    theme: Theme,
    window: &mut Window,
) {
    window.paint_quad(PaintQuad {
        bounds: Bounds::new(
            point(origin.x + px(cluster.x), origin.y + px(cluster.y)),
            gpui::size(px(cluster.w), px(cluster.h)),
        ),
        corner_radii: Corners::all(px(10.)),
        background: theme.overlay.opacity(0.5).into(),
        border_widths: Edges::all(px(1.0)),
        border_color: theme.border,
        border_style: BorderStyle::Solid,
    });
}

fn paint_edge(edge: &PlacedEdge, origin: gpui::Point<Pixels>, theme: Theme, window: &mut Window) {
    let points: Vec<gpui::Point<Pixels>> = edge
        .points
        .iter()
        .map(|(x, y)| point(origin.x + px(*x), origin.y + px(*y)))
        .collect();
    if points.len() < 2 {
        return;
    }
    let width = if edge.style == EdgeStyle::Thick {
        px(2.0)
    } else {
        px(1.25)
    };
    let mut builder = PathBuilder::stroke(width);
    if edge.style == EdgeStyle::Dotted {
        builder = builder.dash_array(&[px(4.), px(4.)]);
    }
    builder.move_to(points[0]);
    for point in &points[1..] {
        builder.line_to(*point);
    }
    if let Ok(path) = builder.build() {
        window.paint_path(path, edge_color(theme));
    }

    let color = edge_color(theme);
    paint_head(
        edge.head,
        points[points.len() - 2],
        points[points.len() - 1],
        color,
        window,
    );
    if edge.both {
        paint_head(edge.head, points[1], points[0], color, window);
    }
}

/// An arrowhead / circle / cross anchored at `tip`, pointing away from
/// `from`.
fn paint_head(
    head: EdgeHead,
    from: gpui::Point<Pixels>,
    tip: gpui::Point<Pixels>,
    color: Hsla,
    window: &mut Window,
) {
    if head == EdgeHead::None {
        return;
    }
    let dx = f32::from(tip.x - from.x);
    let dy = f32::from(tip.y - from.y);
    let length = (dx * dx + dy * dy).sqrt();
    if length < 0.5 {
        return;
    }
    let (ux, uy) = (dx / length, dy / length);
    // Perpendicular, for the two back corners of the triangle.
    let (px_, py_) = (-uy, ux);

    match head {
        EdgeHead::Arrow => {
            let size = 9.0;
            let back = (f32::from(tip.x) - ux * size, f32::from(tip.y) - uy * size);
            let half = size * 0.45;
            let mut builder = PathBuilder::fill();
            builder.move_to(tip);
            builder.line_to(point(px(back.0 + px_ * half), px(back.1 + py_ * half)));
            builder.line_to(point(px(back.0 - px_ * half), px(back.1 - py_ * half)));
            builder.close();
            if let Ok(path) = builder.build() {
                window.paint_path(path, color);
            }
        }
        EdgeHead::Circle => {
            let r = 3.5;
            let center = point(tip.x - px(ux * r), tip.y - px(uy * r));
            window.paint_quad(PaintQuad {
                bounds: Bounds::new(
                    point(center.x - px(r), center.y - px(r)),
                    gpui::size(px(r * 2.0), px(r * 2.0)),
                ),
                corner_radii: Corners::all(px(r)),
                background: color.into(),
                border_widths: Edges::all(px(0.)),
                border_color: color,
                border_style: BorderStyle::Solid,
            });
        }
        EdgeHead::Cross => {
            let size = 5.0;
            let back = (f32::from(tip.x) - ux * size, f32::from(tip.y) - uy * size);
            let mut builder = PathBuilder::stroke(px(1.5));
            builder.move_to(point(px(back.0 + px_ * size), px(back.1 + py_ * size)));
            builder.line_to(point(px(back.0 - px_ * size), px(back.1 - py_ * size)));
            if let Ok(path) = builder.build() {
                window.paint_path(path, color);
            }
        }
        EdgeHead::None => {}
    }
}

fn paint_node(node: &PlacedNode, origin: gpui::Point<Pixels>, theme: Theme, window: &mut Window) {
    let bounds = Bounds::new(
        point(origin.x + px(node.x), origin.y + px(node.y)),
        gpui::size(px(node.w), px(node.h)),
    );
    let fill = node_fill(theme);
    let border = node_border(theme);
    let stroke = px(1.0);

    match node.shape {
        NodeShape::Diamond => {
            let mut builder = PathBuilder::fill();
            builder.add_polygon(&diamond(bounds), true);
            if let Ok(path) = builder.build() {
                window.paint_path(path, fill);
            }
            let mut outline = PathBuilder::stroke(stroke);
            outline.add_polygon(&diamond(bounds), true);
            if let Ok(path) = outline.build() {
                window.paint_path(path, border);
            }
        }
        NodeShape::Hexagon => {
            let mut builder = PathBuilder::fill();
            builder.add_polygon(&hexagon(bounds), true);
            if let Ok(path) = builder.build() {
                window.paint_path(path, fill);
            }
            let mut outline = PathBuilder::stroke(stroke);
            outline.add_polygon(&hexagon(bounds), true);
            if let Ok(path) = outline.build() {
                window.paint_path(path, border);
            }
        }
        shape => {
            let radius = match shape {
                NodeShape::Circle => f32::from(bounds.size.width.min(bounds.size.height)) / 2.0,
                NodeShape::Stadium => f32::from(bounds.size.height) / 2.0,
                NodeShape::Rounded | NodeShape::Cylinder => 10.0,
                _ => 6.0,
            };
            window.paint_quad(PaintQuad {
                bounds,
                corner_radii: Corners::all(px(radius)),
                background: fill.into(),
                border_widths: Edges::all(stroke),
                border_color: border,
                border_style: BorderStyle::Solid,
            });
            if matches!(shape, NodeShape::Subroutine) {
                // Double side bars: the mermaid subroutine mark, without a
                // separate path for the box itself.
                for inset in [5.0f32, 9.0] {
                    let mut builder = PathBuilder::stroke(px(1.0));
                    builder.move_to(point(bounds.origin.x + px(inset), bounds.origin.y + px(3.)));
                    builder.line_to(point(
                        bounds.origin.x + px(inset),
                        bounds.origin.y + bounds.size.height - px(3.),
                    ));
                    builder.move_to(point(
                        bounds.origin.x + bounds.size.width - px(inset),
                        bounds.origin.y + px(3.),
                    ));
                    builder.line_to(point(
                        bounds.origin.x + bounds.size.width - px(inset),
                        bounds.origin.y + bounds.size.height - px(3.),
                    ));
                    if let Ok(path) = builder.build() {
                        window.paint_path(path, border);
                    }
                }
            }
        }
    }
}

fn diamond(bounds: Bounds<Pixels>) -> Vec<gpui::Point<Pixels>> {
    let cx = bounds.origin.x + bounds.size.width / 2.0;
    let cy = bounds.origin.y + bounds.size.height / 2.0;
    vec![
        point(cx, bounds.origin.y),
        point(bounds.origin.x + bounds.size.width, cy),
        point(cx, bounds.origin.y + bounds.size.height),
        point(bounds.origin.x, cy),
    ]
}

fn hexagon(bounds: Bounds<Pixels>) -> Vec<gpui::Point<Pixels>> {
    let inset = (bounds.size.width * 0.12).min(px(18.));
    let cy = bounds.origin.y + bounds.size.height / 2.0;
    vec![
        point(bounds.origin.x + inset, bounds.origin.y),
        point(bounds.origin.x + bounds.size.width - inset, bounds.origin.y),
        point(bounds.origin.x + bounds.size.width, cy),
        point(
            bounds.origin.x + bounds.size.width - inset,
            bounds.origin.y + bounds.size.height,
        ),
        point(
            bounds.origin.x + inset,
            bounds.origin.y + bounds.size.height,
        ),
        point(bounds.origin.x, cy),
    ]
}
