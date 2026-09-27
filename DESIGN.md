# vim-shapes — design

A terminal diagramming tool for enterprise and solution architecture, driven like vim, with an ontology under it. This document is the architecture and the reasoning: what was decided, and why the obvious alternative was wrong.

## Stack

| Concern | Choice | Why |
| --- | --- | --- |
| Language | Rust | A single binary, no runtime, and the type system holds the ontology's invariants. |
| TUI | `ratatui` 0.30 | The `Canvas` widget with braille markers draws real curves — an ellipse is an ellipse, not a box of `─`. |
| Terminal | `crossterm` 0.29 | Ships with ratatui; raw mode and the alternate screen and nothing else needed. |
| Files | `serde` + `serde_json` | Human-readable, diffable, hand-editable diagrams. |

No other dependencies. There is nothing to configure and nothing that can fail at runtime that is not a file.

## The idea, in one paragraph

Every diagramming tool lets you draw a box and a line. What makes a drawing a *model* is that the boxes are kinds of thing and the lines are kinds of relationship, and that some pairings mean something while others are nonsense. So this tool starts from the **ontology** — the layers, the kinds of shapes, the kinds of relations, and one function that says which relation may join which pair — and everything else is a way of instantiating it: the palette lists it, the relation picker reads it, `:lint` checks against it, the manual is generated from it, and `--ontology` emits it for a model that has never seen the app. The TUI is vim's grammar applied to that: motions, an operator or two, a `:` line, and a single command table that the menu, the footer and the dispatcher all read.

## Layout

```
src/
  main.rs            args, terminal init, the loop, teardown
  model.rs           Document: elements, relations, metadata. What undo clones and the file is.
  persistence.rs     save / load / validate
  drawio_export.rs   :export — a draw.io file with the notation mapped
  shapes.rs          outlines as braille primitives; where a relation leaves a box
  layout.rs          :layout — by layer, or by flow
  ontology/
    mod.rs           layers, kinds, relations, views, the rules, and the rules in words
    idiom.rs         worked shapes, built and linted by the tests
    emit.rs          --ontology (JSON, byte-stable) and --check
    tests.rs         the anti-drift tests
  ui/
    mod.rs           App: state, on_key, draw, run_excmd — the one place anything happens
    keymap.rs        every command, once: keys, what they do, and when
    help.rs          the ? menu, rendered from keymap
    excmd.rs         the : vocabulary, and Tab-completion by argument shape
    cmdline.rs       the : line's editing state (history, recall, cycling)
    wildmenu.rs      the completion popup
    palette.rs       :add — a search over kinds, grouped by layer, narrowed by view
    relpick.rs       the relation-kind picker
    manual.rs        :help — prose pages plus generated pages
    canvas.rs        the diagram: braille outlines and relations, text over the top
    chrome.rs        the app's own look: panel, hint, marker
    theme.rs         the accent palette
    splash.rs        the title screen
```

## The ontology

### Layers, categories, kinds of shapes

An element lives on one **layer** — motivation, strategy, business, application, technology, implementation, or the composite layer that boxes them — and is one **category**: active structure (something that acts), behaviour (something that happens), passive structure (something acted upon), or the motivation, implementation and composite categories that stand aside. The layer says where; the category says what for; and the rules turn on the pairing.

The vocabulary follows the common layered enterprise-architecture modelling convention, because that is what an architect already reads, but the names are kept plain — `process`, `component`, `node` — so a diagram made here is legible to someone who has never used the language it borrows from. Forty-three kinds. Each has a full name, a slug you type, a short tag drawn inside its box, a shape, a layer, a category, a one-line tagline and a paragraph. **A kind with any of those missing does not build**: the tests walk `ElementKind::ALL` and refuse silence.

### Shapes carry the distinction

A terminal cannot draw an icon in the corner of a box, so the *outline* has to do the work: a rounded box is behaviour, a square one is structure, an ellipse is an actor, a diamond is an event, a cylinder is data, a cloud is a network, a dashed box is a grouping. The short tag is printed inside as well, so nothing rests on the reader knowing the code. Everything is drawn from points and straight segments through one braille canvas, and nothing is a box-drawing glyph that would refuse to meet a curve at the corner.

### The rules are one function, and they are advisory

`ontology::allowed(relation, src, dst) -> Result<(), &'static str>` is the entire grammar. Eleven kinds of relation, fifty-two kinds of shape, and one function that says whether a pairing means something — composition stays on one layer and one category, realization points up the stack, serving points up the stack, assignment is structure to the behaviour it performs, access points at data, triggering and flow stay on one layer, influence lands on a reason, specialization needs the same kind, association is always allowed.

**The app never refuses a line.** A diagram is a sketch before it is a model, and a tool that will not let you draw the wrong line is a tool you stop using. Instead: the picker offers the allowed kinds first and dims the rest *with the reason beside them*; a refused relation is drawn red and marked; `:lint` lists every one; the header counts them. The rules teach rather than police. The reasons are `&'static str` so that the picker, the status line and `--check` all print the same words for the same refusal — one string, three places.

