//! Evaluates MiniJinja templates into the renderer's internal layout tree.

use ansi_term::ANSIStrings;
use chrono::{Local, TimeZone};
use minijinja::value::Kwargs;
use minijinja::{context, Environment, Error, ErrorKind, State as TemplateState, Value};
use serde::Serialize;
use zellij_tile::prelude::*;
use zellij_tile_utils::style;

use super::{layout_error, ClickAction};

const MARKER: &str = "\u{e000}ZT:";
const MARKER_END: char = '\u{e001}';
const ACTION_PREFIX: &str = "\u{e002}ZT:";

/// Built-in template used when plugin configuration provides no override.
pub(super) const DEFAULT_TEMPLATE: &str = r#"{%- call Flex(direction="row") -%}
{%- call Flex(shrink=0) -%}{{ session.name }} {% endcall -%}
{%- call Flex(direction="row", grow=1, shrink=1, overflow="scroll") -%}
{%- for tab in session.tabs -%}
{%- call Button(on_click=context.actions.switch_tab(tab.index), focused=tab.active) -%}{{ tab.name }}{%- endcall -%}
{%- endfor -%}
{%- endcall -%}
{%- call Button(on_click=context.actions.new_tab()) -%}+{%- endcall -%}
{%- endcall -%}"#;

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

