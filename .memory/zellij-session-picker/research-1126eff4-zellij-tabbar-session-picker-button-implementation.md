# Tabbar button for Zellij session picker

Access date: 2026-07-15

## Thinking

Question decomposed into four owned layers:

1. How `zellij-tabbar` turns template `Button` cells into host actions.
2. Which pinned `zellij-tile` API opens a plugin pane, and which permission gates it.
3. Which URL should identify the session picker while preserving user overrides.
4. Whether “open” must also reproduce Zellij's default launch-or-focus behavior.

Evidence standard: repository source for local behavior; Zellij documentation and source at the exact revision pinned by this repository for host behavior. One owner-level source is stronger than padding each API claim with unrelated secondary sources.

Skills used:

- `eng-research`: primary-source research and repo-local note workflow.
- `devtools-codemapper`: AST-oriented repository mapping before line-level inspection.
- `shells-zellij`: terminology and Zellij session/plugin context.
- repository skill `.agents/skills/zellij-plugin-dev/SKILL.md`: local Zellij plugin-development references; factual conclusions below were checked against upstream source rather than trusting the skill text.

Scope: design/research only. No production code changed.

## Research

### Existing tabbar action path

`zellij-tabbar` already has the full clickable-button pipeline. Templates produce typed actions through `ActionRegistry`; rendered cells retain those actions as hitboxes; `Event::Mouse(Mouse::LeftClick(...))` resolves the hitbox and dispatches the `ClickAction` variant. Current variants are only `SwitchTab(usize)` and `NewTab`. [Local `render.rs` lines 17-24, 44-79](https://github.com/zenobi-us/zellij-plugins/blob/7f3bd87197dc/pkgs/zellij-tabbar/src/render.rs#L17-L24) [Local `render.rs` lines 44-79](https://github.com/zenobi-us/zellij-plugins/blob/7f3bd87197dc/pkgs/zellij-tabbar/src/render.rs#L44-L79) [Local `main.rs` lines 53-84](https://github.com/zenobi-us/zellij-plugins/blob/7f3bd87197dc/pkgs/zellij-tabbar/src/main.rs#L53-L84)

The public template syntax already requires `on_click` to be a value returned by `actions.*`; constructed strings are rejected. This typed boundary should remain intact. [Local README lines 218-243](https://github.com/zenobi-us/zellij-plugins/blob/7f3bd87197dc/pkgs/zellij-tabbar/README.md#L218-L243)

Minimal local extension:

```rust
// render.rs
pub(crate) enum ClickAction {
    SwitchTab(usize),
    NewTab,
    OpenSessionManager,
}

// ActionRegistry
.with("open_session_manager", |args| {
    if !args.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidOperation,
            "open_session_manager expects no arguments",
        ));
    }
    Ok(ClickAction::OpenSessionManager)
})
```

Template use:

```jinja
{{ Button(on_click=actions.open_session_manager(), label="sessions") }}
```

Dispatch belongs beside the existing `SwitchTab`/`NewTab` match in `main.rs`. No renderer abstraction or generic arbitrary-plugin action is needed.

### Pinned Zellij API

This package pins both `zellij-tile` dependencies to Zellij revision `4bfda976b110b5d3182fca004ee7031f6322253e`; compatibility must be judged against that revision, not current `main`. [Local `Cargo.toml` lines 8-16](https://github.com/zenobi-us/zellij-plugins/blob/7f3bd87197dc/pkgs/zellij-tabbar/Cargo.toml#L8-L16)

At that revision, `zellij_tile::prelude::*` exports `open_plugin_pane_floating`. Signature:

```rust
pub fn open_plugin_pane_floating(
    plugin_url: &str,
    configuration: BTreeMap<String, String>,
    coordinates: Option<FloatingPaneCoordinates>,
    context: BTreeMap<String, String>,
) -> Option<PaneId>
```

It opens a focused floating plugin pane and returns its pane ID when successful. [Zellij `zellij-tile/src/shim.rs` lines 1010-1055](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-tile/src/shim.rs#L1010-L1055)

Smallest dispatch:

```rust
ClickAction::OpenSessionManager => {
    open_plugin_pane_floating(
        "session-manager",
        BTreeMap::new(),
        None,
        BTreeMap::new(),
    );
},
```

The server turns that command into `Action::NewFloatingPluginPane` with `no_focus: false`, so the picker receives focus. [Zellij `zellij-server/src/plugins/zellij_exports.rs` lines 1164-1211](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-server/src/plugins/zellij_exports.rs#L1164-L1211)

### Permission change is mandatory

`OpenPluginPaneFloating` is gated by `PermissionType::OpenTerminalsOrPlugins`, not by the tabbar's existing `ChangeApplicationState` grant. [Zellij `zellij-server/src/plugins/zellij_exports.rs` lines 5470-5485](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-server/src/plugins/zellij_exports.rs#L5470-L5485) [Official permission reference](https://zellij.dev/documentation/plugin-api-permissions)

Current `State::load` requests `ReadApplicationState`, `ChangeApplicationState`, and conditionally `FullHdAccess`; it does not request `OpenTerminalsOrPlugins`. [Local `main.rs` lines 24-33](https://github.com/zenobi-us/zellij-plugins/blob/7f3bd87197dc/pkgs/zellij-tabbar/src/main.rs#L24-L33)

Required change:

```rust
PermissionType::OpenTerminalsOrPlugins,
```

The README's permission list and sample `permissions.kdl` block must gain the same permission. Current documentation only lists/caches the existing permissions. [Local README lines 65-96](https://github.com/zenobi-us/zellij-plugins/blob/7f3bd87197dc/pkgs/zellij-tabbar/README.md#L65-L96)

Because this project documents pre-seeding the permission cache for a one-row plugin, code-only implementation would appear broken for existing users. Documentation update is part of the minimum complete change.

### Use the alias, not the built-in URL

Use `"session-manager"`, not `"zellij:session-manager"`.

Zellij defines `session-manager` as a plugin alias whose default target is `zellij:session-manager`; users may replace that alias with another implementation. [Official session-manager alias reference](https://zellij.dev/documentation/session-manager-alias) [Official plugin-alias reference](https://zellij.dev/documentation/plugin-aliases) [Pinned default configuration lines 229-241](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-utils/assets/config/default.kdl#L229-L241)

The pinned server parses a bare value as `RunPluginOrAlias::Alias`, and the plugin subsystem later populates it from configured aliases. Therefore `open_plugin_pane_floating("session-manager", ...)` respects user configuration. [Zellij `zellij-utils/src/input/layout.rs` lines 104-190](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-utils/src/input/layout.rs#L104-L190) [Zellij `zellij-server/src/plugins/mod.rs` lines 362-376](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-server/src/plugins/mod.rs#L362-L376)

The first-party target is the correct picker: its default active screen is `AttachToSession`, it subscribes to `SessionUpdate`, refreshes the session list, and names its pane `Session Manager`. [Zellij session-manager `main.rs` lines 20-89](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/default-plugins/session-manager/src/main.rs#L20-L89)

### Open versus launch-or-focus

Zellij's default `Ctrl o`, `w` binding uses `LaunchOrFocusPlugin "session-manager"` with `floating true` and `move_to_focused_tab true`. [Pinned default configuration lines 114-124](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-utils/assets/config/default.kdl#L114-L124)

`open_plugin_pane_floating` instead creates `Action::NewFloatingPluginPane`. Repeated tabbar clicks can therefore create multiple session-manager panes; it does not provide the default keybinding's focus-existing semantics. [Zellij server implementation lines 1189-1204](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-server/src/plugins/zellij_exports.rs#L1189-L1204)

This is the only meaningful design fork:

- If requirement means “a button opens the picker,” use `open_plugin_pane_floating`; smallest diff, official API, alias-aware.
- If requirement means exact parity with default keybinding, add lifecycle state: store returned `PaneId`, focus it on later clicks, subscribe to pane updates, and clear stale IDs after closure. That is materially more code and should be driven by observed duplicate-pane harm.

The pinned API exposes generic `run_action`, gated by `RunActionsAsUser`, but no dedicated `launch_or_focus_plugin` host function was found. Building a generic action bridge would widen capability and template surface for one fixed button; reject that design unless arbitrary plugin actions become a real requirement. [Zellij `run_action` shim lines 2899-2905](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-tile/src/shim.rs#L2899-L2905) [Zellij permission mapping lines 5646-5655](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-server/src/plugins/zellij_exports.rs#L5646-L5655)

### Flow

```text
[MiniJinja template]
        |
        | actions.open_session_manager()
        v
[ActionRegistry validates zero args]
        |
        v
[ClickAction::OpenSessionManager stored in rendered hitbox]
        |
        | Mouse::LeftClick(row, col)
        v
[State::update resolves hitbox]
        |
        v
[open_plugin_pane_floating("session-manager", ...)]
        |
        | requires OpenTerminalsOrPlugins
        v
[Zellij resolves configured session-manager alias]
        |
        v
[focused floating Session Manager / replacement picker]
```

## Verification

| Claim | Owner evidence | Confidence | Contradictions / limits |
|---|---|---:|---|
| Existing template buttons already carry typed host actions through hitboxes | This repo: `render.rs`, `main.rs`, README | HIGH | None found. |
| Pinned API can open a floating plugin and return `PaneId` | Zellij shim and server at exact pinned revision | HIGH | Current online docs may describe newer APIs; pinned source controls compatibility. |
| Opening plugins requires `OpenTerminalsOrPlugins` | Zellij server permission map plus official permission docs | HIGH | Existing `ChangeApplicationState` is insufficient for this command. |
| Bare `session-manager` preserves alias replacement | Official alias docs, pinned default config, parser and plugin-loader source | HIGH | Passing `zellij:session-manager` would bypass user alias replacement. |
| First-party plugin is session picker/manager | Official alias docs and built-in plugin source | HIGH | Name “session picker” is informal; upstream calls it `session-manager`. |
| Repeated direct opens may duplicate panes | Server maps call to `NewFloatingPluginPane`; default binding separately uses `LaunchOrFocusPlugin` | HIGH | Runtime behavior was not exercised interactively in a Zellij session during this research. |
| Exact launch-or-focus parity needs extra state or a broader action bridge | Pinned public shim inspection | MEDIUM | A lower-level/private integration may exist elsewhere, but no dedicated public shim function exists at pinned revision. |

Validation performed:

- Confirmed repository HEAD used for local permalinks: `7f3bd87197dc`.
- Confirmed Cargo pins Zellij revision `4bfda976b110b5d3182fca004ee7031f6322253e`.
- Inspected the locally fetched Cargo checkout for that exact revision.
- Cross-checked API name, server action mapping, permission mapping, alias parser, default alias, default keybinding, and first-party session-manager implementation.
- No compile/runtime test performed because no implementation was requested.

Source ledger:

| Source | Owner / publisher | Type | Accessed |
|---|---|---|---|
| [Local tabbar `main.rs`](https://github.com/zenobi-us/zellij-plugins/blob/7f3bd87197dc/pkgs/zellij-tabbar/src/main.rs) | `zenobi-us/zellij-plugins` | Repository source | 2026-07-15 |
| [Local tabbar `render.rs`](https://github.com/zenobi-us/zellij-plugins/blob/7f3bd87197dc/pkgs/zellij-tabbar/src/render.rs) | `zenobi-us/zellij-plugins` | Repository source | 2026-07-15 |
| [Local tabbar README](https://github.com/zenobi-us/zellij-plugins/blob/7f3bd87197dc/pkgs/zellij-tabbar/README.md) | `zenobi-us/zellij-plugins` | First-party project docs | 2026-07-15 |
| [Pinned Zellij shim](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-tile/src/shim.rs) | Zellij project | Upstream API source | 2026-07-15 |
| [Pinned Zellij plugin exports](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-server/src/plugins/zellij_exports.rs) | Zellij project | Upstream server source | 2026-07-15 |
| [Pinned plugin URL/alias model](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-utils/src/input/layout.rs) | Zellij project | Upstream source | 2026-07-15 |
| [Pinned default configuration](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-utils/assets/config/default.kdl) | Zellij project | Upstream first-party config | 2026-07-15 |
| [Pinned first-party session-manager](https://github.com/zellij-org/zellij/tree/4bfda976b110b5d3182fca004ee7031f6322253e/default-plugins/session-manager) | Zellij project | Upstream plugin source | 2026-07-15 |
| [Plugin API commands](https://zellij.dev/documentation/plugin-api-commands) | Zellij project | Official docs | 2026-07-15 |
| [Plugin API permissions](https://zellij.dev/documentation/plugin-api-permissions) | Zellij project | Official docs | 2026-07-15 |
| [Session-manager alias](https://zellij.dev/documentation/session-manager-alias) | Zellij project | Official docs | 2026-07-15 |
| [Plugin aliases](https://zellij.dev/documentation/plugin-aliases) | Zellij project | Official docs | 2026-07-15 |

## Insights

[bias: smallest stable public API surface]

1. Add a dedicated `open_session_manager` template action. Do not add generic `open_plugin(url)`: current need is fixed, generic URLs expand trust and validation concerns.
2. Dispatch through `open_plugin_pane_floating("session-manager", ...)` so user alias overrides continue working.
3. Request and document `OpenTerminalsOrPlugins`; omission is guaranteed permission failure, not optional cleanup.
4. Accept new-pane-on-click semantics for first version. Add pane-ID tracking only when exact launch-or-focus behavior is explicitly required.
5. Tests should cover action argument validation and mapping to `ClickAction::OpenSessionManager`. Host-call behavior needs either a tiny dispatch seam or interactive/manual Zellij verification; do not build a broad mocking layer for one call.

Likely files for implementation:

- `pkgs/zellij-tabbar/src/render.rs`
- `pkgs/zellij-tabbar/src/main.rs`
- `pkgs/zellij-tabbar/templates/main.jinja` only if button belongs in default layout
- `pkgs/zellij-tabbar/README.md`

## Summary

Support path is small and already fits existing architecture: register typed `actions.open_session_manager()`, add `ClickAction::OpenSessionManager`, dispatch it with `open_plugin_pane_floating("session-manager", empty config, no coordinates, empty context)`, and add `OpenTerminalsOrPlugins` to requested/documented permissions.

Using alias `session-manager` is critical because it preserves configured replacement plugins. Direct floating open does not match default keybinding's launch-or-focus behavior and can create duplicates; defer lifecycle tracking unless exact parity is required.

Implementation follow-up, 2026-07-15: production semantics selected tracked-pane reload rather than open-only. `actions.open_or_reload_plugin(url, x=?, y=?, w=?, h=?)` now opens a centered 50% floating pane by default, then floats, repositions, focuses, and reloads that tracked pane on later clicks. `PaneUpdate` removes closed pane IDs so the next click reopens them.
