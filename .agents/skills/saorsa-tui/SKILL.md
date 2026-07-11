---
name: saorsa-tui
description: Guides Rust terminal UI development with saorsa-tui, when building retained widget trees, TCSS layouts, reactive state, overlays, Unicode-safe rendering, or terminal tests, resulting in small applications that follow the crate's high-level runtime and API patterns.
---

# saorsa-tui

## Overview

Use `saorsa-tui` as a retained-mode terminal framework: persistent DOM nodes feed TCSS styling, Taffy layout, widget rendering, compositing, and differential terminal output.

Read [references/api.md](references/api.md) before implementing. It records version `0.4.0`, public subsystems, widget catalog, and runtime rules.

## When to Use

Use for:

- Rust TUIs using `saorsa-tui`
- retained widget trees and focus handling
- TCSS selectors, themes, flexbox, or grid
- built-in widgets such as `TextArea`, `DataTable`, `Tree`, `Tabs`, `Modal`, or `MarkdownRenderer`
- reactive `Signal`, `Computed`, `Effect`, or bindings
- overlays, Unicode-safe text, differential rendering, or `TestBackend`

Do not substitute Ratatui, Cursive, or raw Crossterm patterns. Their immediate-mode and event-loop assumptions differ.

## Core Workflow

1. Add `saorsa-tui = "0.4"`.
2. Create persistent widgets in `app::Dom`.
3. Wrap each widget with `Leaf`, `Interactive`, `StyledLeaf`, or `StyledInteractive`.
4. Assign root, parent-child links, CSS ids/classes, and focusability.
5. Create `App` from a TCSS string/file or `StylesheetLoader`.
6. In event loop: poll style reload, render if dirty, handle resize, dispatch events.
7. Mutate widgets through saved `NodeRef` and `downcast_widget_mut`; request render when needed.
8. Verify rendering with `TestBackend`.

```text
input → App::handle_event → focused widget → state mutation
  ↑                                           ↓
terminal ← differential render ← layout/style ← dirty
```

## Defaults

- Prefer `App` + `Dom`; low-level renderer only when explicitly needed.
- Prefer TCSS for layout/style; avoid hard-coded coordinates.
- Prefer built-in widgets; custom `Widget` only when catalog lacks behavior.
- Use least capable wrapper fitting widget traits.
- Use `EventResult::Consumed` only after handling event.
- Use crate display-width/grapheme helpers; never byte-index terminal text.
- Handle terminal teardown on every error path.
- Check current rustdoc when API differs from reference version.

## Common Mistakes

- Rebuilding DOM every frame: retain nodes and mutate state.
- Using `Leaf` for input widget: use `Interactive` or `StyledInteractive`.
- Styling inside render code: assign type/id/class and use TCSS.
- Forgetting `set_focusable`: focused input and `:focus` styling fail.
- Mutating widget without dirtying app: call `request_render`.
- Ignoring resize: call `handle_resize(Size)`.
- Counting chars for width: use `string_display_width` and width-safe truncation.
- Copying another TUI framework's loop: verify `Terminal` and `App` APIs in rustdoc.

## Reference

Detailed API: [references/api.md](references/api.md)
