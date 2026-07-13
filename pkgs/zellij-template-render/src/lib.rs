//! MiniJinja-driven terminal layout rendering for Zellij plugins.
//!
//! Hosts provide template data, typed actions, and button presentation policy.
//! The renderer owns template helpers, layout, clipping, and click hitboxes.

mod file_template;
mod host;
mod layout;
mod template;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

pub use file_template::environment as file_template_environment;
pub use host::{TemplateContext, TemplateEnvironment, TemplateHost, TemplateSource, TemplateTheme};
pub use minijinja::{context, Environment, Error, ErrorKind, Value};
use unicode_width::UnicodeWidthChar;

/// Typed decoder for one function exposed under the template `actions` object.
type ActionDecoder<A> = Arc<dyn Fn(&[Value]) -> Result<A, Error> + Send + Sync>;

/// Template action functions and their host-side typed decoders.
pub struct ActionRegistry<A> {
    decoders: BTreeMap<String, ActionDecoder<A>>,
}

impl<A> Default for ActionRegistry<A> {
    fn default() -> Self {
        Self {
            decoders: BTreeMap::new(),
        }
    }
}

impl<A> ActionRegistry<A> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &mut self,
        name: impl Into<String>,
        decode: impl Fn(&[Value]) -> Result<A, Error> + Send + Sync + 'static,
    ) {
        self.decoders.insert(name.into(), Arc::new(decode));
    }

    pub fn with(
        mut self,
        name: impl Into<String>,
        decode: impl Fn(&[Value]) -> Result<A, Error> + Send + Sync + 'static,
    ) -> Self {
        self.register(name, decode);
        self
    }
}

/// Terminal viewport dimensions in cells.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Viewport {
    pub rows: usize,
    pub cols: usize,
}

/// Viewport output and coordinate-matched typed click targets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame<A> {
    pub lines: Vec<String>,
    pub hitboxes: Vec<Vec<Option<A>>>,
    /// Earliest delay requested by template helpers before rendering again.
    pub refresh_after: Option<Duration>,
}

impl<A> Default for Frame<A> {
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            hitboxes: Vec::new(),
            refresh_after: None,
        }
    }
}

/// Parsed button input supplied to host presentation policy.
pub struct ButtonView<'a, A> {
    pub label: &'a str,
    pub action: &'a A,
    pub focused: Option<bool>,
}

/// Host-produced button text and resolved focus state.
pub struct ButtonPresentation {
    pub label: String,
    pub focused: bool,
}

/// Reusable renderer configured with a plugin's typed template actions.
pub struct Renderer<A> {
    actions: ActionRegistry<A>,
}

