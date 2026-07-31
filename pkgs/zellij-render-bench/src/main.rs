use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

use serde::Serialize;
use zellij_template_render::{
    context, error_frame, ActionRegistry, ButtonPresentation, ButtonView, Error, ErrorKind, Frame,
    Renderer, Value, Viewport,
};
use zellij_tile::prelude::*;

const TEMPLATE: &str = r#"
{%- call Flex(direction="column", gap=0) -%}
  {%- for row in buttons -%}
    {%- call Flex(direction="row", gap=1, shrink=0) -%}
      {%- for button in row -%}
        {%- call Button(on_click=actions.select(button.id), focused=button.id == selected) -%}
          {{- button.label -}}
        {%- endcall -%}
      {%- endfor -%}
    {%- endcall -%}
  {%- endfor -%}
  {%- call Flex(shrink=0) -%}
    {{- stats -}}
  {%- endcall -%}
{%- endcall -%}
"#;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Action {
    Select(usize),
}

#[derive(Serialize)]
struct ButtonModel {
    id: usize,
    label: String,
}

#[derive(Default)]
struct State {
    selected: usize,
    animated: bool,
    interval: f64,
    samples: VecDeque<Duration>,
    frame: Frame<Action>,
}

register_plugin!(State);

impl State {
    fn renderer() -> Renderer<Action> {
        Renderer::new(ActionRegistry::new().with("select", |args| {
            let id = args
                .first()
                .and_then(Value::as_usize)
                .ok_or_else(|| Error::new(ErrorKind::InvalidOperation, "select expects an id"))?;
            Ok(Action::Select(id))
        }))
    }

    fn buttons() -> Vec<Vec<ButtonModel>> {
        (0..10)
            .map(|row| {
                (0..10)
                    .map(|col| {
                        let id = row * 10 + col;
                        ButtonModel {
                            id,
                            label: format!("B{id:02}"),
                        }
                    })
                    .collect()
            })
            .collect()
    }

    fn stats(&self) -> String {
        if self.samples.is_empty() {
            return "render: waiting".to_string();
        }
        let mut values = self
            .samples
            .iter()
            .map(Duration::as_micros)
            .collect::<Vec<_>>();
        values.sort_unstable();
        let p50 = percentile(&values, 50);
        let p95 = percentile(&values, 95);
        let max = *values.last().unwrap_or(&0);
        format!(
            "render samples={} p50={}us p95={}us max={}us mode={}",
            values.len(),
            p50,
            p95,
            max,
            if self.animated { "animated" } else { "static" }
        )
    }

    fn render_frame(&mut self, rows: usize, cols: usize) -> Frame<Action> {
        let start = Instant::now();
        let result = Self::renderer().render(
            TEMPLATE,
            context! {
                buttons => Value::from_serialize(Self::buttons()),
                selected => self.selected,
                stats => self.stats(),
            },
            Viewport { rows, cols },
            present_button,
        );
        let elapsed = start.elapsed();
        self.samples.push_back(elapsed);
        while self.samples.len() > 240 {
            self.samples.pop_front();
        }
        result.unwrap_or_else(|error| error_frame(&error, Viewport { rows, cols }))
    }
}

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.animated = configuration
            .get("animated")
            .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes"));
        let fps = configuration
            .get("fps")
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(20.0)
            .clamp(1.0, 60.0);
        self.interval = 1.0 / fps;
        subscribe(&[EventType::Mouse, EventType::Timer]);
        if self.animated {
            set_timeout(self.interval);
        }
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::Timer(_) if self.animated => {
                set_timeout(self.interval);
                true
            },
            Event::Mouse(Mouse::LeftClick(row, col)) => {
                let Some(action) = usize::try_from(row)
                    .ok()
                    .and_then(|row| self.frame.hitboxes.get(row))
                    .and_then(|line| line.get(col))
                    .and_then(Clone::clone)
                else {
                    return false;
                };
                match action {
                    Action::Select(id) if self.selected != id => {
                        self.selected = id;
                        true
                    },
                    _ => false,
                }
            },
            _ => false,
        }
    }

    fn render(&mut self, rows: usize, cols: usize) {
        self.frame = self.render_frame(rows, cols);
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

fn present_button(button: ButtonView<'_, Action>) -> Result<ButtonPresentation, Error> {
    let label = if button.focused.unwrap_or(false) {
        format!("[{}]", button.label)
    } else {
        format!(" {} ", button.label)
    };
    Ok(ButtonPresentation {
        label,
        focused: button.focused.unwrap_or(false),
    })
}

fn percentile(values: &[u128], percentile: usize) -> u128 {
    if values.is_empty() {
        return 0;
    }
    let index = ((values.len() - 1) * percentile) / 100;
    values[index]
}
