# Research: clickable tabbar button for Zellij session picker

Access date: 2026-07-15

Repository revision: `7f3bd87197dc187480f1c769d8ad8d6ec622ea1c`

Pinned Zellij revision: `4bfda976b110b5d3182fca004ee7031f6322253e` (`zellij-tile`/`zellij-tile-utils` 0.45.0)

## Thinking

Question split into four evidence layers:

1. Existing `zellij-tabbar` button, rendering, hitbox, and click-dispatch architecture.
2. Plugin API command and permission available at this repository's pinned Zellij revision.
3. Official session-manager alias, built-in plugin location, and floating launch contract.
4. Smallest packaging-compatible change, test boundary, and known limitation.

Skills used:

- `devtools-codemapper` — mapped Rust/Markdown symbols and traced `button_marker -> decode_action` plus renderer call relationships without relying only on text search.
- `eng-research` — enforced primary-source ownership and one repo-local Markdown deliverable.
- `experts-language-specialists-rust-engineer` — checked Cargo revision pinning, public Rust API signatures, enum exhaustiveness, and test implications.
- `shells-zellij` — supplied Zellij vocabulary, then all material claims were reverified against official docs/source.
- `devtools-code-library-docs` — preferred the cached exact upstream checkout and repository-native documentation/source over secondary summaries.
- Repo-local `.agents/skills/zellij-plugin-dev/SKILL.md` — identified relevant plugin lifecycle, permissions, and event surfaces; treated as navigation only, not primary evidence.
- `devtools-lynx-web-search` — selected for terminal retrieval of official docs; `lynx` was unavailable, so official pages were fetched directly and checked against pinned source.

[bias: smallest secure API surface] Prefer Zellij's dedicated `open_plugin_pane_floating` command over synthesizing a general user action. It needs the narrower `OpenTerminalsOrPlugins` permission and no new dependency.

## Research

### 1. Existing tabbar button/event architecture

Current flow is already generic enough for another typed click action:

```text
MiniJinja template
  Button(on_click=actions.<name>(...))
        |
        v
TabBarRenderer::from_configuration
  ActionRegistry decoder -> ClickAction variant
        |
        v
zellij-template-render::button_marker
  validates action token + asks present_button for styled label
        |
        v
layout::text_canvas
  paints label cells with cloned Option<ClickAction>
        |
        v
Frame { lines, hitboxes[row][col] }
        |
        v
State::render stores frame and prints lines
        |
        v
Event::Mouse(Mouse::LeftClick(row, col))
  indexes frame.hitboxes[row][col]
        |
        +--> SwitchTab(index) -> switch_tab_to(index)
        +--> NewTab          -> new_tab(...)
        +--> proposed OpenSessionManager
                                |
                                v
                     open_plugin_pane_floating(
                       "session-manager", {}, None, {}
                     )
                                |
                                v
                  configured alias -> floating plugin pane
```

Evidence:

- `State::load` already requests permissions, disables selectability, and subscribes to `EventType::Mouse`; `State::update` resolves the clicked cell from `frame.hitboxes` and dispatches `ClickAction`. Local: `pkgs/zellij-tabbar/src/main.rs:24-50,53-84`; [permalink](https://github.com/zenobi-us/zellij-plugins/blob/7f3bd87197dc187480f1c769d8ad8d6ec622ea1c/pkgs/zellij-tabbar/src/main.rs#L24-L84). Confidence: **HIGH**.
- `ClickAction` currently has only `SwitchTab(usize)` and `NewTab`; `ActionRegistry` exposes only `actions.switch_tab` and `actions.new_tab`. Local: `pkgs/zellij-tabbar/src/render.rs:17-24,44-80`; [permalink](https://github.com/zenobi-us/zellij-plugins/blob/7f3bd87197dc187480f1c769d8ad8d6ec622ea1c/pkgs/zellij-tabbar/src/render.rs#L17-L80). Confidence: **HIGH**.
- `present_button` and `style_button` contain exhaustive matches over those two variants, so adding one variant requires updating focus, label, alternation, and foreground branches. Local: `pkgs/zellij-tabbar/src/render.rs:127-211`; [permalink](https://github.com/zenobi-us/zellij-plugins/blob/7f3bd87197dc187480f1c769d8ad8d6ec622ea1c/pkgs/zellij-tabbar/src/render.rs#L127-L211). Confidence: **HIGH**.
- Shared renderer registers template action functions, turns their results into opaque action tokens, decodes `Button(on_click=...)`, and emits typed button nodes. Local: `pkgs/zellij-template-render/src/lib.rs:20-49,96-176`, `pkgs/zellij-template-render/src/template.rs:180-276`; [registry permalink](https://github.com/zenobi-us/zellij-plugins/blob/7f3bd87197dc187480f1c769d8ad8d6ec622ea1c/pkgs/zellij-template-render/src/lib.rs#L20-L176), [marker permalink](https://github.com/zenobi-us/zellij-plugins/blob/7f3bd87197dc187480f1c769d8ad8d6ec622ea1c/pkgs/zellij-template-render/src/template.rs#L180-L276). Confidence: **HIGH**.
- Layout converts each rendered button cell into `Some(action)` and preserves those cells in `Frame.hitboxes`. Local: `pkgs/zellij-template-render/src/layout.rs:8-76,78-93`; [permalink](https://github.com/zenobi-us/zellij-plugins/blob/7f3bd87197dc187480f1c769d8ad8d6ec622ea1c/pkgs/zellij-template-render/src/layout.rs#L8-L93). Confidence: **HIGH**.
- Default template currently renders session name as plain text, tab names as switch buttons, and `+` as new-tab button. Local: `pkgs/zellij-tabbar/src/template/main.jinja:1-8`, `pkgs/zellij-tabbar/src/template/tabs.jinja:1-7`; [main permalink](https://github.com/zenobi-us/zellij-plugins/blob/7f3bd87197dc187480f1c769d8ad8d6ec622ea1c/pkgs/zellij-tabbar/src/template/main.jinja#L1-L8), [tabs permalink](https://github.com/zenobi-us/zellij-plugins/blob/7f3bd87197dc187480f1c769d8ad8d6ec622ea1c/pkgs/zellij-tabbar/src/template/tabs.jinja#L1-L7). Confidence: **HIGH**.

### 2. Exact pinned API and permissions

Repository pins both Zellij crates to one Git revision, not an unconstrained crates.io release:

- `pkgs/zellij-tabbar/Cargo.toml:13-14` pins `zellij-tile` and `zellij-tile-utils` to `4bfda976...`; [permalink](https://github.com/zenobi-us/zellij-plugins/blob/7f3bd87197dc187480f1c769d8ad8d6ec622ea1c/pkgs/zellij-tabbar/Cargo.toml#L13-L14).
- `Cargo.lock:2608-2624` resolves both as version `0.45.0` at the same revision; [permalink](https://github.com/zenobi-us/zellij-plugins/blob/7f3bd87197dc187480f1c769d8ad8d6ec622ea1c/Cargo.lock#L2608-L2624). Confidence: **HIGH**.

At that exact revision, `zellij_tile::prelude::*` exports:

```rust
pub fn open_plugin_pane_floating(
    plugin_url: &str,
    configuration: BTreeMap<String, String>,
    coordinates: Option<FloatingPaneCoordinates>,
    context: BTreeMap<String, String>,
) -> Option<PaneId>
```

The implementation documents a new floating plugin pane and returns its pane ID. Upstream: [`zellij-tile/src/shim.rs:1035-1055`](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-tile/src/shim.rs#L1035-L1055). Official docs expose the same signature and name: [Plugin API commands — `open_plugin_pane_floating`](https://zellij.dev/documentation/plugin-api-commands#open_plugin_pane_floating). Confidence: **HIGH**.

Required permission is `PermissionType::OpenTerminalsOrPlugins`:

- Pinned server command-to-permission mapping includes `PluginCommand::OpenPluginPaneFloating`. Upstream: [`zellij-server/src/plugins/zellij_exports.rs:5470-5486`](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-server/src/plugins/zellij_exports.rs#L5470-L5486).
- Official permission definition: “Start new terminals and plugins.” [Plugin API permissions — `OpenTerminalsOrPlugins`](https://zellij.dev/documentation/plugin-api-permissions#openterminalsorplugins). Confidence: **HIGH**.

`ChangeApplicationState`, already requested by tabbar, does **not** authorize this command. The pinned server maps these command families separately. Therefore the permission list must explicitly add `OpenTerminalsOrPlugins`. Confidence: **HIGH**.

### 3. Session-manager/session-picker identity and launch semantics

Official naming is `session-manager`, not `session-picker`:

- Zellij's official guide states that alias `session-manager` defaults to internal URL `zellij:session-manager`, opens as a floating pane, and may be replaced by user configuration. [The session-manager alias](https://zellij.dev/documentation/session-manager-alias). Confidence: **HIGH**.
- Pinned default config launches `LaunchOrFocusPlugin "session-manager"` with `floating true` and `move_to_focused_tab true`. Upstream: [`zellij-utils/assets/config/default.kdl:114-124`](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-utils/assets/config/default.kdl#L114-L124). Confidence: **HIGH**.
- The same config maps alias `session-manager` to `zellij:session-manager`. Upstream: [`zellij-utils/assets/config/default.kdl:229-238`](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-utils/assets/config/default.kdl#L229-L238). Confidence: **HIGH**.
- Alias parsing treats a string without a recognized URL scheme as a `PluginAlias`, preserving later config resolution. Upstream: [`zellij-utils/src/input/layout.rs:159-184`](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-utils/src/input/layout.rs#L159-L184). Confidence: **HIGH**.

Therefore the tabbar should pass `"session-manager"`, not `"zellij:session-manager"`. Alias use preserves user replacement and matches Zellij's own default keybinding. Confidence: **HIGH**.

Important semantic mismatch:

- `open_plugin_pane_floating` constructs `Action::NewFloatingPluginPane`; it does not use `Action::LaunchOrFocusPlugin`. Upstream: [`zellij-server/src/plugins/zellij_exports.rs:1165-1204`](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-server/src/plugins/zellij_exports.rs#L1165-L1204).
- Zellij's default keybinding deliberately uses launch-or-focus semantics. `Action::LaunchOrFocusPlugin` is a distinct action. Upstream: [`zellij-utils/src/input/actions.rs:380-389`](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-utils/src/input/actions.rs#L380-L389).

Consequence: direct API is guaranteed to open a floating session-manager, but repeated clicks may create another instance instead of focusing an existing one. Pinned source does not show de-duplication in the `NewFloatingPluginPane` path. Confidence: **MEDIUM-HIGH**; runtime integration test should confirm behavior against the packaged Zellij binary.

Using `run_action(Action::LaunchOrFocusPlugin { ... })` would reproduce default keybinding semantics, but it requires broader `RunActionsAsUser` permission and constructing upstream internal action payload types. Pinned permission mapping: [`zellij-server/src/plugins/zellij_exports.rs:5651`](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-server/src/plugins/zellij_exports.rs#L5651). That is a worse first implementation unless focus-existing behavior is explicitly required. Confidence: **HIGH**.

### 4. Packaging/config compatibility and minimal path

Smallest viable production change:

1. Add `ClickAction::OpenSessionManager` in `pkgs/zellij-tabbar/src/render.rs`.
2. Register zero-argument template action `actions.open_session_manager()` beside `switch_tab` and `new_tab`.
3. Handle the new variant in `present_button`/`style_button` as an unfocused, non-tab action, matching `NewTab` styling.
4. Add `PermissionType::OpenTerminalsOrPlugins` to `State::load`.
5. Dispatch the click with:

   ```rust
   open_plugin_pane_floating(
       "session-manager",
       BTreeMap::new(),
       None,
       BTreeMap::new(),
   );
   ```

6. Make existing session-name area the button in `src/template/main.jinja`:

   ```jinja
   {% call Button(on_click=actions.open_session_manager()) %}
   {{ session.name }}
   {% endcall %}
   ```

Wrapping the existing session label is smaller than adding another status-bar segment and avoids consuming more horizontal space. It also creates a discoverable association: session name opens session manager. Confidence: **HIGH** as a repository-fit recommendation; UX preference remains subjective.

Documentation changes belong in existing `pkgs/zellij-tabbar/README.md` Button/action section, not a new architecture abstraction. The README already documents permission grants and custom `Button` syntax. Local: `pkgs/zellij-tabbar/README.md:65-96,214-268`; [permalink](https://github.com/zenobi-us/zellij-plugins/blob/7f3bd87197dc187480f1c769d8ad8d6ec622ea1c/pkgs/zellij-tabbar/README.md#L65-L96), [Button section](https://github.com/zenobi-us/zellij-plugins/blob/7f3bd87197dc187480f1c769d8ad8d6ec622ea1c/pkgs/zellij-tabbar/README.md#L214-L268). Confidence: **HIGH**.

No new crate, configuration key, plugin-path setting, or shared-renderer change is needed. `zellij-tile` already supplies the command, `BTreeMap` is already imported, and typed action/hitbox infrastructure already exists. Confidence: **HIGH**.

## Verification

Repository inspection and executable checks:

- `cargo test --workspace --target x86_64-unknown-linux-gnu` passed: 3 tabbar tests, 19 shared-renderer tests, and 0 doctest failures. Plain `cargo test --workspace` follows repository default `wasm32-wasip1` target and cannot execute the produced WASM test binary directly on this host (`Exec format error`); this is a harness limitation, not a test failure in Rust logic.
- Markdown validation found exactly one matching research file, all five required sections, source ledger, access date, confidence labels, and ASCII diagram. `git diff --check` passed, and all 25 distinct cited HTTP URLs returned status 200 on 2026-07-15.
- CodeMapper indexed 23 relevant Rust/Markdown files and identified the typed action/rendering symbols.
- `Mouse::LeftClick` occurs only in `pkgs/zellij-tabbar/src/main.rs`; no current unit test exercises host click dispatch.
- Existing Rust tests cover rendered actions/styling in `pkgs/zellij-tabbar/src/render.rs:233-353`; Bats coverage checks template model/layout/error rendering in `pkgs/zellij-tabbar/tests/template_e2e.bats:95-125`. Neither verifies launching another plugin. Confidence: **HIGH**.
- `docs/adr/` contains one terse architectural decision, not a reusable research-note convention: `docs/adr/0001-trust-external-templates-with-host-file-access.md:1-3`. Saving this note under `.memory/zellij-session-picker/` follows task instruction. Confidence: **HIGH**.

Recommended validation when production code is implemented:

1. Unit test that `actions.open_session_manager()` decodes to `ClickAction::OpenSessionManager` and paints a full button hitbox.
2. Refactor only the tiny host dispatch into a testable function if needed; assert the new variant maps to the launch call boundary. Do not introduce a general command bus.
3. Run `cargo test --workspace`.
4. Extend Bats/runtime fixture only if it can observe plugin-pane creation deterministically; verify first click opens a floating “Session Manager” and second click behavior is documented.
5. Test custom alias replacement:

   ```kdl
   plugins {
       session-manager location="file:/path/to/replacement.wasm"
   }
   ```

   The button must follow the alias.

Contradictions and uncertainty:

- Official session-manager contract says floating pane; no contradiction with `open_plugin_pane_floating`.
- Default Zellij keybinding says launch-or-focus; dedicated plugin API says open a **new** floating plugin pane. This is a real semantic difference, not documentation noise.
- Whether Zellij packaging or built-in plugin cache prevents duplicate visible session-manager instances is not established by the cited `NewFloatingPluginPane` path. Treat duplicate behavior as unverified until runtime test. Confidence: **MEDIUM**.
- Current official docs may advance beyond pinned revision. Exact compatibility claims above use the pinned `4bfda976...` source as authority; docs are corroboration.

## Insights

- Session label is already present and semantically correct trigger. Reusing it avoids another icon, another width-pressure branch, and another configuration option.
- Alias string is compatibility seam. Hardcoding built-in URL would silently break user replacement of session manager.
- Permission should match command, not perceived effect. `ChangeApplicationState` sounds broad but does not authorize opening plugin panes in pinned server mapping.
- Shared template renderer needs no modification. New behavior belongs to tabbar's `ClickAction` registry and host dispatch boundary.
- General `run_action` is tempting because it matches Zellij's keybinding exactly, but broader permission and upstream action construction are unnecessary debt for initial “open picker” requirement.

### Source ledger

All sources accessed 2026-07-15.

| Source | Publisher/owner | Type | Why kept |
|---|---|---|---|
| `pkgs/zellij-tabbar/src/main.rs:24-84` ([permalink](https://github.com/zenobi-us/zellij-plugins/blob/7f3bd87197dc187480f1c769d8ad8d6ec622ea1c/pkgs/zellij-tabbar/src/main.rs#L24-L84)) | zenobi-us/zellij-plugins | Repository source | Permission, subscription, hitbox lookup, click dispatch |
| `pkgs/zellij-tabbar/src/render.rs:17-211` ([permalink](https://github.com/zenobi-us/zellij-plugins/blob/7f3bd87197dc187480f1c769d8ad8d6ec622ea1c/pkgs/zellij-tabbar/src/render.rs#L17-L211)) | zenobi-us/zellij-plugins | Repository source | Typed actions, registry, styling exhaustiveness |
| `pkgs/zellij-template-render/src/lib.rs`, `template.rs`, `layout.rs` ([renderer](https://github.com/zenobi-us/zellij-plugins/blob/7f3bd87197dc187480f1c769d8ad8d6ec622ea1c/pkgs/zellij-template-render/src/lib.rs#L20-L176)) | zenobi-us/zellij-plugins | Repository source | End-to-end action token and hitbox construction |
| `pkgs/zellij-tabbar/Cargo.toml:13-14`, `Cargo.lock:2608-2624` | zenobi-us/zellij-plugins | Manifest/lockfile | Exact dependency revision/version |
| [`zellij-tile/src/shim.rs:1035-1055`](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-tile/src/shim.rs#L1035-L1055) | zellij-org | Upstream API source | Exact callable command signature at pinned revision |
| [`zellij-server/src/plugins/zellij_exports.rs:5470-5486`](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-server/src/plugins/zellij_exports.rs#L5470-L5486) | zellij-org | Upstream server source | Exact permission mapping |
| [`zellij-server/src/plugins/zellij_exports.rs:1165-1204`](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-server/src/plugins/zellij_exports.rs#L1165-L1204) | zellij-org | Upstream server source | New-floating-pane semantics |
| [`zellij-utils/assets/config/default.kdl:114-124,229-238`](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/zellij-utils/assets/config/default.kdl#L114-L124) | zellij-org | First-party default config | Official alias and default launch behavior |
| [`default-plugins/session-manager/src/main.rs`](https://github.com/zellij-org/zellij/blob/4bfda976b110b5d3182fca004ee7031f6322253e/default-plugins/session-manager/src/main.rs) | zellij-org | First-party plugin implementation | Confirms built-in plugin identity and UI behavior |
| [Plugin API commands](https://zellij.dev/documentation/plugin-api-commands#open_plugin_pane_floating) | zellij-org | Official documentation | Public command contract |
| [Plugin API permissions](https://zellij.dev/documentation/plugin-api-permissions#openterminalsorplugins) | zellij-org | Official documentation | Public permission description |
| [The session-manager alias](https://zellij.dev/documentation/session-manager-alias) | zellij-org | Official documentation | Alias, built-in URL, replacement, floating contract |

Dropped:

- Blogs, issue discussions, community plugin summaries, and Wikipedia — not authoritative for API ownership or pinned compatibility.
- Repo-local generated Zellij skill references — useful navigation, but not primary evidence for material claims.

## Summary

Recommendation: extend tabbar's existing typed `ClickAction` pipeline with zero-argument `OpenSessionManager`, request `OpenTerminalsOrPlugins`, and call `open_plugin_pane_floating("session-manager", ..., None, ...)`. Wrap existing session-name segment in that button and document the template action/permission.

Confidence: **HIGH** that this is smallest compatible change and opens configured session-manager as a floating pane on pinned Zellij 0.45.0. Confidence: **MEDIUM** on repeated-click behavior because dedicated API opens a new pane while Zellij's default keybinding uses launch-or-focus; verify and document duplicates before release.