use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use minijinja::value::{from_args, Kwargs};
use minijinja::{Error, ErrorKind, Value};
use zellij_tile::prelude::{
    change_floating_panes_coordinates, float_multiple_panes, focus_plugin_pane,
    focus_terminal_pane, new_tab, open_plugin_pane_floating, pipe_message_to_plugin,
    reload_plugin_with_id, run_command, switch_session, switch_tab_to, FloatingPaneCoordinates,
    MessageToPlugin, NewPluginArgs, PaneId, PaneManifest, PermissionType,
};

/// Typed decoder for one function exposed under the template `actions` object.
pub(crate) type ActionDecoder<A> = Arc<dyn Fn(&[Value]) -> Result<A, Error> + Send + Sync>;

/// Template action functions and their host-side typed decoders.
pub struct ActionRegistry<A> {
    pub(crate) decoders: BTreeMap<String, ActionDecoder<A>>,
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

#[derive(Clone, Debug)]
pub enum BuiltinAction {
    FocusSession(Option<String>),
    FocusTab(usize),
    FocusTerminalPane {
        id: u32,
        should_float_if_hidden: bool,
        should_be_in_place_if_hidden: bool,
    },
    FocusPluginPane {
        id: u32,
        should_float_if_hidden: bool,
        should_be_in_place_if_hidden: bool,
    },
    NewTab,
    RunPlugin {
        url: String,
        coordinates: FloatingPaneCoordinates,
    },
    SendDataToPlugin(MessageToPlugin),
    RunShellCommand {
        command: Vec<String>,
        context: BTreeMap<String, String>,
    },
}

impl PartialEq for BuiltinAction {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::FocusSession(left), Self::FocusSession(right)) => left == right,
            (Self::FocusTab(left), Self::FocusTab(right)) => left == right,
            (
                Self::FocusTerminalPane {
                    id: left_id,
                    should_float_if_hidden: left_float,
                    should_be_in_place_if_hidden: left_in_place,
                },
                Self::FocusTerminalPane {
                    id: right_id,
                    should_float_if_hidden: right_float,
                    should_be_in_place_if_hidden: right_in_place,
                },
            ) => {
                left_id == right_id && left_float == right_float && left_in_place == right_in_place
            },
            (
                Self::FocusPluginPane {
                    id: left_id,
                    should_float_if_hidden: left_float,
                    should_be_in_place_if_hidden: left_in_place,
                },
                Self::FocusPluginPane {
                    id: right_id,
                    should_float_if_hidden: right_float,
                    should_be_in_place_if_hidden: right_in_place,
                },
            ) => {
                left_id == right_id && left_float == right_float && left_in_place == right_in_place
            },
            (Self::NewTab, Self::NewTab) => true,
            (
                Self::RunPlugin {
                    url: left_url,
                    coordinates: left_coordinates,
                },
                Self::RunPlugin {
                    url: right_url,
                    coordinates: right_coordinates,
                },
            ) => left_url == right_url && left_coordinates == right_coordinates,
            (Self::SendDataToPlugin(left), Self::SendDataToPlugin(right)) => {
                same_plugin_message(left, right)
            },
            (
                Self::RunShellCommand {
                    command: left_command,
                    context: left_context,
                },
                Self::RunShellCommand {
                    command: right_command,
                    context: right_context,
                },
            ) => left_command == right_command && left_context == right_context,
            _ => false,
        }
    }
}

impl Eq for BuiltinAction {}

fn same_plugin_message(left: &MessageToPlugin, right: &MessageToPlugin) -> bool {
    left.plugin_url == right.plugin_url
        && left.destination_plugin_id == right.destination_plugin_id
        && left.plugin_config == right.plugin_config
        && left.message_name == right.message_name
        && left.message_payload == right.message_payload
        && left.message_args == right.message_args
        && left.floating_pane_coordinates == right.floating_pane_coordinates
        && match (&left.new_plugin_args, &right.new_plugin_args) {
            (Some(left), Some(right)) => {
                left.should_float == right.should_float
                    && left.pane_id_to_replace == right.pane_id_to_replace
                    && left.pane_title == right.pane_title
                    && left.cwd == right.cwd
                    && left.skip_cache == right.skip_cache
                    && left.should_focus == right.should_focus
            },
            (None, None) => true,
            _ => false,
        }
}

impl ActionRegistry<BuiltinAction> {
    pub fn with_builtins(self) -> Self {
        self.with("focus_session", decode_focus_session)
            .with("focus_tab", decode_focus_tab)
            .with("switch_tab", decode_focus_tab)
            .with("focus_terminal_pane", decode_focus_terminal_pane)
            .with("focus_plugin_pane", decode_focus_plugin_pane)
            .with("new_tab", decode_new_tab)
            .with("run_plugin", decode_run_plugin)
            .with("open_or_reload_plugin", decode_run_plugin)
            .with("send_data_to_plugin", decode_send_data_to_plugin)
            .with("run_shell_cmd", decode_run_shell_cmd)
    }
}

