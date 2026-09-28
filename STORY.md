# vim-shapes

As an engineer, I fell in love with vim and nvim. When I stepped off .NET and into other languages, I wanted to understand this infamous editor. After integrating it into my practice (with the encouragement from friends along the way), I felt lightning fast, able to crush anything, and I realized it was the keybindings that made it possible. Well, to be specific, not just the keybindings--the key _grammar_ made it possible.

When I became an architect, I had to step out of the terminal and point-and-click through diagrams in Lucidchart or Visio. Things slowed down, and I was generally uninspired. My son once saw me with a large diagram open and said, "This isn't work. This is just _shapes_." My whole workflow from the last 10 years became unusable, because the applications prioritized reasoning with the mouse over the keyboard.

All diagramming tools are repackaging the same experience: poor feature matching between the mouse and keyboard, arbitrary keyboard shortcuts when those are enabled, a mouse-dependent workflow and navigation for everything else, and diagrams treated more like an art gallery than an engineering practice--rearranging pretty shapes with only relative significance.

vim-shapes returns to keyboard grammar, not just shortcuts. If I want to "open workbench" then all I do is press 'W'. If I want to "add a shape", I press 'a'. When I want to "go to the next tab" it's 'gt', or backwards with 'gT'. Of course, there are also system commands ':' like writing to a specific path or exiting without saving, etc. In vim-shapes, you're not just typing with your keyboard--you're expressing the activity of diagramming using a vim-like keyboard grammar.

Lastly, as advancements in LLMs continue to unfold like the bullish early days of any market phenomenon, the practice of enterprise architecture has to adjust like every field. The power we've tapped into with LLMs needs to be grounded in a reality, a digital representation of the on-the-ground realities of a business. That's why vim-shapes has two main diagram types: freeform and architecture.

Freeform is no-rules diagramming. Architecture _leans_ (not 100% adoption) on the TOGAF and ArchiMate models and will raise issues when it doesn't pass validation. In addition to this, vim-shapes includes the ability to visually model ontologies. It includes references to BFO/CCO as well as an ontology engineering practice that's becoming widely popular through Palantir's Foundry. With an export from Ontology Manager, you can link your object types to business processes and stakeholder maps. Finally, you can understand your application portfolio alongside the enterprise's ontological commitments.

I hope you enjoy vim-shapes. While it doesn't leave the mouse out entirely--there are a large array of mouse-supported interactions in the TUI--it flips the priorities for diagramming tools and focuses on keyboard-first. And yes, you can use the arrow keys.
