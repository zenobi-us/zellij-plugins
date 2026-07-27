# Glossary

## Template Globals

Template-visible snapshot used to produce a frame. `session`, `system`, `theme`, and `actions` are top-level globals. `theme` contains semantic colour tokens from the active Zellij theme. `actions` exposes opaque click-action constructors. Rendering does not perform plugin state changes.

## External Template

User-supplied template entry point loaded from a host filesystem path when the plugin loads. Configuration names the path as the host sees it; WASI mount details are not part of the user-facing path. Relative paths are anchored to the Zellij configuration directory. The entry point may compose other host files without a directory boundary. External templates are trusted input with the same file-read reach as the plugin.

## External Template Reload

Automatic replacement of a loaded External Template and its composed host files after they change, without restarting the plugin. A reload may produce either a new Rendered Frame or a visible template error while later reloads continue.

## Rendered Frame

Complete render result for one viewport. It contains terminal lines, a same-coordinate two-dimensional hitbox grid, and an optional refresh request describing when dynamic content should render again.

## Click Action

Opaque, typed operation attached to button cells in a rendered frame. State dispatches it only after a left click on a matching hitbox.
