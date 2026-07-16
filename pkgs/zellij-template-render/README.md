# zellij-template-render

Reusable MiniJinja terminal renderer for Zellij plugins.

The crate provides:

- nested `Flex` row and column layouts with fixed cell gaps
- typed `Button` actions and two-dimensional click hitboxes
- focus-following overflow
- ANSI-aware measurement and clipping
- `Clock(format=..., tz=...)` with IANA timezone support and host-scheduled refresh metadata; `tz` defaults to `env.TZ`, then UTC
- `bold`, `dim`, `fg`, `bg`, and time-format filters
- `TemplateHost` ownership of template sources, external loading, environment allowlists, and shared `theme`, `env`, and `system` context derived from Zellij `ModeInfo`

Plugins own template data, action semantics, button presentation, and the `ModeInfo` event lifecycle. The renderer converts the current `ModeInfo` into its template colour contract.

Use low-level `Renderer` methods for standalone strings. Zellij plugins should normally construct a `TemplateHost` from `TemplateSource` and `TemplateEnvironment`, then pass the latest `ModeInfo` when rendering. The host validates `template`/`template_file`, defaults the environment allowlist to `TZ`, `LANG`, and `TERM`, and retains named environments so includes remain cached.

```rust
use zellij_template_render::{
    context, ActionRegistry, ButtonPresentation, Renderer, Value, Viewport,
};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Action {
    Select(usize),
}

let actions = ActionRegistry::new().with("select", |args| {
    let index = args.first().and_then(Value::as_usize).unwrap();
    Ok(Action::Select(index))
});
let frame = Renderer::new(actions).render(
    r#"{% call Button(on_click=actions.select(2)) %}two{% endcall %}"#,
    context! {},
    Viewport { rows: 1, cols: 10 },
    |button| Ok(ButtonPresentation {
        label: button.label.to_string(),
        focused: button.focused.unwrap_or(false),
    }),
)?;
# Ok::<(), zellij_template_render::Error>(())
```

`Button` only accepts values returned by registered functions under `actions`. Action decoder results become typed values in `Frame::hitboxes`.

## Filters

`format_time(pattern)` accepts a Unix timestamp in whole seconds and formats it in the renderer's local timezone. Patterns support Chrono `strftime` directives such as `%Y`, `%m`, `%d`, `%H`, `%M`, and `%S`, plus the friendly aliases `YYYY`, `YY`, `HH`, `MM`, and `SS`.

```jinja
{{ system.time | format_time("%Y-%m-%d %H:%M") }}
{{ system.time | format_time("HH:MM:SS") }}
```

## Components

### Flex

`Flex` arranges its body in a row or column. Nest calls to build complete terminal layouts.

| Prop | Type / values | Default | Guide |
|---|---|---|---|
| `direction` | `row`, `column` | `row` | Select main layout axis. |
| `grow` | Non-negative integer | `0` | Share unused cells with growing siblings. |
| `shrink` | Non-negative integer | `1` | Share overflow reduction with shrinking siblings. Use `0` for fixed controls. |
| `basis` | `auto` or non-negative cell count | `auto` | Set initial main-axis size before grow or shrink. |
| `gap` | Non-negative cell count | `0` | Insert cells between direct children. |
| `justify` | `start`, `center`, `end`, `space-between`, `space-around` | `start` | Position children on main axis when free cells remain. |
| `align` | `start`, `center`, `end`, `stretch` | `start` | Position children on cross axis. |
| `overflow` | `normal`, `scroll` | `normal` | Clip overflow, or follow focused descendant inside a scrolling viewport. |

Basic row with fixed edges and a flexible centre:

```jinja
{% call Flex(direction="row", gap=1) %}
  {% call Flex(shrink=0) %}left edge{% endcall %}
  {% call Flex(grow=1, overflow="scroll") %}
    ...focused buttons...
  {% endcall %}
  {% call Flex(shrink=0) %}right edge{% endcall %}
{% endcall %}
```

Use `grow=1` on the region that should consume remaining width. Use `shrink=0` on controls that must remain visible. With `overflow="scroll"`, the viewport follows the descendant `Button` whose resolved focus state is true. There is no separate mouse-controlled scroll position.

## Colour contract

`TemplateHost::render` accepts the current Zellij `ModeInfo` and exposes semantic colours through the top-level `theme` object. Consumers do not map `ModeInfo.style.colors` themselves.

