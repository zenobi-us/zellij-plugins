# zellij-tabbar

A focusable, template-driven tab bar plugin for [Zellij](https://zellij.dev/).

Requires Zellij 0.45.x.

It provides:

- clickable tab and new-tab buttons
- a built-in default layout
- custom [MiniJinja](https://github.com/mitsuhiko/minijinja) templates
- nested horizontal and vertical Flex layouts
- focus-following tab overflow
- Zellij theme colours

## Install

Build the plugin:

```bash
moon run repo:build
```

The WASM file is written to:

```text
target/wasm32-wasip1/release/zellij-tabbar.wasm
```

Copy it into your Zellij plugin directory:

```bash
moon run repo:install
```

By default this installs:

```text
~/.config/zellij/plugins/zellij-tabbar.wasm
```

Set `ZELLIJ_PLUGIN_DIR` to install elsewhere.

## Add it to Zellij

Add a plugin alias to `~/.config/zellij/config.kdl`:

```kdl
plugins {
    tabbar location="file:/home/you/.config/zellij/plugins/zellij-tabbar.wasm"
}
```

Use it in a layout:

```kdl
layout {
    pane size=1 borderless=true {
        tabbar
    }
    pane
}
```

On first launch, grant the requested permissions. The plugin needs application-state access for tab data and application-change access for button actions.

## Default layout

No configuration is required. The default template renders:

```text
session name | scrollable tabs | +
```

The active tab stays visible when tabs exceed available width. Click a tab to switch to it. Click `+` to create a tab.

## Custom template

Set `template` in the plugin configuration:

```kdl
plugin location="file:/home/you/.config/zellij/plugins/zellij-tabbar.wasm" {
    template r#"
{%- call Flex(direction="row") -%}
  {%- call Flex(shrink=0) -%}
    {{ session.name }}
  {%- endcall -%}

  {%- call Flex(direction="row", grow=1, overflow="scroll") -%}
    {%- for tab in session.tabs -%}
      {%- call Button(
        on_click=context.actions.switch_tab(tab.index),
        focused=tab.active
      ) -%}
        {{- tab.name -}}
      {%- endcall -%}
    {%- endfor -%}
  {%- endcall -%}

  {%- call Button(on_click=context.actions.new_tab()) -%}
    +
  {%- endcall -%}
{%- endcall -%}
"#
}
```

Invalid templates render a visible `template error:` message instead of silently using the default.

## Template data

### Session

```jinja
{{ session.name }}
```

### Tabs

```jinja
{% for tab in session.tabs %}
  {{ tab.index }}
  {{ tab.name }}
  {{ tab.active }}
{% endfor %}
```

`tab.index` is one-based.

### Time

```jinja
{{ system.time | format("HH:MM") }}
```

Supported format tokens:

| Token | Meaning |
|---|---|
| `YYYY` | Four-digit year |
| `YY` | Two-digit year |
| `HH` | Hour |
| `MM` | Minute |
| `SS` | Second |

## Buttons

`Button` owns styling and creates a left-click hitbox.

Switch tabs:

```jinja
{% call Button(
  on_click=context.actions.switch_tab(tab.index),
  focused=tab.active
) %}
  {{ tab.name }}
{% endcall %}
```

Create a tab:

```jinja
{% call Button(on_click=context.actions.new_tab()) %}
  +
{% endcall %}
```

Only actions supplied through `context.actions` are accepted. Constructed action strings are rejected.

Supported mouse interaction is left click. Right click and middle click have no configured button actions.

## Flex layout

`Flex` supports nested row and column layouts:

```jinja
{% call Flex(
  direction="row",
  grow=1,
  shrink=1,
  basis="auto",
  justify="start",
  align="start",
  overflow="normal"
) %}
  ...
{% endcall %}
```

| Option | Values | Default |
|---|---|---|
| `direction` | `row`, `column` | `row` |
| `grow` | Non-negative integer ratio | `0` |
| `shrink` | Non-negative integer ratio | `1` |
| `basis` | `auto` or terminal-cell integer | `auto` |
| `justify` | `start`, `center`, `end`, `space-between`, `space-around` | `start` |
| `align` | `start`, `center`, `end`, `stretch` | `start` |
| `overflow` | `normal`, `scroll` | `normal` |

Use `overflow="scroll"` around tab buttons. The viewport automatically follows the button marked `focused=true`. It does not maintain a separate mouse-controlled scroll position.

## Styling text

Available filters:

```jinja
{{ "bold" | bold }}
{{ "dim" | dim }}
{{ "text" | fg(context.theme.text) }}
{{ "active" | fg(context.theme.active_text) | bg(context.theme.active_background) }}
```

Theme values:

- `context.theme.text`
- `context.theme.background`
- `context.theme.active_text`
- `context.theme.active_background`
- `context.theme.muted_text`
- `context.theme.muted_background`
- `context.theme.alert`

`fg` and `bg` only accept colours supplied by `context.theme`.

## Strict template rules

The renderer rejects:

- unknown helper arguments
- unsupported enum values
- malformed or nested button helpers
- invalid click actions or tab indexes
- malformed internal layout markers
- tabs and carriage returns in rendered text
- malformed ANSI escape sequences

Failures appear directly in the tab bar as `template error: ...`.

## Migrating old templates

`Tab` and `Stack` no longer exist.

Replace:

```jinja
{% call Tab(index=tab.index, label=tab.name) %}{% endcall %}
```

With:

```jinja
{% call Button(
  on_click=context.actions.switch_tab(tab.index),
  focused=tab.active
) %}
  {{ tab.name }}
{% endcall %}
```

Replace `Stack` layouts with nested `Flex` calls.

## Development

```bash
moon run repo:build
moon run repo:check
moon run repo:test
moon run repo:e2e # requires bats, python3, and zellij
```

Publish the built WASM to an existing GitHub release:

```bash
PUBLISH_TAG=v0.1.0 moon run repo:publish
```

Set `GITHUB_REPOSITORY=owner/repo` when the current checkout has no GitHub remote.
