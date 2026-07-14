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
pkgs/zellij-tabbar/target/wasm32-wasip1/release/zellij-tabbar.wasm
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

## Demo

Build the local WASM and start a fresh Zellij session using [`demo.kdl`](demo.kdl):

```bash
moon run zellij-tabbar:demo
```

The task opens the session in a new Ghostty window. Before every launch it deletes the isolated demo permission cache at `target/demo-cache/zellij/permissions.kdl`, forcing Zellij to ask again without touching your normal cache. The tabbar remains selectable while the prompt is active; press `y` or `n`. Set `ZELLIJ_TABBAR_DEMO_SESSION` to choose the session name; otherwise the task generates a unique name.

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
    pane size=1 borderless=true focus=true {
        tabbar
    }
    pane
}
```

## Grant permissions

The plugin needs:

- `ReadApplicationState` for session and tab data
- `ChangeApplicationState` for tab, pane positioning, focus, and reload actions
- `OpenTerminalsOrPlugins` for opening plugin panes
- `FullHdAccess` when `template_file` loads templates from the host filesystem

On first load, focus the one-row tabbar so its interactive Zellij permission prompt can consume `y` or `n`. Zellij renders a compact prompt when the pane is too short for the full permission list. After the answer, the plugin becomes non-selectable and keeps its configured one-row height.

To pre-grant permissions instead, open `${XDG_CACHE_HOME:-$HOME/.cache}/zellij/permissions.kdl` and add a block keyed by the absolute plugin path:

```kdl
"/home/you/.config/zellij/plugins/zellij-tabbar.wasm" {
    ReadApplicationState
    ChangeApplicationState
    OpenTerminalsOrPlugins
}
```

Create the directory and file when they do not exist:

```bash
mkdir -p "${XDG_CACHE_HOME:-$HOME/.cache}/zellij"
touch "${XDG_CACHE_HOME:-$HOME/.cache}/zellij/permissions.kdl"
```

The path must exactly match the plugin path Zellij resolves. Use this to obtain it:

```bash
realpath ~/.config/zellij/plugins/zellij-tabbar.wasm
```

Restart the Zellij session after changing the cache. Remove this block when you want Zellij to request permission again.

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
        on_click=actions.switch_tab(tab.index),
        focused=tab.active
      ) -%}
        {{- tab.name -}}
      {%- endcall -%}
    {%- endfor -%}
  {%- endcall -%}

  {%- call Button(on_click=actions.new_tab()) -%}
    +
  {%- endcall -%}
{%- endcall -%}
"#
}
```

Or set `template_file` to a host filesystem path:

```kdl
plugin location="file:/home/you/.config/zellij/plugins/zellij-tabbar.wasm" {
    template_file "tabbar/main.jinja"
}
```

Relative paths use `${ZELLIJ_CONFIG_DIR:-$HOME/.config/zellij}`. Absolute paths and `~` are supported. Includes, imports, and inheritance resolve relative to the including file, load lazily, and remain cached until the plugin reloads. External templates are trusted: they can read any host file available to the plugin. `template` and `template_file` cannot be used together.

Invalid configuration, unreadable files, and template failures render a visible `template error:` message instead of silently using the default.

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

### Environment

Host environment variables exposed by the WASI runtime are available through the top-level `env` object:

```jinja
{{ env.TZ }}
```

`TZ`, `LANG`, and `TERM` are allowed by default. Override the allowlist with a comma-separated `env_vars` plugin setting:

```kdl
plugin location="file:/home/you/.config/zellij/plugins/zellij-tabbar.wasm" {
    env_vars "TZ,LANG,TERM,COLORTERM"
}
```

Missing or runtime-hidden variables remain undefined. Do not allowlist secrets: template files can render exposed values.

## Theme

The top-level `theme` object exposes colours derived from the active Zellij theme. Use these values with the `fg` and `bg` filters; they are colour tokens shaped as `rgb:R,G,B` or `index:N`.

| Property | Use |
|---|---|
| `theme.text` | Default foreground |
| `theme.background` | Default background |
| `theme.active_text` | Foreground for the active or focused item |
| `theme.active_background` | Background for the active or focused item |
| `theme.muted_text` | Foreground for inactive or secondary content |
| `theme.muted_background` | Background for inactive or secondary content |
| `theme.alert` | Warning or attention colour |

Apply foreground and background colours by piping text through filters:

```jinja
{{ session.name | fg(theme.text) | bg(theme.background) }}
{{ tab.name | fg(theme.active_text) | bg(theme.active_background) }}
{{ "!" | fg(theme.alert) }}
```

Choose colours by meaning rather than by expected RGB value. Zellij users can change themes, and the same template follows their active palette automatically. `theme` is top-level; `context.theme` is unsupported.

## Components

Templates can emit plain text and use renderer primitives. See the renderer's [`Flex` documentation](../zellij-template-render/README.md#flex) for layout behavior. `Button` and `OnOverflow` are MiniJinja call blocks. `Clock` is a function. Text styling uses filters.

### Button

`Button` renders host-styled text and creates a left-click hitbox over its visible cells.

| Prop | Type / values | Default | Guide |
|---|---|---|---|
| `on_click` | Value returned by `actions.*` | Required | Select action executed by left click. Constructed strings are rejected. |
| `focused` | Boolean | Host policy | Marks item as focused and lets scrollable ancestors keep it visible. |
| `label` | String | Call body | Supply label directly instead of using a call body. |

Tab button using a call body:

```jinja
{% call Button(
  on_click=actions.switch_tab(tab.index),
  focused=tab.active
) %}
  {{ tab.name }}
{% endcall %}
```

Button using the `label` prop:

