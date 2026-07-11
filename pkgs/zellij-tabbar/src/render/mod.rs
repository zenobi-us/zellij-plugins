//! Template rendering, Flex layout, button styling, and click-hitbox generation.
//!
//! Templates produce an internal layout tree. Layout turns that tree into a
//! viewport-sized frame whose text cells and click actions share coordinates.

mod layout;
mod template;

use minijinja::{Error, ErrorKind};
use unicode_width::UnicodeWidthChar;
use zellij_tile::prelude::*;

use self::template::DEFAULT_TEMPLATE;

/// Typed operation attached to cells rendered by `Button`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ClickAction {
    SwitchTab(usize),
    NewTab,
}

/// Viewport output and its coordinate-matched click targets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RenderedFrame {
    pub lines: Vec<String>,
    pub hitboxes: Vec<Vec<Option<ClickAction>>>,
}

/// Chooses configured template, falling back to the built-in template.
pub(crate) fn selected_template(override_template: Option<&str>) -> &str {
    override_template.unwrap_or(DEFAULT_TEMPLATE)
}

/// Renders template and context into terminal lines plus two-dimensional hitboxes.
pub(crate) fn render(
    template: &str,
    session_name: Option<&str>,
    tabs: &[TabInfo],
    rows: usize,
    cols: usize,
    colors: Styling,
    capabilities: PluginCapabilities,
) -> Result<RenderedFrame, Error> {
    if rows == 0 || cols == 0 {
        return Ok(RenderedFrame::default());
    }
    let root = template::render_tree(template, session_name, tabs, colors, capabilities)?;
    Ok(layout::layout(&root, cols, rows)?.into_frame())
}

/// Produces visible, clipped output for template or layout failures.
pub(crate) fn error_frame(error: &Error, rows: usize, cols: usize) -> RenderedFrame {
    let text = format!("template error: {error}");
    let clipped: String = text
        .chars()
        .scan(0usize, |width, ch| {
            let next = *width + UnicodeWidthChar::width(ch).unwrap_or(0);
            if next > cols {
                None
            } else {
                *width = next;
                Some(ch)
            }
        })
        .collect();
    let mut hitboxes = vec![vec![None; cols]; rows.min(1)];
    if rows == 0 {
        hitboxes.clear();
    }
    RenderedFrame {
        lines: if rows == 0 { vec![] } else { vec![clipped] },
        hitboxes,
    }
}

pub(super) fn layout_error(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::InvalidOperation, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain_text(value: &str) -> String {
        let mut output = String::new();
        let mut chars = value.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\u{1b}' {
                layout::consume_ansi(&mut chars, &mut String::new()).unwrap();
            } else {
                output.push(ch);
            }
        }
        output
    }

    #[test]
    fn default_and_custom_template_selection() {
        assert_eq!(selected_template(None), DEFAULT_TEMPLATE);
        assert_eq!(selected_template(Some("custom")), "custom");
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
        let frame = render(
            DEFAULT_TEMPLATE,
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
    }

    #[test]
    fn malformed_template_gets_visible_error_frame() {
        let error = layout_error("bad layout");
        let frame = error_frame(&error, 1, 80);
        assert!(frame.lines[0].contains("template error:"));
        assert!(frame.lines[0].contains("bad layout"));
    }
}
