# Glossary

## Template Globals

Template-visible snapshot used to produce a frame. `session`, `system`, `theme`, and `actions` are top-level globals. `theme` contains semantic colour tokens from the active Zellij theme. `actions` exposes opaque click-action constructors. Rendering does not perform plugin state changes.

## Rendered Frame

Complete render result for one viewport. It contains terminal lines and a same-coordinate two-dimensional hitbox grid.

## Click Action

Opaque, typed operation attached to button cells in a rendered frame. State dispatches it only after a left click on a matching hitbox.
