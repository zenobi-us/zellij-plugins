//! Tabbar-specific template data, actions, and button styling.

use ansi_term::ANSIStrings;
use serde::Serialize;
use std::collections::BTreeMap;
use zellij_template_render::{
    error_frame as render_error_frame, ActionRegistry, BuiltinAction as ClickAction,
    ButtonPresentation, ButtonView, Environment, Error, ErrorKind, Frame, Renderer,
    TemplateContext, TemplateHost, Value, Viewport,
};
use zellij_tile::prelude::*;
use zellij_tile_utils::style;

/// Built-in template used when plugin configuration provides no override.
const DEFAULT_TEMPLATE_NAME: &str = "main.jinja";

pub(crate) type RenderedFrame = Frame<ClickAction>;

/// Long-lived tabbar renderer owned by the plugin state.
pub(crate) struct TabBarRenderer {
    host: TemplateHost<ClickAction>,
}

#[derive(Serialize)]
struct TemplateSession<'a> {
    name: &'a str,
    tabs: Vec<TemplateTab<'a>>,
}

#[derive(Serialize)]
struct TemplateTab<'a> {
    name: &'a str,
    index: usize,
    active: bool,
}

impl TabBarRenderer {
    pub(crate) fn from_configuration(
        configuration: &BTreeMap<String, String>,
    ) -> Result<Self, Error> {
        let mut embedded = Environment::new();
        minijinja_embed::load_templates!(&mut embedded);
        Ok(Self {
            host: TemplateHost::from_configuration(
                Renderer::new(ActionRegistry::new().with_builtins()),
                configuration,
                embedded,
                DEFAULT_TEMPLATE_NAME,
            )?,
        })
    }

    /// Renders tabbar data through the shared template renderer.
    pub(crate) fn render(
        &mut self,
        mode_info: &ModeInfo,
        tabs: &[TabInfo],
        rows: usize,
        cols: usize,
    ) -> Result<RenderedFrame, Error> {
        let model = TemplateSession {
            name: mode_info.session_name.as_deref().unwrap_or_default(),
            tabs: tabs
                .iter()
                .map(|tab| TemplateTab {
                    name: &tab.name,
                    index: tab.position + 1,
                    active: tab.active,
                })
                .collect(),
        };
        let tabs = tabs.to_vec();
        let colors = mode_info.style.colors;
        let capabilities = mode_info.capabilities;
        let viewport = Viewport { rows, cols };
        self.host.render(
            TemplateContext::new().with("session", Value::from_serialize(model)),
            mode_info,
            viewport,
            move |button| present_button(button, &tabs, colors, capabilities),
        )
    }

    pub(crate) fn error_frame(&self, error: &Error, rows: usize, cols: usize) -> RenderedFrame {
        let mut frame = render_error_frame(error, Viewport { rows, cols });
        frame.refresh_after = self.host.refresh_after();
        frame
    }
}

fn present_button(
    button: ButtonView<'_, ClickAction>,
    tabs: &[TabInfo],
    colors: Styling,
    capabilities: PluginCapabilities,
) -> Result<ButtonPresentation, Error> {
    let focused = button.focused.unwrap_or_else(|| match button.action {
        ClickAction::FocusTab(index) => tabs
            .iter()
            .any(|tab| tab.active && tab.position + 1 == *index),
        ClickAction::NewTab | ClickAction::RunPlugin { .. } => false,
        _ => false,
    });
    Ok(ButtonPresentation {
        label: style_button(
            button.label,
            button.action,
            focused,
            tabs,
            colors,
            capabilities,
        )?,
        focused,
    })
}

fn style_button(
    label: &str,
    action: &ClickAction,
    focused: bool,
    tabs: &[TabInfo],
    palette: Styling,
    capabilities: PluginCapabilities,
) -> Result<String, Error> {
    let separator = if capabilities.arrow_fonts { "" } else { "" };
    let label = match action {
        ClickAction::FocusTab(index) => {
            let tab = find_tab(tabs, *index)?;
            let mut label = label.to_string();
            if tab.is_fullscreen_active {
                label.push_str(" (FULLSCREEN)");
            } else if tab.is_sync_panes_active {
                label.push_str(" (SYNC)");
            }
            if tab.has_bell_notification || tab.is_flashing_bell {
                label.push_str(" [!]");
            }
            label
        },
        _ => label.to_string(),
    };
    let alternate = match action {
        ClickAction::FocusTab(index) => index % 2 == 0 && capabilities.arrow_fonts,
        ClickAction::NewTab | ClickAction::RunPlugin { .. } => {
            tabs.len() % 2 == 1 && capabilities.arrow_fonts
        },
        _ => false,
    };
    let background = if focused {
        palette.ribbon_selected.background
    } else if alternate {
        palette.ribbon_unselected.emphasis_1
    } else {
        palette.ribbon_unselected.background
    };
    let foreground = match action {
        ClickAction::FocusTab(index) => {
            let tab = find_tab(tabs, *index)?;
            if tab.is_flashing_bell || tab.has_bell_notification {
                if focused {
                    palette.ribbon_selected.emphasis_3
                } else {
                    palette.ribbon_unselected.emphasis_3
                }
            } else if focused {
                palette.ribbon_selected.base
            } else {
                palette.ribbon_unselected.base
            }
        },
        _ => palette.ribbon_unselected.base,
    };
    let fill = palette.text_unselected.background;
    let left = style!(fill, background).paint(separator);
    let text = style!(foreground, background)
        .bold()
        .paint(format!(" {} ", label));
    let right = style!(background, fill).paint(separator);
    Ok(ANSIStrings(&[left, text, right]).to_string())
}