Two consequences worth knowing. Association is always allowed and always sorted *last*, so the picker's default is the most specific line the rules permit — `Enter` twice draws the best line, never the vaguest. And a service is *realized*, never *assigned*: the difference is the seam between layers, and the rule that enforces it is what makes "component realizes application service" the picker's first offer between those two kinds.

### Views narrow the palette

`:kind` says what sort of diagram this is. A palette that lists forty-three kinds on a technology diagram is a menu of *choices*; one that lists nodes, networks, software and the components they host is a menu of *possibilities*. A view is the app's way of knowing the difference. It narrows the palette and nothing else — `:add <kind>` still adds anything, and `:lint` lists elements outside the view rather than refusing them.

### Idioms are answers, not merely legal shapes

Seven small diagrams every architect draws over and over — the service seam, the full stack, a motivation chain, a process, data across the layers, a deployment, a migration — each a worked example of how the layers meet. `:idiom` stamps one in. **Every idiom is built and linted by the tests**: it becomes a real document, laid out, and the rules must pass every relation in it. An idiom that quietly taught a forbidden line would be a lesson in the wrong direction, and the build refuses it.

### Emitted, not written

`vim-shapes --ontology` walks the same tables the palette and the picker read and prints them as JSON, with the rules *tabulated*: for each relation, which sources may take it to which targets. A hand-written spec is a second copy of the truth, and a second copy of the truth is a lie with a delay on it. Every object is a sorted map and a test asserts the output is the same bytes twice, because the spec is meant to be the cached prefix of a prompt and a prompt cache matches on exact bytes.

## Starting

Opened with no file, the app assumes a new diagram and asks the two questions a new one needs answered, or offers to open an old one. One panel, always the same size — a dialog that changed shape with the window would be a different dialog every time — in two halves: the choice on the left, and on the right whatever that choice needs. `Tab` walks the halves and, within the new-diagram form, the name and then the kind. The two kinds are drawn as small braille pictures rather than named, because the difference between a whiteboard and a layered stack is a picture. Opening walks the working directory like any open dialog: up first, then folders, then diagram files, then the rest. `^Enter` confirms from anywhere once everything is valid, and because some terminals deliver it as a bare `Enter`, `Enter` on the last thing in each half confirms as well. A new diagram is named and placed but not written until `:w`, which is vim's own bargain. `Esc` goes on with an unnamed diagram; a file on the command line skips the dialog entirely.

## The cursor is inverse

The shape under the cursor has its inside tinted and its label drawn black on yellow; the focused node of a link is drawn the same way. An outline in a different colour was the first design and it was too subtle to find on a busy diagram — a cursor has to be the one thing on the screen you cannot miss. The tint is the cursor's own yellow, dimmed, so the two read as one thing.

## The ontology layer

