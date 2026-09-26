# The wireframe ink — research and plan

*Written 2026-09-26. Braille reads badly on a screenshare: a line is a string of pinpricks with the ground showing through. The decision is a wireframe ink — box-drawing line art — with a richer glyph vocabulary for the shapes that are not rectangles. This file is the research and the plan; nothing is built.*

## 1. What the character sets offer

Four blocks matter, in order of how safely they can be relied on.

**Tier 0 — Box Drawing (U+2500–257F) and Block Elements (U+2580–259F).** 128 line pieces: light `─│`, heavy `━┃`, double `═║`, every corner and tee and cross in every mix of weights, the four arc corners `╭╮╰╯` (U+256D–2570), the diagonals `╱╲╳` (U+2571–2573), and dashed lines in double, triple and quadruple dash at both weights (`╌╍ ┄┅ ┈┉` and their verticals `╎╏ ┆┇ ┊┋`). Every modern terminal — Ghostty, Kitty, WezTerm, iTerm2, Terminal.app, xterm.js, VTE — *draws these itself* rather than trusting the font, which is why they tile without seams and why this tier is safe everywhere. Blocks give `▀▄█▌▐` and the quadrants `▖▗▘▝▚▞▙▛▜▟`.

**Tier 1 — Miscellaneous Technical (U+2300–23FF), Geometric Shapes (U+25A0–25FF), Arrows.** Font glyphs, not synthesised, but present in every monospace font worth the name. The pieces of large parentheses `⎛⎜⎝ ⎞⎟⎠` (U+239B–23A0) are an ellipse's sides; the square-bracket pieces `⎡⎢⎣ ⎤⎥⎦` are a predefined process's double bars; the curly pieces `⎧⎨⎩ ⎫⎬⎭ ⎰⎱` are a brace or a wave; `⌜⌝⌞⌟` are open corners; `⌒ ⌢ ⌣` are an arc, a frown and a smile — a document's wavy bottom. Geometric Shapes give filled and hollow arrowheads `▶◀▲▼ ▷◁△▽`, the diamonds `◆◇`, dots `●○`, the corner triangles `◢◣◤◥` and the quarter arcs `◜◝◞◟`. Arrows give `➤ ⟶ ⇒` where a heavier head is wanted.

**Tier 2 — Symbols for Legacy Computing (U+1FB00–1FBFF).** The 1970s home-computer graphics, and the only place Unicode has *slopes other than 45°*: 52 smooth-mosaic wedges (U+1FB3C–1FB6F, "lower left block diagonal lower middle left to lower centre" and its kin — filled triangles whose hypotenuse runs at 1:2, 1:1, 2:1 across the cell), the triangular quarter and three-quarter blocks (U+1FB68–1FB6F), the half-cell triangles (U+1FB9A–1FB9B), and fifteen *box-drawing light diagonal segments* (U+1FBA0–1FBAE) that join a cell's edge midpoints and corners in the combinations `╱╲` cannot — a shallow slope, a V, a caret, a diamond. Support: Ghostty draws the whole block itself (100 %), VTE terminals 94 %, Kitty and xterm.js the important part; Cascadia Code ships them since 2404; Terminal.app and most fonts do not. So Tier 2 is an *upgrade* behind a switch, never the default.

**Plain ASCII.** The convention every ASCII-diagram tool converges on — ditaa, svgbob, asciiflow, Monodraw, the IETF's RFC figures — is worth knowing because it is what a diagram pasted into a ticket must still say: corners `. '` for round and `+` for sharp, `/ \` for 45°, `( )` for an ellipse's sides, `-` `|` `=` for lines, `< > ^ v` for heads, `~` for a wave, `: !` for a broken vertical, `{s} {d} {io}` in ditaa to ask for a cylinder, a document, an input/output slant. svgbob turns `.` and `'` into real arcs and `( )` into real curves, which is the same trick the wireframe plays with `╭╮╰╯` and `⎛⎞`.

## 2. The vocabulary, shape by shape

The rule: a shape is a **stencil** — a template parametrised by its width and height, placed cell by cell — not a rasterised curve. A stencil is exact, symmetric and the same every frame; a raster of a curve at cell resolution is a staircase that jitters when the box moves a cell. Each stencil has a Tier 0/1 form and, where a slope wants it, a Tier 2 form.

