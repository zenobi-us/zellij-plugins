//! High-level template source, environment, and shared context management.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use minijinja::{context, Environment, Error, ErrorKind, Value};
use zellij_tile::prelude::{ModeInfo, PaletteColor, Styling};

use crate::file_template::{environment as file_template_environment, environment_unchecked};
use crate::{ButtonPresentation, ButtonView, Frame, Renderer, Viewport};

const DEFAULT_ENVIRONMENT_VARIABLES: [&str; 3] = ["TZ", "LANG", "TERM"];
const EXTERNAL_TEMPLATE_REFRESH: Duration = Duration::from_secs(1);

pub enum TemplateSource {
    Inline(String),
    Named {
        environment: Box<Environment<'static>>,
        entry: String,
    },
}

struct ExternalTemplateReload {
    files: Arc<Mutex<BTreeMap<PathBuf, FileSnapshot>>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum FileSnapshot {
    Contents(String),
    Error(io::ErrorKind),
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
    external_reload: Option<ExternalTemplateReload>,
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
            external_reload: None,
        }
    }

    /// Builds a configured host with automatic reload for external templates.
    pub fn from_configuration(
        renderer: Renderer<A>,
        configuration: &BTreeMap<String, String>,
        embedded: Environment<'static>,
        embedded_entry: impl Into<String>,
    ) -> Result<Self, Error> {
        let environment = TemplateEnvironment::from_configuration(configuration);
        match (
            configuration.get("template"),
            configuration.get("template_file"),
        ) {
            (Some(_), Some(_)) => Err(Error::new(
                ErrorKind::InvalidOperation,
                "template and template_file cannot be configured together",
            )),
            (Some(source), None) => Ok(Self::new(
                renderer,
                TemplateSource::Inline(source.clone()),
                environment,
            )),
            (None, Some(path)) => {
                let (template_environment, entry, external_reload) =
                    load_reloadable_external_template(path)?;
                Ok(Self {
                    renderer,
                    source: TemplateSource::Named {
                        environment: Box::new(template_environment),
                        entry,
                    },
                    environment,
                    external_reload: Some(external_reload),
                })
            },
            (None, None) => Ok(Self::new(
                renderer,
                TemplateSource::Named {
                    environment: Box::new(embedded),
                    entry: embedded_entry.into(),
                },
                environment,
            )),
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
        self.reload_if_changed()?;
        let reload_after = self.refresh_after();
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

        let mut frame = match &mut self.source {
            TemplateSource::Inline(source) => {
                self.renderer.render(source, data, viewport, present_button)
            },
            TemplateSource::Named { environment, entry } => {
                self.renderer
                    .render_named_mut(environment, entry, data, viewport, present_button)
            },
        }?;
        if let Some(reload_after) = reload_after {
            frame.refresh_after = Some(
                frame
                    .refresh_after
                    .map_or(reload_after, |current| current.min(reload_after)),
            );
        }
        Ok(frame)
    }

    /// Returns the polling delay required to keep external template reload active.
    pub fn refresh_after(&self) -> Option<Duration> {
        self.external_reload
            .as_ref()
            .map(|_| EXTERNAL_TEMPLATE_REFRESH)
    }

    fn reload_if_changed(&mut self) -> Result<(), Error> {
        let Some(reload) = &self.external_reload else {
            return Ok(());
        };
        let changed = reload.changed()?;
        if changed {
            let TemplateSource::Named { environment, .. } = &mut self.source else {
                return Err(Error::new(
                    ErrorKind::InvalidOperation,
                    "external template reload requires a named environment",
                ));
            };
            environment.clear_templates();
            reload.clear()?;
        }
        Ok(())
    }
}

impl ExternalTemplateReload {
    fn changed(&self) -> Result<bool, Error> {
        let files = self
            .files
            .lock()
            .map_err(|_| template_reload_lock_error())?;
        Ok(files
            .iter()
            .any(|(path, previous)| snapshot_external_template(path) != *previous))
    }

    fn clear(&self) -> Result<(), Error> {
        self.files
            .lock()
            .map_err(|_| template_reload_lock_error())?
            .clear();
        Ok(())
    }
}

fn color_token(color: PaletteColor) -> String {
    match color {
        PaletteColor::Rgb((r, g, b)) => format!("rgb:{r},{g},{b}"),
        PaletteColor::EightBit(index) => format!("index:{index}"),
    }
}

fn load_external_template(path: &str) -> Result<(Environment<'static>, String), Error> {
    let (entry, home) = resolve_external_template_path(path)?;
    file_template_environment(entry, home, read_external_template)
}

fn load_reloadable_external_template(
    path: &str,
) -> Result<(Environment<'static>, String, ExternalTemplateReload), Error> {
    let (entry, home) = resolve_external_template_path(path)?;
    let files = Arc::new(Mutex::new(BTreeMap::new()));
    let loader_files = Arc::clone(&files);
    let (environment, entry) = environment_unchecked(entry, home, move |path| {
        let result = read_external_template(path);
        let snapshot = snapshot_result(&result);
        loader_files
            .lock()
            .map_err(|_| io::Error::other("template reload lock poisoned"))?
            .insert(path.to_path_buf(), snapshot);
        result
    })?;
    Ok((environment, entry, ExternalTemplateReload { files }))
}

