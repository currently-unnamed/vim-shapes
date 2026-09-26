# A SwiftUI frontend — plan

*Written 2026-09-26. Nothing here is built. This is the plan for a second frontend — a native
macOS app, rendered rather than drawn in glyphs, that keeps the keyboard grammar and adds the
mouse — for people who want vim-shapes' ontology without the terminal. `AGENTS.md` and
`DESIGN.md` describe the tool as it stands; this file describes the seam a GUI needs and the
order it gets built in. Delete it, or fold its decisions into `DESIGN.md`, once it is.*

## 0. What this is and is not

**Is:** a macOS SwiftUI app that opens the same `.json` workspace files, understands the same
keystrokes, and adds mouse gestures for selecting, moving, connecting and marqueeing — built on
the *same* Rust core, so the ontology, the rules, the file format and the command table are not
reimplemented, only re-presented.

**Is not:** a rewrite. Not a second ontology, a second ruleset, or a second idea of what a key
does. Not (yet) Windows or Linux — SwiftUI means macOS first; a second native shell is a later
problem to have, and this plan's core-crate work is what makes that problem cheap when it comes.
Not, unless Phase 0 below is decided otherwise, a departure from the cell grid.

## 1. What the codebase currently makes true

This section is facts, checked against the source as it stands, because every phase below is
sized against them.

- **There is one crate, one binary.** `model`, `ontology`, `shapes`, `layout`, `persistence`,
  `export`, `render` and the whole of `ui` compile into one target. Nothing here is a library
  another Swift-adjacent target could depend on.
- **The keyboard is `crossterm::event::KeyEvent`, all the way to the bottom.** `keymap::Cmd`'s
  `what` and `avail` are `fn(&Where) -> _`, and `App::on_key(&mut self, k: KeyEvent)` in
  `ui/mod.rs` is where every keystroke lands. The command table is already decoupled from *what
  a key means*; it is not decoupled from *how a key arrives*.
- **Geometry is already terminal-agnostic.** `shapes::drawn` turns an `Element` into
  `Vec<CurvePrimitive>` — points and line segments in world (cell) coordinates — with no
  ratatui in sight. `ui/canvas.rs` rasterises that into braille dots on a ratatui `Canvas`;
  `render.rs` rasterises the *terminal's own cell buffer* through a font into a PNG, deliberately
  choosing not to redraw shapes at pixel resolution, so the screen and the export are one
  claim. `shapes.rs` is the layer a GUI renderer reuses; `ui/canvas.rs` and `render.rs` are the
  layer it replaces.
- **Positions are integer cells.** `model.rs`: "Positions are in cells, world coordinates... the
  camera in the UI decides which part of the world is on screen; nothing here knows the screen
  exists." The file format, `layout.rs`'s arrangement, and every idiom in `ontology::idiom` are
  built on that.
- **There is no mouse handling anywhere.** Not in the terminal binary either — `crossterm`
  supports `Event::Mouse` but nothing reads it. Click, drag, marquee and hover are new design,
  not a port.
- **`ontology::emit::spec()` is the existing precedent for "one Rust source of truth, serialized
  for a consumer that has never seen the app."** It is a `serde_json::Map` (sorted, byte-stable)
  walked out of `ShapeKind::ALL`, `RelationKind::ALL` and `allowed`. The FFI boundary this plan
  needs is the same idea applied to *state*, not just to the ontology.
- **Config is a small JSON file at `$XDG_CONFIG_HOME/vim-shapes/config.json`** (`config.rs`) —
  theme and ink, today. A GUI frontend needs its own entries in the same file (window state is
  not a theme, so it may not belong here at all — see open questions).

## 2. Decision 0: keep the cell grid

"Work freely in a rendered visual presentation" could mean either of two different projects:

1. Draw the same cell-grid model with real vector shapes and smooth zoom instead of glyphs, at
   whatever pixel scale the window is. Nothing about `Element.x/y/w/h`, the file format, layout,
   or idioms changes.
2. Let elements sit at arbitrary sub-cell, even sub-pixel, positions — a real freehand canvas.

