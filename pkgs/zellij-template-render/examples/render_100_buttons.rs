use std::env;
use std::time::{Duration, Instant};

use serde::Serialize;
use zellij_template_render::{
    context, ActionRegistry, ButtonPresentation, ButtonView, Error, ErrorKind, Renderer, Value,
    Viewport,
};

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
{%- endcall -%}
"#;

#[derive(Clone)]
enum Action {
    Select(usize),
}

#[derive(Serialize)]
struct ButtonModel {
    id: usize,
    label: String,
}

fn main() -> Result<(), Error> {
    let iterations = env::args()
        .nth(1)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(1_000);
    let renderer = Renderer::new(ActionRegistry::new().with("select", |args| {
        let id = args
            .first()
            .and_then(Value::as_usize)
            .ok_or_else(|| Error::new(ErrorKind::InvalidOperation, "select expects an id"))?;
        Ok(Action::Select(id))
    }));
    let buttons = buttons();
    let viewport = Viewport { rows: 11, cols: 90 };

    for _ in 0..50 {
        render_once(&renderer, &buttons, viewport)?;
    }

    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        render_once(&renderer, &buttons, viewport)?;
        samples.push(start.elapsed());
    }
    samples.sort_unstable();

    println!("iterations={iterations}");
    println!("p50={}us", percentile(&samples, 50).as_micros());
    println!("p95={}us", percentile(&samples, 95).as_micros());
    println!("p99={}us", percentile(&samples, 99).as_micros());
    println!(
        "max={}us",
        samples.last().unwrap_or(&Duration::ZERO).as_micros()
    );
    Ok(())
}

fn render_once(
    renderer: &Renderer<Action>,
    buttons: &[Vec<ButtonModel>],
    viewport: Viewport,
) -> Result<(), Error> {
    let frame = renderer.render(
        TEMPLATE,
        context! {
            buttons => Value::from_serialize(buttons),
            selected => 0usize,
        },
        viewport,
        present_button,
    )?;
    assert!(frame.lines.len() >= 10);
    assert!(frame.hitboxes.len() >= 10);
    Ok(())
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

fn present_button(button: ButtonView<'_, Action>) -> Result<ButtonPresentation, Error> {
    let Action::Select(id) = button.action;
    let selected = button.focused.unwrap_or(false);
    Ok(ButtonPresentation {
        label: if selected {
            format!("[{id:02}]")
        } else {
            format!(" {id:02} ")
        },
        focused: selected,
    })
}

fn percentile(samples: &[Duration], percentile: usize) -> Duration {
    if samples.is_empty() {
        return Duration::ZERO;
    }
    let index = ((samples.len() - 1) * percentile) / 100;
    samples[index]
}
