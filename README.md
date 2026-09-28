# vim-shapes

Architecture diagrams, driven like vim: one key per action, everything on the keyboard, the whole diagram drawn right there in the terminal, in line art by default. You put down actors, processes, services, components and nodes, join them with relations that *mean* something, and the tool tells you when a line says something an architect would not — without leaving the terminal.

This README is the user's guide: how to build and run it, how to get help while you're inside it, and a tour of the workflow. `DESIGN.md` is the architecture and the reasoning; `AGENTS.md` is for anyone (or anything) changing the code.

---

## Prerequisites

- **A truecolor terminal.** Every modern one qualifies: Ghostty, Kitty, WezTerm, Alacritty, iTerm2, Windows Terminal. Switching to `:ink braille` additionally wants a font with braille, which most already have.

Nothing else. No accounts, no network — and no Rust toolchain either, if you install a release below rather than building from source.

---

## Installing a release

macOS (Homebrew):

```sh
brew install currently-unnamed/tap/vim-shapes
brew upgrade vim-shapes   # later, to pick up a new release
```

macOS or Linux, without Homebrew:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/currently-unnamed/vim-shapes/releases/latest/download/vim-shapes-installer.sh | sh
```

**Windows**, in PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/currently-unnamed/vim-shapes/releases/latest/download/vim-shapes-installer.ps1 | iex"
```

