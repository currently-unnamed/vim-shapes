# AGENTS.md

Notes for agents working on vim-shapes. `README.md` is the user's guide and `DESIGN.md` is the architecture; this is what to hold in your head so that changes land as *this* project rather than as a generic Rust TUI.

---

## 1. This is a modelling tool, not a drawing program

A box here is a **kind of thing** and a line is a **kind of relationship**, and the whole value of the tool is that the pairing means something. Almost every mistake worth catching in this repo is a mistake about the ontology wearing the clothes of a UI problem.

- **The ontology is the single source of truth.** `ontology::allowed` is the grammar; `ElementKind::ALL` and `RelationKind::ALL` are the vocabulary. The palette, the picker, `:lint`, the header's `⚠` count, the manual's generated pages and `--ontology` all read them. Never write a second copy of any of it — not in the manual, not in a doc, not in a test fixture. If you need the rules in words, generate them.
- **The rules are advisory and must stay so.** The app draws what it is asked and *marks* what the rules refuse. Do not add a refusal to `Document::connect` or the picker; add a reason to `allowed`. A tool that will not let you draw the wrong line is a tool people stop using.
- **Reasons are the product.** Every `Err` from `allowed` is a `&'static str` someone reads at the moment it can change their mind. It must say what the relation is for and what to do instead — "a service is realized, not assigned — the component behind it realizes it" — never "not allowed". A test refuses reasons shorter than a sentence.
- **Association is always allowed and always last.** That is what makes the picker's default the most specific line the rules permit. Do not reorder `RelationKind::ALL` without knowing that.

## 2. The hard constraints

1. **Nothing may describe a kind that does not exist.** Add a kind to the enum and to `ALL`, and the tests demand its name, slug, short tag, layer, category, shape, tagline and summary. Forget one and the build says which.
2. **Every idiom builds and lints clean.** An idiom that teaches a refused relation fails `every_idiom_builds_and_lints_clean`. If you change a rule and an idiom breaks, the rule is probably wrong — idioms are the diagrams architects actually draw.
3. **`--ontology` is byte-stable.** Every object is a `serde_json::Map` (sorted); never enable `preserve_order`. The spec is a prompt-cache prefix and a key in a different order is a silent miss.
4. **The command table is the only description of what keys do.** New key → new `Cmd` in `keymap::COMMANDS`, with a `what` for every context, an `avail` with a reason, and a `run` if the menu can replay it. The dispatcher in `ui/mod.rs` must not act on a key the table does not gate. Two tests hold this: no two commands live on one key in one place, and the footer stays under six.
5. **The manual's links resolve.** `|tag|` must name a `*tag*` somewhere; a test walks every page. Tags are single tokens.
6. **A file format change carries a `VERSION` note** in `model.rs`'s doc comment and keeps older files opening: `#[serde(default)]` on anything new.

## 3. How this codebase is written

### Tables describe; `mod.rs` acts

`keymap::COMMANDS` says what a key can do and when; `excmd::COMMANDS` says what a typed word names; the ontology says what a relation may join. Their predicates are read-only on purpose — a predicate that could reach into the `App` could also change it, and "what can I press right now" must never do anything. Behaviour lives in `App::on_key` and `App::run_excmd`, after a `checkpoint`, so every edit is one undo step.

### A rule has exactly one home

`Document::members` is where "what is inside a grouping" is decided (by position). `App::movable` is where "what does a move apply to" is decided. `Document::lint` is the only place a relation is checked against the rules, and it calls `allowed`. When you find the same decision made twice, that is a bug that has not happened yet: delete one, don't keep them in step.

### Comments say *why*

Doc comments carry the argument, not the mechanics. Constants explain their number. When you fix something subtle, leave a note that says what went wrong and why the obvious version was wrong; when you are about to change code that has one, read it first.

### Tests are sentences

`a_refused_kind_is_still_drawn_and_marked`, `dirtiness_is_a_comparison_so_undo_makes_it_clean_again`. Drive the app the way fingers do (`press(&mut app, "vl ")`) rather than calling handlers — a test that built its own state and called the handler would pass while the key did nothing. Three kinds worth writing deliberately: **anti-drift** tests between two halves that can disagree, **real-path** tests through `on_key`, and **invariant** tests on the tables.

### Chrome

No box-drawing character in `chrome.rs` — a test says so. Panels are a filled ground and a title strip. Colours come from `theme.rs` only; a layer's colour is the same everywhere it appears. The footer shows what is live and stays short; the menu is the cheatsheet.

## 4. Working here

```
cargo test       # must be green; ~90 tests, well under a second
cargo build      # must be ZERO warnings
cargo clippy     # should be quiet
```

A change is not finished until:

- the tests pass and there are no warnings — both, every time;
- **the manual is updated** (`ui/manual.rs`) for anything a user would look up. `:help` is how features are found;
- **new keys are in the keymap table**, new `:` commands in `excmd::COMMANDS` with a `what`;
- **a new kind or relation has its prose** — the tests will insist;
- the source stays hand-formatted: don't run `cargo fmt`.

To eyeball a screen without a terminal: `cargo test eyeball -- --ignored --nocapture` renders a diagram, the menu, the palette and the picker to text.

In the app, `:debug` opens a panel of live state down the right-hand side — mode, cursor, focused relation and its ports, reshape state, tab, camera, undo depth, the last keys and what the command table made of each. When a user reports a key that "does nothing", the `last key` row is the first thing to ask for: it says whether the table refused it, and why.

## 5. Things that look like bugs and are not

- **Properties are fields in `ui/form.rs`, each with a tab and a unit; `form::apply` is the only writer.** The sheet (`ui/sheet.rs`) only shows them and takes keys. A new property is one field and one arm of `apply`; never write the document from the sheet.
- **The start dialog only appears when no file was given.** `main` sets `loading = false` and opens the file; the dialog is armed by the first key on the title screen when `path` is `None`.
- **The current tab's diagram is `App.doc`, not `App.tabs[tab].doc`.** Other tabs are parked in slots; the current slot's `doc` is an empty placeholder until you switch away. `App::workspace()` assembles the whole thing.

- **A refused relation is still drawn.** That is the design; the mark and `:lint` are the enforcement.
- **A grouping has no member list.** Membership is by position. Dragging in is joining.
- **`:add actor` works on a technology view.** The view narrows the *palette*, never the command; `:lint` lists the element as outside the view.
- **Assignment to a service is refused.** A service is realized, and the difference is the seam between layers.
- **`?` is refused inside the manual.** The manual owns the keyboard; `q` closes it first.
- **`Relation::notation()` is the only place a relation's look is decided.** A plain link carries its own; every other kind's is the kind's. Read it; never read `kind.notation()` directly in a renderer.
- **The PNG renderer draws cells, not shapes.** That is the design: the file is the terminal's picture. Resist redrawing at pixel resolution.
- **Labels are escaped twice in the draw.io export.** Once for HTML (the cell is `html=1`) and once for the XML attribute; draw.io unwraps both.
