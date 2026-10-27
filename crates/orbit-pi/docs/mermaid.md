# Mermaid diagrams

Orbit draws a subset of Mermaid natively. This file records what the subset is,
why the rest falls back to source, and how the renderer is put together.
`INTENT.md` D2 is the decision; this is the implementation.

## The rule

**Never present a diagram that isn't rendered.** A fence is upgraded to a
diagram only when the parser understands *all* of it. Anything else — another
diagram type, `subgraph` clustering, a statement the parser cannot represent,
a cycle-free graph it cannot rank — keeps the copyable source block it has
today, labelled `Mermaid · source (preview unavailable)`.

A partially drawn graph is worse than the source: it looks authoritative and
silently drops structure the author wrote.

## What renders today

`flowchart` / `graph`:

| Feature | Supported |
|---|---|
| Directions | `TD`/`TB`, `BT`, `LR`, `RL` (no direction → `TD`) |
| Shapes | `A[rect]`, `A(rounded)`, `A([stadium])`, `A[[subroutine]]`, `A[(cylinder)]`, `A((circle))`, `A{diamond}`, `A{{hexagon}}`, `A>asymmetric]`, `A[/parallel/]` |
| Edges | `-->`, `---`, `-.->`, `-.-`, `==>`, `===`, `--o`, `--x`, `<-->`, `<==>`, `<-.->` |
| Edge labels | `A -->|label| B`, `A -- label --> B`, `A -. label .-> B`, `A == label ==> B` |
| Chains | `A --> B --> C` on one statement |
| Label markup | quotes stripped, `<br>`/`<br/>`/`<br />` become line breaks, `**bold**`/`` `code` `` markers removed |
| Groups | `subgraph name["Title"] … end`, nested, drawn as a titled frame around their members |
| Comments / separators | `%%` lines, `;` between statements (only outside brackets) |
| Ignored directives | `classDef`, `class`, `style`, `linkStyle`, `click`, `direction`, `accTitle`, `accDescr` |

Everything else declines: `sequenceDiagram`, `classDiagram`, `stateDiagram`,
`erDiagram`, `gantt`, `pie`, `journey`, and any statement the tokenizer cannot
parse.

## Rendering shape

```
render_code_block()          transcript_view.rs
  └─ mermaid_diagram()       fence tag + `mermaid::diagram(source)`
       └─ render_mermaid_block()   card: header + body + source disclosure
            ├─ mermaid::render_diagram()  canvas (edges, shapes) + text labels
            └─ code_lines_body()          the source, unchanged from code blocks
```

* `src/mermaid/flowchart.rs` — parse + layout, no gpui. Unit-tested without a
  window: ranks, ordering, node boxes, edge routing, cycles, label placement.
* `src/mermaid/render.rs` — one canvas paints edges (polylines, dashes, three
  arrowhead kinds) and node shapes (quads and polygons); node and edge labels
  are positioned text elements on top, so they keep the window's font
  smoothing instead of being hand-painted glyphs.
* `src/mermaid/measure.rs` — line widths from the window's text system, since
  layout must size boxes before any element exists. The shell installs it once
  per frame (`OrbitApp::render`); without an install, layout falls back to a
  character-count estimate. It is never used to paint a diagram in the app.
* `src/mermaid/mod.rs` — `diagram(source)` memoizes layout by source hash and
  interface font (a transcript row re-renders on every heartbeat).

The block model is untouched: a mermaid fence stays `Block::Code`, so copy,
selection, and fold state work exactly as before. The source disclosure in a
diagram card reuses the code fold's `expanded_blocks` key — the diagram and the
fold never coexist for one block, so one key covers both.

**A fence upgrades only once its row settles.** A streaming fence is not a graph
yet: it would re-lay-out on every heartbeat (~90 ms) and flicker while the agent
typed. Live rows keep the source block; the diagram appears when the run ends.
Static documents (skills, PR/commit bodies, the Files viewer) are settled from
the start.

## Layout

Layered ("Sugiyama-lite"):

1. **Rank** — longest path from a source over the acyclic part of the graph.
   Back edges (cycles) are found by DFS and excluded from ranking, which is why
   a self-referencing diagram cannot hang.
2. **Order** — four barycenter sweeps (down, up, down, up) reduce crossings;
   the first-seen order is the tie-break, so a diagram without crossings is
   never shuffled.
3. **Place** — siblings share a rank with `NODE_GAP` between them; each rank is
   centred on the widest one; ranks stack with `RANK_GAP`.
4. **Mirror** — `BT`/`RL` mirror the finished geometry, so edge anchors stay
   purely geometric.
5. **Group** — `subgraph` frames are built inside-out (a parent's box covers
   its nested children's), and members are sorted contiguously inside each rank
   so a frame only encloses its own nodes. Frames get `CLUSTER_GAP` of extra
   space between them, so two neighbouring groups never overlap.
6. **Route** — out of the source's leading side, into the target's trailing
   side, with one orthogonal dogleg between them. Same-rank and backward edges
   leave sideways so they never cut through a node. An edge that **skips a
   rank** (A → C with B between) cannot dogleg in the middle — the midpoint is
   inside B — so it runs out to a channel outside every box and frame, down,
   and back in above the target's rank. Every segment stays in a gap; a test
   asserts no edge overlaps a node box.
7. **Trim** — bounds come from nodes, frames, edge elbows, and labels, then
   everything translates to `MARGIN`.

Node sizing comes from the measured text plus per-shape padding: a diamond is
inflated so its text fits the inscribed area, a circle becomes a square, a
hexagon gains its points.

## Adding a diagram type

Each type is a parser + layout, not a new pipeline:

1. `parse()` in a new `src/mermaid/<kind>.rs`, returning `None` for anything it
   cannot fully represent.
2. `layout()` producing absolute node/edge/label geometry.
3. Dispatch on the fence's first keyword in `mermaid::diagram`.
4. A `render` side only if the shapes are new (sequence diagrams need lifelines
   and activation bars; pie needs an arc path).

Sequence diagrams are the highest-value next type: agents write them often and
the layout is rows and columns rather than layered ranking.

## Known gaps

* Tall diagrams are not height-capped; a 40-rank flowchart makes a tall
  transcript row (the list measures it correctly, it is simply long).
* Very wide diagrams scroll horizontally, like code blocks.
* Group frames are bounding boxes, not true cluster layout: a rank whose nodes
  interleave two groups puts one group's frame around the other's nodes. The
  contiguous-member sort makes that rare, not impossible.
* A group title is drawn even when it would overlap an edge crossing the top of
  its frame.
* Back edges (cycles) route between ranks rather than around the diagram's
  edge; readable, not pretty.
* No `classDef`/`style` colours — a diagram uses the theme's code surface for
  every node, so it reads correctly in both appearances.