#[derive(Serialize)]
struct TemplateTheme {
    text: String,
    background: String,
    active_text: String,
    active_background: String,
    muted_text: String,
    muted_background: String,
    alert: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Direction {
    Row,
    Column,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Justify {
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Align {
    Start,
    Center,
    End,
    Stretch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Overflow {
    Normal,
    Scroll,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Basis {
    Auto,
    Cells(usize),
}

#[derive(Clone, Debug)]
pub(super) struct FlexSpec {
    pub(super) direction: Direction,
    pub(super) grow: usize,
    pub(super) shrink: usize,
    pub(super) basis: Basis,
    pub(super) justify: Justify,
    pub(super) align: Align,
    pub(super) overflow: Overflow,
}

impl Default for FlexSpec {
    fn default() -> Self {
        Self {
            direction: Direction::Row,
            grow: 0,
            shrink: 1,
            basis: Basis::Auto,
            justify: Justify::Start,
            align: Align::Start,
            overflow: Overflow::Normal,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) enum Node {
    Text(String),
    Button {
        action: ClickAction,
        focused: bool,
        label: String,
    },
    Flex {
        spec: FlexSpec,
        children: Vec<Node>,
    },
}

pub(super) fn render_tree(
    template: &str,
    session_name: Option<&str>,
    tabs: &[TabInfo],
    colors: Styling,
    capabilities: PluginCapabilities,
) -> Result<Node, Error> {
    let mut env = Environment::new();
    env.add_filter("format", format_time);
    env.add_filter("bold", bold);
    env.add_filter("dim", dim);
    env.add_filter("fg", foreground);
    env.add_filter("bg", background);
    env.add_function("Flex", flex_marker);

    let tab_infos = tabs.to_vec();
    env.add_function(
        "Button",
        move |state: &TemplateState<'_, '_>, kwargs: Kwargs| {
            button_marker(state, kwargs, &tab_infos, colors, capabilities)
        },
    );

    let actions = context! {
        switch_tab => Value::from_function(action_switch_tab),
        new_tab => Value::from_function(action_new_tab),
    };
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
    let rendered = env.render_str(
        template,
        context! {
            session => model,
            system => context! { time => Local::now().timestamp() },
            context => context! { actions => actions, theme => theme },
        },
    )?;
    Ok(Node::Flex {
        spec: FlexSpec::default(),
        children: parse_nodes(&rendered)?,
    })
}

fn action_switch_tab(index: usize) -> String {
    format!("{ACTION_PREFIX}switch:{index}")
}

fn action_new_tab() -> String {
    format!("{ACTION_PREFIX}new")
}

fn decode_action(value: &Value) -> Result<ClickAction, Error> {
    let value = value.as_str().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidOperation,
            "Button on_click must come from context.actions",
        )
    })?;
    let encoded = value.strip_prefix(ACTION_PREFIX).ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidOperation,
            "Button on_click must come from context.actions",
        )
    })?;
    if encoded == "new" {
        return Ok(ClickAction::NewTab);
    }
    if let Some(index) = encoded.strip_prefix("switch:") {
        let index = index
            .parse()
            .map_err(|_| Error::new(ErrorKind::InvalidOperation, "invalid switch_tab action"))?;
        return Ok(ClickAction::SwitchTab(index));
    }
    Err(Error::new(
        ErrorKind::InvalidOperation,
        "unknown click action",
    ))
}

fn button_marker(
    state: &TemplateState<'_, '_>,
    kwargs: Kwargs,
    tabs: &[TabInfo],
    colors: Styling,
    capabilities: PluginCapabilities,
) -> Result<String, Error> {
    let action = decode_action(&kwargs.get::<Value>("on_click")?)?;
    let explicit_focused = kwargs.get::<Option<bool>>("focused")?;
    let label = kwargs.get::<Option<String>>("label")?;
    let caller = kwargs.get::<Option<Value>>("caller")?;
    kwargs.assert_all_used()?;
    let label = match (label, caller) {
        (Some(label), _) => label,
        (None, Some(caller)) => state.format(caller.call(state, &[])?)?,
        (None, None) => {
            return Err(Error::new(
                ErrorKind::MissingArgument,
                "Button expects label or caller body",
            ))
        },
    };
    if label.contains(MARKER) {
        return Err(Error::new(
            ErrorKind::InvalidOperation,
            "Button label cannot contain layout helpers",
        ));
    }
    let focused = explicit_focused.unwrap_or_else(|| match action {
        ClickAction::SwitchTab(index) => tabs
            .iter()
            .any(|tab| tab.active && tab.position + 1 == index),
        ClickAction::NewTab => false,
    });
    let styled = style_button(&label, &action, focused, tabs, colors, capabilities)?;
    Ok(format!(
        "{MARKER}B|{}|{}{}{}{MARKER}/B{MARKER_END}",
        encode_action(&action),
        usize::from(focused),
        MARKER_END,
        styled
    ))
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
            let tab = tabs
                .iter()
                .find(|tab| tab.position + 1 == *index)
                .ok_or_else(|| {
                    Error::new(
                        ErrorKind::InvalidOperation,
                        "switch_tab index does not exist",
                    )
                })?;
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
            let tab = tabs
                .iter()
                .find(|tab| tab.position + 1 == *index)
                .ok_or_else(|| {
                    Error::new(
                        ErrorKind::InvalidOperation,
                        "switch_tab index does not exist",
                    )
                })?;
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

fn encode_action(action: &ClickAction) -> String {
    match action {
        ClickAction::SwitchTab(index) => format!("switch:{index}"),
        ClickAction::NewTab => "new".into(),
    }
}

fn flex_marker(state: &TemplateState<'_, '_>, kwargs: Kwargs) -> Result<String, Error> {
    let direction = parse_choice(
        kwargs
            .get::<Option<String>>("direction")?
            .as_deref()
            .unwrap_or("row"),
        &["row", "column"],
        "direction",
    )?;
    let grow = kwargs.get::<Option<usize>>("grow")?.unwrap_or(0);
    let shrink = kwargs.get::<Option<usize>>("shrink")?.unwrap_or(1);
    let basis = match kwargs.get::<Option<Value>>("basis")? {
        None => "auto".into(),
        Some(value) if value.as_str() == Some("auto") => "auto".into(),
        Some(value) => value
            .as_usize()
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidOperation,
                    "Flex basis must be auto or an integer",
                )
            })?
            .to_string(),
    };
    let justify = parse_choice(
        kwargs
            .get::<Option<String>>("justify")?
            .as_deref()
            .unwrap_or("start"),
        &["start", "center", "end", "space-between", "space-around"],
        "justify",
    )?;
    let align = parse_choice(
        kwargs
            .get::<Option<String>>("align")?
            .as_deref()
            .unwrap_or("start"),
        &["start", "center", "end", "stretch"],
        "align",
    )?;
    let overflow = parse_choice(
        kwargs
            .get::<Option<String>>("overflow")?
            .as_deref()
            .unwrap_or("normal"),
        &["normal", "scroll"],
        "overflow",
    )?;
    let caller: Value = kwargs.get("caller")?;
    kwargs.assert_all_used()?;
    let body = state.format(caller.call(state, &[])?)?;
    Ok(format!("{MARKER}F|{direction}|{grow}|{shrink}|{basis}|{justify}|{align}|{overflow}{MARKER_END}{body}{MARKER}/F{MARKER_END}"))
}

