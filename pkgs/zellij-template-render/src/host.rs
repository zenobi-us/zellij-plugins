//! High-level template source, environment, and shared context management.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::Utc;
use minijinja::{context, Environment, Error, ErrorKind, Value};
use zellij_tile::prelude::{ModeInfo, PaletteColor, Styling};

use crate::{file_template_environment, ButtonPresentation, ButtonView, Frame, Renderer, Viewport};

const DEFAULT_ENVIRONMENT_VARIABLES: [&str; 3] = ["TZ", "LANG", "TERM"];

pub enum TemplateSource {
    Inline(String),
    Named {
        environment: Box<Environment<'static>>,
        entry: String,
    },
}

impl TemplateSource {
    pub fn from_configuration(
        configuration: &BTreeMap<String, String>,
        embedded: Environment<'static>,
        embedded_entry: impl Into<String>,
    ) -> Result<Self, Error> {
        match (
            configuration.get("template"),
            configuration.get("template_file"),
        ) {
            (Some(_), Some(_)) => Err(Error::new(
                ErrorKind::InvalidOperation,
                "template and template_file cannot be configured together",
            )),
            (Some(source), None) => Ok(Self::Inline(source.clone())),
            (None, Some(path)) => {
                let (environment, entry) = load_external_template(path)?;
                Ok(Self::Named {
                    environment: Box::new(environment),
                    entry,
                })
            },
            (None, None) => Ok(Self::Named {
                environment: Box::new(embedded),
                entry: embedded_entry.into(),
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateEnvironment {
    values: BTreeMap<String, String>,
}

impl TemplateEnvironment {
    pub fn from_configuration(configuration: &BTreeMap<String, String>) -> Self {
        let names = configuration.get("env_vars").map_or_else(
            || DEFAULT_ENVIRONMENT_VARIABLES.to_vec(),
            |names| {
                names
                    .split(',')
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .collect()
            },
        );
        Self {
            values: names
                .into_iter()
                .filter_map(|name| {
                    std::env::var(name)
                        .ok()
                        .map(|value| (name.to_string(), value))
                })
                .collect(),
        }
    }

    pub fn from_values(values: BTreeMap<String, String>) -> Self {
        Self { values }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TemplateTheme {
    pub text: String,
    pub background: String,
    pub active_text: String,
    pub active_background: String,
    pub muted_text: String,
    pub muted_background: String,
    pub alert: String,
}

impl From<&ModeInfo> for TemplateTheme {
    fn from(mode_info: &ModeInfo) -> Self {
        Self::from(mode_info.style.colors)
    }
}

impl From<Styling> for TemplateTheme {
    fn from(colors: Styling) -> Self {
        Self {
            text: color_token(colors.text_unselected.base),
            background: color_token(colors.text_unselected.background),
            active_text: color_token(colors.ribbon_selected.base),
            active_background: color_token(colors.ribbon_selected.background),
            muted_text: color_token(colors.ribbon_unselected.base),
            muted_background: color_token(colors.ribbon_unselected.background),
            alert: color_token(colors.ribbon_unselected.emphasis_3),
        }
    }
}

#[derive(Default)]
pub struct TemplateContext {
    values: BTreeMap<String, Value>,
}

impl TemplateContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, name: impl Into<String>, value: impl Into<Value>) -> Self {
        self.values.insert(name.into(), value.into());
        self
    }
}

pub struct TemplateHost<A> {
    renderer: Renderer<A>,
    source: TemplateSource,
    environment: TemplateEnvironment,
}

impl<A> TemplateHost<A> {
    pub fn new(
        renderer: Renderer<A>,
        source: TemplateSource,
        environment: TemplateEnvironment,
    ) -> Self {
        Self {
            renderer,
            source,
            environment,
        }
    }

    pub fn render<F>(
        &mut self,
        context: TemplateContext,
        mode_info: &ModeInfo,
        viewport: Viewport,
        present_button: F,
    ) -> Result<Frame<A>, Error>
    where
        A: Clone + Send + 'static,
        F: Fn(ButtonView<'_, A>) -> Result<ButtonPresentation, Error> + Send + Sync + 'static,
    {
        let theme = TemplateTheme::from(mode_info);
        let mut values = context.values;
        values.insert(
            "env".to_string(),
            Value::from_iter(self.environment.values.clone()),
        );
        values.insert(
            "system".to_string(),
            context! { time => Utc::now().timestamp() },
        );
        values.insert(
            "theme".to_string(),
            context! {
                text => theme.text,
                background => theme.background,
                active_text => theme.active_text,
                active_background => theme.active_background,
                muted_text => theme.muted_text,
                muted_background => theme.muted_background,
                alert => theme.alert,
            },
        );
        let data = Value::from_iter(values);

        match &mut self.source {
            TemplateSource::Inline(source) => {
                self.renderer.render(source, data, viewport, present_button)
            },
            TemplateSource::Named { environment, entry } => {
                self.renderer
                    .render_named_mut(environment, entry, data, viewport, present_button)
            },
        }
    }
}

fn color_token(color: PaletteColor) -> String {
    match color {
        PaletteColor::Rgb((r, g, b)) => format!("rgb:{r},{g},{b}"),
        PaletteColor::EightBit(index) => format!("index:{index}"),
    }
}

fn load_external_template(path: &str) -> Result<(Environment<'static>, String), Error> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut entry = PathBuf::from(path);
    if entry.is_relative() && !entry.starts_with("~") {
        let config_dir = std::env::var_os("ZELLIJ_CONFIG_DIR")
            .map(PathBuf::from)
            .or_else(|| home.as_ref().map(|home| home.join(".config/zellij")))
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidOperation,
                    "relative template_file requires ZELLIJ_CONFIG_DIR or HOME",
                )
            })?;
        entry = config_dir.join(entry);
    }
    file_template_environment(entry, home, |path| {
        std::fs::read_to_string(plugin_host_path(path))
    })
}

#[cfg(target_arch = "wasm32")]
fn plugin_host_path(path: &Path) -> PathBuf {
    Path::new("/host").join(path.strip_prefix("/").unwrap_or(path))
}

#[cfg(not(target_arch = "wasm32"))]
fn plugin_host_path(path: &Path) -> PathBuf {
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ActionRegistry;
    use zellij_tile::prelude::{Style, StyleDeclaration};

    #[derive(Clone)]
    enum TestAction {}

    #[test]
    fn conflicting_template_settings_are_rejected() {
        let configuration = BTreeMap::from([
            ("template".to_string(), "inline".to_string()),
            ("template_file".to_string(), "/tmp/main.jinja".to_string()),
        ]);

        let error =
            TemplateSource::from_configuration(&configuration, Environment::new(), "main.jinja")
                .err()
                .unwrap();

        assert_eq!(
            error.to_string(),
            "invalid operation: template and template_file cannot be configured together"
        );
    }

    #[test]
    fn host_adds_environment_theme_and_system_context() {
        let source = TemplateSource::Inline(
            r#"{{ env.TZ }} {{ theme.alert }} {{ system.time > 0 }} {{ session }}"#.to_string(),
        );
        let environment = TemplateEnvironment::from_values(BTreeMap::from([(
            "TZ".to_string(),
            "Etc/UTC".to_string(),
        )]));
        let mut host = TemplateHost::new(
            Renderer::new(ActionRegistry::<TestAction>::new()),
            source,
            environment,
        );
        let mode_info = ModeInfo {
            style: Style {
                colors: Styling {
                    ribbon_unselected: StyleDeclaration {
                        emphasis_3: PaletteColor::Rgb((1, 2, 3)),
                        ..StyleDeclaration::default()
                    },
                    ..Styling::default()
                },
                ..Style::default()
            },
            ..ModeInfo::default()
        };
        let frame = host
            .render(
                TemplateContext::new().with("session", "demo"),
                &mode_info,
                Viewport { rows: 1, cols: 80 },
                |button| {
                    Ok(ButtonPresentation {
                        label: button.label.to_string(),
                        focused: false,
                    })
                },
            )
            .unwrap();

        assert_eq!(frame.lines[0], "Etc/UTC rgb:1,2,3 true demo");
    }

    #[test]
    fn mode_info_maps_zellij_colors_to_template_tokens() {
        let mode_info = ModeInfo {
            style: Style {
                colors: Styling {
                    text_unselected: StyleDeclaration {
                        base: PaletteColor::EightBit(42),
                        ..StyleDeclaration::default()
                    },
                    ribbon_selected: StyleDeclaration {
                        base: PaletteColor::Rgb((1, 2, 3)),
                        ..StyleDeclaration::default()
                    },
                    ..Styling::default()
                },
                ..Style::default()
            },
            ..ModeInfo::default()
        };

        let theme = TemplateTheme::from(&mode_info);

        assert_eq!(theme.text, "index:42");
        assert_eq!(theme.active_text, "rgb:1,2,3");
    }
}
