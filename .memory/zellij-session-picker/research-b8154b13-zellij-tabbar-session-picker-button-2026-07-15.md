# Research: clickable Zellij session-picker button in `zellij-tabbar`

Access date: 2026-07-15  
Repository revision inspected: `7f3bd87197dc187480f1c769d8ad8d6ec622ea1c`  
Pinned Zellij revision inspected: `4bfda976b110b5d3182fca004ee7031f6322253e` (`zellij-tile`/`zellij-tile-utils` 0.45.0)

## Thinking

### Question decomposition

1. Trace the existing tabbar template button, typed action, hitbox, and mouse-dispatch path.
2. Identify the pinned Zellij plugin command that can open the session picker and its required permission.
3. Verify the first-party session manager's alias, floating launch contract, and lifecycle behavior.
4. Determine the smallest repository change that preserves package, template, and configuration compatibility.

### Skills used

| Skill | Why it was used | Result |
|---|---|---|
| `eng-research` | Enforce primary-source research and one repo-local Markdown deliverable. | Restricted evidence to this repository, official Zellij documentation, and upstream Zellij source. |
| `devtools-codemapper` | Perform AST-based repository mapping and call tracing. | Mapped Rust/Markdown symbols; traced `decode_action -> button_marker -> render_tree_in` and `TabBarRenderer::render` dependencies. |
| `experts-language-specialists-rust-engineer` | Check Cargo revisions, Rust API shape, target, and test strategy. | Verified the git-pinned 0.45.0 API and `wasm32-wasip1` build path. |
| repo-local `zellij-plugin-dev` | Establish Zellij plugin lifecycle, permissions, events, and command vocabulary. | Used as navigation guidance only; factual claims below are tied back to official/upstream primary sources. |
| `devtools-code-library-docs` | Prefer local checked-out dependency source before summaries. | Reused Cargo's exact Zellij git checkout at the pinned revision. |
| `devtools-lynx-web-search` | Retrieve official documentation in text form. | Skill loaded, but `lynx` binary was unavailable. Official pages were retrieved through `web_search` and direct HTML fetch instead. |

### Evidence policy and note placement

Primary sources only: repository files at the inspected revision, Zellij's official user guide, and Zellij source at the exact pinned commit. Community posts, blogs, issue commentary, and Wikipedia were excluded.

At initial inspection, `docs/adr/` contained one terse architectural decision, not a research-note convention (`docs/adr/0001-trust-external-templates-with-host-file-access.md:1-3`), and no repo-local research notes were present. This note therefore follows the requested `.memory/zellij-session-picker/` fallback.

## Research

### 1. Existing tabbar button and event architecture

The tabbar already has the required generic mechanism. `ClickAction` is the typed value stored in rendered cells; current variants are `SwitchTab(usize)` and `NewTab`. `TabBarRenderer::from_configuration` exposes MiniJinja action constructors under `actions.switch_tab(...)` and `actions.new_tab()`. [`pkgs/zellij-tabbar/src/render.rs:17-24`](../../pkgs/zellij-tabbar/src/render.rs), [`pkgs/zellij-tabbar/src/render.rs:44-80`](../../pkgs/zellij-tabbar/src/render.rs). **Confidence: HIGH.**

The shared renderer does not execute actions. It validates that `Button.on_click` came from the typed action registry, asks the host to present the button, writes an action marker, parses that marker into a `Node::Button`, and paints the action into each visible terminal cell. `Canvas::into_frame` then emits coordinate-matched `Frame.hitboxes`. [`pkgs/zellij-template-render/src/template.rs:180-210`](../../pkgs/zellij-template-render/src/template.rs), [`pkgs/zellij-template-render/src/template.rs:212-276`](../../pkgs/zellij-template-render/src/template.rs), [`pkgs/zellij-template-render/src/layout.rs:19-78`](../../pkgs/zellij-template-render/src/layout.rs), [`pkgs/zellij-template-render/src/layout.rs:80-92`](../../pkgs/zellij-template-render/src/layout.rs). **Confidence: HIGH.**

`State::load` subscribes to `EventType::Mouse`. On `Mouse::LeftClick(row, col)`, `State::update` resolves the hitbox and dispatches the typed action to Zellij. This is the only production dispatch seam that needs another match arm. [`pkgs/zellij-tabbar/src/main.rs:24-50`](../../pkgs/zellij-tabbar/src/main.rs), [`pkgs/zellij-tabbar/src/main.rs:53-86`](../../pkgs/zellij-tabbar/src/main.rs). **Confidence: HIGH.**