impl<A> Renderer<A>
where
    A: Clone + Send + 'static,
{
    pub fn new(actions: ActionRegistry<A>) -> Self {
        Self { actions }
    }

    /// Renders inline template data into terminal lines and typed hitboxes.
    pub fn render<F>(
        &self,
        template: &str,
        data: Value,
        viewport: Viewport,
        present_button: F,
    ) -> Result<Frame<A>, Error>
    where
        F: Fn(ButtonView<'_, A>) -> Result<ButtonPresentation, Error> + Send + Sync + 'static,
    {
        if viewport.rows == 0 || viewport.cols == 0 {
            return Ok(Frame::default());
        }
        let (root, refresh_after) =
            template::render_tree(template, data, &self.actions, present_button)?;
        let mut frame = layout::layout(&root, viewport.cols, viewport.rows)?.into_frame();
        frame.refresh_after = refresh_after;
        Ok(frame)
    }

    /// Renders a named template from a preconfigured MiniJinja environment.
    pub fn render_named<F>(
        &self,
        environment: Environment<'_>,
        template_name: &str,
        data: Value,
        viewport: Viewport,
        present_button: F,
    ) -> Result<Frame<A>, Error>
    where
        F: Fn(ButtonView<'_, A>) -> Result<ButtonPresentation, Error> + Send + Sync + 'static,
    {
        let mut environment = environment;
        self.render_named_mut(
            &mut environment,
            template_name,
            data,
            viewport,
            present_button,
        )
    }

    /// Renders from a retained environment so lazy-loaded templates stay cached.
    pub fn render_named_mut<F>(
        &self,
        environment: &mut Environment<'_>,
        template_name: &str,
        data: Value,
        viewport: Viewport,
        present_button: F,
    ) -> Result<Frame<A>, Error>
    where
        F: Fn(ButtonView<'_, A>) -> Result<ButtonPresentation, Error> + Send + Sync + 'static,
    {
        if viewport.rows == 0 || viewport.cols == 0 {
            return Ok(Frame::default());
        }
        let (root, refresh_after) = template::render_named_tree(
            environment,
            template_name,
            data,
            &self.actions,
            present_button,
        )?;
        let mut frame = layout::layout(&root, viewport.cols, viewport.rows)?.into_frame();
        frame.refresh_after = refresh_after;
        Ok(frame)
    }
}

/// Produces visible, clipped output for template or layout failures.
pub fn error_frame<A>(error: &Error, viewport: Viewport) -> Frame<A> {
    let text = format!("template error: {error}");
    let clipped: String = text
        .chars()
        .scan(0usize, |width, ch| {
            let next = *width + UnicodeWidthChar::width(ch).unwrap_or(0);
            if next > viewport.cols {
                None
            } else {
                *width = next;
                Some(ch)
            }
        })
        .collect();
    let mut hitboxes = (0..viewport.rows.min(1))
        .map(|_| {
            std::iter::repeat_with(|| None)
                .take(viewport.cols)
                .collect()
        })
        .collect::<Vec<_>>();
    if viewport.rows == 0 {
        hitboxes.clear();
    }
    Frame {
        lines: if viewport.rows == 0 {
            vec![]
        } else {
            vec![clipped]
        },
        hitboxes,
        refresh_after: None,
    }
}

fn layout_error(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::InvalidOperation, message.into())
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum TestAction {
        Open(usize),
    }

    #[test]
    fn renders_typed_actions() {
        let renderer = Renderer::new(ActionRegistry::new().with("open", |args| {
            let index = args
                .first()
                .and_then(Value::as_usize)
                .ok_or_else(|| Error::new(ErrorKind::InvalidOperation, "open expects an index"))?;
            Ok(TestAction::Open(index))
        }));
        let frame = renderer
            .render(
                r#"{% call Button(on_click=actions.open(7), focused=true) %}go{% endcall %}"#,
                context! {},
                Viewport { rows: 1, cols: 2 },
                |button| {
                    Ok(ButtonPresentation {
                        label: button.label.to_string(),
                        focused: button.focused.unwrap_or(false),
                    })
                },
            )
            .unwrap();
        assert_eq!(frame.lines, ["go"]);
        assert_eq!(frame.hitboxes[0][0], Some(TestAction::Open(7)));
    }

    #[test]
    fn flex_gap_inserts_cells_between_children() {
        let renderer = Renderer::new(ActionRegistry::<TestAction>::new());
        let frame = renderer
            .render(
                r#"{% call Flex(gap=2) %}{% call Flex() %}a{% endcall %}{% call Flex() %}b{% endcall %}{% endcall %}"#,
                context! {},
                Viewport { rows: 1, cols: 4 },
                |button| {
                    Ok(ButtonPresentation {
                        label: button.label.to_string(),
                        focused: button.focused.unwrap_or(false),
                    })
                },
            )
            .unwrap();
        assert_eq!(frame.lines, ["a  b"]);
    }

    #[test]
    fn clock_renders_and_requests_refresh() {
        let renderer = Renderer::new(ActionRegistry::<TestAction>::new());
        let frame = renderer
            .render(
                r#"{{ Clock(format="HH:MM") }} {{ Clock(format="HH:MM:SS") }}"#,
                context! {},
                Viewport { rows: 1, cols: 18 },
                |button| {
                    Ok(ButtonPresentation {
                        label: button.label.to_string(),
                        focused: button.focused.unwrap_or(false),
                    })
                },
            )
            .unwrap();
        assert_eq!(frame.lines[0].chars().filter(|ch| *ch == ':').count(), 3);
        assert!(frame
            .refresh_after
            .is_some_and(|delay| { !delay.is_zero() && delay <= Duration::from_secs(1) }));
    }

    #[test]
    fn clock_uses_explicit_timezone_then_env_tz() {
        let renderer = Renderer::new(ActionRegistry::<TestAction>::new());
        let frame = renderer
            .render(
                r#"{{ Clock(format="%z", tz="UTC") }} {{ Clock(format="%z") }}"#,
                context! { env => context! { TZ => "America/New_York" } },
                Viewport { rows: 1, cols: 11 },
                |button| {
                    Ok(ButtonPresentation {
                        label: button.label.to_string(),
                        focused: button.focused.unwrap_or(false),
                    })
                },
            )
            .unwrap();

        assert!(frame.lines[0].starts_with("+0000 "));
        assert_ne!(&frame.lines[0][6..], "+0000");
    }

    #[test]
    fn retained_environment_caches_lazy_includes_across_renders() {
        let reads = Arc::new(Mutex::new(Vec::new()));
        let reader_reads = Arc::clone(&reads);
        let files = BTreeMap::from([
            (
                PathBuf::from("/templates/main.jinja"),
                "{% include 'part.jinja' %}",
            ),
            (PathBuf::from("/templates/part.jinja"), "part"),
        ]);
        let (mut environment, entry) = file_template_environment(
            PathBuf::from("/templates/main.jinja"),
            None,
            move |path: &Path| {
                reader_reads.lock().unwrap().push(path.to_path_buf());
                files
                    .get(path)
                    .map(|source| source.to_string())
                    .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "missing template"))
            },
        )
        .unwrap();
        let renderer = Renderer::new(ActionRegistry::<TestAction>::new());
        for _ in 0..2 {
            renderer
                .render_named_mut(
                    &mut environment,
                    &entry,
                    context! {},
                    Viewport { rows: 1, cols: 4 },
                    |button| {
                        Ok(ButtonPresentation {
                            label: button.label.to_string(),
                            focused: button.focused.unwrap_or(false),
                        })
                    },
                )
                .unwrap();
        }
        assert_eq!(reads.lock().unwrap().len(), 2);
    }

    #[test]
    fn malformed_template_gets_visible_error_frame() {
        let frame: Frame<TestAction> =
            error_frame(&layout_error("bad layout"), Viewport { rows: 1, cols: 80 });
        assert!(frame.lines[0].contains("template error:"));
        assert!(frame.lines[0].contains("bad layout"));
    }
}