| Variable | Meaning | Zellij source |
|---|---|---|
| `theme.text` | Default foreground colour | `text_unselected.base` |
| `theme.background` | Default background colour | `text_unselected.background` |
| `theme.active_text` | Foreground colour for active items | `ribbon_selected.base` |
| `theme.active_background` | Background colour for active items | `ribbon_selected.background` |
| `theme.muted_text` | Foreground colour for inactive or secondary items | `ribbon_unselected.base` |
| `theme.muted_background` | Background colour for inactive or secondary items | `ribbon_unselected.background` |
| `theme.alert` | Emphasis colour for alerts | `ribbon_unselected.emphasis_3` |

The `fg` and `bg` filters accept three colour sources:

```jinja
{{ "explicit RGB" | fg("rgb:255,128,0") }}
{{ "terminal palette" | fg("index:208") }}
{{ "active theme" | fg(theme.active_text) }}
```

- `rgb:R,G,B` uses three decimal channels from `0` to `255`.
- `index:N` uses an xterm 256-colour palette index from `0` to `255`. Indices `0`–`15` are controlled by the terminal theme, `16`–`231` form the 6×6×6 colour cube, and `232`–`255` form the grayscale ramp. See the [xterm 256-colour reference](https://invisible-island.net/xterm/xterm.faq.html).
- `theme.<role>` is a MiniJinja value derived from the active Zellij theme. It resolves to either an `rgb:R,G,B` or `index:N` token before reaching `fg` or `bg`; do not quote it as a string.

Theme values can be chained with other filters:

```jinja
{{ "normal" | fg(theme.text) | bg(theme.background) }}
{{ "active" | fg(theme.active_text) | bg(theme.active_background) }}
{{ "warning" | fg(theme.alert) }}
```

`theme` is top-level. The removed `context.theme.*` path and string values such as `"theme.active_text"` are unsupported.

## Template sources

The renderer accepts inline source as `&str`, a named template from a preconfigured MiniJinja `Environment`, or a filesystem environment built with `file_template_environment`.

Use these terms consistently:

- **inline template**: source supplied directly through plugin configuration
- **embedded template**: a named source file bundled into the plugin WASM at build time
- **external template**: a source file read from the host filesystem at plugin load time

An embedded template is no longer read from disk at runtime. `minijinja-embed` packages template sources inside the WASM and loads them into a MiniJinja environment.

### Ownership boundary

The plugin host owns template selection and maps host paths into its filesystem namespace. This crate owns template loading, MiniJinja caching, evaluation, and terminal layout.

```text
plugin configuration
  ├─ inline template ─────────────────────→ Renderer::render(...)
  ├─ external template → file environment → Renderer::render_named(...)
  └─ no override → embedded environment ──→ Renderer::render_named(...)
```

Embed a template directory from the consuming plugin's `build.rs`:

```rust
fn main() {
    minijinja_embed::embed_templates!("src/template", &[".jinja"]);
}
```

Load that bundle and render its named entry template:

```rust
use zellij_template_render::Environment;

let mut environment = Environment::new();
minijinja_embed::load_templates!(&mut environment);
renderer.render_named(environment, "main.jinja", data, viewport, present_button)?;
```

The consuming plugin needs `minijinja` and `minijinja-embed` as normal dependencies, plus `minijinja-embed` as a build dependency.

Build an external environment with a host-provided reader:

```rust
use zellij_template_render::file_template_environment;

let (mut environment, entry) = file_template_environment(
    configured_path.into(),
    home_directory,
    |path| std::fs::read_to_string(path),
)?;
renderer.render_named_mut(&mut environment, &entry, data, viewport, present_button)?;
```

The environment validates the entry immediately. Includes, imports, and inheritance resolve relative to the including file. MiniJinja loads each template name once and caches it for the environment lifetime. The reader controls host path mapping; a Zellij WASI plugin maps host paths through `/host` and requests `FullHdAccess`.

Host implementations should:

- reject configuration containing both inline and external template settings
- create one external environment during plugin load unless hot reload is explicitly required
- report read and parse failures instead of silently falling back to the embedded default
- define whether configured paths are relative to Zellij's host folder; arbitrary absolute host paths can require `/host` remapping and `FullHdAccess`

### Includes and inheritance

Embedded `{% include %}`, `{% extends %}`, and import relationships work through `Renderer::render_named` when every referenced template is present in the loaded bundle. Filesystem relationships use `file_template_environment`; template paths are unrestricted, so callers must treat external templates as trusted input.