pub fn builtin_action_permissions() -> &'static [PermissionType] {
    &[
        PermissionType::ChangeApplicationState,
        PermissionType::OpenTerminalsOrPlugins,
        PermissionType::RunCommands,
        PermissionType::MessageAndLaunchOtherPlugins,
    ]
}

#[derive(Default)]
pub struct BuiltinActionDispatcher {
    open_plugins: BTreeMap<String, PaneId>,
}

impl BuiltinActionDispatcher {
    pub fn retain_open_plugins(&mut self, panes: &PaneManifest) {
        let plugin_ids = panes
            .panes
            .values()
            .flatten()
            .filter(|pane| pane.is_plugin)
            .map(|pane| pane.id)
            .collect::<BTreeSet<_>>();
        self.open_plugins
            .retain(|_, pane_id| matches!(pane_id, PaneId::Plugin(id) if plugin_ids.contains(id)));
    }

    pub fn dispatch(&mut self, action: BuiltinAction) {
        match action {
            BuiltinAction::FocusSession(name) => switch_session(name.as_deref()),
            BuiltinAction::FocusTab(index) => switch_tab_to(index as u32),
            BuiltinAction::FocusTerminalPane {
                id,
                should_float_if_hidden,
                should_be_in_place_if_hidden,
            } => focus_terminal_pane(id, should_float_if_hidden, should_be_in_place_if_hidden),
            BuiltinAction::FocusPluginPane {
                id,
                should_float_if_hidden,
                should_be_in_place_if_hidden,
            } => focus_plugin_pane(id, should_float_if_hidden, should_be_in_place_if_hidden),
            BuiltinAction::NewTab => {
                new_tab::<&str>(None, None);
            },
            BuiltinAction::RunPlugin { url, coordinates } => {
                if let Some(PaneId::Plugin(plugin_id)) = self.open_plugins.get(&url).cloned() {
                    float_multiple_panes(vec![PaneId::Plugin(plugin_id)]);
                    change_floating_panes_coordinates(vec![(
                        PaneId::Plugin(plugin_id),
                        coordinates,
                    )]);
                    focus_plugin_pane(plugin_id, true, false);
                    reload_plugin_with_id(plugin_id);
                } else if let Some(pane_id) = open_plugin_pane_floating(
                    &url,
                    BTreeMap::new(),
                    Some(coordinates),
                    BTreeMap::new(),
                ) {
                    self.open_plugins.insert(url, pane_id);
                }
            },
            BuiltinAction::SendDataToPlugin(message) => pipe_message_to_plugin(message),
            BuiltinAction::RunShellCommand { command, context } => {
                let command = command.iter().map(String::as_str).collect::<Vec<_>>();
                run_command(&command, context);
            },
        }
    }
}

fn decode_focus_session(args: &[Value]) -> Result<BuiltinAction, Error> {
    match args {
        [] => Ok(BuiltinAction::FocusSession(None)),
        [name] => Ok(BuiltinAction::FocusSession(Some(required_string(
            name,
            "focus_session expects a session name",
        )?))),
        _ => Err(invalid("focus_session expects zero or one session name")),
    }
}

fn decode_focus_tab(args: &[Value]) -> Result<BuiltinAction, Error> {
    let index = args
        .first()
        .and_then(Value::as_usize)
        .ok_or_else(|| invalid("focus_tab expects an integer index"))?;
    if args.len() != 1 {
        return Err(invalid("focus_tab expects one integer index"));
    }
    Ok(BuiltinAction::FocusTab(index))
}

fn decode_focus_terminal_pane(args: &[Value]) -> Result<BuiltinAction, Error> {
    decode_focus_pane(args, true)
}

fn decode_focus_plugin_pane(args: &[Value]) -> Result<BuiltinAction, Error> {
    decode_focus_pane(args, false)
}

fn decode_focus_pane(args: &[Value], terminal: bool) -> Result<BuiltinAction, Error> {
    let (positional, kwargs) = from_args::<(&[Value], Kwargs)>(args)?;
    if positional.len() != 1 {
        return Err(invalid("focus pane expects one pane id"));
    }
    let id = positional[0]
        .as_usize()
        .and_then(|id| u32::try_from(id).ok())
        .ok_or_else(|| invalid("focus pane expects a non-negative u32 pane id"))?;
    let should_float_if_hidden = kwargs
        .get::<Option<bool>>("float_if_hidden")?
        .unwrap_or(true);
    let should_be_in_place_if_hidden = kwargs
        .get::<Option<bool>>("in_place_if_hidden")?
        .unwrap_or(false);
    kwargs.assert_all_used()?;
    if terminal {
        Ok(BuiltinAction::FocusTerminalPane {
            id,
            should_float_if_hidden,
            should_be_in_place_if_hidden,
        })
    } else {
        Ok(BuiltinAction::FocusPluginPane {
            id,
            should_float_if_hidden,
            should_be_in_place_if_hidden,
        })
    }
}

