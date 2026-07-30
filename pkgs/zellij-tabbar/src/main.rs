//! Zellij plugin shell: tracks host state, renders template frames, and dispatches clicks.

mod render;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use render::{RenderedFrame, TabBarRenderer};
use zellij_template_render::{builtin_action_permissions, BuiltinActionDispatcher};
use zellij_tile::prelude::*;

const TIMER_BOUNDARY_PADDING: Duration = Duration::from_millis(10);

#[derive(Default)]
struct RefreshTimer {
    active: Option<Duration>,
    superseded: Vec<Duration>,
}

impl RefreshTimer {
    fn schedule(&mut self, requested: Option<Duration>) -> Option<Duration> {
        let requested = requested? + TIMER_BOUNDARY_PADDING;
        match self.active {
            None => {
                self.active = Some(requested);
                Some(requested)
            },
            Some(active) if requested < active => {
                self.superseded.push(active);
                self.active = Some(requested);
                Some(requested)
            },
            Some(_) => None,
        }
    }

    fn expired(&mut self, elapsed_seconds: f64) -> bool {
        let Ok(elapsed) = Duration::try_from_secs_f64(elapsed_seconds) else {
            return false;
        };
        let Some(active) = self.active else {
            if let Some((index, _)) = self
                .superseded
                .iter()
                .enumerate()
                .min_by_key(|(_, duration)| duration.abs_diff(elapsed))
            {
                self.superseded.swap_remove(index);
            }
            return false;
        };
        let active_distance = active.abs_diff(elapsed);
        let stale = self
            .superseded
            .iter()
            .enumerate()
            .min_by_key(|(_, duration)| duration.abs_diff(elapsed));

        // ponytail: Zellij timers have no IDs or cancellation. Match their elapsed duration;
        // replace this with opaque timer IDs if Zellij adds them.
        if let Some((index, _)) =
            stale.filter(|(_, duration)| duration.abs_diff(elapsed) < active_distance)
        {
            self.superseded.swap_remove(index);
            false
        } else {
            self.active = None;
            true
        }
    }
}

/// Host-facing plugin state. Rendering details stay inside the `render` module.
#[derive(Default)]
struct State {
    tabs: Vec<TabInfo>,
    mode_info: ModeInfo,
    frame: RenderedFrame,
    tabbar_renderer: Option<TabBarRenderer>,
    pending_template: Option<PendingTemplate>,
    template_error: Option<String>,
    refresh_timer: RefreshTimer,
    builtin_actions: BuiltinActionDispatcher,
}

register_plugin!(State);

struct PendingTemplate {
    host_folder: PathBuf,
    configuration: BTreeMap<String, String>,
}

fn prepare_external_template(
    mut configuration: BTreeMap<String, String>,
    home: Option<&Path>,
    config_dir: Option<&Path>,
) -> Result<PendingTemplate, String> {
    let configured_path = configuration
        .get("template_file")
        .ok_or_else(|| "template_file is missing".to_string())?;
    let path = Path::new(configured_path);
    let path = if let Ok(relative) = path.strip_prefix("~") {
        home.ok_or_else(|| "cannot expand template path without a home directory".to_string())?
            .join(relative)
    } else if path.is_relative() {
        config_dir
            .map(Path::to_path_buf)
            .or_else(|| home.map(|home| home.join(".config/zellij")))
            .ok_or_else(|| "relative template_file requires ZELLIJ_CONFIG_DIR or HOME".to_string())?
            .join(path)
    } else {
        path.to_path_buf()
    };
    let host_folder = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| format!("template_file has no parent directory: {}", path.display()))?
        .to_path_buf();
    let entry = path
        .file_name()
        .ok_or_else(|| format!("template_file has no file name: {}", path.display()))?;

    // Mounting the template directory lets Zellij resolve host-side symlinks before WASI's
    // capability boundary. Absolute symlinks under a root /host mount fail with ENOTCAPABLE.
    configuration.insert(
        "template_file".to_string(),
        Path::new("/").join(entry).to_string_lossy().into_owned(),
    );
    Ok(PendingTemplate {
        host_folder,
        configuration,
    })
}