fn parse_choice(value: &str, valid: &[&str], name: &str) -> Result<String, Error> {
    if valid.contains(&value) {
        Ok(value.to_string())
    } else {
        Err(Error::new(
            ErrorKind::InvalidOperation,
            format!("Flex {name} has invalid value {value:?}"),
        ))
    }
}

fn format_time(timestamp: i64, pattern: String) -> Result<String, Error> {
    let time = Local
        .timestamp_opt(timestamp, 0)
        .single()
        .ok_or_else(|| Error::new(ErrorKind::InvalidOperation, "invalid system time"))?;
    let pattern = pattern
        .replace("YYYY", "%Y")
        .replace("YY", "%y")
        .replace("HH", "%H")
        .replace("MM", "%M")
        .replace("SS", "%S");
    Ok(time.format(&pattern).to_string())
}

fn bold(value: String) -> String {
    format!("\u{1b}[1m{value}\u{1b}[22m")
}

fn dim(value: String) -> String {
    format!("\u{1b}[2m{value}\u{1b}[22m")
}

fn foreground(value: String, color: String) -> Result<String, Error> {
    Ok(format!(
        "{}{}\u{1b}[39m",
        color_escape(&color, false)?,
        value
    ))
}

fn background(value: String, color: String) -> Result<String, Error> {
    Ok(format!(
        "{}{}\u{1b}[49m",
        color_escape(&color, true)?,
        value
    ))
}

fn color_token(color: PaletteColor) -> String {
    match color {
        PaletteColor::Rgb((r, g, b)) => format!("rgb:{r},{g},{b}"),
        PaletteColor::EightBit(index) => format!("index:{index}"),
    }
}

fn color_escape(token: &str, background: bool) -> Result<String, Error> {
    let channel = if background { 48 } else { 38 };
    if let Some(rgb) = token.strip_prefix("rgb:") {
        let values: Vec<_> = rgb.split(',').collect();
        if values.len() == 3 && values.iter().all(|value| value.parse::<u8>().is_ok()) {
            return Ok(format!(
                "\u{1b}[{channel};2;{};{};{}m",
                values[0], values[1], values[2]
            ));
        }
    }
    if let Some(index) = token.strip_prefix("index:") {
        if let Ok(index) = index.parse::<u8>() {
            return Ok(format!("\u{1b}[{channel};5;{index}m"));
        }
    }
    Err(layout_error("color must come from context.theme"))
}

#[derive(Debug)]
enum ParseOpen {
    Root(Vec<Node>),
    Flex(FlexSpec, Vec<Node>),
    Button(ClickAction, bool, String),
}

fn parse_nodes(input: &str) -> Result<Vec<Node>, Error> {
    let mut stack = vec![ParseOpen::Root(Vec::new())];
    let mut rest = input;
    while let Some(at) = rest.find(MARKER) {
        push_text(&mut stack, &rest[..at])?;
        rest = &rest[at + MARKER.len()..];
        let end = rest
            .find(MARKER_END)
            .ok_or_else(|| layout_error("unterminated internal marker"))?;
        let marker = &rest[..end];
        rest = &rest[end + MARKER_END.len_utf8()..];
        if marker == "/F" || marker == "/B" {
            let open = stack
                .pop()
                .ok_or_else(|| layout_error("unexpected closing marker"))?;
            let node = match (marker, open) {
                ("/F", ParseOpen::Flex(spec, children)) => Node::Flex { spec, children },
                ("/B", ParseOpen::Button(action, focused, label)) => Node::Button {
                    action,
                    focused,
                    label,
                },
                _ => return Err(layout_error("mismatched closing marker")),
            };
            push_node(&mut stack, node)?;
        } else if let Some(spec) = marker.strip_prefix("F|") {
            stack.push(ParseOpen::Flex(parse_flex_spec(spec)?, Vec::new()));
        } else if let Some(button) = marker.strip_prefix("B|") {
            let mut fields = button.split('|');
            let action = parse_action(fields.next().unwrap_or_default())?;
            let focused = match fields.next() {
                Some("0") => false,
                Some("1") => true,
                _ => return Err(layout_error("invalid Button focused marker")),
            };
            if fields.next().is_some() {
                return Err(layout_error("invalid Button marker"));
            }
            stack.push(ParseOpen::Button(action, focused, String::new()));
        } else {
            return Err(layout_error("unknown internal marker"));
        }
    }
    push_text(&mut stack, rest)?;
    if stack.len() != 1 {
        return Err(layout_error("unclosed layout helper"));
    }
    match stack.pop().unwrap() {
        ParseOpen::Root(nodes) => Ok(nodes),
        _ => unreachable!(),
    }
}

