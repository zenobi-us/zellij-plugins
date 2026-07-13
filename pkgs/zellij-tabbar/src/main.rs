//! Zellij plugin shell: tracks host state, renders template frames, and dispatches clicks.

mod render;

use std::collections::BTreeMap;
use std::time::Duration;

use render::{ClickAction, RenderedFrame, TabBarRenderer};
use zellij_tile::prelude::*;

/// Host-facing plugin state. Rendering details stay inside the `render` module.
#[derive(Default)]
struct State {
    tabs: Vec<TabInfo>,
    mode_info: ModeInfo,
    frame: RenderedFrame,
    tabbar_renderer: Option<TabBarRenderer>,
    template_error: Option<String>,
    timer_armed: bool,
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        let mut permissions = vec![
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
        ];
        if configuration.contains_key("template_file") {
            permissions.push(PermissionType::FullHdAccess);
        }
        request_permission(&permissions);
        match TabBarRenderer::from_configuration(&configuration) {
            Ok(renderer) => self.tabbar_renderer = Some(renderer),
            Err(error) => self.template_error = Some(error.to_string()),
        }

        // we dont need to be selectable, since we handle mouse clicks ourselves
        // in fact, making the pane selectable causes a border to be drawn and for height=1 panes,
        // this obscures the tab line, so we disable it here
        set_selectable(false);

        // subscribe to the events we care about
        subscribe(&[
            EventType::TabUpdate,
            EventType::ModeUpdate,
            EventType::Mouse,
            EventType::Timer,
        ]);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
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
            Event::Timer(_) => {
                self.timer_armed = false;
                !self.tabs.is_empty()
            },
            Event::Mouse(Mouse::LeftClick(row, col)) => {
                if let Some(action) = usize::try_from(row)
                    .ok()
                    .and_then(|row| self.frame.hitboxes.get(row))
                    .and_then(|line| line.get(col))
                    .and_then(Clone::clone)
                {
                    match action {
                        ClickAction::SwitchTab(index) => switch_tab_to(index as u32),
                        ClickAction::NewTab => {
                            new_tab::<&str>(None, None);
                        },
                    }
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
                match renderer.render(
                    self.mode_info.session_name.as_deref(),
                    &self.tabs,
                    rows,
                    cols,
                    self.mode_info.style.colors,
                    self.mode_info.capabilities,
                ) {
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
        if !self.timer_armed {
            if let Some(delay) = self.frame.refresh_after {
                // Cross the clock boundary before repainting. Exact-boundary timers can fire
                // slightly early and leave the displayed minute unchanged.
                set_timeout((delay + Duration::from_millis(10)).as_secs_f64());
                self.timer_armed = true;
            }
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