fn resolve_external_template_path(path: &str) -> Result<(PathBuf, Option<PathBuf>), Error> {
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
    Ok((entry, home))
}

fn read_external_template(path: &Path) -> io::Result<String> {
    std::fs::read_to_string(plugin_host_path(path))
}

fn snapshot_external_template(path: &Path) -> FileSnapshot {
    snapshot_result(&read_external_template(path))
}

fn snapshot_result(result: &io::Result<String>) -> FileSnapshot {
    match result {
        Ok(contents) => FileSnapshot::Contents(contents.clone()),
        Err(error) => FileSnapshot::Error(error.kind()),
    }
}

fn template_reload_lock_error() -> Error {
    Error::new(ErrorKind::InvalidOperation, "template reload lock poisoned")
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
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::ActionRegistry;
    use zellij_tile::prelude::{Style, StyleDeclaration};

    #[derive(Clone)]
    enum TestAction {}

    fn temp_template_directory(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "zellij-template-render-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    fn external_host(entry: &Path) -> TemplateHost<TestAction> {
        let configuration = BTreeMap::from([(
            "template_file".to_string(),
            entry.to_string_lossy().into_owned(),
        )]);
        TemplateHost::from_configuration(
            Renderer::new(ActionRegistry::<TestAction>::new()),
            &configuration,
            Environment::new(),
            "main.jinja",
        )
        .unwrap()
    }

    fn render_external(host: &mut TemplateHost<TestAction>) -> Result<Frame<TestAction>, Error> {
        host.render(
            TemplateContext::new(),
            &ModeInfo::default(),
            Viewport { rows: 1, cols: 10 },
            |button| {
                Ok(ButtonPresentation {
                    label: button.label.to_string(),
                    focused: false,
                })
            },
        )
    }

    #[test]
    fn external_template_reloads_changed_include() {
        let directory = temp_template_directory("reload");
        let entry = directory.join("main.jinja");
        let include = directory.join("part.jinja");
        fs::write(&entry, "{% include 'part.jinja' %}").unwrap();
        fs::write(&include, "one").unwrap();
        let mut host = external_host(&entry);

        assert_eq!(render_external(&mut host).unwrap().lines, ["one"]);
        fs::write(&include, "two").unwrap();
        assert_eq!(render_external(&mut host).unwrap().lines, ["two"]);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn external_template_recovers_after_missing_include_is_restored() {
        let directory = temp_template_directory("recovery");
        let entry = directory.join("main.jinja");
        let include = directory.join("part.jinja");
        fs::write(&entry, "{% include 'part.jinja' %}").unwrap();
        fs::write(&include, "one").unwrap();
        let mut host = external_host(&entry);

        assert_eq!(render_external(&mut host).unwrap().lines, ["one"]);
        fs::remove_file(&include).unwrap();
        assert!(render_external(&mut host).is_err());
        fs::write(&include, "two").unwrap();
        assert_eq!(render_external(&mut host).unwrap().lines, ["two"]);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn initially_invalid_external_template_recovers_after_edit() {
        let directory = temp_template_directory("initial-error");
        let entry = directory.join("main.jinja");
        fs::write(&entry, "{{").unwrap();
        let mut host = external_host(&entry);

        assert!(render_external(&mut host).is_err());
        fs::write(&entry, "fixed").unwrap();
        assert_eq!(render_external(&mut host).unwrap().lines, ["fixed"]);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn unchanged_external_template_keeps_cached_templates() {
        let directory = temp_template_directory("cache");
        let entry = directory.join("main.jinja");
        let include = directory.join("part.jinja");
        fs::write(&entry, "{% include 'part.jinja' %}").unwrap();
        fs::write(&include, "one").unwrap();
        let mut host = external_host(&entry);
        render_external(&mut host).unwrap();
        let TemplateSource::Named { environment, .. } = &host.source else {
            panic!("expected named template source")
        };
        let loaded = environment.templates().count();

        host.reload_if_changed().unwrap();

        assert_eq!(loaded, 2);
        let TemplateSource::Named { environment, .. } = &host.source else {
            panic!("expected named template source")
        };
        assert_eq!(environment.templates().count(), loaded);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn inline_and_embedded_templates_do_not_request_reload() {
        let inline = TemplateHost::new(
            Renderer::new(ActionRegistry::<TestAction>::new()),
            TemplateSource::Inline("inline".to_string()),
            TemplateEnvironment::from_values(BTreeMap::new()),
        );
        let mut environment = Environment::new();
        environment.add_template("main.jinja", "embedded").unwrap();
        let embedded = TemplateHost::new(
            Renderer::new(ActionRegistry::<TestAction>::new()),
            TemplateSource::Named {
                environment: Box::new(environment),
                entry: "main.jinja".to_string(),
            },
            TemplateEnvironment::from_values(BTreeMap::new()),
        );

        assert_eq!(inline.refresh_after(), None);
        assert_eq!(embedded.refresh_after(), None);
    }

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
