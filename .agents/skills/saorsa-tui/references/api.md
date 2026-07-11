# saorsa-tui API Reference

Version: `0.4.0` (latest observed 2026-07-11)

Sources:

- https://crates.io/crates/saorsa-tui
- https://docs.rs/saorsa-tui/0.4.0/saorsa_tui/
- https://github.com/saorsa-labs/saorsa-tui/tree/v0.4.0/crates/saorsa-tui

`saorsa-tui` is a retained-mode, CSS-styled Rust terminal UI framework. It combines a persistent widget DOM, TCSS styling, Taffy flex/grid layout, reactive state, overlays, differential rendering, Unicode-aware text handling, and Crossterm terminal I/O.

## Install

```toml
[dependencies]
saorsa-tui = "0.4"
```

No optional crate features are declared in `0.4.0`.

## Architecture

```text
DOM + widgets + signals
        ↓
TCSS cascade and computed styles
        ↓
Taffy layout rectangles
        ↓
Widget::render → ScreenBuffer
        ↓
Compositor layers and overlays
        ↓
Differential Renderer
        ↓
Terminal backend
```

Prefer the high-level `app` runtime for retained applications. Use the lower-level buffer, compositor, renderer, and terminal modules only when building custom infrastructure.

## High-level application API

### `app::Dom`

Persistent widget tree with selector metadata and focus state.

Key methods:

```rust
let mut dom = Dom::new();
let root = dom.create("container", Box::new(StyledLeaf::new(Container::new())));
let label = dom.create("label", Box::new(StyledLeaf::new(Label::new("Hello"))));

dom.set_root(root);
dom.append_child(root, label);
dom.set_css_id(root, "app");
dom.add_class(label, "title");
dom.set_focusable(label, false);
```

| Method | Purpose |
|---|---|
| `create(type_name, Box<dyn NodeWidget>)` | Create unattached node and return `NodeRef` |
| `set_root(node)` | Set DOM and selector-tree root |
| `append_child(parent, child)` | Attach child |
| `detach(node)` | Detach from parent |
| `remove_subtree(node)` | Remove node and descendants |
| `set_css_id(node, id)` | Set `#id` selector metadata |
| `add_class(node, class)` | Add `.class` selector metadata |
| `set_focusable(node, bool)` | Add/remove node from tab traversal |
| `widget` / `widget_mut` | Access erased `NodeWidget` |
| `downcast_widget_mut::<T>` | Mutate concrete widget by `NodeRef` |
| `focus` / `focus_mut` | Access `FocusManager` |

### DOM wrappers

Widgets need wrappers because `Dom` stores `dyn NodeWidget`:

| Wrapper | Widget requirements | Use |
|---|---|---|
| `Leaf<T>` | `Widget` | Render-only |
| `Interactive<T>` | `Widget + InteractiveWidget` | Render and input |
| `StyledLeaf<T>` | `Widget + ApplyComputedStyle` | Render and TCSS |
| `StyledInteractive<T>` | `Widget + InteractiveWidget + ApplyComputedStyle` | Render, input, and TCSS |

### `app::App`

Owns DOM, styles, layout, focus dispatch, and render state.

Constructors:

```rust
let app = App::new(&terminal, dom, stylesheet_loader)?;
let app = App::from_tcss_string(&terminal, dom, css)?;
let app = App::from_tcss_file(&terminal, dom, "app.tcss")?; // hot reload watcher
```

Main methods:

| Method | Purpose |
|---|---|
| `handle_event(&Event)` | Dispatch key/mouse/focus events; returns `EventResult` |
| `render_if_needed(&mut Terminal)` | Render only when dirty |
| `render_frame(&mut Terminal)` | Force style, layout, and render pass |
| `handle_resize(Size)` | Resize buffers and invalidate layout |
| `request_render()` | Mark application dirty |
| `poll_stylesheet_reload()` | Apply watched TCSS file changes |
| `reload_stylesheet_string(css)` | Replace stylesheet from string |
| `set_active_theme(Option<&str>)` | Activate named theme variables |
| `register_action(name, handler)` | Register application action |
| `bind_key(KeyEvent, action)` | Map key chord to action |
| `query(selector)` / `query_one(selector)` | Query DOM with TCSS selector |
| `mount(parent, child)` | Attach post-construction node and update caches |
| `remove_subtree(node)` | Run unmount lifecycle and remove node |
| `rect_of(node)` | Read last computed rectangle |

Typical loop:

```rust
loop {
    app.poll_stylesheet_reload()?;
    app.render_if_needed(&mut terminal)?;

    let event = terminal.read_event()?;
    if matches!(event, Event::Key(KeyEvent { code: KeyCode::Char('q'), .. })) {
        break;
    }
    if let Event::Resize(width, height) = event {
        app.handle_resize(Size::new(width, height));
    } else {
        app.handle_event(&event)?;
    }
}
```

Check exact terminal setup and teardown methods against current rustdoc before copying a complete `main`; terminal APIs are backend-specific and must restore raw mode/alternate screen on failure.

## Core traits

```rust
pub trait Widget {
    fn render(&self, area: Rect, buf: &mut ScreenBuffer);
}

pub trait SizedWidget: Widget {
    fn min_size(&self) -> (u16, u16);
    fn preferred_size(&self) -> (u16, u16) { self.min_size() }
}

pub trait InteractiveWidget: Widget {
    fn handle_event(&mut self, event: &Event) -> EventResult;
}

pub enum EventResult {
    Consumed,
    Ignored,
}
```

Custom retained widgets may implement `app::NodeWidget` directly for lifecycle hooks (`on_mount`, `on_unmount`), event handling, computed-style application, and downcasting. Prefer wrappers for ordinary widgets.

## Built-in widgets

Public root exports:

| Widget | Purpose |
|---|---|
| `Container` | Styled region with borders/background |
| `Label` | Styled aligned text |
| `StaticWidget` | Static segment content |
| `TextArea` | Editable multiline text |
| `DataTable` / `Column` | Tabular data |
| `Tree` / `TreeNode` | Hierarchical data |
| `DirectoryTree` | Filesystem tree |
| `SelectList` / `OptionList` | Selectable options |
| `Checkbox`, `RadioButton`, `Switch` | Form controls |
| `Tabs` / `Tab` | Tabbed content |
| `Collapsible` | Expand/collapse content |
| `MarkdownRenderer` | Markdown display |
| `DiffView` | Unified or side-by-side diffs |
| `RichLog` | Styled append-only log |
| `ProgressBar` | Determinate/indeterminate progress |
| `LoadingIndicator` | Spinner/loading state |
| `Sparkline` | Compact numeric series chart |
| `Modal` | Centered dialog overlay |
| `Toast` | Notification overlay |
| `Tooltip` | Anchored help overlay |

Common supporting enums include `Alignment`, `BorderStyle`, `DiffMode`, `IndicatorStyle`, `ProgressMode`, `TabBarPosition`, `ToastPosition`, and overlay `Placement`.

## TCSS styling

Module: `saorsa_tui::tcss`

TCSS supports CSS-like selectors, declarations, variables, themes, pseudo-classes, cascade resolution, computed styles, and file hot reload.

Core types include:

- `Stylesheet`, `StylesheetLoader`, `StylesheetEvent`
- `Selector`, `SelectorList`, `StyleMatcher`, `MatchCache`
- `ComputedStyle`, `ApplyComputedStyle`, `CascadeResolver`
- `VariableEnvironment`, `VariableMap`
- `Theme`, `ThemeManager`
- `WidgetNode`, `WidgetTree`

DOM selector metadata comes from node type names, CSS ids, classes, tree relationships, and pseudo-state. Build nodes with meaningful type names and assign ids/classes through `Dom`; do not embed styling logic into render methods when TCSS covers it.

Illustrative TCSS:

```css
$accent: #89b4fa;

#app {
  display: flex;
  flex-direction: column;
  padding: 1;
}

.title {
  color: $accent;
  font-weight: bold;
}

button:focus {
  border-color: $accent;
}
```

Property support evolves. Verify exact names and value grammar in `tcss::property` and rustdoc for version used.

## Reactive state

Module: `saorsa_tui::reactive`

| Type/function | Purpose |
|---|---|
| `Signal<T>` | Mutable reactive state |
| `Computed<T>` | Derived value with dependency tracking |
| `Effect` | Side effect rerun when dependencies change |
| `batch(...)` | Coalesce multiple updates |
| `ReactiveScope` | Own and dispose reactive resources |
| `Binding`, `OneWayBinding`, `TwoWayBinding` | Connect reactive properties |
| `BindingScope` | Own groups of bindings |

Use signals for shared application state, computed values for derivation, and effects only for external side effects. Batch related writes to avoid redundant reactions.

## Layout

Module: `saorsa_tui::layout`

Two layout levels exist:

- `Layout`, `Constraint`, `Direction`, `Dock`: manual terminal-area splitting.
- `LayoutEngine`: retained Taffy-backed flexbox/grid layout used by `App`.

Supporting types: `LayoutRect`, `LayoutError`, `OverflowBehavior`, `ScrollManager`, `ScrollState`.

Prefer TCSS/Taffy layout inside `App`. Use manual constraints for isolated low-level widgets or custom renderers.