fn find_tab(tabs: &[TabInfo], index: usize) -> Result<&TabInfo, Error> {
    tabs.iter()
        .find(|tab| tab.position + 1 == index)
        .ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidOperation,
                "focus_tab index does not exist",
            )
        })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use super::*;

    fn plain_text(value: &str) -> String {
        let mut output = String::new();
        let mut chars = value.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\u{1b}' {
                consume_ansi(&mut chars);
            } else {
                output.push(ch);
            }
        }
        output
    }

    fn consume_ansi(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
        match chars.next() {
            Some('[') => {
                for ch in chars.by_ref() {
                    if ('@'..='~').contains(&ch) {
                        break;
                    }
                }
            },
            Some(']') => {
                while let Some(ch) = chars.next() {
                    if ch == '\u{7}' {
                        break;
                    }
                    if ch == '\u{1b}' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            },
            _ => {},
        }
    }

    #[test]
    fn default_template_renders_buttons_and_actions() {
        let mut first = TabInfo {
            name: "one".into(),
            active: true,
            ..TabInfo::default()
        };
        first.position = 0;
        let second = TabInfo {
            name: "two".into(),
            position: 1,
            ..TabInfo::default()
        };
        let mode = ModeInfo {
            session_name: Some("demo".to_string()),
            capabilities: PluginCapabilities { arrow_fonts: false },
            ..ModeInfo::default()
        };
        let frame = TabBarRenderer::from_configuration(&BTreeMap::new())
            .unwrap()
            .render(&mode, &[first, second], 1, 80)
            .unwrap();
        assert!(plain_text(&frame.lines[0]).contains("one"));
        assert!(frame.hitboxes[0]
            .iter()
            .any(|action| action == &Some(ClickAction::FocusTab(1))));
        assert!(frame.hitboxes[0]
            .iter()
            .any(|action| action == &Some(ClickAction::NewTab)));
        assert!(frame.refresh_after.is_some_and(|delay| {
            !delay.is_zero() && delay <= std::time::Duration::from_secs(60)
        }));
    }

    #[test]
    fn external_template_error_frame_keeps_reload_timer() {
        let directory = std::env::temp_dir().join(format!(
            "zellij-tabbar-reload-error-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        let entry = directory.join("main.jinja");
        fs::write(&entry, "{{").unwrap();
        let mut renderer = TabBarRenderer::from_configuration(&BTreeMap::from([(
            "template_file".to_string(),
            entry.to_string_lossy().into_owned(),
        )]))
        .unwrap();
        let error = renderer
            .render(&ModeInfo::default(), &[], 1, 80)
            .err()
            .unwrap();
        let frame = renderer.error_frame(&error, 1, 80);

        assert_eq!(frame.refresh_after, Some(Duration::from_secs(1)));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn shared_host_supplies_top_level_theme() {
        let mode = ModeInfo {
            capabilities: PluginCapabilities { arrow_fonts: false },
            ..ModeInfo::default()
        };
        let mut renderer = TabBarRenderer::from_configuration(&BTreeMap::from([(
            "template".to_string(),
            r#"{{ "x" | fg(theme.text) }}"#.to_string(),
        )]))
        .unwrap();
        let frame = renderer.render(&mode, &[], 1, 20).unwrap();
        assert!(plain_text(&frame.lines[0]).contains('x'));
    }

    #[test]
    fn missing_explicit_focus_still_follows_active_tab() {
        let tab = TabInfo {
            name: "one".into(),
            active: true,
            ..TabInfo::default()
        };
        let mode = ModeInfo::default();
        let mut renderer = TabBarRenderer::from_configuration(&BTreeMap::from([(
            "template".to_string(),
            r#"{% call Flex(overflow="scroll") %}{% call Button(on_click=actions.switch_tab(1)) %}one{% endcall %}{% endcall %}"#.to_string(),
        )]))
        .unwrap();
        let frame = renderer.render(&mode, &[tab], 1, 3).unwrap();
        assert!(frame.hitboxes[0]
            .iter()
            .any(|action| action == &Some(ClickAction::FocusTab(1))));
    }

    #[test]
    fn plugin_action_defaults_to_centered_half_screen() {
        let action = render_plugin_action(
            r#"{{ Button(on_click=actions.open_or_reload_plugin("session-manager"), label="open") }}"#,
        );
        let expected = FloatingPaneCoordinates::new(
            None,
            None,
            Some("50%".to_string()),
            Some("50%".to_string()),
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            action,
            ClickAction::RunPlugin {
                url: "session-manager".to_string(),
                coordinates: expected,
            }
        );
    }

    #[test]
    fn plugin_action_accepts_fixed_and_percent_coordinates() {
        let action = render_plugin_action(
            r#"{{ Button(on_click=actions.open_or_reload_plugin("session-manager", x=0, y=0, w=32, h="100%"), label="open") }}"#,
        );
        let expected = FloatingPaneCoordinates::new(
            Some("0".to_string()),
            Some("0".to_string()),
            Some("32".to_string()),
            Some("100%".to_string()),
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            action,
            ClickAction::RunPlugin {
                url: "session-manager".to_string(),
                coordinates: expected,
            }
        );
    }

    fn render_plugin_action(template: &str) -> ClickAction {
        let mode = ModeInfo {
            capabilities: PluginCapabilities { arrow_fonts: false },
            ..ModeInfo::default()
        };
        let mut renderer = TabBarRenderer::from_configuration(&BTreeMap::from([(
            "template".to_string(),
            template.to_string(),
        )]))
        .unwrap();
        renderer.render(&mode, &[], 1, 20).unwrap().hitboxes[0]
            .iter()
            .find_map(Clone::clone)
            .unwrap()
    }
}