impl State {
    fn load_renderer(&mut self, configuration: &BTreeMap<String, String>) {
        match TabBarRenderer::from_configuration(configuration) {
            Ok(renderer) => {
                self.tabbar_renderer = Some(renderer);
                self.template_error = None;
            },
            Err(error) => self.template_error = Some(error.to_string()),
        }
    }
}

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        let mut permissions = vec![
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
            PermissionType::OpenTerminalsOrPlugins,
        ];
        for permission in builtin_action_permissions() {
            if !permissions.contains(permission) {
                permissions.push(*permission);
            }
        }
        let has_template_file = configuration.contains_key("template_file");
        let has_conflicting_template = has_template_file && configuration.contains_key("template");
        if has_template_file && !has_conflicting_template {
            permissions.push(PermissionType::FullHdAccess);
            match prepare_external_template(
                configuration,
                std::env::var_os("HOME").as_deref().map(Path::new),
                std::env::var_os("ZELLIJ_CONFIG_DIR")
                    .as_deref()
                    .map(Path::new),
            ) {
                Ok(pending_template) => self.pending_template = Some(pending_template),
                Err(error) => self.template_error = Some(error),
            }
        } else {
            self.load_renderer(&configuration);
        }

        // Permission prompts consume y/n through the plugin pane. Keep it selectable until the
        // result arrives, then return to the borderless mouse-only tabbar.
        set_selectable(true);
        subscribe(&[
            EventType::TabUpdate,
            EventType::ModeUpdate,
            EventType::Mouse,
            EventType::PaneUpdate,
            EventType::PermissionRequestResult,
            EventType::HostFolderChanged,
            EventType::FailedToChangeHostFolder,
            EventType::Timer,
        ]);
        request_permission(&permissions);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::PermissionRequestResult(status) => {
                set_selectable(false);
                if self.pending_template.is_some() {
                    match status {
                        PermissionStatus::Granted => {
                            change_host_folder(
                                self.pending_template
                                    .as_ref()
                                    .expect("pending template checked above")
                                    .host_folder
                                    .clone(),
                            );
                        },
                        PermissionStatus::Denied => {
                            self.pending_template = None;
                            self.template_error =
                                Some("template_file requires FullHdAccess permission".to_string());
                        },
                    }
                }
                true
            },
            Event::HostFolderChanged(_) => {
                if let Some(pending_template) = self.pending_template.take() {
                    self.load_renderer(&pending_template.configuration);
                }
                true
            },
            Event::FailedToChangeHostFolder(error) => {
                if self.pending_template.take().is_some() {
                    self.template_error = Some(error.unwrap_or_else(|| {
                        "failed to mount host filesystem for template_file".to_string()
                    }));
                }
                true
            },
            Event::ModeUpdate(mode_info) => {
                let changed = self.mode_info != mode_info;
                self.mode_info = mode_info;
                changed && !self.tabs.is_empty()
            },
            Event::TabUpdate(tabs) => {
                self.tabs = tabs;
                // Always repaint: tab closure can produce an empty or otherwise equal-looking update.
                true
            },
            Event::PaneUpdate(panes) => {
                self.builtin_actions.retain_open_plugins(&panes);
                false
            },
            Event::Timer(elapsed) => self.refresh_timer.expired(elapsed) && !self.tabs.is_empty(),
            Event::Mouse(Mouse::LeftClick(row, col)) => {
                if let Some(action) = usize::try_from(row)
                    .ok()
                    .and_then(|row| self.frame.hitboxes.get(row))
                    .and_then(|line| line.get(col))
                    .and_then(Clone::clone)
                {
                    self.builtin_actions.dispatch(action);
                }
                false
            },
            _ => false,
        }
    }

    fn render(&mut self, rows: usize, cols: usize) {
        if self.tabs.is_empty() {
            // Clear stale output after the final visible tab disappears.
            self.frame = RenderedFrame::default();
        } else {
            self.frame = if let Some(renderer) = &mut self.tabbar_renderer {
                match renderer.render(&self.mode_info, &self.tabs, rows, cols) {
                    Ok(frame) => frame,
                    Err(error) => renderer.error_frame(&error, rows, cols),
                }
            } else {
                let error = zellij_template_render::Error::new(
                    zellij_template_render::ErrorKind::InvalidOperation,
                    self.template_error
                        .clone()
                        .unwrap_or_else(|| "template host unavailable".to_string()),
                );
                zellij_template_render::error_frame(
                    &error,
                    zellij_template_render::Viewport { rows, cols },
                )
            };
        }
        if let Some(delay) = self.refresh_timer.schedule(self.frame.refresh_after) {
            // Cross clock and animation boundaries before repainting. Exact-boundary timers can
            // fire slightly early and leave the displayed value unchanged.
            set_timeout(delay.as_secs_f64());
        }
        let output = (0..rows)
            .map(|row| {
                let line = self.frame.lines.get(row).map_or("", String::as_str);
                format!("\u{1b}[2K{line}")
            })
            .collect::<Vec<_>>()
            .join("\n");
        print!("{output}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faster_refresh_supersedes_armed_timer() {
        let mut timer = RefreshTimer::default();

        assert_eq!(
            timer.schedule(Some(Duration::from_secs(60))),
            Some(Duration::from_millis(60_010))
        );
        assert_eq!(
            timer.schedule(Some(Duration::from_millis(100))),
            Some(Duration::from_millis(110))
        );
        assert_eq!(timer.schedule(Some(Duration::from_millis(500))), None);
        assert_eq!(timer.active, Some(Duration::from_millis(110)));
    }

    #[test]
    fn superseded_timer_does_not_start_second_render_loop() {
        let mut timer = RefreshTimer::default();
        timer.schedule(Some(Duration::from_secs(60)));
        timer.schedule(Some(Duration::from_millis(100)));

        assert!(!timer.expired(60.01));
        assert_eq!(timer.active, Some(Duration::from_millis(110)));
        assert!(timer.expired(0.11));
        assert_eq!(timer.active, None);
    }

    #[test]
    fn equal_refresh_does_not_arm_duplicate_timer() {
        let mut timer = RefreshTimer::default();
        assert!(timer.schedule(Some(Duration::from_millis(100))).is_some());
        assert_eq!(timer.schedule(Some(Duration::from_millis(100))), None);
    }

    #[test]
    fn external_template_mounts_parent_and_uses_guest_root_entry() {
        let configuration = BTreeMap::from([(
            "template_file".to_string(),
            "~/.config/zellij/tab-bar/main.jinja".to_string(),
        )]);

        let pending =
            prepare_external_template(configuration, Some(Path::new("/var/home/q")), None).unwrap();

        assert_eq!(
            pending.host_folder,
            PathBuf::from("/var/home/q/.config/zellij/tab-bar")
        );
        assert_eq!(
            pending
                .configuration
                .get("template_file")
                .map(String::as_str),
            Some("/main.jinja")
        );
    }
}
