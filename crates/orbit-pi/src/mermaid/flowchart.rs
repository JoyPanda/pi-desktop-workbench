//! Mermaid flowcharts: parse the subset Orbit renders, then lay it out in
//! logical pixels.
//!
//! Nothing here touches gpui — layout is plain arithmetic over font metrics
//! handed in by the caller, so every rule below is unit-testable without a
//! window. Painting lives in [`super::render`].
//!
//! Supported today: `graph`/`flowchart` with TD/TB, BT, LR and RL directions;
//! the common node shapes; chained edges with `-->`, `---`, `-.->`, `==>` and
//! their labelled forms; edge labels; and `subgraph … end` clustering, drawn as
//! a titled frame around its members. Anything outside that — class/style
//! directives, sequence/gantt/pie diagrams, a statement the tokenizer cannot
//! represent — makes [`parse`] return `None` so the transcript keeps showing
//! the source rather than a diagram that silently dropped part of the graph.

use std::collections::HashMap;

/// Label text size and line height, in logical pixels. The renderer draws
/// node and edge labels at exactly these metrics.
pub const LABEL_SIZE: f32 = 13.0;
pub const LINE_H: f32 = 18.0;
const PAD_X: f32 = 14.0;
const PAD_Y: f32 = 9.0;
const MIN_W: f32 = 56.0;
const MIN_H: f32 = 36.0;
/// Space between ranks (the direction the graph flows in).
const RANK_GAP: f32 = 52.0;
/// Space between siblings within one rank.
const NODE_GAP: f32 = 26.0;
/// How far outside the widest box a skip-rank edge's channel runs.
const CHANNEL_GAP: f32 = 16.0;
/// Breathing room around the whole drawing.
const MARGIN: f32 = 10.0;
const EDGE_LABEL_LINE_H: f32 = 16.0;
/// A `subgraph` frame: inset around its members, a title strip above them, and
/// extra space between two neighbouring clusters so their frames never touch.
const CLUSTER_PAD_X: f32 = 12.0;
const CLUSTER_PAD_Y: f32 = 12.0;
const CLUSTER_HEAD: f32 = 32.0;
const CLUSTER_GAP: f32 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    TopDown,
    BottomUp,
    LeftRight,
    RightLeft,
}

impl Direction {
    /// True when ranks stack along y (TD/BT), false when they run along x.
    fn vertical(self) -> bool {
        matches!(self, Self::TopDown | Self::BottomUp)
    }