```jinja
{{ Button(on_click=actions.new_tab(), label="+") }}
```

Style a button label with theme colours:

```jinja
{% call Button(
  on_click=actions.switch_tab(tab.index),
  focused=tab.active
) %}
  {% if tab.active %}
    {{ tab.name | fg(theme.active_text) | bg(theme.active_background) }}
  {% else %}
    {{ tab.name | fg(theme.muted_text) | bg(theme.muted_background) }}
  {% endif %}
{% endcall %}
```

Available actions:

| Function | Props | Result |
|---|---|---|
| `actions.switch_tab(index)` | `index`: one-based tab index | Switch to the selected tab. |
| `actions.new_tab()` | None | Create a tab. |
| `actions.open_or_reload_plugin(url, x=?, y=?, w=?, h=?)` | `url`: plugin URL or alias; coordinates: cells or percentage strings | Open a focused floating plugin pane, or reload the pane previously opened for this URL. |

`open_or_reload_plugin` defaults to a centered pane covering 50% of screen width and height. `x` and `y` are optional; `w` and `h` default to `"50%"`. Repeated clicks float, reposition, focus, and reload the tracked pane. Closing it makes the next click open a new pane.

```jinja
{% call Button(on_click=actions.open_or_reload_plugin("session-manager")) %}
  {{ session.name }}
{% endcall %}

{% call Button(
  on_click=actions.open_or_reload_plugin(
    "session-manager",
    x=0,
    y=0,
    w=32,
    h="100%"
  )
) %}
  {{ session.name }}
{% endcall %}
```

Button labels may contain styled text, but cannot contain `Flex`, `Button`, or other layout helpers. Only left click is mapped; right and middle click have no button action.

### OnOverflow

`OnOverflow` renders its body only when the content of its direct parent `Flex` exceeds the available width or height. The parent must use `overflow="scroll"`. The indicator stays fixed while the other children scroll.

`OnOverflow` has no props and each `Flex` accepts at most one.

```jinja
{% call Flex(grow=1, overflow="scroll") %}
  {% for tab in session.tabs %}
    {% call Button(
      on_click=actions.switch_tab(tab.index),
      focused=tab.active
    ) %}
      {{ tab.name }}
    {% endcall %}
  {% endfor %}

  {% call OnOverflow() %}
    {% call Button(on_click=actions.show_tab_list_modal()) %}
      ▼
    {% endcall %}
  {% endcall %}
{% endcall %}
```

The indicator is excluded from the initial overflow measurement. When overflow occurs, its natural size and one parent gap are reserved before the remaining children are laid out again.

`actions.show_tab_list_modal()` is illustrative. A host must register that action before the template can use it; this plugin currently exposes only the actions listed above.

### Clock

`Clock` renders time and asks the host to repaint at the next relevant boundary. Its optional `tz` argument accepts an IANA timezone and defaults to `env.TZ`, then UTC when `TZ` is unavailable.

```jinja
{{ Clock(format=" HH:MM ") }}
{{ Clock(format=" HH:MM ", tz="Europe/London") }}
```

| Prop | Type / values | Default | Guide |
|---|---|---|---|
| `format` | String | Required | Time pattern using friendly tokens or Chrono `strftime` directives. |

```jinja
{{ Clock(format="HH:MM") }}
{{ Clock(format="HH:MM:SS") | dim }}
{{ Clock(format="%Y-%m-%d %H:%M") }}
```

Formats containing `SS` or `%S` repaint at the next second boundary. Other formats repaint at the next minute boundary. `system.time | format("HH:MM")` formats the current frame timestamp without scheduling another repaint.

Friendly format tokens:

| Token | Meaning |
|---|---|
| `YYYY` | Four-digit year |
| `YY` | Two-digit year |
| `HH` | Hour, 24-hour clock |
| `MM` | Minute |
| `SS` | Second |

### Text filters

Filters transform text before layout. They can be chained.

| Filter | Props | Result |
|---|---|---|
| `bold` | None | Bold text, reset after value. |
| `dim` | None | Dim text, reset after value. |
| `fg(color)` | `color`: `rgb:R,G,B` or `index:N` | Set foreground, then reset it. |
| `bg(color)` | `color`: `rgb:R,G,B` or `index:N` | Set background, then reset it. |
| `format(pattern)` | `pattern`: time format string | Format a Unix timestamp in local time. |

```jinja
{{ "active" | bold | fg(theme.active_text) | bg(theme.active_background) }}
{{ "inactive" | dim | fg(theme.muted_text) }}
{{ system.time | format("HH:MM") }}
```

ANSI styling is measured and clipped by visible terminal cells, so escape sequences do not consume layout width.

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

Template source loading, environment and theme context, helper registration, Flex layout, ANSI clipping, and typed hitboxes come from the shared [`zellij-template-render`](../zellij-template-render) crate. Tab data, actions, Zellij colour conversion, and button presentation remain owned by this plugin.

## Migrating old templates

`Tab` and `Stack` no longer exist.

Replace:

```jinja
{% call Tab(index=tab.index, label=tab.name) %}{% endcall %}
```

With:

```jinja
{% call Button(
  on_click=actions.switch_tab(tab.index),
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
PUBLISH_TARGET=zellij-tabbar \
PUBLISH_CHANNEL=latest \
PUBLISH_VERSION=0.1.0 \
PUBLISH_RELEASE_TAG=zellij-tabbar-v0.1.0 \
PUBLISH_REF="$(git rev-parse HEAD)" \
moon run zellij-tabbar:publish
```

Set `GITHUB_REPOSITORY=owner/repo` when the current checkout has no GitHub remote. Normal publication runs through `.github/workflows/publish.yml`.