| shape | stencil (Tier 0/1) | notes |
| --- | --- | --- |
| rectangle | `┌─┐ │ └─┘` | heavy `┏━┓` for stroke 2, double `╔═╗` for stroke 3 and for the abstract (an interface); dashed `┄┆` for a grouping; rounded → `╭─╮ ╰─╯` |
| ellipse / circle | `╭╯ ╰╮` steps at each end, `⎛⎜⎝ ⎞⎟⎠` on tall sides | a stadium built from paired arc corners reads as a curve at cell resolution; tall ones take the parenthesis pieces |
| diamond | `╱╲` apex, `╱  ╲` flanks, `╲  ╱`, `╲╱` foot | Tier 2: wedges for the 1:2 slope a wide diamond needs |
| parallelogram | `╱───╱` slanted sides | Tier 2 for shallow slants |
| trapezoid | `╱───╲` over a wider base | |
| hexagon | `╱──╲ │  │ ╲──╱` | pointy ends when narrow |
| triangle | `╱╲` apex over `╱  ╲` and a `────` base | Tier 2 wedges for a wide, flat triangle |
| cylinder | `╭──╮` lid, `├──┤` the lid's lower rim, `│  │`, `╰──╯` | ditaa's `{s}`; the rim is what says cylinder |
| cloud | scallops of `╭─╮` and `╰─╯` around a rounded body | five to seven bumps by width, the way the braille cloud has five |
| document | box with a wavy foot `╰⌣⌢⌣╯` or `╰─╮╭─╯` | ditaa's `{d}` |
| note | box with a folded corner `─┬┐ / └┤` | |
| card | box with one cut corner `╱──┐` | |
| callout | rounded box with a tail `╰┬─╯ / ╱` | |
| cube | a box with `╱` offsets top and right and `╲` corner | |
| step (an action) | `┌──╲ / │   ╲ / │   ╱ / └──╱` and a notch `╲ ╱` on the left | |
| tape | `╭╮╰╯` pairs along the top and bottom | |
| delay | flat left side, `⎞⎟⎠` right side | |
| display | `╱ ╲` point on the left, rounded right | |
| manual input | slanted top `╱───┐` | |
| off-page | box over a `╲╱` foot | |
| block arrow / double arrow | `┌──╲ ▶` / `◀ ╱──╲ ▶` with `▶◀` heads | |
| and / or | a circle with `┼` / `╳` inside | |
| stick figure | `○` head, `╱│╲` arms, `╱ ╲` legs | |
| data storage | `╭──╮ │  ⎞ ╰──╯` | |
| predefined process | box with `⎢ ⎥` inner bars | |
| internal storage | box with `┬ ├┼` inner rules | |
| text | nothing — the label | |

Every stencil is checked by a test that it fits its box exactly, is left-right symmetric where the shape is, and joins: every `─` meets a `│` or a corner, never a gap.

### Lines

A relation is drawn on the cell grid with a Bresenham walk from port to port, and each cell gets the glyph its *slope* asks for: `─` runs where the slope is under a third, `│` where it is over three, `╱` or `╲` between; a route with corners uses `╭╮╰╯` for rounded elbows (an orthogonal route becomes clean box drawing with one glyph at the turn). Where a line meets a box edge the edge's glyph becomes the tee — `┤ ├ ┬ ┴` — and where two lines cross, `┼`; that is the connectivity resolver (Section 3). Styles: dashed `╌ ╎`, dotted `┈ ┊`, heavy `━ ┃` for width 2, heavy dashed `╍ ╏` — every combination is in Tier 0. Tier 2 replaces the 45° stairs on a shallow diagonal with the U+1FBA0 segments, which is what makes a long slanted link read as one line.

### Ends

| end | horizontal | vertical |
| --- | --- | --- |
| arrow (filled) | `▶ ◀` | `▼ ▲` |
| open | `> <` | `v ^` |
| triangle (hollow) | `▷ ◁` | `▽ △` |
| diamond / hollow diamond | `◆ ◇` | the same |
| dot / circle | `● ○` | the same |
| bar (one) | `┤ ├` | `┴ ┬` |
| crow's foot (many) | `⟩ ⟨`, with `⋎ ⋏` for a vertical foot | the nearest one-cell glyph; a two-cell `─<` where there is room |

### Marks and the cursor

The braille node marks (◆ at each node, ● where a port is patched) become `◆ ◇ ●` glyphs on the line's cell, which is what they already look like. The cursor's inverse tint and the handles are unchanged; a handle sits on its corner or edge cell.

## 3. How it is drawn