fn decode_new_tab(args: &[Value]) -> Result<BuiltinAction, Error> {
    if !args.is_empty() {
        return Err(invalid("new_tab expects no arguments"));
    }
    Ok(BuiltinAction::NewTab)
}

fn decode_run_plugin(args: &[Value]) -> Result<BuiltinAction, Error> {
    let (positional, kwargs) = from_args::<(&[Value], Kwargs)>(args)?;
    if positional.len() != 1 {
        return Err(invalid("run_plugin expects one plugin URL"));
    }
    let url = required_string(&positional[0], "run_plugin expects a non-empty plugin URL")?;
    let x = coordinate_argument(&kwargs, "x", None, true)?;
    let y = coordinate_argument(&kwargs, "y", None, true)?;
    let width = coordinate_argument(&kwargs, "w", Some("50%"), false)?;
    let height = coordinate_argument(&kwargs, "h", Some("50%"), false)?;
    kwargs.assert_all_used()?;
    let coordinates = FloatingPaneCoordinates::new(x, y, width, height, None, None)
        .ok_or_else(|| invalid("invalid floating pane size"))?;
    Ok(BuiltinAction::RunPlugin { url, coordinates })
}

fn decode_send_data_to_plugin(args: &[Value]) -> Result<BuiltinAction, Error> {
    let (positional, kwargs) = from_args::<(&[Value], Kwargs)>(args)?;
    if positional.len() != 2 {
        return Err(invalid(
            "send_data_to_plugin expects plugin URL and message name",
        ));
    }
    let plugin_url = required_string(&positional[0], "send_data_to_plugin expects a plugin URL")?;
    let message_name =
        required_string(&positional[1], "send_data_to_plugin expects a message name")?;
    let message_payload = kwargs.get::<Option<String>>("payload")?;
    let should_float = kwargs.get::<Option<bool>>("float")?;
    let should_focus = kwargs.get::<Option<bool>>("focus")?;
    kwargs.assert_all_used()?;
    Ok(BuiltinAction::SendDataToPlugin(MessageToPlugin {
        plugin_url: Some(plugin_url),
        message_name,
        message_payload,
        new_plugin_args: Some(NewPluginArgs {
            should_float,
            should_focus,
            ..NewPluginArgs::default()
        }),
        ..MessageToPlugin::default()
    }))
}

fn decode_run_shell_cmd(args: &[Value]) -> Result<BuiltinAction, Error> {
    if args.is_empty() {
        return Err(invalid(
            "run_shell_cmd expects at least one command argument",
        ));
    }
    let command = args
        .iter()
        .map(|arg| required_string(arg, "run_shell_cmd expects string arguments"))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(BuiltinAction::RunShellCommand {
        command,
        context: BTreeMap::new(),
    })
}

fn coordinate_argument(
    kwargs: &Kwargs,
    name: &str,
    default: Option<&str>,
    allow_zero: bool,
) -> Result<Option<String>, Error> {
    let value = kwargs.get::<Option<Value>>(name)?;
    let value = match value {
        Some(value) if value.as_str().is_some() => value.as_str().unwrap().to_string(),
        Some(value) if value.as_i64().is_some_and(|value| value >= 0) => {
            value.as_i64().unwrap().to_string()
        },
        Some(_) => return Err(invalid_coordinate(name)),
        None => return Ok(default.map(str::to_string)),
    };
    let number = if let Some(percent) = value.strip_suffix('%') {
        let percent = percent
            .parse::<usize>()
            .map_err(|_| invalid_coordinate(name))?;
        if percent > 100 {
            return Err(invalid_coordinate(name));
        }
        percent
    } else {
        value
            .parse::<usize>()
            .map_err(|_| invalid_coordinate(name))?
    };
    if !allow_zero && number == 0 {
        return Err(invalid_coordinate(name));
    }
    Ok(Some(value))
}

fn required_string(value: &Value, message: &str) -> Result<String, Error> {
    value
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| invalid(message))
}

fn invalid_coordinate(name: &str) -> Error {
    invalid(format!("invalid {name} coordinate"))
}

fn invalid(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::InvalidOperation, message.into())
}