Current default template renders session name as plain text, tab buttons, and a `+` new-tab button. [`pkgs/zellij-tabbar/src/template/main.jinja:1-8`](../../pkgs/zellij-tabbar/src/template/main.jinja), [`pkgs/zellij-tabbar/src/template/tabs.jinja:1-7`](../../pkgs/zellij-tabbar/src/template/tabs.jinja). README documents only the two current actions and states that visible button cells become left-click hitboxes. [`pkgs/zellij-tabbar/README.md:214-267`](../../pkgs/zellij-tabbar/README.md). **Confidence: HIGH.**

Current unit coverage verifies the default frame contains both current typed actions. Current Bats tests verify model/rendering/error behavior but do not inject mouse clicks or assert host command dispatch. [`pkgs/zellij-tabbar/src/render.rs:273-352`](../../pkgs/zellij-tabbar/src/render.rs), [`pkgs/zellij-tabbar/tests/template_e2e.bats:95-123`](../../pkgs/zellij-tabbar/tests/template_e2e.bats). **Confidence: HIGH.**

### 2. End-to-end state machine

```text
[Zellij TabUpdate / ModeUpdate / Timer]
                  |
                  v
        State::render(rows, cols)
                  |
                  v
      TabBarRenderer::render(model)
                  |
                  v
 MiniJinja Button(on_click=actions.*)
                  |
                  v
 ActionRegistry decoder -> ClickAction
                  |
                  v
 Node::Button -> Canvas cell.action
                  |
                  v
 Frame { lines, hitboxes[row][col] }
                  |
        terminal output displayed
                  |
                  v
       user left-clicks visible cell
                  |
                  v
 Event::Mouse(Mouse::LeftClick(row, col))
                  |
                  v
      hitboxes[row][col].clone()
                  |
        +---------+----------+----------------------+
        |                    |                      |
        v                    v                      v
 SwitchTab(index)         NewTab         OpenSessionManager
        |                    |                      |
 switch_tab_to()          new_tab()     open_plugin_pane_floating(
                                           "session-manager", ...)
                                                |
                                                v
                                  Zellij resolves configured alias
                                                |
                                                v
                               floating session-manager plugin pane
```

### 3. Pinned Zellij API and permissions

