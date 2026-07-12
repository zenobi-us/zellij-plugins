# zellij-template-render

Reusable MiniJinja terminal renderer for Zellij plugins.

The crate provides:

- nested `Flex` row and column layouts with fixed cell gaps
- typed `Button` actions and two-dimensional click hitboxes
- focus-following overflow
- ANSI-aware measurement and clipping
- `bold`, `dim`, `fg`, `bg`, and time-format filters

Plugins own template data, action semantics, and button presentation. The renderer does not depend on `zellij-tile`.

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

## Template sources

The renderer accepts inline source as `&str` or a named template from a preconfigured MiniJinja `Environment`. It deliberately does not decide where templates come from.

Use these terms consistently:

- **inline template**: source supplied directly through plugin configuration
- **embedded template**: a named source file bundled into the plugin WASM at build time
- **external template**: a source file read from the host filesystem at plugin load time

An embedded template is no longer read from disk at runtime. `minijinja-embed` packages template sources inside the WASM and loads them into a MiniJinja environment.

### Ownership boundary

The plugin host owns template selection and filesystem access. This crate owns template evaluation and terminal layout.

```text
plugin configuration
  ├─ inline template ─────────────────────→ Renderer::render(...)
  ├─ external template → read from host ──→ Renderer::render(...)
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

A native host can load an external template with `std::fs::read_to_string`. A Zellij WASI plugin reads files through its `/host` mount, so path interpretation and required Zellij permissions remain host concerns:

```rust
use std::path::Path;

let source = std::fs::read_to_string(Path::new("/host").join(configured_path))?;
```

Host implementations should:

- reject configuration containing both inline and external template settings
- load external templates once during plugin load unless hot reload is explicitly required
- report read and parse failures instead of silently falling back to the embedded default
- define whether configured paths are relative to Zellij's host folder; arbitrary absolute host paths can require `/host` remapping and `FullHdAccess`

### Includes and inheritance

Basic external file loading still requires no renderer-specific abstraction: read the file, then pass its contents to `Renderer::render`.

Embedded `{% include %}`, `{% extends %}`, and import relationships work through `Renderer::render_named` when every referenced template is present in the loaded bundle. Runtime template directories can use the same method with `Environment::set_loader` or `minijinja::path_loader`.