## Rendering primitives

| Type | Purpose |
|---|---|
| `Segment` | Styled text rendering unit |
| `Style` / `Color` | Foreground, background, and attributes |
| `Cell` | Grapheme cluster, style, display width |
| `ScreenBuffer` | 2D cell grid with change tracking |
| `RenderContext` | Frame lifecycle and buffers |
| `Renderer` / `DeltaBatch` | Differential ANSI output |
| `Compositor` / `Layer` | Z-order, clipping, layer flattening |
| `Rect`, `Size`, `Position` | Geometry |

Widgets must respect their supplied `Rect`. Text width is terminal display width, not byte count or Unicode scalar count.

## Events, focus, and input

Root event exports:

- `Event`
- `KeyEvent`, `KeyCode`, `Modifiers`
- `MouseEvent`

Focus exports:

- `FocusManager`
- `FocusState`
- `WidgetId`

Interactive widgets return `EventResult::Consumed` to stop propagation or `Ignored` to allow parent/application handling. Mark interactive DOM nodes focusable and let `App` synchronize focus pseudo-state for TCSS.

## Overlays

Module: `saorsa_tui::overlay`

- `ScreenStack`: stack of overlay screens
- `OverlayConfig`: placement, size, z-order, and clipping configuration
- `OverlayId`: overlay handle
- `OverlayPosition`, `Placement`: positioning

`Modal`, `Toast`, and `Tooltip` can produce overlay configuration and rendered lines for pushing onto `ScreenStack`.

## Text editing and Unicode

| API | Purpose |
|---|---|
| `TextBuffer` | Rope-backed editable text storage |
| `UndoStack`, `EditOperation` | Undo/redo history |
| `CursorPosition`, `CursorState`, `Selection` | Editing cursor and selection |
| `Viewport` | Scrollable clipped content |
| `wrap_line`, `wrap_lines`, `WrapResult` | Soft wrapping |
| `string_display_width` | Terminal display width |
| `truncate_to_display_width` | Width-safe truncation |
| `preprocess`, `expand_tabs`, `filter_control_chars` | Safe text preparation |
| `Highlighter` | Pluggable highlighting |

Never index visible text by bytes. Use crate text helpers, grapheme-aware cells, and display-width functions for CJK, combining marks, and emoji.

## Terminal and testing

Module: `saorsa_tui::terminal`

- `Terminal`: backend abstraction
- `CrosstermBackend`: real terminal backend
- `TestBackend`: deterministic tests without a real terminal
- `TerminalCapabilities`, `TerminalInfo`, `TerminalKind`
- `detect`, `detect_terminal`, `detect_multiplexer`
- `MultiplexerKind`, `merge_multiplexer_limits`, `profile_for`

Use `TestBackend` and inspect `ScreenBuffer` cells for widget/runtime tests. Test narrow widths, wide graphemes, resize, focus movement, and ignored/consumed event propagation.

## Error handling

Root aliases:

```rust
pub type Result<T> = std::result::Result<T, SaorsaTuiError>;
```

`SaorsaTuiError` covers terminal, rendering, style, layout, widget, and related framework failures. Propagate errors with `?`; terminal teardown must still run after failures.

## Public module map

- `app`: retained DOM/runtime
- `buffer`, `cell`: screen storage
- `color`, `style`, `segment`: styling primitives
- `compositor`: layers and clipping
- `cursor`, `text_buffer`, `undo`: text editing
- `event`, `focus`: input routing
- `geometry`: coordinates and rectangles
- `highlight`: syntax highlighting
- `layout`, `viewport`, `wrap`: sizing and scrolling
- `overlay`: modals/toasts/tooltips
- `reactive`: signals, computed values, effects, bindings
- `render_context`, `renderer`: frame and terminal deltas
- `tcss`: CSS parser, matching, cascade, themes, reload
- `terminal`: real/test backends and capability detection
- `text`: Unicode-aware preprocessing and width helpers
- `widget`: built-in widgets and widget traits

## Design rules

1. Keep persistent widgets in `Dom`; do not rebuild whole UI every frame.
2. Wrap widgets with the least capable DOM wrapper that fits.
3. Put layout and visual state in TCSS when supported.
4. Mutate concrete widgets through stored `NodeRef` plus `downcast_widget_mut`.
5. Call `request_render` after mutations not already tracked by runtime state.
6. Return `Consumed` only when widget handled event.
7. Use display width and grapheme-aware APIs for all terminal text.
8. Poll stylesheet reload when using `from_tcss_file`.
9. Handle resize events with `handle_resize` before next render.
10. Use `TestBackend` for deterministic rendering checks.