A new module, `ui/wire.rs`, beside the braille canvas. The canvas already produces geometry — outlines as primitives, routes as points — but the wireframe does not rasterise those; it takes the *shape* and the *route* and lays glyphs:

1. **Stencils** — `fn stencil(shape, w, h, look) -> Vec<(dx, dy, Glyph)>`, one function per shape, where `Glyph` is the connectivity form (`Line { n, e, s, w, weight, style }`, `Arc(corner)`, `Diag(NE|NW)`, `Fixed(char)`) rather than a character. Rounded, heavy, double and dashed are looks on the same stencil.
2. **A cell buffer of connectivity** — every cell holds which of its eight directions are joined and at what weight. Stencils and lines *merge* into it: two lines crossing become a cross, a line ending on a box edge becomes a tee, a box corner touched by a line becomes the right junction. This is asciiflow's and Monodraw's model, and it is why their output looks drawn rather than pasted.
3. **The resolver** — a table from a cell's connectivity to a character: the box-drawing block is exactly that table, and the arc corners, diagonals and dashed variants are looked up by look. Where a combination has no glyph (a heavy line meeting a double one), the resolver degrades to the nearest weight rather than leaving a hole.
4. **Text last** — labels, tags, rows, node marks, as now.

Tier 2 is a `glyphs` setting (`basic` / `rich`) beside the ink: `rich` lets the resolver reach for wedges and diagonal segments; `basic` never does. Auto-detected from the terminal (`TERM_PROGRAM` is Ghostty, kitty, WezTerm; `VTE_VERSION` is set) and overridable in the config file.

The PNG rendering of the terminal picture draws the cell buffer through a font, so it follows the ink for free; the clean picture is unchanged — it was never braille.

## 4. Where it is chosen

`:ink braille | lines` — the reader's preference, kept in the config file beside `:theme`, since a screenshare is about the viewer and not the diagram. `\` (presenting) can pin `lines` for the duration. `:debug` names the ink and the glyph tier and where they came from.

## 5. Phases

*Phases 1–3 were built on 2026-09-26 and `lines` is the default ink; phase 4 remains.*

1. **The resolver and the rectangle.** ✅ The connectivity buffer, the glyph table for Tier 0 (light, heavy, double, dashed; arcs; diagonals), rectangles and rounded rectangles as stencils, straight and orthogonal routes with every end, `:ink lines`. Everything else still draws in braille inside the wireframe, so the mode is usable from the first day.
2. **The stencils.** ✅ *Ellipse, cylinder, cloud and diamond are stencils; the rest are slope-walked edges. The dedicated stencils for document, note, card, callout, cube, step and the other flowchart shapes come where the edge walk falls short.* The rest of the table above, Tier 0/1, each with its fitting-and-joining test; the cylinder, ellipse and cloud first, since they are the ones every diagram has.
3. **The seams.** ✅ Tees where lines meet boxes, junction merging, handles and node marks in the new ink, the inverse cursor, the compartment rule reusing the box's own weight.
4. **Tier 2 and polish.** *(remaining)* The wedge and diagonal-segment upgrade behind `glyphs rich` with detection; the PNG rendering checked in both inks; the manual page; `DESIGN.md` records the decisions; this file is deleted.

## 6. Left out, and why

- **Rasterising curves to cells.** A stencil is exact and stable; a rasterised ellipse jitters as the box moves and never closes cleanly. Stencils cost a function per shape, which is the price of shapes that look drawn.
- **Sextant/octant "mosaic ink".** Bolder than braille and keeps every shape, but it is a different answer to the same question, and two inks are enough to maintain; it can come back as a third `:ink` if the wireframe leaves something wanting.
- **A plain-ASCII export.** Tempting — a diagram in a ticket — but a separate `:export x.txt` with svgbob's alphabet, not part of the screen ink.

## 7. Open questions for the user

1. Should `lines` become the *default* ink, with braille kept as the option? Recommended: yes, once phase 2 lands — the screenshare case is the common case.
2. On a Tier 0 terminal, a shallow diagonal link is a 45° staircase of `╱` with `─` runs. Acceptable, or should links on such terminals default to the orthogonal route? Recommended: orthogonal by default in `lines`, since it is what box drawing does perfectly.
3. The crow's foot has no good one-cell glyph. `⟩` (angle bracket), or a two-cell `─<`? Recommended: `⟩`, with the ER bar as `┤`, and the manual saying so.