**This plan assumes (1) and treats (2) as out of scope.** (2) forks the coordinate model the
ontology, layout and file format all agree on, and AGENTS.md rule 6 says a file format change
needs a `VERSION` bump and a compatibility story — (2) would need one for a reason that has
nothing to do with the ontology, which is exactly the kind of change this project's rules exist
to make someone justify out loud. (1) is also, on its own, most of what "rendered visual
presentation" is asking for: shapes drawn as shapes, not as braille dots or box-drawing corners,
smooth at any zoom, with real fonts. If (2) turns out to be what's wanted, it is Phase 4's
`Document`/`model.rs` work that would need revisiting first — say so before Phase 3 is spent
building a Snapshot format around cells.

## 3. Target shape

```
                 ┌─────────────────────────┐
                 │        core crate        │   model, ontology, shapes, layout,
                 │  (no crossterm, no ratatui) │  persistence, export, render, App
                 └────────────┬────────────┘
                              │  Key, Snapshot, FFI methods
              ┌───────────────┴───────────────┐
   ┌──────────┴──────────┐           ┌────────┴─────────┐
   │  vim-shapes (bin)     │           │  VimShapesCore.xcframework │
   │  crossterm + ratatui  │           │  (uniffi-generated)        │
   └──────────┬──────────┘           └────────┬─────────┘
              │                                │
        terminal, as today               ┌─────┴─────┐
                                          │  SwiftUI  │
                                          │  macOS app │
                                          └───────────┘
```

One core, two shells. The terminal binary is not deprecated or changed in behavior — it becomes
a thin adapter over the same logic the Swift app calls.

## 4. Phases

### Phase 1 — extract `vim-shapes-core`

Move `model.rs`, `ontology/`, `shapes.rs`, `layout.rs`, `persistence.rs`, `export.rs`,
`render.rs`, `fonts.rs`, `clean.rs`, `config.rs`, and `App`'s *state and logic* (everything in
`ui/mod.rs` that is not drawing to a ratatui `Frame`) into a `vim-shapes-core` library crate.
The existing binary becomes a workspace member depending on it, unchanged in behavior.

- `render.rs` stays exactly where its doc comment says it must: it rasterises the terminal's
  cell buffer, which means it keeps its `ratatui::buffer::Buffer` dependency and is *not* fully
  portable — it can move into core as an optional, ratatui-gated piece, or stay in the terminal
  binary and be called from there only. Either is fine; what must not happen is a second PNG
  renderer for the Swift app that draws shapes instead of cells, contradicting the one thing
  `render.rs`'s comment insists on. (The Swift app's *screen* rendering is allowed, and meant,
  to look different from the terminal — that is Phase 6's job, not this one's. Its PNG/export
  story, if it wants one, is Phase 8.)
- `ui/canvas.rs`, `ui/chrome.rs`, and everything under `ui/` that draws widgets stay in the
  terminal binary; `keymap.rs`, `excmd.rs`, and the rest of the state-and-tables layer move.
- **Acceptance:** `cargo test`, `cargo build`, `cargo clippy` all green, in the workspace, with
  no behavior change to the terminal app. This is a pure extraction — reviewed as one, or as a
  short series of moves, never as a rewrite.
