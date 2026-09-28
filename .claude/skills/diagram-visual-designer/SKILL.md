---
name: diagram-visual-designer
description: Use when drawing or judging the RENDERED picture of a diagram in vim-shapes — the clean PNG/SVG/PDF/HTML export (`clean.rs`), the `V` preview, `Options::preview()`'s defaults, `Layer::pastel()`'s fills, or anything a non-terminal viewer sees (a screenshot handed to a stakeholder, a wiki embed, a printed page). Grounds the work in this app's own gruvbox identity so a rendered diagram reads as vim-shapes drew it, never as a copy of Archi's or draw.io's stock ArchiMate palette wearing this app's shapes.
---

# Visual designer for the rendered diagram

## Stance

`clean.rs` exists for one moment: someone who is not in the terminal looks at
this picture. That is the whole job — `V`, `:export --style clean`, the SVG
and PDF and HTML that go into a doc or a chat. It is a *different* surface
from the terminal chrome (`ascii-tui-designer` owns that one) and answers to a
different question: not "would a 1993 terminal show this" but "does this look
like *vim-shapes* drew it, or like it was imported from somewhere else and
just happens to use this app's shapes."

The trap is copying the wrong reference. A desktop ArchiMate tool (Archi,
draw.io's ArchiMate library) is the thing everyone who models has seen a
thousand times: saturated primary pastels — candy cyan for application,
candy yellow for business — on stark `#ffffff`, under a ruled grid by
default. That is a real, legible convention, but it is *someone else's*
palette. Reach for it as a comparison to avoid, not a comparison to match.

## Rules

1. **Colour comes from `theme.rs`'s palette, translated to raw pixels, never
   invented fresh.** `Layer::pastel()` (in `ontology/mod.rs`) is each layer's
   `PAINTS_LIGHT`/`theme::LIGHT.layers` accent, washed toward paper — not a
   generic ArchiMate primary. `clean.rs`'s `INK`, `PAPER`, `GRID_MINOR`,
   `GRID_MAJOR`, and the kind-tag `dim` colour are all either a literal
   theme role value (`theme::LIGHT.ink`, `.dim`) or that value mixed toward
   paper with `ontology::mix`, with the ratio left in a comment. If you need
   a new colour here, derive it the same way — from an existing accent — and
   say what you derived it from and why, the same discipline `theme.rs`'s own
   doc comment asks of every role and accent.
2. **Legibility per layer still matters more than restraint.** The entire
   point of colouring by layer is that an architect can tell at a glance
   which one a shape belongs to. Muting toward the app's own palette must
   not wash every layer into the same pale tan — check adjacent layers
   against each other, not just against paper, before committing a value.
3. **The paper is calm, not clinical.** Pure `#ffffff` reads as a spreadsheet
   printout; this app's own light mode is a warm cream
   (`theme::LIGHT.ground`). The clean picture's `PAPER` should sit between
   those two — light enough to still read as "white," warm enough to not be
   `#ffffff` outright. Whatever appearance mode a picture is in, the ink and
   paper should look like they belong to the same design as the grid lines
   and the layer fills — no cool grey mixed with a warm cream.
4. **A first look defaults to plain.** `Options::preview()` — what `V` opens
   without being asked anything — should default to the fewest marks that
   still read as a diagram: no grid unless the diagram's own page metadata
   asks for one, no border chrome beyond the diagram's own margin. Grid,
   zoom, transparency and the rest are what `:export`'s dialog is *for*;
   the one-key preview is not the place to default every knob on.
5. **The picture is still paint over one geometry, never a second one.**
   Restyling here means colours, weights, fonts, paper — never a new curve
   sampler, a new fill rule, a new arrowhead shape. Every outline, route and
   label already comes from `shapes::outline`/`relation_along` and
   `canvas::label_lines`; the terminal and every export agree about *where*
   things are because of that. If a colour change needs new geometry to look
   right, the geometry belongs in `shapes.rs`, read by every surface, not
   duplicated in `clean.rs` alone.
6. **Check both appearances and both directions.** A fill or ink colour
   picked against `Appearance::Light` needs a look at `Appearance::Dark`
   too — `clean.rs` already branches `ink`/`paper`/`dim` on `dark`; a new
   constant should get the same branch rather than assuming light. Render an
   actual PNG (`cargo test eyeball_the_three_looks -- --ignored --nocapture`,
   or add a throwaway `#[ignore]` eyeball test modelled on the existing ones
   and delete it once you've looked) rather than reasoning about hex values
   on paper — pastel washes and warm greys read differently than their
   numbers suggest.

## When reviewing someone else's rendered-picture change

Flag: any hardcoded RGB that isn't a `theme.rs` role/accent value or a
documented `ontology::mix` of one; `PAPER` moved back toward pure white or
pure black; a new default that turns *more* on rather than less; a colour
picked for light mode with no corresponding check in dark mode (or vice
versa); layer fills that have drifted close enough in hue that two adjacent
layers are hard to tell apart at a glance.