The architecture has a layer for a data platform's ontology — Palantir Foundry's, in its own words, so a diagram reads to its engineers without a legend: object types, link types, interfaces, action types, shared properties, value types, functions, datasources, object type groups. It is a layer (`Layer::Ontology`) rather than a kind of diagram, because an ontology is *part of* an architecture: the digital twin sits beside the business it models and the applications that read it, and a picture of one without the other is half a picture. Within the layer its relations have their own grammar (`allowed_ontology`); between two ontology types the architecture's relations are refused and the ontology's are refused elsewhere, each with a reason that names the other vocabulary. Where the layers meet there are three bridges, and only three: an object type realizes the business object it is the twin of (a datasource realizes a data object), the application layer accesses object types as it accesses data, and an action or a function serves the process that invokes it. `:kind ontology` narrows the palette to the layer and those two neighbours. The notation borrows what reads: UML-style compartment boxes for object types and interfaces (Graffoo, OWLGrEd, Chowlk), crow's-foot ends for cardinality (entity-relationship — which is exactly Foundry's one-or-many per side), dashed outlines for the abstract and a status shade for the experimental and deprecated (VOWL), and actions as nodes of their own joined by their verbs (Foundry's own ontology graph). VOWL's circles were left out: an object type's properties are the point, and a circle has no room for rows.

An object type is its properties, so an `Element` carries rows — `properties`, one `Property` each with a base type from Foundry's list, marks for the primary key, the title, shared, array, required, a value type — and the box has a header of three rows (tag, name, rule) and a compartment beneath, grown to fit. The same rows are an interface's properties and an action's parameters. They are edited in a property browser modelled on the layer browser, docked above the sheet, one key per thing a row can be, every change an undo step; the sheet keeps the type's own metadata (API name, plural, status, visibility). Every surface draws the compartment from `Element::row_lines`, and draw.io gets a real UML class — a swimlane with a stacked child cell per row — since that is what its own palette makes.

The view exists for documentation, so two exports come with it: the diagram as a Markdown reference page and as a plain JSON definition in the SDK's casing, both generated from the picture so they cannot drift from it, and the schema's own checks (`ontology::doc::check`) in `:lint` and `--check`. Left out on purpose: objects and object sets (instances — a freeform box beside the diagram is the honest example), datasource column mappings, permissions, function bodies and criteria logic (text a box can name, not draw), and Foundry's exact definition format, which is not a public authoring format.

## Grounding the ontology: the common core and BFO

Merging data sources into Foundry from different application ontologies is routine, and nothing in the ontology layer said whether two independently-built domain types meant the same thing. Two more layers answer that: `Layer::CommonCore`, the Common Core Ontologies' mid-level classes (Agent, Person, Organization, Artifact, Act, Event, Information Content Entity, Geospatial Region, Facility), and `Layer::UpperOntology`, the Basic Formal Ontology's top-level backbone (Continuant, Occurrent, and what each divides into), used unchanged from BFO 2020 (ISO/IEC 21838-2) the way the ontology layer uses Foundry's own words unchanged. A curated core rather than either standard in full — CCO alone runs to hundreds of classes across eleven modules — extensible later the same way any kind is: an enum variant, its prose, and the tests that demand both.

One new relation carries the whole thing: `SubsumedBy`, "is a kind of" — deliberately not a repurposing of `Specialization`, whose existing rule (`src == dst`, same-kind only) is a separate invariant with its own tests, and the ontology layer had already set the precedent of giving a domain its own relation rather than overloading that one (`Implements`/`Extends`, not specialization, for the same reason). Its grammar (`allowed_alignment`) is a hard-coded table, `upper_parent`, giving each of the curated classes its one real published parent, so a wrong edge is refused by name: "a role is a kind of realizable entity, not a disposition." The one seam is deliberately narrow: an object type or interface may be subsumed by any common-core class (the tool cannot know whether "Customer" is really a `Person` or an `Organization` — that is the modeller's judgement, exactly as `Implements`/`Extends` never check whether a specific interface is really the right parent), but never straight into the upper ontology — the common core is grounded in BFO already, the way a business process never touches a component straight, only through a service. `:kind alignment` narrows the palette to both new layers and the ontology layer they ground. Two object types from different sources subsumed by the same common-core class are the payoff: `:idiom cross-domain-alignment` draws it, and is what makes a join between them a claim the model licenses rather than a coincidence of naming.

## Tabs, and the two kinds of diagram

A file is a **workspace**: a list of tabs, each a diagram of its own, plus the two things that belong to the workspace rather than to any diagram — which tab was open, and whether the grid is shown. It is the draw.io model of pages, and it is what makes a set of related diagrams (a motivation view, an application view, a deployment) one thing to open and save. A version-2 file, one bare diagram from before there were tabs, still opens: as a workspace of one tab named after the file.

A tab is one of two kinds, and the kind is chosen when it is made because it shapes everything after. **Freeform** offers plain shapes only and the rules never have anything to say — the whiteboard a general-purpose tool gives you. **Architecture** offers the ontology, and `:kind` narrows it to one view. In the code the two are simply `View::Freeform` against the architecture views, which is what lets every surface that already read the view — the palette, the header badge, `:lint` — handle both without a special case.

In the app the current tab's state lives in `App`'s own fields and every other tab is parked in a slot: its diagram, cursor, camera, undo and jumplist. Switching is a swap. That keeps every method that says `self.doc` meaning "the diagram you are looking at", which is the reading that a hundred call sites already had.

`^t` asks the kind with a two-row picker, makes the tab, and then opens the `:` line with `tabrename ` already typed — the name is the next thing anyone wants to type, so the line is ready for it. Closing a tab asks when its diagram is not on disk; the last tab cannot be closed, because a workspace with no diagram is not a workspace.

## Links: the plain line, and the three nodes

A relation has three **nodes** — the tail, the centre, the head — and each can carry a label of its own: a cardinality or a role at the ends, the name in the middle. On a relation the cursor is on one of them; `h`/`l` walk along the link, `t` labels the node, `gd` goes to the element at that end. The model stores the three labels as three optional fields, absent from the file when empty, so nothing saved before them changes.

**A plain link** (`RelationKind::Link`) is the line a freeform diagram joins its shapes with: always allowed, meaningless to the rules, and drawn however it is configured. Its look — the line solid, dashed or dotted; either end nothing, an arrow, an open arrow, a triangle, a diamond, a hollow diamond, a dot, or an entity-relationship crow's foot, bar or ring — is stored on the relation as a `Notation` of its own. Every other kind's look is the kind's, and setting the kind clears any look of its own; only a link keeps one. `Relation::notation()` is the one place that choice is made, and the canvas, the export and the manual all read it.

**`o` opens a related shape** off the cursor's in one move: the palette (kinds that can take a specific relation from here lit, the rest dimmed), then on an architecture tab the relation picker, then the new shape's label. On a freeform tab there is no picker: a plain link is drawn and the label is typed at once. The same two tables — the palette's rows and the picker's — that `:add` and `Enter` already read are all it needs; the flow is the composition, not a new mechanism.

## Export, and the preview

`V` is the preview: the tab as a PNG in a scratch file, opened with whatever opens pictures on the machine, nothing asked — the diagram sets the size, each shape its font. `:export` is the dialog any diagram tool has: format, file, zoom, the width and height that zoom gives (typeable, and typing one sets the zoom), a transparent ground, dark or light, border, grid. `:export file.svg` skips it and writes the format the extension names, the way `:w path` skips a question.

Two styles. **Clean** is what `V` previews and what a desktop diagram tool draws: white paper, a ruled grid, solid outlines with a paper fill (the convex hull of the outline, so the grid stops at the edge; a cloud is its five bumps filled and only their outer arcs stroked), filled arrowheads, dashes as a real dash pattern, sans-serif labels. It has its own small vector rasteriser — antialiased strokes by distance, scanline polygons — because a preview that needs no viewer but the one the machine already has was worth two hundred lines. **Terminal** is the braille picture, the terminal's own, kept for those who want the file to be what the screen showed.

Five formats, two kinds. PNG is the raster below, the one export that is exactly the picture on screen. SVG, PDF and HTML are **vector**: one `Picture` of lines, dots and text is built from the same functions the canvas draws with — `shapes::outline`, `shapes::relation`, `canvas::label_lines` — and three small writers serialise it. The relation geometry was pulled out of the canvas's paint calls into pure data for exactly this: one geometry, so five files and one screen cannot disagree about where a line is. The PDF writer is a hundred lines of hand-written PDF with Courier, no compression and no embedded fonts, because a file any reader opens from a writer small enough to read is worth more than a dependency. XML is the draw.io file, for editing on.

`Format::Diagram` is not a picture at all: it is `serde_json::to_string_pretty` on the `Document` itself, the same shape `persistence::save` writes one tab as. Every other format loses something on the way out — a link's own look, a shape's paint, all of it — on purpose, since a picture is not obliged to be the model. This one is obliged to be, so `:tabnew <path>` can read it back as a new tab, on this session or another, exactly as it left. `persistence::load` already accepted a bare `Document` with no `"tabs"` key (naming the tab after the file's stem), which is what let this be an insertion path (`tabnew_from_path`, additive, no dirty-check needed) rather than new parsing.

## Rendering

`:render <out.png>` writes a tab as a picture, and `--render` does it headlessly. It is not a redrawing at pixel resolution — that would be a nicer picture and a *different* picture from the one on screen, breaking the only claim the export makes, that the thing in the terminal and the thing in the file are one rendering. Instead the diagram is laid out as cells, framed by its bounds with a margin and no chrome, and every cell's glyph is rasterised through a real monospace font at a cell size the font's own metrics decide: the advance across, ascent plus descent plus line gap down.

Braille is the catch. Most monospace fonts have no braille block and a terminal quietly falls back to one that does; so does the renderer, with a small stack of fonts tried per glyph. `\` presents the diagram alone on screen, framed the same way, so what you see is what will be written.

## The sheet

The properties of whatever the cursor is on live in a **sheet docked down the right**, in three tabs — style, text, arrange — with different fields for a shape and for a link. It is a pane and not a modal because it is about the *selection*: it follows the cursor while it is up, and has two focus states. `c` opens it and gives it the keyboard; `Esc` hands the keyboard back to the diagram and leaves the sheet showing whatever you walk to; `c` re-enters; `q` closes. The tab and the field you were on are kept across retargets where the next item has them — a shape and a link both have a `colour` and a `label` — so a pass over many links is `l`, `l`, `l`. On a terminal narrower than a hundred columns there is no room for a dock, so the sheet floats over the diagram, and only while it has the keys.

A shape can lean: `<` and `>` shear it sideways a cell at a time, `{` and `}` up and down, on the diagram or from inside it, and `skew x` and `skew y` on the arrange tab set the leans in cells. The shear is applied in `shapes::drawn`, where every surface gets an outline, with the top edge moved half the skew left and the bottom half right (and the left edge half up, the right half down) so the shape's centre stays on its box; the box itself, its handles and its ports do not lean, which keeps a relation attached where it was. The draw.io file gets a parallelogram for a rectangle leaned sideways, flipped for a left lean; a lean up or down, and any other shape's lean, show only in the pictures, since the desktop tool has no shear for them.

A link has a route: straight, orthogonal or curved, decided once in `shapes::route` as the points the line passes through, and every surface draws the notation along those points — legs in the line's style, the ends turned to the first and last legs, node marks and labels placed by length along the route rather than along the chord. Orthogonal leaves along the longer axis as the eye sees it (a cell is twice as tall as wide), and `elbow` says where the first leg turns; the draw.io file gets that turn as a waypoint. End sizes scale the heads in braille and on paper alike.

A picked set stretches: in visual mode the resize keys scale the set's bounding box by a few cells, and every picked shape's place and size scale with it from the box's top-left, which is what dragging a corner of a desktop tool's group selection does. It stops before the box would fold below one shape's own minimum.

A picked set arranges: its arrange tab ends with align, distribute and match, actions only a set has, to the first picked shape — which is the one the cursor was on when picking began — so "align to the cursor" needs no second notion of a cursor inside the form.

A picked set is a target too. In visual mode `c` puts the sheet on every picked shape at once: the fields they all have, by name and unit, with one value where they agree and `mixed` where they do not, and a value set there is written to each of them — checked on every shape first, so a value one refuses changes none, and a locked shape is passed over. The set is the intersection of the shapes' fields rather than a shape's fields, so it stays true the day two kinds of shape stop sharing one.

The diagram is a target too. `:diagram` pins the sheet on it, and `c` lands there when the cursor is on nothing: its looks (rounded, sketch, shadow), its grounds, its title and view, its grid and its page. The three looks apply to everything on the diagram, and they are decided in one place each — `Page::shape` turns a rectangle's corners, `shapes::drawn` sketches an outline from a seed that is the element's id, so a shape wobbles the same way every frame and in every export, and `Element::casts_shadow` says what has a body. The canvas and both picture builders call those, so the screen, the PNG and the draw.io file agree without three copies of the decision.

A label's look — bold, italic, underline, colour, a band, wrap, width, padding — is one `TextStyle`, and a relation carries one for its three labels. On screen bold, italic and underline are the terminal's own; in the pictures the family's bold and italic files are found by the font catalogue from the file suffixes it already reads, and where a family lacks a face it is faked (bold drawn twice a pixel apart, italic sheared a row at a time) rather than silently regular. The PDF uses the four faces of Helvetica and Courier that every reader has. The band is a step off the ground, so a label reads over a line or the grid without a colour of its own.

A shape's style is fill, outline, line colour, line pattern, stroke and opacity, with a `look` over the top: one of the eight fill-and-line pairs every desktop tool offers, read back from the pair it set and `custom` otherwise, so it is never a second source of truth. `auto` fill is the load-bearing default — nothing on screen, the layer's pastel on paper and in the draw.io file, so an architecture diagram exports in the colours architects already read, and the paper itself on a dark ground where a pastel would glare. Opacity has no alpha anywhere: every colour is moved toward the ground by how see-through the shape is, which on a flat ground is what an alpha would show, and what the terminal can do. Dashes are a length pair on a picture line, so a dotted shape and a dotted link are one thing in every writer.

The fields are declared once, in `form.rs`, each with its tab and its unit, and `form::apply` is the only thing that writes one; the sheet is a reader. That is what keeps undo, the file, `:lint` and every export seeing one change made one way — and it is why adding a property is one field in one table.

Left out of the sheet on purpose, and why: gradient, glass and comic looks (decoration with no screen effect and no meaning); line jumps (an artefact of a raster picture with crossings — the orthogonal route and the port choice are the honest fixes); rotation and flip (a rotated label cannot be shown in a terminal, and a shape without its label is not the shape); formatted text, writing direction and automatic font size (HTML in a label, which the file would carry and the screen could never show); per-edge label padding (folded into one `padding`); a draw.io edge-label position (its geometry is a child cell there; the centre label stays centred); and, from the desktop Diagram tab, connection arrows, connection points and guides (mouse aids with nothing to do when the cursor is a shape).

## Layers

Every shape and relation sits on a layer, and the layers are a stack drawn back to front; within a layer, document order is drawing order, and the sheet's Arrange tab moves a thing about in it. Most diagram tools have layers and show you none of them — "to front" and "to back" and that is all — so `:layers` is a browser like a picture editor's: the stack top first, each layer shown or hidden, unlocked or locked, what sits on it counted, the one new things go on marked. A hidden layer's things are not drawn and cannot be stood on, and a relation to a hidden shape is hidden with it, because a line to nowhere is a lie. A locked layer's things refuse every change, with the reason, and a single thing can be locked on its own; the lock is enforced in `form::apply` and in the few gestures that move things without it, so there is no third door. Nothing of it reaches the file until a second layer exists: a diagram with the one layer is byte-identical to one from before layers were a thing.

## Handles are ports

Inside a shape (`i`) the eight handles do two jobs, and the second is the one that matters: a handle is a **port**, where a relation attaches. A relation may be anchored to a port at either end (`from_port`, `to_port`, absent by default); unanchored, it leaves wherever the edge faces the other element, and the handle it shows on is simply the nearest one to that point. `Document::end_points` is the one place the two cases meet, and the canvas, the walk and the renderer all read it.

From a handle, three things: `x` disconnects what is attached there; `Enter` on a patched handle picks up that end and `Enter` on an open one puts it down, with `Esc` returning it where it was, so a line patched the wrong way is one move to fix; and a linked shape opens *out of* a handle — `^hjkl` for the sides, `^yubn` for the corners, `o` for the one you stand on. The new shape lands beside that side or off that corner, further along if something is already there, and the relation is anchored handle to facing handle, so the picture keeps its direction however the shapes are later moved.

The diagonals are the rogue-like's `y u b n`, because a keyboard has no diagonal arrows and those four letters have meant up-left, up-right, down-left and down-right on a terminal for forty years. `o` on a corner handle is the same gesture said the other way round: the handle you are standing on already *is* a direction.

## The palette

Ten named colours — red, orange, yellow, green, aqua, blue, purple, sand, white, grey — that a shape's outline or a link's line can be painted in from its configuration. Named rather than hex, because a colour on a dark terminal is meant to *mean* something, a warning, a path, a group, and not to match a brand; and the values are the theme's own accents, so a painted shape sits in the same picture as an unpainted one. Blank is the default: the layer's colour for a shape, grey for a link. The rules never look at a colour. The draw.io export keeps them as stroke colours.

A link's look — line, width in braille dots, both ends, colour — can be set on **every** relation, not only the plain link. A kind gives the look its default; any setting made becomes the relation's own full `Notation`, so a kind change resetting it is one field going away and not five. Width is drawn as extra strokes a dot to either side in the aspect-corrected space, so a thick line is thick the same amount whichever way it runs.

## Typography

A shape's label has a `TextStyle`: alignment across and down, a font family, a size. The two halves are honoured in the two places they can be. **Alignment is honoured on screen**: `canvas::label_lines` places the wrapped lines by it, and the renderer reads the same function so a label lands in the same rows in the file. **Font and size are honoured in the rendering**: a terminal has one font and one size, so the panel shows them, the file carries them, and `:render` sets each label in its own face, re-measuring the run in pixels for its alignment because a different size changes how wide the text really is. The renderer therefore lays the cells out *without* the labels and draws them itself afterwards, which is the one deliberate departure from "the file is the terminal's picture", made for the one property a terminal cannot show.

The font catalogue (`fonts.rs`) is scanned from the folders a terminal finds its fonts in, by directory listing alone: the file's stem, weight suffix dropped, is the family's name, and the regular face is the file. A name typed into the panel resolves by exact match, then unique prefix, then unique substring, and an ambiguous one is refused with the candidates named. The style stays out of the file while it is the default.

## The grid, and the page

Faint dots every fourth column and every second row — square-ish in cells that are twice as tall as they are wide — so an empty diagram still reads as a surface with a position on it. The diagram can say how it is marked (`grid style`): `auto` is each surface's own way — dots on the screen, where a rule would fight the braille, and ruled on paper, the way a desktop tool draws it — while `dots` and `lines` say one way for every surface. Ruled on screen is box-drawing along every grid row and column with a cross where they meet, in the grid's colour; ruled on paper is a line every quarter step and a heavier one at the step, the way graph paper is; dots on paper is a dot at each step. `grid size` is the density of any of them. They are aligned to the world, so they scroll with the picture, and they are drawn first so anything real overwrites them. `:grid` toggles; the spacing and the colour are the diagram's, on its sheet. The grid is per diagram, not per workspace, because it is part of how a diagram is meant to be read; a file from before that had one switch for every tab, and it is honoured on each tab when opened.

A diagram can be laid out on paper: `page view` draws the page's edge, dashed and dim, from the origin, and every export is then framed to the page instead of to the shapes' bounds — which is how a page prints. Paper sizes are kept in the pixels of a 100 % export, rounded to whole cells (a cell is ten pixels wide and twenty tall), so a page is a whole number of rows and within a millimetre or two of the real sheet. A custom size is stored in cells.

## The wireframe ink

Braille reads badly on a screenshare — a line is a string of pinpricks with the ground showing through — so the screen has two inks and line art is the default. The wireframe (`ui/wire.rs`) does not rasterise curves: every cell holds *connectivity*, which of its eight neighbours it is joined to and how heavily, and one resolver turns that into the character with those arms — the box-drawing block is exactly that table, with the arc corners for a rounded two-arm cell, the diagonals for the slanted, and the dashed and dotted runs kept only where a cell is a run. Shapes and lines merge into the same cells, so a line ending on a box edge becomes a tee and two lines crossing a cross, which is asciiflow's and Monodraw's model and why their output looks drawn. The curved shapes are stencils: an ellipse is a stadium whose rows come from its own equation so it closes and is the same every frame, a cylinder is a rounded box with a second rule for the lid's rim, a cloud is scallops with `╰╯` valleys, a diamond `╱╲` over runs. Everything else is its outline's edges, each walked cell by cell in the glyph its slope asks for — a run, a post, or a slash with the run on the side the line enters from — and a cell keeps the first diagonal it is given, so two edges meeting at a vertex share it rather than crossing in it. Orthogonal routes leave a shape squarely from the edge they start on, since a route along an edge is invisible in braille and a mess in lines. The ink is the reader's preference, kept in the config file beside the theme; the PNG rendering follows it, and the clean picture never was braille. Not done: the Symbols for Legacy Computing wedges and diagonal segments for slopes other than 45°, which only some terminals draw — `PLAN.md` keeps that phase.

## Light and dark

Every colour on screen is one of two things. A *role* is what a thing is — ink, a dim hint, a panel's ground, the cursor's tint — and a *accent* has a meaning that must survive a change of ground — the cursor's yellow, a refusal's red, a layer, a named paint. `theme.rs` holds one `Theme` struct of both, in two instances, gruvbox dark and light, behind one accessor that reads a mode atomic; nothing else in the interface names a colour by value, and a test greps `src/ui` for bare greys so a new panel cannot regress. The mode comes from a flag, the config file (`~/.config/vim-shapes/config.json`, the app's first preference, kept apart from the diagram because it is the reader's), the terminal's `COLORFGBG` hint, or dark; `:theme` switches and keeps it. There is no OSC query: the reply arrives among the keys and a handful of terminals answer wrongly, and a variable plus a one-word command covers every case. Named paints have two value tables in the ontology, dark and light, so the screen, a dark picture and a light one each take the value for their ground while the file keeps the name; the draw.io file is paper and takes the light one. A contrast test holds every role to 3:1 on both grounds, and the cursor's inverse label to 3:1 on every accent.

## Colour

A colour anywhere is one type, `Colour`: a palette name, or a hex. It is written to the file as the string the sheet shows — `"purple"`, `"#e6e6e6"` — so a file reads the way the sheet does, and the palette's ten names carry the terminal theme's own accents, whose hexes are the same in every export. One picker serves every colour field: names, recent picks, and a generated grid of twelve hues by eight lightnesses under a row of greys, laid out the way every colour picker is, so the hand knows where teal is. The picker writes nothing itself; what it picks goes back through `form::apply`, the same write typing the name would make.

## The document

`Document` is elements, relations and a little metadata (a title, a view). Positions are world cells, `y` growing downward the way rows do. Elements have ids; relations name two of them. Removing an element removes every relation on it, because a line to nowhere is a file the loader would rightly refuse.

**A grouping's members are derived, not stored.** A composite *is* the box, so what is in it is what is drawn in it: nothing to keep in sync, nothing that can go stale when something is dragged out. Moving the box moves what is inside; dragging an element in is how it joins.

**Loading treats the file as untrusted.** Every relation must name two elements that exist; every box must be possible; a newer format is refused. A file that does not check out is refused whole rather than opened half-broken.

**Dirtiness is a comparison** against the serialized last-saved document, not a flag set on every edit — so undoing back to the saved state correctly reports clean, and quitting asks only when there is really something to lose. Quitting is the one action nothing downstream can catch, so it is the one action that interrupts; `:q!` and `ZQ` skip the question, `:o` and `:n` ask the same question for the same reason.

## The control surface

### Navigation — modal, vim-flavoured

There is no free cursor. The cursor is always **on an element**, and `hjkl` hops to the nearest element in that direction: strictly on that side, minimizing distance along the axis with a penalty off it. A count hops further. A composite is measured from its label, up on its top edge — its centre would coincide with whatever is drawn inside it, and the box would be impossible to hop to or away from.

`Tab` steps the cursor *onto* the element's relations, one at a time, in a stable order (outgoing, then incoming). Everything that acts on a relation — re-kind, remove, label, follow — is gated on standing on one, and the footer changes to say so.

`gd` on a relation goes to its other end; `^o` retraces and `^i` walks forward, the jumplist vim has. `f` puts a letter on every element and jumps to the one you press. `/` searches labels; `n`/`N` step. The camera follows the cursor, moving as little as it can, so travelling along a row nudges the view rather than recentring on every keypress; `zz` recentres on purpose. A large sheet has regions the cursor is not in, so the view can also be panned on its own: `zh zj zk zl`, `zH zJ zK zL`, shift+arrows, and `zv` for a mode where `hjkl` pan until `esc`. A pan marks the view as placed on purpose, and the camera stops following the cursor until the cursor next moves — otherwise the first draw after a pan would snap straight back.

A label can be dragged: `T` enters a mode where `hjkl` move the cursor's label anywhere, the outside of its shape included, and the offset is kept on the text style (or, for a relation, per node), applied in the one function every surface places labels with, so the screen and every export agree. The whole drag is one undo step.

### Shift+ctrl, and the terminal

`^H ^J ^K ^L` (shift+ctrl) move a shape four cells, and `^h ^j ^k ^l` (ctrl) open a linked shape on that side. A plain terminal cannot tell the two apart: shift+ctrl+h and ctrl+h are one byte, and ctrl+h is Backspace. So `main` asks for the kitty keyboard protocol where the terminal has it, and the app knows whether it got it. With it, the chords are what the table says. Without it, on the diagram the ambiguous chord is the **move** — the foundational control — and the linked shape keeps `o`, the corner chords and the reshape mode's sides; the `?` menu says so, and `:debug` names the kind of terminal. Inside a shape nothing changes, because there a ctrl chord already means one thing whether or not shift is down.

### Drawing a relation is two moves

`Enter` on an element takes hold of a relation; travel to another element with the same keys you always use; `Enter` drops it. The line follows the cursor, dashed, so the intent is visible the whole way, and `Esc` lets go without a trace. Dropping opens the picker — see the rules above — and landing on the new relation from the element it was drawn to means `x` undoes a mistake with one more key.

### The command table is the single source of truth

`?` opens a menu of every command, and what it shows depends on where you are standing: on a relation you can re-kind and remove; on an element you can relate and delete. That context is real, so the menu has to model it rather than print a fixed cheatsheet.

The temptation is to write that list next to the dispatcher. Then there are two descriptions of the same rules, and the day one changes is the day the help starts lying. So `ui/keymap.rs` holds **one table**, and everything reads it:

- the **menu** lists it, dimming what you cannot do *and saying why*;
- the **footer** is generated from it;
- **`on_key` refuses from it**. A command the table says is unavailable never reaches its handler, and the reason the footer prints is the same `&'static str` the menu dimmed it with.

**A command is keyed by its keystroke, not by its meaning.** `Enter` means three things depending on what the cursor is on — start a relation, drop one, re-kind one — so both its description and its availability are *functions* of where you are. A test asserts no two commands are ever available on the same key in the same place, because that invariant is what makes the lookup unambiguous.

**The footer is not a cheatsheet — the menu is.** Only two kinds of command earn a place along the bottom: `?` (which leads everywhere else) and the ones that act directly on whatever the cursor is on. A test caps the footer at six commands anywhere you can stand.

**The menu runs commands by replaying keystrokes into the dispatcher**, rather than calling handlers itself. There stays exactly one path through which anything happens.

**Bare keys are grammar; everything else is `:`.** Motions, relating, labelling, moving, deleting, undo, picking — the things vim itself would leave bare. Adding, arranging, checking, file I/O, the manual — `:add`, `:layout`, `:lint`, `:kind`, `:idiom`, `:w`, `:help` — live on the `:` line, exactly the way vim keeps `:write` and `:set` off the letters that make up its motions. `ZZ`/`ZQ` are vim's own precedent for the one exception.

### The `:` line

`ui/excmd.rs` is the vocabulary — a flat table of `name`, `aliases`, `op`, `arg`, matched by exact alias first and then by unique prefix of the canonical name (vim's own rule; `:h` is registered as help's alias, not inferred). `ui/cmdline.rs` is the line's editing state, born fresh every time `:` opens; history lives on `App` because it has to outlive the line. What each command **does** lives in `App::run_excmd` — deliberately the opposite shape from `keymap::Cmd`, whose predicates are read-only on purpose so that "what can I press here" can never do anything.

**Tab-completion knows the shape of each command's argument**: `:add` completes kinds, `:kind` completes views, `:idiom` completes idioms, `:help` completes tags, `:w` and `:o` walk the filesystem the way a shell does. The wildmenu opens upward from the line, sized to its own content.

### The palette is a search

`:add` with no name opens it, and every letter thereafter is a query, so the arrows — not `j`/`k` — move the selection. It matches on what a kind *is* as well as its name, because the thing you cannot remember is never the term: typing `server` finds Node, `database` finds Data Object. Rows are grouped by layer and narrowed by the view. Choosing one adds the element beside the cursor and starts its label straight away, because a label is the next thing anyone types.

### Chrome

The rule is not "no borders". It is *the content defines its own edge, and chrome that repeats what the content already says is deleted*. The diagram owns the body and needs no edge — the terminal is the edge. A panel floating over it does need one, or it reads as text that has landed on other text; what it does not need is a drawn rectangle. A filled ground and a title strip separate it more clearly than a line does, and they cost one row instead of two. A `▸` marks the focused row, so rows do not shift sideways when the cursor arrives. There is no box-drawing character in `chrome.rs`, and a test says so.

The palette is a dark palette of a few accents — one for the cursor, one per layer, one for a refusal — and nothing reaches for a colour outside it. The cursor's element is yellow everywhere; a refused relation is red everywhere; a layer's colour is the same on the canvas, in the palette's headings and in the header badge.

### The manual

`:help` is the product, not documentation about it. The prose pages are authored; the layer, element and relation pages are **generated** from the ontology, including, for every kind, the relations it can start and receive — so `:help component` can never describe a component the rules do not have. A test asserts every `|link|` resolves to a real tag. The manual is a surface of its own, with a reader's keys.

## Layouts

**By layer**: rows in layer order, and within a row each element pulled toward the mean position of what it relates to in the row above — one barycentre pass, because the row above is always placed first. It is the picture an architect expects.

**By flow**: columns by rank, the *longest* path to each element from anything that nothing feeds — longest, not shortest, because an element must sit to the right of everything that leads to it. Relations may form cycles (a flow both ways is legal), so the relaxation is bounded rather than run to a fixed point: a merely odd layout instead of a hang.

Both work in world cells and change nothing; the app applies them as one undo step.

## Export

`:export` writes a draw.io file. The mapping is deliberate rather than pixel-faithful: each kind keeps its shape, each layer takes the fill colour architects already read it in, and each relation takes the line and arrowheads of its notation. Cells become ten by twenty pixels, roughly a terminal cell's aspect, so the picture keeps its proportions.

## Tests

Ninety, in seconds. Names read as claims — `a_refused_kind_is_still_drawn_and_marked`, `dirtiness_is_a_comparison_so_undo_makes_it_clean_again`. Three kinds are deliberate:

- **Anti-drift tests** between two halves that can disagree: every kind described, every idiom clean, every link resolving, the spec the same bytes twice, the rules the app enforces the same rules the spec tabulates.
- **Real-path tests** that drive the app the way fingers do — `press(&mut app, "vl ")` — rather than calling handlers.
- **Invariant tests** on the command table: no two commands live on one key in one place; the footer never exceeds six.