fn push_text(stack: &mut [ParseOpen], text: &str) -> Result<(), Error> {
    if text.is_empty() {
        return Ok(());
    }
    match stack
        .last_mut()
        .ok_or_else(|| layout_error("text outside layout root"))?
    {
        ParseOpen::Root(nodes) | ParseOpen::Flex(_, nodes) => {
            nodes.push(Node::Text(text.to_string()))
        },
        ParseOpen::Button(_, _, label) => label.push_str(text),
    }
    Ok(())
}

fn push_node(stack: &mut [ParseOpen], node: Node) -> Result<(), Error> {
    match stack
        .last_mut()
        .ok_or_else(|| layout_error("node outside layout root"))?
    {
        ParseOpen::Root(nodes) | ParseOpen::Flex(_, nodes) => nodes.push(node),
        ParseOpen::Button(_, _, _) => {
            return Err(layout_error("Button cannot contain layout helpers"))
        },
    }
    Ok(())
}

fn parse_action(value: &str) -> Result<ClickAction, Error> {
    if value == "new" {
        return Ok(ClickAction::NewTab);
    }
    value
        .strip_prefix("switch:")
        .and_then(|v| v.parse().ok())
        .map(ClickAction::SwitchTab)
        .ok_or_else(|| layout_error("invalid Button action marker"))
}

fn parse_flex_spec(value: &str) -> Result<FlexSpec, Error> {
    let fields: Vec<_> = value.split('|').collect();
    if fields.len() != 7 {
        return Err(layout_error("invalid Flex marker"));
    }
    Ok(FlexSpec {
        direction: match fields[0] {
            "row" => Direction::Row,
            "column" => Direction::Column,
            _ => return Err(layout_error("invalid Flex direction")),
        },
        grow: fields[1]
            .parse()
            .map_err(|_| layout_error("invalid Flex grow"))?,
        shrink: fields[2]
            .parse()
            .map_err(|_| layout_error("invalid Flex shrink"))?,
        basis: if fields[3] == "auto" {
            Basis::Auto
        } else {
            Basis::Cells(
                fields[3]
                    .parse()
                    .map_err(|_| layout_error("invalid Flex basis"))?,
            )
        },
        justify: match fields[4] {
            "start" => Justify::Start,
            "center" => Justify::Center,
            "end" => Justify::End,
            "space-between" => Justify::SpaceBetween,
            "space-around" => Justify::SpaceAround,
            _ => return Err(layout_error("invalid Flex justify")),
        },
        align: match fields[5] {
            "start" => Align::Start,
            "center" => Align::Center,
            "end" => Align::End,
            "stretch" => Align::Stretch,
            _ => return Err(layout_error("invalid Flex align")),
        },
        overflow: match fields[6] {
            "normal" => Overflow::Normal,
            "scroll" => Overflow::Scroll,
            _ => return Err(layout_error("invalid Flex overflow")),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::layout::layout;

    #[test]
    fn custom_template_parses_nested_flex_and_button() {
        let rendered = format!(
            "{MARKER}F|column|1|1|auto|center|stretch|normal{MARKER_END}{MARKER}B|new|1{MARKER_END}+{MARKER}/B{MARKER_END}{MARKER}/F{MARKER_END}"
        );
        let nodes = parse_nodes(&rendered).unwrap();
        let frame = layout(
            &Node::Flex {
                spec: FlexSpec::default(),
                children: nodes,
            },
            3,
            1,
        )
        .unwrap()
        .into_frame();
        assert_eq!(frame.hitboxes[0][0], Some(ClickAction::NewTab));
    }

    #[test]
    fn theme_filters_accept_only_theme_tokens() {
        assert!(foreground("x".into(), "rgb:1,2,3".into())
            .unwrap()
            .contains("\u{1b}[38;2;1;2;3m"));
        assert!(background("x".into(), "red".into()).is_err());
    }
}