This repository pins both `zellij-tile` and `zellij-tile-utils` to commit `4bfda976...`; `Cargo.lock` resolves them as version 0.45.0. [`pkgs/zellij-tabbar/Cargo.toml:10-20`](../../pkgs/zellij-tabbar/Cargo.toml), [`Cargo.lock:2607-2633`](../../Cargo.lock). The upstream workspace at that commit also declares version 0.45.0. [Upstream workspace manifest](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/Cargo.toml#L68-L78). **Confidence: HIGH.**

The exact pinned `zellij_tile::prelude` exports `open_plugin_pane_floating`. Its signature is:

```rust
pub fn open_plugin_pane_floating(
    plugin_url: &str,
    configuration: BTreeMap<String, String>,
    coordinates: Option<FloatingPaneCoordinates>,
    context: BTreeMap<String, String>,
) -> Option<PaneId>
```

The function creates an `OpenPluginPaneFloating` plugin command. [Pinned `zellij-tile` source](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-tile/src/shim.rs#L1033-L1055). Official documentation gives the same signature, says it opens a **new** floating plugin pane, and assigns `OpenTerminalsOrPlugins`. [Official command documentation](https://zellij.dev/documentation/plugin-api-commands#open_plugin_pane_floating). **Confidence: HIGH.**

Pinned server permission mapping explicitly maps `OpenPluginPaneFloating` to `PermissionType::OpenTerminalsOrPlugins`. [Pinned permission mapping](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-server/src/plugins/zellij_exports.rs#L5470-L5486). Official permission documentation defines that permission as allowing plugins to start terminals and plugins. [Official permission documentation](https://zellij.dev/documentation/plugin-api-permissions#openterminalsorplugins). **Confidence: HIGH.**

Therefore `State::load` must request `PermissionType::OpenTerminalsOrPlugins` in addition to its current read/change permissions. Current source requests only `ReadApplicationState`, `ChangeApplicationState`, and conditional `FullHdAccess`. [`pkgs/zellij-tabbar/src/main.rs:24-33`](../../pkgs/zellij-tabbar/src/main.rs). **Confidence: HIGH.**

The server implementation parses the string through `RunPluginOrAlias::from_url`, converts a non-URL string into a plugin alias, then creates `Action::NewFloatingPluginPane` with focus enabled (`no_focus: false`). [Pinned floating-plugin implementation](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-server/src/plugins/zellij_exports.rs#L1165-L1204), [pinned alias parser](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-utils/src/input/layout.rs#L115-L190). **Confidence: HIGH.**

### 4. Official session-manager/session-picker location and semantics

Zellij's official name for the picker is the **session manager**. The official guide says the `session-manager` alias maps by default to `zellij:session-manager`, is normally loaded with `Ctrl o` then `w`, opens as a floating pane, and can be replaced by user configuration. [Official session-manager alias contract](https://zellij.dev/documentation/session-manager-alias). **Confidence: HIGH.**

The pinned default config confirms both layers:

- keybinding: `LaunchOrFocusPlugin "session-manager" { floating true; move_to_focused_tab true }`
- alias: `session-manager location="zellij:session-manager"`

[Pinned default keybinding](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-utils/assets/config/default.kdl#L114-L124), [pinned alias declaration](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-utils/assets/config/default.kdl#L229-L237). **Confidence: HIGH.**

Use `"session-manager"`, not `"zellij:session-manager"`, in tabbar code. Alias use preserves user replacement of the picker. Hard-coding the internal URL would bypass that supported configuration seam. **Confidence: HIGH.**

The first-party plugin identifies its pane as `Session Manager`, subscribes to session/key/visibility events, closes itself on bare `Esc`, and generally hides itself after selecting or creating a session. [Pinned first-party session manager load](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/default-plugins/session-manager/src/main.rs#L61-L95), [close behavior](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/default-plugins/session-manager/src/main.rs#L658-L680), [selection behavior](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/default-plugins/session-manager/src/main.rs#L1070-L1111). **Confidence: HIGH.**

### 5. Launch semantics contradiction

Official default keybinding uses **launch-or-focus**, while the smallest dedicated plugin API available to tabbar is documented and implemented as opening a **new** floating plugin pane. These semantics differ. [Default keybinding](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-utils/assets/config/default.kdl#L118-L123), [dedicated API](https://zellij.dev/documentation/plugin-api-commands#open_plugin_pane_floating), [server action construction](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-server/src/plugins/zellij_exports.rs#L1193-L1204). **Confidence: HIGH.**

The generic `run_action` command could theoretically dispatch `Action::LaunchOrFocusPlugin`, but it requires broader `RunActionsAsUser` permission. At the pinned revision, constructing that action also reaches into `RunPluginOrAlias`, an internal `zellij-utils::input::layout` type not re-exported by the tabbar's current `zellij-tile` prelude. Doing this cleanly would require an extra direct dependency on internal Zellij types or an upstream dedicated shim. [Official `run_action` documentation](https://zellij.dev/documentation/plugin-api-commands#run_action), [pinned action definition](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-utils/src/input/actions.rs#L372-L400), [pinned prelude exports](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-tile/src/prelude.rs#L1-L6). **Confidence: HIGH on API shape; MEDIUM on whether a future 0.45.x patch adds a cleaner public constructor.**

Repeated calls to `open_plugin_pane_floating` can create separate plugin instances rather than focusing an existing hidden manager. The pinned loader allocates a fresh plugin ID for each load. [Pinned loader](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-server/src/plugins/wasm_bridge.rs#L270-L319). Impact is usage-dependent because `Esc` closes the manager and session selection commonly switches sessions before hiding it. **Confidence: HIGH that the command is a new load; MEDIUM on practical accumulation frequency.**

### 6. Packaging and configuration compatibility

No dependency or build-system change is needed for the smallest path: `open_plugin_pane_floating` is already in the exact pinned `zellij-tile` dependency. The package already builds for `wasm32-wasip1`; its E2E harness explicitly requires Zellij 0.45.x. [`pkgs/zellij-tabbar/moon.yml:7-30`](../../pkgs/zellij-tabbar/moon.yml), [`pkgs/zellij-tabbar/tests/template_e2e.bats:3-11`](../../pkgs/zellij-tabbar/tests/template_e2e.bats). **Confidence: HIGH.**

Permission packaging must change. The README tells users to pre-grant permissions because a one-row tabbar cannot display the full interactive permission request. Its permission cache example and E2E fixture currently omit `OpenTerminalsOrPlugins`; both must be updated with production code. [`pkgs/zellij-tabbar/README.md:65-96`](../../pkgs/zellij-tabbar/README.md), [`pkgs/zellij-tabbar/tests/template_e2e.bats:51-57`](../../pkgs/zellij-tabbar/tests/template_e2e.bats). **Confidence: HIGH.**

Custom templates remain source-compatible if a new action is only additive. They can opt into `actions.open_session_manager()` later. Existing installed permission-cache entries are not runtime-compatible until users add the new permission; otherwise the click command is denied. **Confidence: HIGH.**

## Verification

### Commands and observed results

| Check | Result |
|---|---|
| `cm stats .`, `cm map . --full`, symbol queries/callers | Repository mapped; typed button/action path confirmed. |
| `cargo check --workspace --target wasm32-wasip1` | PASS. Existing workspace compiles against the pinned API. |
| `cargo test --workspace --target x86_64-unknown-linux-gnu` | PASS: 22 tests total, 0 failures. |
| `cd pkgs/zellij-tabbar && bats tests` | 3 tests discovered; all skipped because installed Zellij is 0.44.3 and harness requires 0.45.x. |
| `zellij --version` | `zellij 0.44.3`; unsuitable for runtime verification of the pinned 0.45 ABI. |
| Cargo manifest/lock vs upstream checkout | Both resolve commit `4bfda976...`, version 0.45.0. |

A first test attempt used the repository's default WASM target and failed with host `Exec format error`; rerunning with the host target, as `pkgs/zellij-tabbar/moon.yml` specifies, passed. This is a test invocation issue, not a repository defect.

### Recommended implementation verification

Minimum checks when implementation happens:

1. Extend `default_template_renders_buttons_and_actions` to assert a visible `ClickAction::OpenSessionManager` hitbox.
2. Run host unit tests with the explicit host target.
3. Run the WASM check/build.
4. Run Bats under Zellij 0.45.x after adding `OpenTerminalsOrPlugins` to its generated permission cache.
5. Manually verify the configured `session-manager` alias is honored by replacing it with another plugin in a temporary config.
6. Manually click twice after a selection that hides the manager; decide whether new-instance behavior is acceptable.

### Source ledger

| Exact URL or path | Publisher / owner | Source type | Used for | Accessed |
|---|---|---|---|---|
| `pkgs/zellij-tabbar/src/main.rs:24-86` | `zenobi-us/zellij-plugins` | Repository source | permissions, event subscription, click dispatch | 2026-07-15 |
| `pkgs/zellij-tabbar/src/render.rs:17-212` | `zenobi-us/zellij-plugins` | Repository source | typed actions, action registry, button presentation | 2026-07-15 |
| `pkgs/zellij-tabbar/src/template/main.jinja:1-8` | `zenobi-us/zellij-plugins` | Repository source | default visible layout | 2026-07-15 |
| `pkgs/zellij-template-render/src/template.rs:180-276` | `zenobi-us/zellij-plugins` | Repository source | action validation and button marker generation | 2026-07-15 |
| `pkgs/zellij-template-render/src/layout.rs:19-92` | `zenobi-us/zellij-plugins` | Repository source | cell actions and frame hitboxes | 2026-07-15 |
| `pkgs/zellij-tabbar/README.md:65-96,214-267` | `zenobi-us/zellij-plugins` | Repository documentation | permission-cache and template action contract | 2026-07-15 |
| `pkgs/zellij-tabbar/tests/template_e2e.bats:3-123` | `zenobi-us/zellij-plugins` | Repository test source | ABI gate, permission fixture, E2E coverage limits | 2026-07-15 |
| `pkgs/zellij-tabbar/Cargo.toml:10-20`; `Cargo.lock:2607-2633` | `zenobi-us/zellij-plugins` | Package manifests | exact dependency revision/version | 2026-07-15 |
| https://zellij.dev/documentation/plugin-api-commands#open_plugin_pane_floating | Zellij project | Official documentation | command signature and new-floating-pane semantics | 2026-07-15 |
| https://zellij.dev/documentation/plugin-api-permissions#openterminalsorplugins | Zellij project | Official documentation | permission meaning | 2026-07-15 |
| https://zellij.dev/documentation/session-manager-alias | Zellij project | Official documentation | alias contract, replacement, floating behavior | 2026-07-15 |
| https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-tile/src/shim.rs#L1033-L1055 | `zellij-org/zellij` | Pinned upstream API source | callable function exact shape | 2026-07-15 |
| https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-server/src/plugins/zellij_exports.rs#L1165-L1204 | `zellij-org/zellij` | Pinned upstream server source | alias parsing and new floating action | 2026-07-15 |
| https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-server/src/plugins/zellij_exports.rs#L5470-L5486 | `zellij-org/zellij` | Pinned upstream permission source | required permission mapping | 2026-07-15 |
| https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-utils/assets/config/default.kdl#L114-L124 | `zellij-org/zellij` | Pinned first-party config | official launch-or-focus keybinding | 2026-07-15 |
| https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-utils/assets/config/default.kdl#L229-L237 | `zellij-org/zellij` | Pinned first-party config | alias-to-built-in mapping | 2026-07-15 |
| https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/default-plugins/session-manager/src/main.rs#L61-L95 | `zellij-org/zellij` | Pinned first-party implementation | plugin identity and subscriptions | 2026-07-15 |
| https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/default-plugins/session-manager/src/main.rs#L658-L680 | `zellij-org/zellij` | Pinned first-party implementation | close behavior | 2026-07-15 |
| https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/default-plugins/session-manager/src/main.rs#L1070-L1111 | `zellij-org/zellij` | Pinned first-party implementation | session selection and hide behavior | 2026-07-15 |
| https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-server/src/plugins/wasm_bridge.rs#L270-L319 | `zellij-org/zellij` | Pinned upstream runtime source | fresh plugin ID/load behavior | 2026-07-15 |

Dropped sources: community guides, blogs, GitHub issues, and search snippets that did not own the API or implementation claim.

## Insights

### Smallest viable change

[bias: smallest diff with current pinned public API]

Add one specific typed action and use the existing button pipeline. Do not add a generic arbitrary-plugin action, a subprocess call, a new dependency, or a separate UI subsystem.

Proposed production shape:

```rust
// render.rs
ClickAction::OpenSessionManager

ActionRegistry::new()
    // existing actions...
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

```rust
// main.rs load()
PermissionType::OpenTerminalsOrPlugins
```

```rust
// main.rs click dispatch
ClickAction::OpenSessionManager => {
    open_plugin_pane_floating(
        "session-manager",
        BTreeMap::new(),
        None,
        BTreeMap::new(),
    );
}
```

```jinja
{# main.jinja: reuse existing session-name area instead of adding width #}
{% call Button(on_click=actions.open_session_manager()) %}
  {{ session.name }}
{% endcall %}
```

Treat `OpenSessionManager` like `NewTab` for focus and styling: not focused, no tab lookup, ordinary inactive ribbon colors. This keeps tab-specific logic confined to `SwitchTab`.

Required companion edits:

- Add `OpenTerminalsOrPlugins` to README permission list/cache block.
- Document `actions.open_session_manager()` in the available-actions table.
- Add `OpenTerminalsOrPlugins` to the Bats permission fixture.
- Extend the existing default-template unit test with the new hitbox assertion.

No `Cargo.toml`, `Cargo.lock`, Moon task, release artifact, or layout alias change is required.

### Known ceiling

`open_plugin_pane_floating` is open-new, not launch-or-focus. This is acceptable only if smallest viable behavior means “click opens the picker.” If strict parity with Zellij's default keybinding is required, do not hide this difference. Prefer an upstream `zellij-tile::launch_or_focus_plugin(...)` command; until that exists at the pinned API, alternatives require broader permission and tighter coupling to internal Zellij action types.

### Non-recommendations

- Do not hard-code `zellij:session-manager`; it defeats user alias replacement.
- Do not shell out to `zellij action launch-or-focus-plugin`; it adds `RunCommands`, process overhead, quoting/error paths, and dependence on host CLI availability.
- Do not add a generic `actions.open_plugin(url)` template API; that expands trust and permission surface beyond the requested feature.
- Do not add configuration for the alias name yet; `session-manager` is already the supported configuration seam.

## Summary

Recommendation: extend the existing typed `Button` action pipeline with `OpenSessionManager`, dispatch it through pinned `open_plugin_pane_floating("session-manager", ...)`, and request/document `OpenTerminalsOrPlugins`. Reuse the visible session-name area as the default button to avoid extra tabbar width. No new dependency or packaging change is needed.

Confidence is **HIGH** for repository architecture, exact 0.45.0 API compatibility, required permission, alias name, and floating launch behavior. Confidence is **MEDIUM** that open-new semantics are acceptable long-term because Zellij's default keybinding uses launch-or-focus and the first-party manager can hide rather than unload after selection. Runtime click verification remains blocked locally by installed Zellij 0.44.3; host tests and WASM compilation pass.