- **Risk:** this is most of the 24.5k-line crate. Do it in slices (model+ontology+shapes first,
  since they have zero UI coupling; persistence+export next; `App`'s state last, since it is
  the most entangled with `ui/mod.rs`'s drawing code).

### Phase 2 — a core-owned key type

Add `core::input::Key { code: KeyCode, mods: Mods }` (names deliberately distinct from
crossterm's, so nothing that touches core needs `use crossterm`). Change `keymap::Cmd`'s `what`
and `avail`, and `App::on_key`, to take core's `Key`. The terminal binary gets a five-line
adapter: `crossterm::event::KeyEvent -> core::input::Key`, including the existing kitty-protocol
shift+ctrl normalization that's currently inline in `on_key` — that normalization is a
crossterm-and-kitty fact, not an app fact, so it belongs in the adapter, not in core.

- **Acceptance:** the existing kitty-keyboard-protocol tests (and every other keyboard-path
  test) still pass unchanged, driven through the adapter.
- A later Swift-side adapter (`NSEvent`/`.onKeyPress` → `core::input::Key`) is symmetric and is
  Phase 6's work, not this phase's — this phase only has to prove core doesn't need crossterm to
  know what `gd` means.

### Phase 3 — the Snapshot and the FFI boundary

Define `core::snapshot::Snapshot` — the GUI analogue of `ontology::emit::spec()`. Where the spec
is "the ontology, for a consumer that has never seen the app," the Snapshot is "the current
state, for a renderer that has never seen ratatui." One pull per frame, no incremental diffing
to start (correctness before cleverness; a diagram-sized `Snapshot` is not going to be a
performance problem at diagram scale).

Contents, roughly:

| Field | Source today | Why the Swift app needs it |
| --- | --- | --- |
| shapes: outline primitives + fill/stroke | `shapes::drawn` + `ui/theme.rs` | drawn as `Path`s in a `Canvas` |
| edges: polyline + notation + label | `shapes` routing + `Relation::notation()` | drawn the same way; `notation()` stays the *only* place a relation's look is decided, same as the rule already says |
| labels, tags, rows | `model::Element`/`Relation` | `Text` overlays |
| cursor, focus, selection, holding, reshape state | `App` fields | highlight rendering and gesture targets |
| mode, cmdline text, status, confirm dialog | `App` fields | native chrome, not ratatui widgets |
| camera, view bounds | `App.camera` | Swift owns pan/zoom presentation; core still owns *where things are* |

Colour is resolved in Rust from `theme.rs` before it crosses the boundary — Swift paints RGBA it
is given, it does not reimplement "what colour is the technology layer." That is the same rule
`DESIGN.md`/`AGENTS.md` already state for the terminal ("colours come from `theme.rs` only");
crossing a language boundary is not a reason to relax it.

**FFI mechanism: [UniFFI](https://mozilla.github.io/uniffi-rs/).** It generates Swift bindings
and an XCFramework from proc-macro-annotated Rust, represents a stateful object (`App`) as a
Swift class with methods, and plain data (`Snapshot`, `Key`) as Swift structs/enums — no
hand-written C header, no `swift-bridge` build-script fragility for a project this size. The
alternative (a hand-rolled `#[no_mangle] extern "C"` layer) is more control and more code to keep
in sync by hand every time a `Cmd` or a model field changes; UniFFI's generation step is exactly
the "generate it, don't write a second copy" principle this codebase already applies to the
manual and the ontology spec.

- **Acceptance:** a minimal round trip — Rust returns a `Snapshot` for an empty diagram, a tiny
  Swift command-line tool (not yet the app) prints it. Proves the toolchain (`cargo build` →
  `.xcframework` → Swift import) before any UI is built on top of it.

### Phase 4 — mouse, as new App behavior

New surface on core's `App`, each piece with a single home, reusing existing state rather than
growing a parallel "mouse mode":

- **Hit-testing:** `Document::element_at(&self, world_pt) -> Option<ElementId>`,
  `handle_at`, `relation_at`. New, added once, in `model.rs` or a sibling module — never
  duplicated later for a context menu or a tooltip that wants the same answer.
- **Click selects; shift-click extends.** Reuses `App.cursor`/`selection`/`visual` — the fields
  the keyboard's visual mode already has. A mouse click is "move the cursor here," not a new
  selection concept.
- **Drag moves.** Mouse-down on a selected element opens a checkpoint (same `checkpoint()` the
  keyboard path calls before an edit); drag deltas move it; mouse-up closes the step. One drag,
  one undo entry — the same invariant the keyboard's every-edit-is-one-undo-step already keeps,
  just opened and closed by mouse events instead of a keypress.
- **Drag from a handle onto another element starts a relation.** Reuses the existing
  "holding" state (`App.holding`, `pending_port`) that `o`/Enter already populate — mouse-up over
  a target invokes the same ontology-ordered relation picker (`relpick`), now presented as a
  popover instead of a modal overlay, but reading the same `ontology::allowed`-ranked list.
- **Marquee select:** new — a drag on empty ground is a rectangle; on mouse-up, every element
  whose bounds intersect it is pushed into `App.selection`, same as `V` + motion does today.
- **Scroll/trackpad pans `camera`.** Reuses the exact field the keyboard's pan commands write.
- **Zoom is Swift-only.** Per Phase 0/2's decision, zoom is a view-scale transform in
  `Canvas`'s draw call, not a core concept — core keeps producing cell coordinates; Swift decides
  how many points a cell is worth this frame. Nothing new in core.
- **Right-click context menu is *generated*, not authored:** it lists `keymap::COMMANDS` entries
  whose `avail(&Where)` at the clicked target is not refused, showing each one's `what` string as
  its label and its `run` strokes as the action to replay. This is the one place a Swift-only
  interaction (right-click) must still resolve through the existing single source of truth for
  "what can happen here" — writing a hand-authored Swift menu would be exactly the "second copy
  of the truth" AGENTS.md warns about, just in a different language.
- **Acceptance:** unit tests in core for each hit-test and gesture method, run the way this
  project already tests everything modal — a "real-path" test that drives `App` through a
  sequence of synthetic mouse events and asserts on `Document`, not a test that pokes internal
  fields directly.

### Phase 5 — SwiftUI skeleton

- SPM package (`VimShapesCore`) wrapping the XCFramework; an Xcode app target depending on it.
- `DocumentStore: ObservableObject` holding the opaque core `App` handle; every mutating call
  (`onKey`, `onMouseDown`, …) re-pulls a `Snapshot` and republishes it — the simplest possible
  correctness model, and consistent with core's own rule that predicates are read-only and
  behaviour goes through checkpointed methods.
- A `Canvas`-based `DiagramView` drawing the `Snapshot`'s shapes and edges as `Path`s, labels as
  `Text`, at a zoom scale it owns.
- Open/save wired to `persistence::load`/`save` through core (unchanged file format, unchanged
  `.json` files — a workspace saved from the Swift app opens in the terminal app and back).
- **Acceptance:** open a file, see it rendered, read-only. No editing yet.

### Phase 6 — keyboard grammar, end to end

- Swift-side adapter: `NSEvent`/`.onKeyPress` → `core::input::Key`, mirroring Phase 2's
  crossterm adapter. Handles macOS's own modifier/repeat semantics; does not need the
  kitty-protocol shift+ctrl workaround, because that workaround exists only for terminals.
- **Acceptance — the anti-drift test this project's testing philosophy asks for by name:** feed
  the same keystroke sequence through the terminal's `on_key` path and through the Swift
  adapter's path into core, assert the resulting `Document`s are byte-identical. This test lives
  in Rust (it can construct both `Key` sequences without needing a real terminal or a real
  Swift runtime) and is the single most important test this whole plan adds — it is what makes
  "keeps the same keyboard grammar" a checked fact instead of a claim.

### Phase 7 — mouse, end to end

Wire Phase 4's gesture methods to SwiftUI's `DragGesture`/`TapGesture`/`MagnificationGesture`/
`.onContinuousHover`. Hover highlights the hit-tested handle or element (a `Snapshot` field, not
new core state — "what's under the cursor" is Phase 4's hit-test, called on hover instead of on
click). Context menu per Phase 4. This phase is almost entirely Swift-side plumbing once Phase 4
is solid; if it turns out not to be, that is a sign Phase 4's methods are shaped wrong for a real
gesture recognizer and worth revisiting before writing more Swift around them.

### Phase 8 — the rest of the chrome

Each ratatui overlay becomes a native view bound to the same state struct, not reimplemented:

| Terminal (`ui/*.rs`) | Swift | Notes |
| --- | --- | --- |
| `palette.rs` | a palette panel/popover | list comes from the ontology's view-narrowed kinds, unchanged |
| `relpick.rs` | a relation-kind popover | ordered by `ontology::allowed`, association last, unchanged |
| `sheet.rs` / `form.rs` | a native inspector panel | `form::apply` stays the only writer; the Swift view only shows fields and sends key-equivalent edits, exactly as `sheet.rs` does today for the TUI |
| `manual.rs` | a native help window | same generated content; the `\|tag\|` link-checking test is untouched since the generator doesn't move |
| `debug.rs` | a native debug sidebar | same fields; the "last key, and what the table made of it" row matters as much here as in the terminal |
| `layers.rs`, `tabpick.rs` | native list panels | |
| `exportdlg.rs`, draw.io export/import | unchanged — pure `Document` operations, called through core exactly as `:export`/`:import` call them today | |
| `start.rs`, `splash.rs` | a native welcome screen | can be simplified rather than ported literally — a splash screen earns its keep differently in a windowed app |

`ui/chrome.rs`'s "no box-drawing character" test and the "panels are a filled ground and a title
strip" rule are TUI-specific and do not bind the Swift app; native chrome uses native idioms.
What *does* still bind it: colours still come from `theme.rs`, the footer/menu text still comes
from `keymap::COMMANDS`, and the manual's content still comes from `manual.rs`.

### Phase 9 — packaging, CI, docs

- SPM + Xcode project checked in; a build script that runs `cargo build --release` for the
  core crate, then `uniffi-bindgen`, then `xcodebuild`, as one command.
- A macOS CI job added to `.github/` alongside the existing `cargo test`/`build`/`clippy`,
  building the Swift app (not necessarily signing/notarizing in CI — that's a distribution
  decision, not a build-correctness one).
- `README.md` gains a short section: two frontends exist, what each is for, where to get each.
  `DESIGN.md` gains the core/shell split as an architecture fact once Phase 1 lands — it is a
  real decision with real reasoning, the same as every other row in `DESIGN.md`'s stack table.
- Code signing/notarization and a distribution channel (direct download vs. the existing
  Homebrew tap vs. the Mac App Store) are a separate decision with its own tradeoffs
  (sandboxing rules, review time, cost) — flagged in open questions, not decided here.

## 5. Cross-cutting: what must not fork

A short list of the exact invariants from `AGENTS.md` that a second frontend puts under most
pressure, and where each one is kept:

- **The ontology is the single source of truth.** The Swift app never lists a kind, a relation,
  or a rule from anywhere but a `Snapshot`/FFI call into core. No Swift-side copy of the palette
  or the picker's ordering.
- **The rules stay advisory.** A refused relation is drawn and marked in the Swift app exactly
  as in the terminal — the mark is a `Snapshot` field, not a Swift `if` that blocks the drag.
- **Reasons are the product.** The `&'static str` reasons from `allowed` cross the FFI boundary
  as plain strings and are shown verbatim — not summarized, not re-worded for "a nicer UI."
- **The command table is the only description of what a key (or now, a click) does.** Phase 4's
  context menu rule is this invariant's mouse-shaped corollary.
- **A file format change carries a `VERSION` note.** Nothing in this plan changes the file
  format; if Decision 0 in Section 2 is ever revisited, this is the rule that decision has to
  answer to.
- **Tests are sentences, in three flavors** — anti-drift (Phase 6's keyboard-parity test is one),
  real-path (Phase 4's gesture tests, driven through `App` methods, not internal fields), and
  invariant (any new table — e.g. a Swift-side gesture-to-`Cmd` mapping, if one ever exists —
  gets the same "nothing describes a thing that doesn't exist" test the ontology and the keymap
  already get).

## 6. Left out, and why

- **Windows/Linux GUI.** SwiftUI is macOS (and iOS/iPadOS, unexplored here). The core-crate
  split is what makes a second native shell affordable later; it is not this plan's job to build
  one speculatively.
- **A second PNG/export renderer that draws vector shapes.** `render.rs` exists to make one true
  claim — the terminal and the file agree — and a "prettier" pixel-resolution export would break
  that claim for the *existing* export, not add a new one. If the Swift app wants its own
  export, it is a new, separately-named export path, not a change to `render.rs`.
- **Real-time collaboration / multi-user editing.** Nothing about this plan touches persistence
  concurrency; out of scope until it's asked for.
- **Free-form (sub-cell) placement.** Section 2's Decision 0.
- **A general cross-platform GUI toolkit (e.g. egui, Tauri) instead of SwiftUI.** The user asked
  for SwiftUI specifically — presumably for native macOS feel and Xcode tooling — so this plan
  doesn't second-guess that, but the core-crate/FFI boundary in Phases 1–4 is toolkit-agnostic
  and would serve an egui or Tauri frontend just as well if that ever changes.

## 7. Open questions for the user

1. **Decision 0 (Section 2):** keep the cell grid, render it richly? Recommended: yes — free
   placement is a materially larger, format-forking project and nothing in "rendered visual
   presentation" strictly requires it.
2. **Does the Swift app's window state (last file, window size, sidebar visibility) belong in
   the shared `config.json`, or a separate macOS-native store (`UserDefaults`)?** Recommended:
   `UserDefaults` — `config.json` is explicitly "the reader's preferences," shared with the
   terminal app, and window geometry is not a preference the terminal app has any use for.
3. **Distribution:** direct-download `.app`, the existing Homebrew tap, or the Mac App Store?
   Each has different signing/sandboxing consequences for how deep the FFI/file-access story in
   Phase 9 needs to go. Recommended: direct download first (fastest path to a testable app),
   revisit once Phase 8 is real.
4. **Scope of "same keyboard grammar":** should the Swift app also support the `:`-command line
   verbatim (a command bar, still typed), or only the modal keystrokes? Recommended: keep it —
   `excmd::COMMANDS` costs nothing extra to wire up once Phase 2/6 exist, and power users
   migrating from the terminal will want it.
