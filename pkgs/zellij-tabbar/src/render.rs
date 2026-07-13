//! Tabbar-specific template data, actions, and button styling.

use ansi_term::ANSIStrings;
use serde::Serialize;
use std::collections::BTreeMap;
use zellij_template_render::{
    error_frame as render_error_frame, ActionRegistry, ButtonPresentation, ButtonView, Environment,
    Error, ErrorKind, Frame, Renderer, TemplateContext, TemplateEnvironment, TemplateHost,
    TemplateSource, TemplateTheme, Value, Viewport,
};
use zellij_tile::prelude::*;
use zellij_tile_utils::style;

/// Built-in template used when plugin configuration provides no override.
const DEFAULT_TEMPLATE_NAME: &str = "main.jinja";

/// Typed operation attached to cells rendered by `Button`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ClickAction {
    SwitchTab(usize),
    NewTab,
}

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
        let source =
            TemplateSource::from_configuration(configuration, embedded, DEFAULT_TEMPLATE_NAME)?;
        Ok(Self {
            host: TemplateHost::new(
                Renderer::new(
                    ActionRegistry::new()
                        .with("switch_tab", |args| {
                            let index =
                                args.first().and_then(Value::as_usize).ok_or_else(|| {
                                    Error::new(
                                        ErrorKind::InvalidOperation,
                                        "switch_tab expects an integer index",
                                    )
                                })?;
                            Ok(ClickAction::SwitchTab(index))
                        })
                        .with("new_tab", |args| {
                            if !args.is_empty() {
                                return Err(Error::new(
                                    ErrorKind::InvalidOperation,
                                    "new_tab expects no arguments",
                                ));
                            }
                            Ok(ClickAction::NewTab)
                        }),
                ),
                source,
                TemplateEnvironment::from_configuration(configuration),
            ),
        })
    }

    /// Renders tabbar data through the shared template renderer.
    pub(crate) fn render(
        &mut self,
        session_name: Option<&str>,
        tabs: &[TabInfo],
        rows: usize,
        cols: usize,
        colors: Styling,
        capabilities: PluginCapabilities,
    ) -> Result<RenderedFrame, Error> {
        let model = TemplateSession {
            name: session_name.unwrap_or_default(),
            tabs: tabs
                .iter()
                .map(|tab| TemplateTab {
                    name: &tab.name,
                    index: tab.position + 1,
                    active: tab.active,
                })
                .collect(),
        };
        let theme = TemplateTheme {
            text: color_token(colors.text_unselected.base),
            background: color_token(colors.text_unselected.background),
            active_text: color_token(colors.ribbon_selected.base),
            active_background: color_token(colors.ribbon_selected.background),
            muted_text: color_token(colors.ribbon_unselected.base),
            muted_background: color_token(colors.ribbon_unselected.background),
            alert: color_token(colors.ribbon_unselected.emphasis_3),
        };
        let tabs = tabs.to_vec();
        let viewport = Viewport { rows, cols };
        self.host.render(
            TemplateContext::new().with("session", Value::from_serialize(model)),
            theme,
            viewport,
            move |button| present_button(button, &tabs, colors, capabilities),
        )
    }

    pub(crate) fn error_frame(&self, error: &Error, rows: usize, cols: usize) -> RenderedFrame {
        render_error_frame(error, Viewport { rows, cols })
    }
}

fn present_button(
    button: ButtonView<'_, ClickAction>,
    tabs: &[TabInfo],
    colors: Styling,
    capabilities: PluginCapabilities,
) -> Result<ButtonPresentation, Error> {
    let focused = button.focused.unwrap_or_else(|| match button.action {
        ClickAction::SwitchTab(index) => tabs
            .iter()
            .any(|tab| tab.active && tab.position + 1 == *index),
        ClickAction::NewTab => false,
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
        ClickAction::SwitchTab(index) => {
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
        ClickAction::NewTab => label.to_string(),
    };
    let alternate = match action {
        ClickAction::SwitchTab(index) => index % 2 == 0 && capabilities.arrow_fonts,
        ClickAction::NewTab => tabs.len() % 2 == 1 && capabilities.arrow_fonts,
    };
    let background = if focused {
        palette.ribbon_selected.background
    } else if alternate {
        palette.ribbon_unselected.emphasis_1
    } else {
        palette.ribbon_unselected.background
    };
    let foreground = match action {
        ClickAction::SwitchTab(index) => {
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
        ClickAction::NewTab => palette.ribbon_unselected.base,
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
                "switch_tab index does not exist",
            )
        })
}

fn color_token(color: PaletteColor) -> String {
    match color {
        PaletteColor::Rgb((r, g, b)) => format!("rgb:{r},{g},{b}"),
        PaletteColor::EightBit(index) => format!("index:{index}"),
    }
}

#[cfg(test)]
mod tests {
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
        let mode = ModeInfo::default();
        let frame = TabBarRenderer::from_configuration(&BTreeMap::new())
            .unwrap()
            .render(
                Some("demo"),
                &[first, second],
                1,
                80,
                mode.style.colors,
                PluginCapabilities { arrow_fonts: false },
            )
            .unwrap();
        assert!(plain_text(&frame.lines[0]).contains("one"));
        assert!(frame.hitboxes[0]
            .iter()
            .any(|action| action == &Some(ClickAction::SwitchTab(1))));
        assert!(frame.hitboxes[0]
            .iter()
            .any(|action| action == &Some(ClickAction::NewTab)));
        assert!(frame.refresh_after.is_some_and(|delay| {
            !delay.is_zero() && delay <= std::time::Duration::from_secs(60)
        }));
    }

    #[test]
    fn shared_host_supplies_top_level_theme() {
        let mode = ModeInfo::default();
        let mut renderer = TabBarRenderer::from_configuration(&BTreeMap::from([(
            "template".to_string(),
            r#"{{ "x" | fg(theme.text) }}"#.to_string(),
        )]))
        .unwrap();
        let capabilities = PluginCapabilities { arrow_fonts: false };

        let frame = renderer
            .render(None, &[], 1, 20, mode.style.colors, capabilities)
            .unwrap();
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
        let frame = renderer
            .render(
                None,
                &[tab],
                1,
                3,
                mode.style.colors,
                PluginCapabilities { arrow_fonts: true },
            )
            .unwrap();
        assert!(frame.hitboxes[0]
            .iter()
            .any(|action| action == &Some(ClickAction::SwitchTab(1))));
    }
}