That puts `vim-shapes.exe` on your `PATH` and prints how to run it. Windows Terminal is the terminal to run it in — it has truecolor and braille by default; the legacy `cmd.exe`/PowerShell console host does not draw either reliably. Without PowerShell, download `vim-shapes-x86_64-pc-windows-msvc.zip` from the [releases page](https://github.com/currently-unnamed/vim-shapes/releases/latest), extract it anywhere, and either run `vim-shapes.exe` from that folder or add the folder to your `PATH` yourself.

Every platform's binary, and the formula and installer scripts above, are built fresh by CI from a pushed version tag — see `dist-workspace.toml`.

---

## Build & run from source

- **Rust** — a recent stable toolchain (the crate is on the 2024 edition, so **Rust 1.85 or newer**). Install from <https://rustup.rs>.

```sh
cargo build --release
cargo run --release                          # the title screen, then a dialog: a new diagram, or open one
cargo run --release -- diagrams/example.json # opens a file
cargo install --path .                       # puts `vim-shapes` on your PATH
```

A few headless modes print and exit:

```sh
vim-shapes --check diagram.json   # lint a file: every relation the rules refuse, every element outside the view
vim-shapes --ontology             # the whole ontology as JSON — layers, kinds, relations, rules, idioms
vim-shapes --render d.json out.png [--tab N] [--px 20] [--font <path>]   # one tab as a PNG
vim-shapes --import-coarchi <src> <dst>   # a coArchi model repository, written straight to a workbench folder
```

`cargo test` runs the suite. The source is hand-formatted; please don't run `cargo fmt` over it.

---

## Getting help while you're in the app

You are never more than one key from help:

- **`a` / `:add`** — the **palette**: a search over the kinds of shape the view offers, with a braille picture of the picked kind, at its own proportions and in its layer's colour, beside what it is.
- **`?`** — the **command menu**. Every command, with the ones you can use *right here* in colour and the rest dimmed with the reason they're out of reach. This is the most useful key to learn: it always answers "what can I do from where I'm standing?", and pressing a command's key inside the menu runs it.
- **`:help`** (or `:h`) — the **manual**. `:help <topic>` jumps to a page: `:help relations`, `:help layers`, `:help component`, `:help serving`. Every kind of shape and every kind of relation has a page, generated from the ontology. Follow `|links|` with `Tab` and `Enter`; `^o` goes back; `q` closes.
- **The footer** — the bottom line shows the few keys that act on whatever the cursor is on *right now*. It stays short on purpose; `?` is the full list.
- **`:`** — the command line. `Tab` completes command names, kinds of shapes, view kinds, idiom names and file paths. It is reachable from anywhere — every mode, every panel, the manual, the start dialog — except while a label is being typed, where the colon is text and `Esc` is one key away.

If you get lost: **`Esc`** backs out of whatever you're in, one step at a time.

If something looks wrong: **`:debug`** opens a panel down the right-hand side with the app's state, live — the mode, what the cursor is on, the tab, the camera, the last keys pressed and what the command table made of each. `:debug` again closes it. Include what it says in a bug report.

---

## The ink

The diagram is drawn in one of two inks. **Lines** is the default: box-drawing line art — `┌─┐`, the arc corners `╭╮╰╯`, the diagonals `╱╲`, heavy, double and dashed rules — the characters every terminal draws itself, so a line is a line and a screenshare can follow it. Boxes, stadiums (an ellipse), cylinders with a rim, scalloped clouds and diamonds are stencils exact to the cell; every other shape is its outline's edges in the glyph each slope asks for. A line that meets a box becomes a tee, an orthogonal route turns on a rounded elbow, and the ends are `▶ ▷ ◆ ● ┤ ⟩` and their kin. **Braille** is the other ink: dots at four times the resolution, curves that curve, and the look the tool started with. `:ink lines` or `:ink braille` switches, and the choice is kept in the config file beside the theme. The PNG rendering follows the ink; the clean picture never was braille.

## Light and dark

The app has two palettes, gruvbox dark and gruvbox light, and everything on screen is drawn from one of them: the roles (text, hints, a panel's ground, the cursor's tint) and the accents (the cursor's yellow, a refusal's red, each layer's colour, the ten named paints). A file that says `aqua` looks like aqua on either ground; a hex is itself. The mode is chosen at start — `--light` or `--dark`, else the config file, else the `COLORFGBG` hint most terminals export, else dark — and `:theme dark | light | auto` switches it and keeps the choice. The export dialog opens in the mode's appearance; the draw.io file is always paper, so a named colour goes out in its light value. `:debug` names the mode and where it came from.

## The screen

- **The header** (top) — the file name (with `+` when there's unsaved work), the tabs, the diagram's kind, the element and relation counts, a red `⚠ n` if any relation is one the rules refuse, and the layer of the element under the cursor.
- **The diagram** — shapes in line art by default (`:ink braille` for the terminal's own dot resolution instead), coloured by layer, over faint dots that mark the canvas (`:grid` toggles them; `:diagram` sets their spacing and colour, the ground, the page and the diagram-wide looks). An architecture shape is filled by default, in its layer's own pastel — a plain sketch shape stays the blank box it always was, unless you give it a fill of its own. The cursor's element is yellow.
- **The footer** (bottom) — the keys live for the thing under the cursor; the command line; questions; messages.

There is no free cursor. The cursor is always **on an element**, and `h j k l` hop to the nearest element in that direction. A count hops further: `3l`. The view follows the cursor, moving as little as it can.

---

## The first minute

```
:add            the palette — type what you mean (a "server" finds Node), enter adds it
Customer        type the label, then esc
o               a NEW shape off this one, already related: pick its kind in the palette,
                then (on an architecture tab) the kind of relation, then type its label
:w flow.json    save
```

That `o` is how most of a diagram gets drawn: stand on a shape, open the next one off it. `^h ^j ^k ^l` do the same in a direction, `^y ^u ^b ^n` at a corner, and the new shape lands that way, joined handle to facing handle. Two shapes that already exist are joined by hand: `Enter` takes hold of a relation, `h j k l` carries it to the other shape, `Enter` drops it and the picker asks which kind.

---

## Tabs: two kinds of diagram

Each tab is a diagram of its own, with its own cursor, view and undo; the file holds them all, the way a draw.io file holds pages. A tab is one of two kinds, chosen when it is made:

- **freeform** — plain shapes, like a whiteboard. No ontology, nothing refused. The standard general-purpose diagram.
- **architecture** — the enterprise ontology below, with `:kind` to narrow it to one view.

`^t` makes a new tab: it asks which kind, then opens the command line ready for the name (`Enter` names it, `Esc` keeps "diagram N"). `:tabnew freeform notes` does the same in one line. `gt` / `gT` move between tabs, `:tab N` jumps, `:tabs` lists, `:tabrename <name>` renames, `:tabclose` closes (asking if the diagram is unsaved). `:tabnew <path>` takes a file instead of a kind — a tab exported with `:export <file.diagram>` — and adds it as a new tab without touching any tab already open; a file holding more than one tab is refused rather than partly applied (`:open` it instead).

---

## Rendering

`V` previews the current tab: a PNG in a scratch file, opened with whatever opens pictures on your machine, no questions asked. It is the *clean* picture, the way a desktop diagram tool draws one: white paper, a ruled grid, solid outlines, filled arrowheads, sans-serif labels. `:export` opens the export dialog — PNG, SVG, PDF, XML (draw.io), HTML and diagram, in either the clean style or the terminal's own style — whichever ink `:ink` is set to — with zoom, size, a transparent or light ground, border and grid — and `:export file.svg` writes the format the extension names at the defaults. The diagram format is not a picture: it is this tab alone, losslessly, and `:tabnew <path>` reads it back as a new tab, on this session or another. `:render out.png` writes the current tab as a picture. It is not a redrawing: the cells, exactly as the terminal shows them, whatever ink they're in, are rasterised through a real monospace font, so what is on screen and what is in the file are one rendering. Most monospace fonts have no braille, so a small stack of fonts is tried per glyph, the way a terminal falls back — line art, the default, has no such gap. `\` presents the diagram alone on screen, framed the way the rendering is.

---

## Elements

**Add** with `:add` (the palette, a search) or `:add <kind>` to skip it: `:add process`, `:add component`, `:add node`. Kinds are completed on `Tab` and resolve on a unique prefix. Every kind has a full name (`Business Process`), a slug you type (`process`) and a short tag drawn inside its box.

Every shape is of a **kind**, and the kinds come in **layers** — motivation, strategy, business, application, technology, implementation — plus two composites, grouping and location. `:help layers` lists them all with a line each; `:help component` says what one is for and which relations it can take.

**Basic shapes** are a layer of their own, the general palette any diagramming tool has: `box`, `rounded-box`, `square`, `circle`, `ellipse`, `triangle`, `diamond`, `parallelogram`, `trapezoid`, `hexagon`, `cylinder`, `cloud`, `text` (a label with no outline), and the flowchart and misc shapes — `predefined-process`, `document`, `internal-storage`, `cube`, `step`, `tape`, `note`, `card`, `callout`, `stick-figure`, `data-storage`, `delay`, `display`, `manual-input`, `off-page`, `block-arrow`, `double-arrow`, `and`, `or`. Each is drawn in the diagram's own ink (line art by default) and exported to draw.io as its own shape. They mean whatever you write in them, the rules leave them alone (a line between two is an association, an arrow is a flow), and they are offered in every view — for the parts of a picture the ontology has no word for.

**Add** a shape on its own with `a` (or `:add`): the palette, and the new shape lands by the cursor, unconnected. **Label** with `t` — text (esc or enter when done). The text tab has the label's look: `bold`, `italic`, `underline` (the terminal's own, and the family's own faces in the pictures where it has them, faked where it does not), `text colour`, a `label band` under it for a label on a busy line or the grid, `wrap` (off cuts it to one line with an ellipsis), `label width` in cells (blank is the shape's inside, wider spills outside), `padding` in cells kept clear of the edge, `font`, `size`, `align` and `valign`. **Place the label** with `T`: `hjkl` drag it anywhere, outside the shape too, `HJKL` four cells at a time, `0` puts it back, `esc` leaves it; the same on any of a relation's three labels, and every export honours it. **Configure** with `c`, on a shape or a relation, always: a panel of every property, each in its own unit — the label as text, the kind as a kind of shape, x, y, width and height as whole cells, and how the label is set: `align` (left / centre / right) and `valign` (top / middle / bottom), which the screen honours, and `font` (any family on this machine, cycled or typed by prefix) and `size` (px), which `:render` honours, since a terminal has one font; on the style tab, `look` (the eight fill-and-line pairs every diagram tool offers: paper, grey, blue, green, orange, yellow, red, purple), `fill` (`auto` for the layer's own pastel — on screen too, for anything but a plain sketch shape, so an architecture diagram reads filled by default — `none`, or any colour), `outline` (off leaves the fill and the label), `colour` for the line (a palette name or any hex, or blank for the layer's own; `Enter` on any colour field opens the picker: the palette, recent colours, and a swatch grid), `line` (solid, dashed, dotted), `stroke` (1 to 3 dots), `opacity` (10 to 100 %) and `ink` (`auto` — the document's own `:ink` — or `lines` / `braille` for this shape alone; never the relation reaching it, its ports, or its cursor tint, which stay the document's). On screen a fill of your own tints the inside and opacity fades the shape toward the ground; the exports fill, dash and blend for real. `j`/`k` pick a field, `i` steps in, type, `esc` leaves it, and the diagram behind changes at once; `h`/`l` cycle a kind; `t` goes straight to the text. Each field left is one undo step. **Move** with `H J K L` (a count moves further), or four cells at a time with `^H ^J ^K ^L` (shift+ctrl); relations follow, at their ports. Terminals that speak the kitty keyboard protocol (kitty, WezTerm, Ghostty, iTerm2, Alacritty) tell shift+ctrl from ctrl; in one that does not (Terminal.app), `^h ^j ^k ^l` on the diagram are the move too, and linked shapes come from `o`, the corners `^y ^u ^b ^n`, or `i` then a side. `:debug` says which kind you have. **Lean** with `<` `>` (sideways) and `{` `}` (up and down), a cell each way, a count further, inside the shape too: a rectangle becomes a parallelogram, and any shape leans the same way; `skew x` and `skew y` on the arrange tab set them in cells. The draw.io file gets a parallelogram for a sideways lean. **Resize** with `- = _ +`, or **step into** the shape with `i`: a handle appears on each side and corner of its box. A handle is a *port*, where a relation attaches, and it is also what you drag. `h j k l` walk the handles; `Enter` on an open one takes hold, then `h j k l` drag it a cell and `^h ^j ^k ^l` (or `H J K L`) drag it four, `Esc` lets go. On a handle with a relation attached (a ring), `Enter` picks up that end: walk to an open handle and `Enter` places it, `Esc` puts it back; `x` disconnects it. `^h ^j ^k ^l` on an open handle open a **linked shape** on that side, `^y ^u ^b ^n` at that corner, and `o` out of whichever handle you stand on; the add dialog then shows a compass, and the direction can still be changed there with the same chords, or `Tab` onto the compass and `h j k l` / `y u b n`. `Esc` steps back out. **Delete** with `d` — it asks first, on the footer, naming what goes — and it is a **cut**, not gone for good: it fills the same register `y` does, so `p` right after brings it straight back. **Copy / paste** with `y` / `p`. **Undo / redo** with `u` / `^r`; every edit is one step.

A **grouping** or **location** is a dashed box. Whatever is drawn inside it moves with it — membership is by position, so dragging an element in is how it joins.

**Pan** the view without moving the cursor: `zh zj zk zl` a step (a count: further), `zH zJ zK zL` half a screen, shift+arrows the same, or `zv` for a view mode where `hjkl` pan freely until `esc`. The view stays where you put it until the cursor next moves; `zz` brings it back. **Find** things with `f` (every element wears a letter; press one), `/` (search labels; `n`/`N` step), `gg`/`G` (first / last, reading order), `zz` (centre the view).

---

## Relations

The quickest way to draw is `o`: on a shape, it opens a new shape off it, already related. The palette asks the kind of shape (the ones that can take a specific relation from here lit, the rest dimmed), then on an architecture tab the picker asks the kind of relation, and then you are typing the new label. On a freeform tab there is no picker: a plain link is drawn.

Two shapes that already exist are joined in two moves. `Enter` on an element takes hold of one; `h j k l` (or `f`) carries it to another element — the line follows, dashed; `Enter` drops it. `Esc` lets go.

Dropping opens the **picker**: every relation kind, the ones the rules allow between *these two* elements first and lit, the rest dimmed with the reason. The first lit row is the most specific line the rules permit, so `Enter` twice draws the best line. A dimmed row can still be chosen — the tool never refuses a line. It is drawn, marked `⚠`, and `:lint` lists it.

A relation is a stop on the walk like any shape: `h j k l` land on it, at the middle of its line, and carry on from it. On one:

| key | does |
|---|---|
| `Enter` or `r` | change its kind |
| `x` | remove it |
| `t` | label it |
| `c` | the sheet: its kind and look, its three labels, its ports, and `reverse` |
| `Tab` | its next node: tail, centre, head — each with its own label |
| `gd` | go to the element at this end (`^o` comes back) |
| `Esc` | back to the element |

`c` on a relation opens the sheet on it: its kind and its look — a `look` preset for the line colour, `route` (orthogonal by default — it leaves along the longer axis, turns, crosses and turns in, with `elbow` on the arrange tab setting where, in per cent along that first leg from the tail — so the turn stays proportionally put as the two shapes move; or straight; or curved), line (solid, dashed, dotted), width (1 to 3 dots), either end (arrows, triangles, diamonds, dots, and the ERD crow's foot, bar and ring), `end size` (small, normal, large), colour, and `opacity`; its three labels, with their own `font`, `size`, `bold`, `italic`, `text colour` and `label band`, and `label at`, where along the line the centre label sits in per cent; and, on the arrange tab, the port at each end and `reverse`, which swaps the ends of a line drawn the wrong way round. A kind gives the look its default, and a setting made here becomes the relation's own. An orthogonal route with no `elbow` of its own picks one that clears any third shape sitting between its ends, rather than always the plain midpoint — bend it by hand once (`i` on the relation grabs its one turn, `h j k l` pulls it) and that choice is yours to keep; the app never nudges it again.

The kinds — composition, aggregation, assignment, realization, serving, access, influence, triggering, flow, specialization, association, and the plain link — are described in `:help relation-kinds`, and each has its own page. The one lesson worth knowing first: **the layer above never touches a component directly.** It uses a *service*, which the component *realizes*. `:help rules` has the rest.

---

## Views, layouts, lint, idioms

- **`:kind <view>`** says what sort of diagram this is — `free` (the default, everything), `layered`, `motivation`, `business`, `application`, `technology`, `implementation`. The palette then offers only the layers that belong, which is the difference between a menu of choices and a menu of possibilities.
- **`:layout`** arranges everything by layer — motivation on top, the core stack beneath it, implementation under that — with each element pulled toward what it relates to in the row above. **`:layout flow`** columns by how far downstream things sit along their relations, for a process or a data flow. Either is one undo step.
- **`:lint`** lists every relation the rules refuse and jumps to the first. The header's `⚠ n` is the same count, always.
- **`:idiom <name>`** stamps a worked shape into the diagram — `service`, `stack`, `motivation`, `process`, `data`, `deployment`, `migration`. Every idiom is built and linted by the test suite, so what it teaches is a shape the rules pass. `:help idioms` explains each one.

---

## The diagram itself

`:diagram` (or `c` on an empty diagram) puts the sheet on the diagram and holds it there. **Style**: `rounded` gives every box corners; `sketch` draws every outline by an unsteady hand, deterministically, on screen and in every export; `shadow` puts a shade under each shape in the exports; `background` and `grid colour` set the two grounds. **Text**: the title and the view. **Arrange**: the grid on or off, `grid style` (`auto` is dots on screen and ruled on paper; `dots` or `lines` say one way everywhere), `grid size` in cells (the density, on screen and in every picture), `page view` (the page's dashed edge on the canvas, and exports framed to the page rather than to the shapes), `paper` (US Letter, Legal, Tabloid, Executive, A0 to A7, B4, B5, 16:9, 16:10, 4:3, or `WxH` in cells), `orientation`, and a `page width` and `page height` that make it custom. Every colour field in the app is the same picker: ten palette names or any hex, typed, cycled, or chosen from the swatch grid with `Enter`.

## The ontology layer

The architecture has an **ontology layer**: a data platform's ontology, in Palantir Foundry's own words, drawn beside the business it twins and the applications that read it. `:kind ontology` narrows the palette to that layer with business and application; the free view offers it with everything else. Its kinds: **object types** with their properties, **link types** whose ends are the cardinality (a bar for `one`, a crow's foot for `many`; one-to-many by default, crow to crow for many-to-many; the centre label is the link's name and the end labels the API name on each side), **interfaces** that object types `implement` and other interfaces `extend`, **action types** drawn as nodes of their own and joined by their verbs (`creates`, `modifies`, `deletes`, `links`, `unlinks`, `calls` a function), **shared properties**, **value types**, **datasources** (`backed-by`) and **object type groups**. Between two ontology types the architecture's relations are refused, and the ontology's are refused elsewhere, each with the reason. Where the two meet, three bridges: an object type **realizes** the business object it is the twin of (and a datasource realizes a data object), the application layer **accesses** object types, and an action or a function **serves** the process that invokes it.

An object type is its properties, so it is drawn as a box with a header and a compartment of rows: a mark, the name, the base type — `⚿ code  string` for the primary key, `✎` for the title, `✱` for a shared property, `[]` for an array, `‹Email›` for a value type, `*` for a required parameter. `P` (or `:props`) on an object type, an interface or an action type opens the **property browser**, docked above the sheet: `n` new, `r` rename, `t` cycles the base type and `T` types one, `p` primary key, `l` title, `s` shared, `[` array, `*` required, `v` value type, `a` API name, `J`/`K` reorder, `d` delete. The box grows to hold its rows. The sheet adds `api name`, `plural`, `status` (experimental draws dashed, deprecated fades with a `†`) and `visibility`.

Two exports pay for the view: `:export ontology.md` writes the diagram as a reference page — object types with property tables, link types with cardinality, interfaces with implementers, actions with parameters and rules — and `:export ontology.json` writes a plain definition in the SDK's casing (`apiName`, `primaryKey`, `baseType`). Draw.io gets real UML class cells. `:lint` and `--check` add the schema's own checks: no primary key, two of them, a title that is not a string, an unnamed link type, an interface nothing implements, an action with no rule.

## Grounding the ontology: the common core and BFO

Merging data sources into Foundry from different application ontologies is the everyday case two more layers answer. **Common core** holds the Common Core Ontologies' mid-level classes (Agent, Person, Organization, Artifact, Act, Event, Information Content Entity, Geospatial Region, Facility); **upper ontology** holds the Basic Formal Ontology's top-level backbone (Continuant, Occurrent, and what each divides into), used unchanged from BFO 2020. `:kind alignment` narrows the palette to both, with the ontology layer they ground. The one relation, **subsumed-by** ("is a kind of"), joins a class to its one real parent — inside the common core, inside the upper ontology, or from the common core down into BFO — and, at the one seam, an object type or interface to the common-core class it is classified under. An object type is never subsumed straight into the upper ontology: the common core is the seam, the way a business process never touches a component straight, only through a service. Two object types from different sources — "Customer" from a CRM source, "Client" from a billing source — that both subsume to the same common-core class are what makes them interoperable: a query joining them is now a claim the model licenses, not a coincidence of naming. `:idiom cross-domain-alignment` stamps the worked example.

## Layers

Every shape and relation sits on a layer, and the layers are a stack. Most tools hide that behind "to front" and "to back"; here `:layers` opens a browser like a picture editor's: the stack top first, each layer shown or hidden (`space`), locked or not (`l`), with what sits on it counted, the current one (where new things go, `Enter`) marked. `n` makes a new layer, `r` renames, `J`/`K` reorder, `d` deletes (its things drop to the layer beneath), and `m` moves the cursor's shape or relation, or the picked set, onto the selected layer. A hidden layer cannot be stood on; a locked one refuses every change with the reason. Within a layer, the sheet's Arrange tab has to front, to back, bring forward, send backward, the layer itself, a per-item lock, and snap to grid. None of it is written to the file until a second layer exists.

---

## Groupings

A grouping is a box drawn round other shapes, and membership is *where things sit*: a shape belongs to a grouping when its centre is inside the box. There is nothing to join and nothing stored — drag a shape in and it has joined, drag it out and it has left. Moving a grouping moves everything inside it. It draws open and dashed so what is inside stays legible.

`g` on a picked set wraps it: a box round their bounds, behind them, and the line opens to name it. `gp` goes up to the grouping the cursor's shape is inside, `gu` dissolves the one under the cursor — the box goes and what was inside stays where it is. In an ontology, a box round object types is an object type group.

## Picking several elements

`v` starts picking; `space` picks the element under the cursor (or puts it back); `h j k l` move between elements as usual. Then `H J K L` move the picked set together (`^H ^J ^K ^L` four cells; relations to shapes left behind follow their moved end), `- =` and `_ +` stretch it narrower or wider and shorter or taller (places and sizes scale together, as when a desktop tool's group is dragged by a corner; a count stretches further), `y` copies it with the relations among it, and `dd` deletes it (`d` asks, and a second `d` is the yes). `c` opens the sheet on the set: only what the shapes share, with `mixed` where they differ, and a value set there is set on all of them at once — one undo step. Its arrange tab ends with the set's own actions: align left / centre / right / top / middle / bottom and match width / height, all to the first picked shape (the one the cursor was on at `v`), and distribute across / down, which keep the two ends and space the rest evenly. `Esc` stops picking.

---

## Files

| command | does |
|---|---|
| `:w [path]` | save — to the file opened, or the path given |
| `:o <path>` | open (asks if there is unsaved work; `:o!` doesn't) |
| `:n` | start an empty diagram |
| `:q` / `:q!` | quit / quit without asking |
| `ZZ` / `ZQ` | `:wq` and `:q!`, as vim chords |
| `:export` | the export dialog: png, svg, pdf, xml (draw.io), html, diagram — format, file, zoom, size, transparent, dark/light, border, grid |
| `:export <file.svg>` | the format the extension names, at the defaults |
| `:export <file.diagram>` | this tab alone, losslessly — `:tabnew <path>` reads it back as a new tab |
| `V` | preview: the tab as a PNG, opened at once |
| `:title <name>` | name the diagram |
| `:ink` | `lines` (box drawing, the default) or `braille` — how the diagram is drawn; kept in the config file |
| `:theme` | `dark`, `light` or `auto` — the palette, kept in `~/.config/vim-shapes/config.json`; bare, says which is on and why. `--light` / `--dark` on the command line for one run |
| `:grid` | toggle the canvas dots (`:grid on` / `:grid off`); the diagram's own setting, saved with it |
| `:props` | the property browser on the cursor's object type, interface or action type (`P`) |
| `:diagram` | the sheet on the diagram: rounded, sketch, shadow, background, grid, paper (`:dia`, `:page`) |
| `:render <out.png> [px] [font]` | this tab as a PNG: the cells, whatever ink they're in, drawn through a monospace font |
| `\` | present — the diagram alone, as `:render` writes it; `esc` comes back |

The file holds every tab. It is JSON, pretty-printed, naming kinds by slug (`"kind": "business_process"`), so it diffs and can be edited by hand. A file that names an element that isn't there is refused whole rather than opened half-broken.

Unsaved work is a **comparison** against what was last saved, not a flag: undo back to the saved state and the diagram is clean again.

---

## Feedback

- The in-app manual (`:help`) is the source of truth for anything this README skims.
- Diagrams are plain JSON (`:w`), so a surprising one is easy to attach to a bug report; `vim-shapes --check` on it says what the tool thinks.
- `cargo test` should pass on a clean checkout with zero warnings from `cargo build`.

## License

MIT — see [LICENSE](LICENSE).
