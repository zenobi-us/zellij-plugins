//! Evaluates MiniJinja templates into the renderer's internal layout tree.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{Local, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use minijinja::value::{Kwargs, Rest};
use minijinja::{Environment, Error, ErrorKind, State as TemplateState, Value};

use super::{layout_error, ActionRegistry, ButtonPresentation, ButtonView};

const MARKER: &str = "\u{e000}ZT:";
const MARKER_END: char = '\u{e001}';
const ACTION_PREFIX: &str = "\u{e002}ZT:";

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
    pub(super) gap: usize,
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
            gap: 0,
            justify: Justify::Start,
            align: Align::Start,
            overflow: Overflow::Normal,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) enum Node<A> {
    Text(String),
    Button {
        action: A,
        focused: bool,
        label: String,
    },
    Flex {
        spec: FlexSpec,
        children: Vec<Node<A>>,
    },
    OnOverflow {
        children: Vec<Node<A>>,
    },
}

pub(super) fn render_tree<A, F>(
    template: &str,
    data: Value,
    actions: &ActionRegistry<A>,
    present_button: F,
) -> Result<(Node<A>, Option<Duration>), Error>
where
    A: Clone + Send + 'static,
    F: Fn(ButtonView<'_, A>) -> Result<ButtonPresentation, Error> + Send + Sync + 'static,
{
    let mut environment = Environment::new();
    render_tree_in(
        &mut environment,
        TemplateTarget::Source(template),
        data,
        actions,
        present_button,
    )
}

pub(super) fn render_named_tree<'source, A, F>(
    environment: &mut Environment<'source>,
    template_name: &str,
    data: Value,
    actions: &ActionRegistry<A>,
    present_button: F,
) -> Result<(Node<A>, Option<Duration>), Error>
where
    A: Clone + Send + 'static,
    F: Fn(ButtonView<'_, A>) -> Result<ButtonPresentation, Error> + Send + Sync + 'static,
{
    render_tree_in(
        environment,
        TemplateTarget::Name(template_name),
        data,
        actions,
        present_button,
    )
}

enum TemplateTarget<'a> {
    Source(&'a str),
    Name(&'a str),
}

fn render_tree_in<'source, A, F>(
    env: &mut Environment<'source>,
    target: TemplateTarget<'_>,
    data: Value,
    actions: &ActionRegistry<A>,
    present_button: F,
) -> Result<(Node<A>, Option<Duration>), Error>
where
    A: Clone + Send + 'static,
    F: Fn(ButtonView<'_, A>) -> Result<ButtonPresentation, Error> + Send + Sync + 'static,
{
    let arena = Arc::new(Mutex::new(Vec::<A>::new()));
    let refresh_after = Arc::new(Mutex::new(None));
    env.add_filter("format", format_time);
    env.add_filter("bold", bold);
    env.add_filter("dim", dim);
    env.add_filter("fg", foreground);
    env.add_filter("bg", background);
    env.add_function("Flex", flex_marker);
    env.add_function("OnOverflow", on_overflow_marker);
    let clock_refresh = Arc::clone(&refresh_after);
    env.add_function(
        "Clock",
        move |state: &TemplateState<'_, '_>, kwargs: Kwargs| {
            clock_marker(state, kwargs, &clock_refresh)
        },
    );

    let action_values = actions
        .decoders
        .iter()
        .map(|(name, decode)| {
            let arena = Arc::clone(&arena);
            let decode = Arc::clone(decode);
            let function = Value::from_function(move |args: Rest<Value>| {
                let action = decode(&args)?;
                let mut actions = arena
                    .lock()
                    .map_err(|_| layout_error("action registry lock poisoned"))?;
                let id = actions.len();
                actions.push(action);
                Ok(format!("{ACTION_PREFIX}{id}"))
            });
            (name.clone(), function)
        })
        .collect::<BTreeMap<_, _>>();
    env.add_global("actions", Value::from_iter(action_values));

    let button_arena = Arc::clone(&arena);
    env.add_function(
        "Button",
        move |state: &TemplateState<'_, '_>, kwargs: Kwargs| {
            button_marker(state, kwargs, &button_arena, &present_button)
        },
    );

    let rendered = match target {
        TemplateTarget::Source(template) => env.render_str(template, data)?,
        TemplateTarget::Name(name) => env.get_template(name)?.render(data)?,
    };
    let root = Node::Flex {
        spec: FlexSpec::default(),
        children: parse_nodes(&rendered, &arena)?,
    };
    let refresh_after = *refresh_after
        .lock()
        .map_err(|_| layout_error("clock refresh lock poisoned"))?;
    Ok((root, refresh_after))
}

fn decode_action<A: Clone>(value: &Value, arena: &Mutex<Vec<A>>) -> Result<(usize, A), Error> {
    let value = value.as_str().ok_or_else(invalid_button_action)?;
    let id = value
        .strip_prefix(ACTION_PREFIX)
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or_else(invalid_button_action)?;
    let action = arena
        .lock()
        .map_err(|_| layout_error("action registry lock poisoned"))?
        .get(id)
        .cloned()
        .ok_or_else(invalid_button_action)?;
    Ok((id, action))
}

fn invalid_button_action() -> Error {
    Error::new(
        ErrorKind::InvalidOperation,
        "Button on_click must come from actions",
    )
}

fn button_marker<A, F>(
    state: &TemplateState<'_, '_>,
    kwargs: Kwargs,
    arena: &Mutex<Vec<A>>,
    present_button: &F,
) -> Result<String, Error>
where
    A: Clone,
    F: Fn(ButtonView<'_, A>) -> Result<ButtonPresentation, Error>,
{
    let (action_id, action) = decode_action(&kwargs.get::<Value>("on_click")?, arena)?;
    let focused = kwargs.get::<Option<bool>>("focused")?;
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
    let presentation = present_button(ButtonView {
        label: &label,
        action: &action,
        focused,
    })?;
    Ok(format!(
        "{MARKER}B|{action_id}|{}{}{label}{MARKER}/B{MARKER_END}",
        usize::from(presentation.focused),
        MARKER_END,
        label = presentation.label,
    ))
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
    let gap = kwargs.get::<Option<usize>>("gap")?.unwrap_or(0);
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
    Ok(format!("{MARKER}F|{direction}|{grow}|{shrink}|{basis}|{gap}|{justify}|{align}|{overflow}{MARKER_END}{body}{MARKER}/F{MARKER_END}"))
}

fn on_overflow_marker(state: &TemplateState<'_, '_>, kwargs: Kwargs) -> Result<String, Error> {
    let caller: Value = kwargs.get("caller")?;
    kwargs.assert_all_used()?;
    let body = state.format(caller.call(state, &[])?)?;
    Ok(format!("{MARKER}O{MARKER_END}{body}{MARKER}/O{MARKER_END}"))
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

fn clock_marker(
    state: &TemplateState<'_, '_>,
    kwargs: Kwargs,
    requested_refresh: &Mutex<Option<Duration>>,
) -> Result<String, Error> {
    let pattern = kwargs.get::<String>("format")?;
    let timezone = kwargs
        .get::<Option<String>>("tz")?
        .or_else(|| {
            state
                .lookup("env")
                .and_then(|env| env.get_attr("TZ").ok())
                .and_then(|value| value.as_str().map(str::to_string))
        })
        .unwrap_or_else(|| "UTC".to_string());
    kwargs.assert_all_used()?;

    let now = Local::now();
    let period_ns = if pattern.contains("SS") || pattern.contains("%S") {
        1_000_000_000
    } else {
        60_000_000_000
    };
    let elapsed_ns = if period_ns == 1_000_000_000 {
        u64::from(now.nanosecond())
    } else {
        u64::from(now.second()) * 1_000_000_000 + u64::from(now.nanosecond())
    };
    let refresh_after = Duration::from_nanos(period_ns - elapsed_ns);
    let mut requested = requested_refresh
        .lock()
        .map_err(|_| layout_error("clock refresh lock poisoned"))?;
    *requested = Some(requested.map_or(refresh_after, |current| current.min(refresh_after)));

    format_time_in_timezone(now.timestamp(), pattern, &timezone)
}

fn format_time_in_timezone(
    timestamp: i64,
    pattern: String,
    timezone: &str,
) -> Result<String, Error> {
    let timezone = timezone.parse::<Tz>().map_err(|_| {
        Error::new(
            ErrorKind::InvalidOperation,
            format!("invalid clock timezone {timezone:?}"),
        )
    })?;
    let time = Utc
        .timestamp_opt(timestamp, 0)
        .single()
        .ok_or_else(|| Error::new(ErrorKind::InvalidOperation, "invalid system time"))?
        .with_timezone(&timezone);
    Ok(time.format(&chrono_pattern(pattern)).to_string())
}

fn format_time(timestamp: i64, pattern: String) -> Result<String, Error> {
    let time = Local
        .timestamp_opt(timestamp, 0)
        .single()
        .ok_or_else(|| Error::new(ErrorKind::InvalidOperation, "invalid system time"))?;
    Ok(time.format(&chrono_pattern(pattern)).to_string())
}

fn chrono_pattern(pattern: String) -> String {
    pattern
        .replace("YYYY", "%Y")
        .replace("YY", "%y")
        .replace("HH", "%H")
        .replace("MM", "%M")
        .replace("SS", "%S")
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
    Err(layout_error("color must be rgb:R,G,B or index:N"))
}

#[derive(Debug)]
enum ParseOpen<A> {
    Root(Vec<Node<A>>),
    Flex(FlexSpec, Vec<Node<A>>),
    Button(A, bool, String),
    OnOverflow(Vec<Node<A>>),
}

fn parse_nodes<A: Clone>(input: &str, arena: &Mutex<Vec<A>>) -> Result<Vec<Node<A>>, Error> {
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
        if marker == "/F" || marker == "/B" || marker == "/O" {
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
                ("/O", ParseOpen::OnOverflow(children)) => Node::OnOverflow { children },
                _ => return Err(layout_error("mismatched closing marker")),
            };
            push_node(&mut stack, node)?;
        } else if let Some(spec) = marker.strip_prefix("F|") {
            stack.push(ParseOpen::Flex(parse_flex_spec(spec)?, Vec::new()));
        } else if let Some(button) = marker.strip_prefix("B|") {
            let mut fields = button.split('|');
            let action_id = fields
                .next()
                .and_then(|value| value.parse::<usize>().ok())
                .ok_or_else(|| layout_error("invalid Button action marker"))?;
            let focused = match fields.next() {
                Some("0") => false,
                Some("1") => true,
                _ => return Err(layout_error("invalid Button focused marker")),
            };
            if fields.next().is_some() {
                return Err(layout_error("invalid Button marker"));
            }
            let action = arena
                .lock()
                .map_err(|_| layout_error("action registry lock poisoned"))?
                .get(action_id)
                .cloned()
                .ok_or_else(|| layout_error("invalid Button action marker"))?;
            stack.push(ParseOpen::Button(action, focused, String::new()));
        } else if marker == "O" {
            stack.push(ParseOpen::OnOverflow(Vec::new()));
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

fn push_text<A>(stack: &mut [ParseOpen<A>], text: &str) -> Result<(), Error> {
    if text.is_empty() {
        return Ok(());
    }
    match stack
        .last_mut()
        .ok_or_else(|| layout_error("text outside layout root"))?
    {
        ParseOpen::Root(nodes) | ParseOpen::Flex(_, nodes) | ParseOpen::OnOverflow(nodes) => {
            nodes.push(Node::Text(text.to_string()))
        },
        ParseOpen::Button(_, _, label) => label.push_str(text),
    }
    Ok(())
}

fn push_node<A>(stack: &mut [ParseOpen<A>], node: Node<A>) -> Result<(), Error> {
    match stack
        .last_mut()
        .ok_or_else(|| layout_error("node outside layout root"))?
    {
        ParseOpen::Root(_) if matches!(node, Node::OnOverflow { .. }) => {
            return Err(layout_error("OnOverflow must be a direct child of Flex"))
        },
        ParseOpen::Root(nodes) | ParseOpen::Flex(_, nodes) | ParseOpen::OnOverflow(nodes) => {
            nodes.push(node)
        },
        ParseOpen::Button(_, _, _) => {
            return Err(layout_error("Button cannot contain layout helpers"))
        },
    }
    Ok(())
}

fn parse_flex_spec(value: &str) -> Result<FlexSpec, Error> {
    let fields: Vec<_> = value.split('|').collect();
    if fields.len() != 8 {
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
        gap: fields[4]
            .parse()
            .map_err(|_| layout_error("invalid Flex gap"))?,
        justify: match fields[5] {
            "start" => Justify::Start,
            "center" => Justify::Center,
            "end" => Justify::End,
            "space-between" => Justify::SpaceBetween,
            "space-around" => Justify::SpaceAround,
            _ => return Err(layout_error("invalid Flex justify")),
        },
        align: match fields[6] {
            "start" => Align::Start,
            "center" => Align::Center,
            "end" => Align::End,
            "stretch" => Align::Stretch,
            _ => return Err(layout_error("invalid Flex align")),
        },
        overflow: match fields[7] {
            "normal" => Overflow::Normal,
            "scroll" => Overflow::Scroll,
            _ => return Err(layout_error("invalid Flex overflow")),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::layout;

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum TestAction {
        New,
    }

    #[test]
    fn custom_template_parses_nested_flex_and_button() {
        let arena = Mutex::new(vec![TestAction::New]);
        let rendered = format!(
            "{MARKER}F|column|1|1|auto|2|center|stretch|normal{MARKER_END}{MARKER}B|0|1{MARKER_END}+{MARKER}/B{MARKER_END}{MARKER}/F{MARKER_END}"
        );
        let nodes = parse_nodes(&rendered, &arena).unwrap();
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
        assert_eq!(frame.hitboxes[0][0], Some(TestAction::New));
    }

    #[test]
    fn theme_filters_validate_color_shape() {
        assert!(foreground("x".into(), "rgb:1,2,3".into())
            .unwrap()
            .contains("\u{1b}[38;2;1;2;3m"));
        assert!(background("x".into(), "red".into()).is_err());
    }

    #[test]
    fn parses_on_overflow_inside_flex() {
        let arena = Mutex::<Vec<TestAction>>::new(Vec::new());
        let rendered = format!(
            "{MARKER}F|row|0|1|auto|0|start|start|scroll{MARKER_END}tabs{MARKER}O{MARKER_END}v{MARKER}/O{MARKER_END}{MARKER}/F{MARKER_END}"
        );
        let nodes = parse_nodes(&rendered, &arena).unwrap();
        assert!(matches!(
            &nodes[0],
            Node::Flex { children, .. } if matches!(&children[1], Node::OnOverflow { .. })
        ));
    }
}