    /// True when the drawn flow runs opposite to rank order.
    fn mirrored(self) -> bool {
        matches!(self, Self::BottomUp | Self::RightLeft)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeShape {
    Rect,
    Rounded,
    Stadium,
    Subroutine,
    Cylinder,
    Circle,
    Diamond,
    Hexagon,
    Asymmetric,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeStyle {
    Solid,
    Dotted,
    Thick,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeHead {
    None,
    Arrow,
    Circle,
    Cross,
}

/// A node with its final geometry. `x`/`y` are the top-left corner of the
/// shape's bounding box.
#[derive(Clone, Debug)]
pub struct PlacedNode {
    pub lines: Vec<String>,
    pub shape: NodeShape,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// An edge's polyline plus its optional label box.
#[derive(Clone, Debug)]
pub struct PlacedEdge {
    pub style: EdgeStyle,
    pub head: EdgeHead,
    /// The source carries a head too (`<-->`, `<-.->`).
    pub both: bool,
    pub points: Vec<(f32, f32)>,
    pub label: Option<PlacedLabel>,
}

#[derive(Clone, Debug)]
pub struct PlacedLabel {
    pub lines: Vec<String>,
    /// Centre of the label box; the renderer centres the text on it and fills
    /// the box with the card background so the edge line reads as passing
    /// behind the text.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// A `subgraph` frame: the box around its members and its title.
#[derive(Clone, Debug)]
pub struct PlacedCluster {
    pub title: Vec<String>,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// A laid-out flowchart, ready to paint: everything is absolute, so the
/// renderer only draws. Clusters come largest-first, so a nested frame paints
/// over the one that contains it.
#[derive(Clone, Debug)]
pub struct Flowchart {
    pub width: f32,
    pub height: f32,
    pub nodes: Vec<PlacedNode>,
    pub edges: Vec<PlacedEdge>,
    pub clusters: Vec<PlacedCluster>,
}

#[derive(Clone, Debug)]
struct NodeDef {
    label: String,
    shape: NodeShape,
    /// Every `subgraph` open where the node was first mentioned, outermost
    /// first: a nested group's frame must be enclosed by its parent's.
    clusters: Vec<usize>,
}

/// A `subgraph` declaration. Membership lives on the nodes, so a group with no
/// members simply draws nothing.
#[derive(Clone, Debug)]
struct ClusterDef {
    title: String,
}

#[derive(Clone, Debug)]
struct EdgeDef {
    from: usize,
    to: usize,
    style: EdgeStyle,
    head: EdgeHead,
    both: bool,
    label: Option<String>,
}

/// The parse result before layout.
#[derive(Clone, Debug)]
pub struct Parsed {
    direction: Direction,
    nodes: Vec<NodeDef>,
    edges: Vec<EdgeDef>,
    clusters: Vec<ClusterDef>,
}

/// Statements Orbit ignores outright: they carry no geometry, so dropping
/// them cannot make the drawing lie about the graph.
const IGNORED: [&str; 8] = [
    "direction",
    "classDef",
    "class",
    "style",
    "linkStyle",
    "click",
    "accTitle",
    "accDescr",
];

/// Parse a `flowchart`/`graph` fence. `None` means "not a diagram Orbit can
/// draw": the caller falls back to the source block.
pub fn parse(source: &str) -> Option<Parsed> {
    let mut statements = statements(source);
    let header = statements.first()?.clone();
    let (keyword, rest) = split_word(&header);
    if !keyword.eq_ignore_ascii_case("flowchart") && !keyword.eq_ignore_ascii_case("graph") {
        return None;
    }
    let direction = match rest.trim().to_ascii_uppercase().as_str() {
        "" | "TD" | "TB" => Direction::TopDown,
        "BT" => Direction::BottomUp,
        "LR" => Direction::LeftRight,
        "RL" => Direction::RightLeft,
        _ => return None,
    };
    statements.remove(0);

    let mut nodes: Vec<NodeDef> = Vec::new();
    let mut ids: HashMap<String, usize> = HashMap::new();
    let mut edges: Vec<EdgeDef> = Vec::new();
    let mut clusters: Vec<ClusterDef> = Vec::new();
    // Open subgraphs, innermost last. A node belongs to every group open where
    // it is first mentioned; `end` closes the innermost.
    let mut open: Vec<usize> = Vec::new();
    for statement in &statements {
        let (word, rest) = split_word(statement);
        if word.eq_ignore_ascii_case("subgraph") {
            // `subgraph` alone is a valid (untitled) group in mermaid, so an
            // empty rest is a heading-less cluster rather than a failure.
            let title = parse_node(rest, 0)
                .map(|(_, label, _, _)| label)
                .unwrap_or_default();
            clusters.push(ClusterDef { title });
            open.push(clusters.len() - 1);
            continue;
        }
        if word.eq_ignore_ascii_case("end") {
            open.pop();
            continue;
        }
        if IGNORED.iter().any(|k| k.eq_ignore_ascii_case(word)) {
            continue;
        }
        parse_statement(statement, &mut nodes, &mut ids, &mut edges, &open)?;
    }
    if nodes.is_empty() {
        return None;
    }
    Some(Parsed {
        direction,
        nodes,
        edges,
        clusters,
    })
}

/// Node boxes, rank assignment and edge routing for a parsed graph.
pub fn layout(parsed: &Parsed, measure: &dyn Fn(&str, f32) -> f32) -> Flowchart {
    let n = parsed.nodes.len();
    if n == 0 {
        return Flowchart {
            width: 0.0,
            height: 0.0,
            nodes: Vec::new(),
            edges: Vec::new(),
            clusters: Vec::new(),
        };
    }
    let vertical = parsed.direction.vertical();

    let mut nodes: Vec<PlacedNode> = parsed
        .nodes
        .iter()
        .map(|node| {
            let lines: Vec<String> = if node.label.is_empty() {
                vec![String::new()]
            } else {
                node.label.split('\n').map(str::to_string).collect()
            };
            let text_w = lines
                .iter()
                .map(|line| measure(line, LABEL_SIZE))
                .fold(0.0f32, f32::max);
            let mut w = (text_w + 2.0 * PAD_X).max(MIN_W).ceil();
            let mut h = (lines.len() as f32 * LINE_H + 2.0 * PAD_Y)
                .max(MIN_H)
                .ceil();
            // Shapes that need more room than their text to stay legible.
            match node.shape {
                NodeShape::Circle => {
                    let d = w.max(h).max(48.0);
                    w = d;
                    h = d;
                }
                NodeShape::Diamond => {
                    w = (w * 1.35 + 20.0).ceil();
                    h = (h * 1.55 + 12.0).ceil();
                }
                NodeShape::Hexagon => w += 28.0,
                NodeShape::Stadium => w += 12.0,
                _ => {}
            }
            PlacedNode {
                lines,
                shape: node.shape,
                x: 0.0,
                y: 0.0,
                w,
                h,
            }
        })
        .collect();

    let cluster_of: Vec<Option<usize>> = parsed
        .nodes
        .iter()
        .map(|node| node.clusters.last().copied())
        .collect();
    let rank = ranks(n, &parsed.edges);
    let order = order_within_ranks(n, &rank, &parsed.edges, &cluster_of);
    let max_rank = rank.iter().copied().max().unwrap_or(0);

    // Rank extent along the flow axis, and total sibling span across it.
    // Neighbours from different groups keep [`CLUSTER_GAP`] between them, so
    // the frames drawn around them never touch.
    let mut extent = vec![0.0f32; max_rank + 1];
    let mut span = vec![0.0f32; max_rank + 1];
    for (i, node) in nodes.iter().enumerate() {
        let (main, _) = axis_sizes(node, vertical);
        extent[rank[i]] = extent[rank[i]].max(main);
    }
    for (r, siblings) in order.iter().enumerate() {
        let mut total = 0.0f32;
        for (ix, &node) in siblings.iter().enumerate() {
            if ix > 0 {
                total += NODE_GAP;
                if cluster_of[siblings[ix - 1]] != cluster_of[node] {
                    total += CLUSTER_GAP;
                }
            }
            let (_, cross) = axis_sizes(&nodes[node], vertical);
            total += cross;
        }
        span[r] = total;
    }
    let widest = span.iter().copied().fold(0.0f32, f32::max);

    let mut main = 0.0f32;
    // Where each rank sits along the flow axis, before mirroring: a skip-rank
    // edge runs its channel inside these gaps.
    let mut rank_main: Vec<(f32, f32)> = Vec::with_capacity(order.len());
    for (r, siblings) in order.iter().enumerate() {
        let mut cross = (widest - span[r]) / 2.0;
        for (ix, &i) in siblings.iter().enumerate() {
            let (node_main, node_cross) = axis_sizes(&nodes[i], vertical);
            if ix > 0 && cluster_of[siblings[ix - 1]] != cluster_of[i] {
                cross += CLUSTER_GAP;
            }
            let main_at = main + (extent[r] - node_main) / 2.0;
            if vertical {
                nodes[i].x = cross;
                nodes[i].y = main_at;
            } else {
                nodes[i].y = cross;
                nodes[i].x = main_at;
            }
            cross += node_cross + NODE_GAP;
        }
        rank_main.push((main, main + extent[r]));
        main += extent[r] + RANK_GAP;
    }
    let flow_total = (main - RANK_GAP).max(0.0);

    // BT/RL draw the same ranks in the opposite order; mirroring here keeps
    // the edge anchors below purely geometric.
    if parsed.direction.mirrored() {
        for node in &mut nodes {
            if vertical {
                node.y = flow_total - node.y - node.h;
            } else {
                node.x = flow_total - node.x - node.w;
            }
        }
        for range in &mut rank_main {
            *range = (flow_total - range.1, flow_total - range.0);
        }
    }

    // Frames are built inside-out: a group's box covers its own nodes plus the
    // boxes of the groups nested inside it, so an outer frame encloses the
    // inner one instead of coinciding with it.
    let cluster_count = parsed.clusters.len();
    let mut depth = vec![0usize; cluster_count];
    let mut parent: Vec<Option<usize>> = vec![None; cluster_count];
    for node in &parsed.nodes {
        for (level, &open) in node.clusters.iter().enumerate() {
            depth[open] = depth[open].max(level);
            if level > 0 {
                parent[open].get_or_insert(node.clusters[level - 1]);
            }
        }
    }
    let mut frames: Vec<Option<PlacedCluster>> = vec![None; cluster_count];
    let deepest = depth.iter().copied().max().unwrap_or(0);
    for level in (0..=deepest).rev() {
        for ix in 0..cluster_count {
            if depth[ix] != level {
                continue;
            }
            let mut extents: Vec<(f32, f32, f32, f32)> = nodes
                .iter()
                .enumerate()
                .filter(|(node_ix, _)| parsed.nodes[*node_ix].clusters.contains(&ix))
                .map(|(_, node)| (node.x, node.y, node.x + node.w, node.y + node.h))
                .collect();
            for (child, frame) in frames.iter().enumerate() {
                if parent[child] == Some(ix) {
                    if let Some(frame) = frame {
                        extents.push((frame.x, frame.y, frame.x + frame.w, frame.y + frame.h));
                    }
                }
            }
            let Some(first) = extents.first() else {
                continue;
            };
            let (mut left, mut top, mut right, mut bottom) = *first;
            for (x0, y0, x1, y1) in &extents[1..] {
                left = left.min(*x0);
                top = top.min(*y0);
                right = right.max(*x1);
                bottom = bottom.max(*y1);
            }
            let x = left - CLUSTER_PAD_X;
            let y = top - CLUSTER_HEAD;
            frames[ix] = Some(PlacedCluster {
                title: parsed.clusters[ix]
                    .title
                    .split('\n')
                    .filter(|line| !line.is_empty())
                    .map(str::to_string)
                    .collect(),
                x,
                y,
                w: right + CLUSTER_PAD_X - x,
                h: bottom + CLUSTER_PAD_Y - y,
            });
        }
    }
    let mut clusters: Vec<PlacedCluster> = frames.into_iter().flatten().collect();
    // Largest first: a nested frame is drawn after the one containing it.
    clusters.sort_by(|a, b| (b.w * b.h).total_cmp(&(a.w * a.h)));

    // The channel a skip-rank edge routes through: just outside the widest node
    // box on the cross axis, and kept inside a group's frame when one encloses
    // it, so a long edge neither crosses a box nor leaves its group.
    let widest_node = nodes
        .iter()
        .map(|node| {
            if vertical {
                node.x + node.w
            } else {
                node.y + node.h
            }
        })
        .fold(0.0f32, f32::max);
    let widest_frame = clusters
        .iter()
        .map(|frame| {
            if vertical {
                frame.x + frame.w
            } else {
                frame.y + frame.h
            }
        })
        .fold(None::<f32>, |widest, edge| {
            Some(widest.map_or(edge, |widest| widest.max(edge)))
        });
    let channel = match widest_frame {
        Some(frame) if frame - CLUSTER_PAD_X / 2.0 > widest_node + 6.0 => {
            (widest_node + CHANNEL_GAP).min(frame - CLUSTER_PAD_X / 2.0)
        }
        _ => widest_node + CHANNEL_GAP,
    };

    let route_ctx = RouteCtx {
        vertical,
        rank,
        rank_main,
        channel,
    };

    let mut edges: Vec<PlacedEdge> = parsed
        .edges
        .iter()
        .map(|edge| {
            let points = route(edge.from, edge.to, &nodes, &route_ctx);
            let label = edge
                .label
                .as_ref()
                .map(|text| place_label(text, &points, measure));
            PlacedEdge {
                style: edge.style,
                head: edge.head,
                both: edge.both,
                points,
                label,
            }
        })
        .collect();

    // Trim to content: edge elbows and labels can reach outside the node
    // boxes, so bounds come from everything drawn.
    let mut min = (f32::MAX, f32::MAX);
    let mut max = (f32::MIN, f32::MIN);
    let mut include = |x: f32, y: f32| {
        min = (min.0.min(x), min.1.min(y));
        max = (max.0.max(x), max.1.max(y));
    };
    for node in &nodes {
        include(node.x, node.y);
        include(node.x + node.w, node.y + node.h);
    }
    for cluster in &clusters {
        include(cluster.x, cluster.y);
        include(cluster.x + cluster.w, cluster.y + cluster.h);
    }
    for edge in &edges {
        for &(x, y) in &edge.points {
            include(x, y);
        }
        if let Some(label) = &edge.label {
            include(label.x, label.y);
            include(label.x + label.w, label.y + label.h);
        }
    }
    let dx = MARGIN - min.0;
    let dy = MARGIN - min.1;
    for node in &mut nodes {
        node.x += dx;
        node.y += dy;
    }
    for cluster in &mut clusters {
        cluster.x += dx;
        cluster.y += dy;
    }
    for edge in &mut edges {
        for point in &mut edge.points {
            point.0 += dx;
            point.1 += dy;
        }
        if let Some(label) = &mut edge.label {
            label.x += dx;
            label.y += dy;
        }
    }

    Flowchart {
        width: max.0 + dx + MARGIN,
        height: max.1 + dy + MARGIN,
        nodes,
        edges,
        clusters,
    }
}

fn axis_sizes(node: &PlacedNode, vertical: bool) -> (f32, f32) {
    if vertical {
        (node.h, node.w)
    } else {
        (node.w, node.h)
    }
}

/// Layered ranks: longest path from a source, with back edges (cycles) left
/// out so a loop in the graph cannot hang the layout.
fn ranks(n: usize, edges: &[EdgeDef]) -> Vec<usize> {
    let mut succ: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
    for (ix, edge) in edges.iter().enumerate() {
        succ[edge.from].push((ix, edge.to));
    }

    // Iterative DFS: a graph the agent wrote is arbitrary, and recursion here
    // would be the one place a huge fence could overflow the stack.
    let mut state = vec![0u8; n]; // 0 unvisited, 1 on stack, 2 done
    let mut back = vec![false; edges.len()];
    let mut post: Vec<usize> = Vec::with_capacity(n);
    for start in 0..n {
        if state[start] != 0 {
            continue;
        }
        state[start] = 1;
        let mut stack: Vec<(usize, usize)> = vec![(start, 0)];
        while !stack.is_empty() {
            let top = stack.len() - 1;
            let (node, next) = stack[top];
            if next < succ[node].len() {
                stack[top].1 += 1;
                let (edge_ix, to) = succ[node][next];
                match state[to] {
                    0 => {
                        state[to] = 1;
                        stack.push((to, 0));
                    }
                    1 => back[edge_ix] = true,
                    _ => {}
                }
            } else {
                state[node] = 2;
                post.push(node);
                stack.pop();
            }
        }
    }

    let mut rank = vec![0usize; n];
    for &node in post.iter().rev() {
        for &(edge_ix, to) in &succ[node] {
            if back[edge_ix] {
                continue;
            }
            rank[to] = rank[to].max(rank[node] + 1);
        }
    }
    rank
}

/// Crossing reduction: four barycenter sweeps, alternating direction. Keeps
/// the first-seen order as the tie-breaker so a diagram without crossings
/// never gets shuffled.
fn order_within_ranks(
    n: usize,
    rank: &[usize],
    edges: &[EdgeDef],
    cluster_of: &[Option<usize>],
) -> Vec<Vec<usize>> {
    let max_rank = rank.iter().copied().max().unwrap_or(0);
    let mut order: Vec<Vec<usize>> = vec![Vec::new(); max_rank + 1];
    for node in 0..n {
        order[rank[node]].push(node);
    }
    let mut pos = vec![0usize; n];
    for siblings in &order {
        for (ix, &node) in siblings.iter().enumerate() {
            pos[node] = ix;
        }
    }

    let mut preds: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut succs: Vec<Vec<usize>> = vec![Vec::new(); n];
    for edge in edges {
        if edge.from != edge.to {
            preds[edge.to].push(edge.from);
            succs[edge.from].push(edge.to);
        }
    }

    for _ in 0..4 {
        barycenter_sweep(&mut order, &mut pos, &preds, 1..=max_rank);
        barycenter_sweep(&mut order, &mut pos, &succs, (0..max_rank).rev());
    }
    // A group's frame only encloses its own nodes when its members are
    // contiguous within a rank; the sort is stable, so crossings still decide
    // the order inside each group.
    for siblings in &mut order {
        siblings.sort_by_key(|&node| cluster_of[node].unwrap_or(usize::MAX));
    }
    order
}

fn barycenter_sweep(
    order: &mut [Vec<usize>],
    pos: &mut [usize],
    neighbors: &[Vec<usize>],
    ranks: impl Iterator<Item = usize>,
) {
    for r in ranks {
        let mut keyed: Vec<(f32, usize, usize)> = order[r]
            .iter()
            .enumerate()
            .map(|(ix, &node)| {
                let linked = &neighbors[node];
                let key = if linked.is_empty() {
                    ix as f32
                } else {
                    linked.iter().map(|&peer| pos[peer] as f32).sum::<f32>() / linked.len() as f32
                };
                (key, ix, node)
            })
            .collect();
        keyed.sort_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.1.cmp(&b.1))
        });
        order[r] = keyed.into_iter().map(|(_, _, node)| node).collect();
        for (ix, &node) in order[r].iter().enumerate() {
            pos[node] = ix;
        }
    }
}

/// What routing needs beyond the two node boxes.
struct RouteCtx {
    vertical: bool,
    /// Rank of each node, aligned with the layout's node vector.
    rank: Vec<usize>,
    /// Each rank's extent along the flow axis, after mirroring.
    rank_main: Vec<(f32, f32)>,
    /// Cross-axis position outside every box and frame.
    channel: f32,
}

impl RouteCtx {
    /// True when the edge jumps over a rank, so a mid-gap dogleg would land
    /// inside a node from the rank in between.
    fn skips_a_rank(&self, from: usize, to: usize) -> bool {
        self.rank[from].abs_diff(self.rank[to]) > 1
    }
}

/// Route one edge: out of the source's leading side, into the target's
/// trailing side, with a single orthogonal dogleg between them. An edge that
/// skips a rank goes out to the channel instead, so every segment stays in a
/// gap and no line is drawn through a box.
fn route(from_ix: usize, to_ix: usize, nodes: &[PlacedNode], ctx: &RouteCtx) -> Vec<(f32, f32)> {
    let from = &nodes[from_ix];
    let to = &nodes[to_ix];
    let vertical = ctx.vertical;
    let from_cx = from.x + from.w / 2.0;
    let from_cy = from.y + from.h / 2.0;
    let to_cx = to.x + to.w / 2.0;
    let to_cy = to.y + to.h / 2.0;

    if vertical {
        if (to.y - from.y).abs() < 1.0 {
            // Same rank: leave sideways instead of cutting back through the
            // node, which is what an elbow would do at zero vertical gap.
            return if to_cx >= from_cx {
                vec![(from.x + from.w, from_cy), (to.x, to_cy)]
            } else {
                vec![(from.x, from_cy), (to.x + to.w, to_cy)]
            };
        }
        let downward = to_cy > from_cy;
        let start = if downward {
            (from_cx, from.y + from.h)
        } else {
            (from_cx, from.y)
        };
        let end = if downward {
            (to_cx, to.y)
        } else {
            (to_cx, to.y + to.h)
        };
        if ctx.skips_a_rank(from_ix, to_ix) {
            return through_channel(start, end, downward, from_ix, to_ix, ctx);
        }
        dogleg(start, end, true)
    } else {
        if (to.x - from.x).abs() < 1.0 {
            return if to_cy >= from_cy {
                vec![(from_cx, from.y + from.h), (to_cx, to.y)]
            } else {
                vec![(from_cx, from.y), (to_cx, to.y + to.h)]
            };
        }
        let rightward = to_cx > from_cx;
        let start = if rightward {
            (from.x + from.w, from_cy)
        } else {
            (from.x, from_cy)
        };
        let end = if rightward {
            (to.x, to_cy)
        } else {
            (to.x + to.w, to_cy)
        };
        if ctx.skips_a_rank(from_ix, to_ix) {
            return through_channel(start, end, rightward, from_ix, to_ix, ctx);
        }
        dogleg(start, end, false)
    }
}

/// A skip-rank edge: out of the source, along the gap past its own rank, across
/// the channel (outside every box), then into the gap above the target's rank
/// and down into the target.
fn through_channel(
    start: (f32, f32),
    end: (f32, f32),
    forward: bool,
    from_ix: usize,
    to_ix: usize,
    ctx: &RouteCtx,
) -> Vec<(f32, f32)> {
    let half = RANK_GAP / 2.0;
    let from_rank = ctx.rank_main[ctx.rank[from_ix]];
    let to_rank = ctx.rank_main[ctx.rank[to_ix]];
    let out = if forward {
        from_rank.1 + half
    } else {
        from_rank.0 - half
    };
    let into = if forward {
        to_rank.0 - half
    } else {
        to_rank.1 + half
    };
    if ctx.vertical {
        vec![
            start,
            (start.0, out),
            (ctx.channel, out),
            (ctx.channel, into),
            (end.0, into),
            end,
        ]
    } else {
        vec![
            start,
            (out, start.1),
            (out, ctx.channel),
            (into, ctx.channel),
            (into, end.1),
            end,
        ]
    }
}

fn dogleg(start: (f32, f32), end: (f32, f32), vertical: bool) -> Vec<(f32, f32)> {
    if vertical {
        if (start.0 - end.0).abs() < 0.75 || (start.1 - end.1).abs() < 1.0 {
            return vec![start, end];
        }
        let mid = (start.1 + end.1) / 2.0;
        vec![start, (start.0, mid), (end.0, mid), end]
    } else {
        if (start.1 - end.1).abs() < 0.75 || (start.0 - end.0).abs() < 1.0 {
            return vec![start, end];
        }
        let mid = (start.0 + end.0) / 2.0;
        vec![start, (mid, start.1), (mid, end.1), end]
    }
}

fn place_label(
    text: &str,
    points: &[(f32, f32)],
    measure: &dyn Fn(&str, f32) -> f32,
) -> PlacedLabel {
    let lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    let w = lines
        .iter()
        .map(|line| measure(line, LABEL_SIZE))
        .fold(0.0f32, f32::max)
        + 10.0;
    let h = lines.len() as f32 * EDGE_LABEL_LINE_H + 6.0;
    let (cx, cy) = label_anchor(points);
    PlacedLabel {
        lines,
        x: cx - w / 2.0,
        y: cy - h / 2.0,
        w,
        h,
    }
}

/// Where an edge label sits: the middle of the dogleg's run, so a label
/// never lands on top of a node.
fn label_anchor(points: &[(f32, f32)]) -> (f32, f32) {
    if points.len() >= 4 {
        let a = points[1];
        let b = points[2];
        return ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
    }
    match (points.first(), points.last()) {
        (Some(a), Some(b)) => ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0),
        _ => (0.0, 0.0),
    }
}

/// Every statement in the fence, with `%%` comments and blank lines gone.
fn statements(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in source.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        for part in split_statements(line) {
            let part = part.trim();
            if !part.is_empty() {
                out.push(part.to_string());
            }
        }
    }
    out
}

/// Split on `;` only at bracket depth zero, so `A[one; two] --> B` stays one
/// statement.
fn split_statements(line: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, ch) in line.char_indices() {
        match ch {
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' => depth -= 1,
            ';' if depth <= 0 => {
                out.push(&line[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&line[start..]);
    out
}

fn parse_statement(
    statement: &str,
    nodes: &mut Vec<NodeDef>,
    ids: &mut HashMap<String, usize>,
    edges: &mut Vec<EdgeDef>,
    open: &[usize],
) -> Option<()> {
    let (id, label, shape, mut i) = parse_node(statement, 0)?;
    let mut prev = intern(nodes, ids, id, label, shape, open);
    loop {
        i = skip_spaces(statement, i);
        if i >= statement.len() {
            return Some(());
        }
        let (style, head, both, label, next) = parse_edge(statement, i)?;
        i = skip_spaces(statement, next);
        let (id, label_shape, shape, next) = parse_node(statement, i)?;
        let cur = intern(nodes, ids, id, label_shape, shape, open);
        edges.push(EdgeDef {
            from: prev,
            to: cur,
            style,
            head,
            both,
            label,
        });
        prev = cur;
        i = next;
    }
}

fn intern(
    nodes: &mut Vec<NodeDef>,
    ids: &mut HashMap<String, usize>,
    id: String,
    label: String,
    shape: NodeShape,
    open: &[usize],
) -> usize {
    if let Some(&ix) = ids.get(&id) {
        // A later mention can spell the label or shape out; the first bare
        // `A` should not win over a later `A[Real label]`.
        let node = &mut nodes[ix];
        if label != id {
            node.label = label;
            node.shape = shape;
        }
        return ix;
    }
    let ix = nodes.len();
    nodes.push(NodeDef {
        label,
        shape,
        clusters: open.to_vec(),
    });
    ids.insert(id, ix);
    ix
}

fn parse_node(s: &str, start: usize) -> Option<(String, String, NodeShape, usize)> {
    let i = skip_spaces(s, start);
    if i >= s.len() {
        return None;
    }
    let (id, j) = if s.as_bytes()[i] == b'"' {
        let end = s[i + 1..].find('"')? + i + 1;
        (s[i + 1..end].to_string(), end + 1)
    } else {
        let mut j = i;
        while j < s.len() && is_id_byte(s.as_bytes()[j]) {
            j += 1;
        }
        if j == i {
            return None;
        }
        (s[i..j].to_string(), j)
    };

    let rest = &s[j..];
    let (shape, close, open_len) = if rest.starts_with("((") {
        (NodeShape::Circle, "))", 2)
    } else if rest.starts_with("([") {
        (NodeShape::Stadium, "])", 2)
    } else if rest.starts_with("[[") {
        (NodeShape::Subroutine, "]]", 2)
    } else if rest.starts_with("[(") {
        (NodeShape::Cylinder, ")]", 2)
    } else if rest.starts_with("{{") {
        (NodeShape::Hexagon, "}}", 2)
    } else if rest.starts_with("[/") {
        (NodeShape::Rect, "/]", 2)
    } else if rest.starts_with("[\\") {
        (NodeShape::Rect, "\\]", 2)
    } else if rest.starts_with('[') {
        (NodeShape::Rect, "]", 1)
    } else if rest.starts_with('(') {
        (NodeShape::Rounded, ")", 1)
    } else if rest.starts_with('{') {
        (NodeShape::Diamond, "}", 1)
    } else if rest.starts_with('>') {
        (NodeShape::Asymmetric, "]", 1)
    } else {
        return Some((id.clone(), id, NodeShape::Rect, j));
    };

    let body = j + open_len;
    let close_at = s[body..].find(close)? + body;
    let label = strip_label(&s[body..close_at]);
    Some((id, label, shape, close_at + close.len()))
}

fn parse_edge(s: &str, i: usize) -> Option<(EdgeStyle, EdgeHead, bool, Option<String>, usize)> {
    // Unlabelled operators, longest first so `<-->` never reads as `<--`.
    const DIRECT: [(&str, EdgeStyle, EdgeHead, bool); 11] = [
        ("<-.->", EdgeStyle::Dotted, EdgeHead::Arrow, true),
        ("<==>", EdgeStyle::Thick, EdgeHead::Arrow, true),
        ("<-->", EdgeStyle::Solid, EdgeHead::Arrow, true),
        ("-.->", EdgeStyle::Dotted, EdgeHead::Arrow, false),
        ("-.-", EdgeStyle::Dotted, EdgeHead::None, false),
        ("-->", EdgeStyle::Solid, EdgeHead::Arrow, false),
        ("---", EdgeStyle::Solid, EdgeHead::None, false),
        ("--o", EdgeStyle::Solid, EdgeHead::Circle, false),
        ("--x", EdgeStyle::Solid, EdgeHead::Cross, false),
        ("==>", EdgeStyle::Thick, EdgeHead::Arrow, false),
        ("===", EdgeStyle::Thick, EdgeHead::None, false),
    ];
    for (op, style, head, both) in DIRECT {
        if !starts_at(s, i, op) {
            continue;
        }
        let after = i + op.len();
        let (label, next) = pipe_label(s, after);
        return Some((style, head, both, label, next));
    }

    // `A -- text --> B`, `A == text ==> B`, `A -. text .-> B`.
    let (opener, style, terminators): (&str, EdgeStyle, &[&str]) = if starts_at(s, i, "-.") {
        ("-.", EdgeStyle::Dotted, &[".->", ".-"])
    } else if starts_at(s, i, "==") {
        ("==", EdgeStyle::Thick, &["==>", "==="])
    } else if starts_at(s, i, "--") {
        ("--", EdgeStyle::Solid, &["-->", "---", "--o", "--x"])
    } else {
        return None;
    };
    let body = i + opener.len();
    for terminator in terminators {
        let Some(rel) = s[body..].find(terminator) else {
            continue;
        };
        let head = match *terminator {
            "--o" => EdgeHead::Circle,
            "--x" => EdgeHead::Cross,
            "-->" | "==>" | ".->" => EdgeHead::Arrow,
            _ => EdgeHead::None,
        };
        return Some((
            style,
            head,
            false,
            Some(strip_label(&s[body..body + rel])),
            body + rel + terminator.len(),
        ));
    }
    None
}

fn pipe_label(s: &str, at: usize) -> (Option<String>, usize) {
    if starts_at(s, at, "|") {
        if let Some(len) = s[at + 1..].find('|') {
            return (
                Some(strip_label(&s[at + 1..at + 1 + len])),
                at + 1 + len + 1,
            );
        }
    }
    (None, at)
}

/// A node/label body as plain text: quotes around the whole label dropped,
/// `<br>` variants turned into line breaks, emphasis markers removed.
fn strip_label(raw: &str) -> String {
    let mut text = raw.trim();
    if text.len() >= 2 {
        let quoted = (text.starts_with('"') && text.ends_with('"'))
            || (text.starts_with('\'') && text.ends_with('\''));
        if quoted {
            text = &text[1..text.len() - 1];
        }
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(lt) = rest.find('<') {
        let after = &rest[lt..];
        let lower = after.to_ascii_lowercase();
        let br = ["<br/>", "<br />", "<br>"]
            .into_iter()
            .find(|tag| lower.starts_with(tag));
        match br {
            Some(tag) => {
                out.push_str(&rest[..lt]);
                out.push('\n');
                rest = &after[tag.len()..];
            }
            None => {
                out.push_str(&rest[..lt + 1]);
                rest = &after[1..];
            }
        }
    }
    out.push_str(rest);
    out.replace("**", "")
        .replace("__", "")
        .replace('`', "")
        .trim()
        .to_string()
}

fn split_word(s: &str) -> (&str, &str) {
    let s = s.trim();
    match s.find(char::is_whitespace) {
        Some(at) => (&s[..at], &s[at..]),
        None => (s, ""),
    }
}

fn skip_spaces(s: &str, mut i: usize) -> usize {
    while i < s.len() && s.as_bytes()[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

fn starts_at(s: &str, i: usize, pattern: &str) -> bool {
    s.as_bytes()[i..].starts_with(pattern.as_bytes())
}

fn is_id_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.'
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in for shaped text: the real metrics come from the window's
    /// text system, but every layout rule below is metric-agnostic.
    fn measure(text: &str, size: f32) -> f32 {
        text.chars().count() as f32 * size * 0.55
    }

    fn laid_out(source: &str) -> Flowchart {
        let parsed = parse(source).expect("parses");
        layout(&parsed, &measure)
    }

    fn node<'a>(chart: &'a Flowchart, label: &str) -> &'a PlacedNode {
        chart
            .nodes
            .iter()
            .find(|node| node.lines.join(" ") == label)
            .unwrap_or_else(|| panic!("no node labelled {label:?}"))
    }

    #[test]
    fn parses_chained_edges_and_directions() {
        let parsed = parse("flowchart LR\n  A[Start] --> B{Ready?}\n  B -->|yes| C[(Done)]")
            .expect("parses");
        assert_eq!(parsed.direction, Direction::LeftRight);
        assert_eq!(parsed.nodes.len(), 3);
        assert_eq!(parsed.nodes[0].shape, NodeShape::Rect);
        assert_eq!(parsed.nodes[1].shape, NodeShape::Diamond);
        assert_eq!(parsed.nodes[2].shape, NodeShape::Cylinder);
        assert_eq!(parsed.edges.len(), 2);
        assert_eq!(parsed.edges[0].from, 0);
        assert_eq!(parsed.edges[0].to, 1);
        assert_eq!(parsed.edges[1].label.as_deref(), Some("yes"));
        assert_eq!(parsed.edges[1].head, EdgeHead::Arrow);
    }

    #[test]
    fn parses_labelled_and_styled_edge_forms() {
        for (source, style) in [
            ("graph TD\n A --> B", EdgeStyle::Solid),
            ("graph TD\n A -.-> B", EdgeStyle::Dotted),
            ("graph TD\n A ==> B", EdgeStyle::Thick),
        ] {
            let parsed = parse(source).expect("parses");
            assert_eq!(parsed.edges[0].style, style);
        }
        let parsed = parse("graph TD\n A -- pick one --> B").expect("parses");
        assert_eq!(parsed.edges[0].label.as_deref(), Some("pick one"));
        let parsed = parse("graph TD\n A -. retry .-> B").expect("parses");
        assert_eq!(parsed.edges[0].style, EdgeStyle::Dotted);
        assert_eq!(parsed.edges[0].label.as_deref(), Some("retry"));
    }

    #[test]
    fn label_markup_becomes_plain_lines() {
        let parsed = parse("graph TD\n A[\"**Bold**<br/>second\"] --> B").expect("parses");
        assert_eq!(parsed.nodes[0].label, "Bold\nsecond");
        assert_eq!(parsed.nodes[0].shape, NodeShape::Rect);
    }

    #[test]
    fn unsupported_sources_fall_back() {
        // Another diagram type, a bad direction, and a statement the parser
        // cannot represent all decline rather than draw a partial graph.
        assert!(parse("sequenceDiagram\n Alice->>Bob: hi").is_none());
        assert!(parse("flowchart SIDEWAYS\n A --> B").is_none());
        assert!(parse("graph TD\n A --> B\n A -->").is_none());
    }

    #[test]
    fn statements_split_outside_brackets() {
        assert_eq!(
            split_statements("A[one; two] --> B"),
            vec!["A[one; two] --> B"]
        );
        assert_eq!(
            split_statements("A --> B; B --> C"),
            vec!["A --> B", " B --> C"]
        );
    }

    #[test]
    fn ranks_follow_edge_direction() {
        let chart = laid_out("graph TD\n A --> B --> C");
        assert!(node(&chart, "A").y < node(&chart, "B").y);
        assert!(node(&chart, "B").y < node(&chart, "C").y);
    }

    #[test]
    fn left_right_flows_along_x() {
        let chart = laid_out("graph LR\n A --> B --> C");
        assert!(node(&chart, "A").x < node(&chart, "B").x);
        assert!(node(&chart, "B").x < node(&chart, "C").x);
    }

    #[test]
    fn bottom_up_mirrors_the_ranks() {
        let chart = laid_out("graph BT\n A --> B");
        assert!(node(&chart, "A").y > node(&chart, "B").y);
    }

    #[test]
    fn nodes_never_overlap() {
        let chart = laid_out(
            "flowchart TD\n A[alpha] --> B[beta]\n A --> C[gamma]\n B --> D[delta]\n C --> D",
        );
        for (ix, a) in chart.nodes.iter().enumerate() {
            for b in chart.nodes.iter().skip(ix + 1) {
                let apart =
                    a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y;
                assert!(apart, "{:?} overlaps {:?}", a.lines, b.lines);
            }
        }
    }

    #[test]
    fn siblings_share_a_row_and_inflow_sits_between_them() {
        let chart = laid_out("graph TD\n A --> B\n A --> C\n B --> D\n C --> D");
        assert_eq!(node(&chart, "B").y, node(&chart, "C").y);
        let b = node(&chart, "B");
        let c = node(&chart, "C");
        let d = node(&chart, "D");
        assert!(d.x + d.w / 2.0 >= b.x && d.x + d.w / 2.0 <= c.x + c.w);
    }

    #[test]
    fn cycles_terminate_and_stay_inside_the_canvas() {
        let chart = laid_out("graph TD\n A --> B\n B --> C\n C --> A");
        assert_eq!(chart.nodes.len(), 3);
        for node in &chart.nodes {
            assert!(node.x >= 0.0 && node.y >= 0.0);
            assert!(node.x + node.w <= chart.width + 0.01);
            assert!(node.y + node.h <= chart.height + 0.01);
        }
    }

    #[test]
    fn edges_start_and_end_on_node_borders() {
        let chart = laid_out("graph TD\n A --> B");
        let a = node(&chart, "A");
        let b = node(&chart, "B");
        let edge = &chart.edges[0];
        let start = edge.points[0];
        let end = *edge.points.last().unwrap();
        assert!((start.0 - (a.x + a.w / 2.0)).abs() < 0.01);
        assert!((start.1 - (a.y + a.h)).abs() < 0.01);
        assert!((end.0 - (b.x + b.w / 2.0)).abs() < 0.01);
        assert!((end.1 - b.y).abs() < 0.01);
    }

    #[test]
    fn edge_labels_sit_between_the_ranks() {
        let chart = laid_out("graph TD\n A -->|maybe| B");
        let label = chart.edges[0].label.as_ref().expect("label placed");
        let a = node(&chart, "A");
        let b = node(&chart, "B");
        assert!(label.y > a.y + a.h);
        assert!(label.y + label.h < b.y);
    }

    #[test]
    fn a_bare_reference_keeps_the_earlier_label() {
        let chart = laid_out("graph TD\n A --> B[Real label]\n B --> C");
        node(&chart, "Real label");
        assert!(chart.nodes.iter().all(|node| node.lines.join(" ") != "B"));
    }

    #[test]
    fn a_subgraph_becomes_a_titled_frame_around_its_members() {
        let chart = laid_out(
            "flowchart TD\n subgraph one[\"First group\"]\n  A[a] --> B[b]\n end\n B --> C[c]",
        );
        assert_eq!(chart.clusters.len(), 1);
        let frame = &chart.clusters[0];
        assert_eq!(frame.title, vec!["First group".to_string()]);
        for label in ["a", "b"] {
            let node = node(&chart, label);
            assert!(node.x >= frame.x && node.x + node.w <= frame.x + frame.w);
            assert!(node.y >= frame.y && node.y + node.h <= frame.y + frame.h);
        }
        // The node outside the group sits below its frame.
        let outside = node(&chart, "c");
        assert!(outside.y >= frame.y + frame.h);
    }

    #[test]
    fn a_subgraph_without_a_title_keeps_its_id_as_the_heading() {
        let chart = laid_out("graph TD\n subgraph one\n A --> B\n end");
        assert_eq!(chart.clusters[0].title, vec!["one".to_string()]);
    }

    #[test]
    fn empty_and_nested_subgraphs_do_not_break_the_frame() {
        // A stray `end` and a group with no members draw nothing; a nested
        // group's frame sits inside its parent's and paints over it.
        let chart = laid_out(
            "graph TD\n end\n subgraph outer[Outer]\n subgraph inner[Inner]\n  A --> B\n end\n end",
        );
        assert_eq!(chart.clusters.len(), 2);
        assert_eq!(chart.clusters[0].title, vec!["Outer".to_string()]);
        assert_eq!(chart.clusters[1].title, vec!["Inner".to_string()]);
        let (outer, inner) = (&chart.clusters[0], &chart.clusters[1]);
        assert!(outer.w * outer.h > inner.w * inner.h, "largest frame first");
        assert!(inner.x >= outer.x && inner.y >= outer.y);
        assert!(inner.x + inner.w <= outer.x + outer.w);
        assert!(inner.y + inner.h <= outer.y + outer.h);

        let empty = laid_out("graph TD\n subgraph none[Empty]\n end\n A --> B");
        assert!(empty.clusters.is_empty(), "an empty group draws no frame");
    }

    #[test]
    fn different_groups_keep_their_frames_apart() {
        // Two single-node groups with no edges between them share a rank.
        let chart =
            laid_out("graph TD\n subgraph one[One]\n  A\n end\n subgraph two[Two]\n  B\n end");
        let (first, second) = (&chart.clusters[0], &chart.clusters[1]);
        let (a, b) = (node(&chart, "A"), node(&chart, "B"));
        assert_eq!(a.y, b.y);
        assert!(
            first.x + first.w <= second.x || second.x + second.w <= first.x,
            "group frames overlap: {first:?} {second:?}"
        );
    }

    /// No edge segment may run through a node box: a line drawn through a box
    /// disappears behind it and reads as broken.
    fn assert_edges_avoid_nodes(chart: &Flowchart) {
        for (edge_ix, edge) in chart.edges.iter().enumerate() {
            for window in edge.points.windows(2) {
                let (a, b) = (window[0], window[1]);
                let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
                let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
                for node in &chart.nodes {
                    // Touching a border is how an edge meets its own node; a
                    // segment that overlaps the box runs through it.
                    let inside = x1 > node.x + 0.5
                        && x0 < node.x + node.w - 0.5
                        && y1 > node.y + 0.5
                        && y0 < node.y + node.h - 0.5;
                    assert!(
                        !inside,
                        "edge {edge_ix} {a:?}->{b:?} crosses {:?}",
                        node.lines
                    );
                }
            }
        }
    }

    #[test]
    fn a_skip_rank_edge_routes_around_the_boxes() {
        // ALT and LIS share a rank; ALT --> MODE jumps over it, so a mid-gap
        // dogleg would land inside LIS.
        let chart = laid_out(
            "flowchart TD\n HK[a] --> LIS[b]\n ALT[c]\n MODE{d}\n LIS --> MODE\n ALT --> MODE",
        );
        assert_edges_avoid_nodes(&chart);
        let long = &chart.edges[2];
        assert!(long.points.len() > 4, "a skip-rank edge takes the channel");
        assert!(
            long.points.iter().any(|p| p.0 > chart.width - 20.),
            "the channel runs outside the widest box: {:?}",
            long.points
        );
    }

    /// The shape of a real agent answer: a whole trigger flow inside one
    /// `subgraph`, quoted labels with `<br/>` and a `·` in the heading.
    #[test]
    fn a_trigger_flow_inside_a_subgraph_lays_out() {
        let chart = laid_out(
            r#"flowchart TD
    subgraph T["1 · Trigger"]
        HK["Global hotkey<br/>default ⌘+Space"] --> LIS["Hotkey listener<br/>plugin + CGEventTap"]
        ALT["Tray · CLI · in-app button"]
        MODE{"Trigger mode?"}
        LIS --> MODE
        ALT --> MODE
        MODE -->|"Toggle"| TOG["Press start / press stop"]
        MODE -->|"Push-to-talk"| PTT["Hold / release"]
        MODE -->|"Hands-free"| HF["VAD splits on silence"]
    end"#,
        );
        assert_eq!(chart.clusters.len(), 1);
        assert_eq!(chart.nodes.len(), 7);
        assert_eq!(chart.edges.len(), 6);
        let frame = &chart.clusters[0];
        assert_eq!(frame.title, vec!["1 · Trigger".to_string()]);
        assert_eq!(node(&chart, "Global hotkey default ⌘+Space").lines.len(), 2);
        for placed in &chart.nodes {
            assert!(placed.x >= frame.x && placed.x + placed.w <= frame.x + frame.w);
            assert!(placed.y >= frame.y && placed.y + placed.h <= frame.y + frame.h);
        }
        assert!(chart.width > 400. && chart.height > 300.);
        assert_edges_avoid_nodes(&chart);
    }
}

#[cfg(test)]
mod probe {
    use super::*;
    #[test]
    fn dump() {
        let src = r#"flowchart TD
    subgraph T["1 · Trigger"]
        HK["Global hotkey<br/>default ⌘+Space"] --> LIS["Hotkey listener<br/>plugin + CGEventTap"]
        ALT["Tray · CLI · in-app button"]
        MODE{"Trigger mode?"}
        LIS --> MODE
        ALT --> MODE
        MODE -->|"Toggle"| TOG["Press start / press stop"]
        MODE -->|"Push-to-talk"| PTT["Hold / release"]
        MODE -->|"Hands-free"| HF["VAD splits on silence"]
    end"#;
        let chart = layout(&parse(src).unwrap(), &|t: &str, s: f32| {
            t.chars().count() as f32 * s * 0.55
        });
        println!("canvas {:.0}x{:.0}", chart.width, chart.height);
        for c in &chart.clusters {
            println!(
                "frame {:?} {:.0},{:.0} {:.0}x{:.0}",
                c.title, c.x, c.y, c.w, c.h
            );
        }
        for n in &chart.nodes {
            println!(
                "node {:?} {:.0},{:.0} {:.0}x{:.0}",
                n.lines, n.x, n.y, n.w, n.h
            );
        }
        for e in &chart.edges {
            println!(
                "edge {:?}",
                e.points
                    .iter()
                    .map(|p| (p.0.round(), p.1.round()))
                    .collect::<Vec<_>>()
            );
        }
    }
}
