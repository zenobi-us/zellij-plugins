//! Measures and paints layout trees into viewport-sized text and hitbox frames.

use minijinja::Error;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::template::{Align, Basis, Direction, FlexSpec, Justify, Node, Overflow};
use super::{layout_error, ClickAction, RenderedFrame};

#[derive(Clone, Debug, Default)]
pub(super) struct Canvas {
    cells: Vec<Vec<Cell>>,
}

#[derive(Clone, Debug, Default)]
struct Cell {
    text: String,
    action: Option<ClickAction>,
}

impl Canvas {
    fn new(width: usize, height: usize) -> Self {
        Self {
            cells: vec![vec![Cell::default(); width]; height],
        }
    }

    fn width(&self) -> usize {
        self.cells.first().map_or(0, Vec::len)
    }

    fn height(&self) -> usize {
        self.cells.len()
    }

    fn blit(&mut self, child: &Canvas, x: usize, y: usize, clip_width: usize, clip_height: usize) {
        for (child_y, row) in child.cells.iter().enumerate().take(clip_height) {
            let Some(target_row) = self.cells.get_mut(y + child_y) else {
                break;
            };
            for (child_x, cell) in row.iter().enumerate().take(clip_width) {
                let Some(target) = target_row.get_mut(x + child_x) else {
                    break;
                };
                if !cell.text.is_empty() || cell.action.is_some() {
                    *target = cell.clone();
                }
            }
        }
    }

    pub(super) fn into_frame(self) -> RenderedFrame {
        let mut lines = Vec::with_capacity(self.height());
        let mut hitboxes = Vec::with_capacity(self.height());
        for row in self.cells {
            let mut line = String::new();
            let mut actions = Vec::with_capacity(row.len());
            for cell in row {
                line.push_str(&cell.text);
                actions.push(cell.action);
            }
            lines.push(line.trim_end_matches(' ').to_string());
            hitboxes.push(actions);
        }
        RenderedFrame { lines, hitboxes }
    }
}
pub(super) fn layout(node: &Node, width: usize, height: usize) -> Result<Canvas, Error> {
    match node {
        Node::Text(text) => text_canvas(text, width, height, None),
        Node::Button {
            action,
            focused: _,
            label,
        } => text_canvas(label, width, height, Some(action.clone())),
        Node::Flex { spec, children } => layout_flex(spec, children, width, height),
    }
}

fn natural_size(node: &Node) -> Result<(usize, usize), Error> {
    match node {
        Node::Text(text) | Node::Button { label: text, .. } => {
            let lines = split_text_lines(text)?;
            Ok((
                lines
                    .iter()
                    .map(|line| visible_width(line))
                    .max()
                    .unwrap_or(0),
                lines.len().max(1),
            ))
        },
        Node::Flex { spec, children } => {
            let sizes: Vec<_> = children
                .iter()
                .map(natural_size)
                .collect::<Result<_, _>>()?;
            Ok(match spec.direction {
                Direction::Row => (
                    sizes.iter().map(|s| s.0).sum(),
                    sizes.iter().map(|s| s.1).max().unwrap_or(1),
                ),
                Direction::Column => (
                    sizes.iter().map(|s| s.0).max().unwrap_or(0),
                    sizes.iter().map(|s| s.1).sum(),
                ),
            })
        },
    }
}

fn layout_flex(
    spec: &FlexSpec,
    children: &[Node],
    width: usize,
    height: usize,
) -> Result<Canvas, Error> {
    let main_available = if spec.direction == Direction::Row {
        width
    } else {
        height
    };
    let cross_available = if spec.direction == Direction::Row {
        height
    } else {
        width
    };
    let naturals: Vec<_> = children
        .iter()
        .map(natural_size)
        .collect::<Result<_, _>>()?;
    let mut sizes: Vec<usize> = children
        .iter()
        .zip(&naturals)
        .map(|(node, natural)| match node {
            Node::Flex {
                spec: child_spec, ..
            } => match child_spec.basis {
                Basis::Auto => {
                    if spec.direction == Direction::Row {
                        natural.0
                    } else {
                        natural.1
                    }
                },
                Basis::Cells(value) => value,
            },
            _ => {
                if spec.direction == Direction::Row {
                    natural.0
                } else {
                    natural.1
                }
            },
        })
        .collect();
    let total: usize = sizes.iter().sum();
    if total < main_available {
        distribute(&mut sizes, children, main_available - total, true);
    } else if total > main_available && spec.overflow == Overflow::Normal {
        distribute(&mut sizes, children, total - main_available, false);
    }
    let content_size: usize = sizes.iter().sum();
    let offset = if spec.overflow == Overflow::Scroll && content_size > main_available {
        focused_offset(children, &sizes, main_available)
    } else {
        0
    };
    let free = main_available.saturating_sub(content_size);
    let (mut cursor, gap, around) = justify(spec.justify, free, children.len());
    let mut canvas = Canvas::new(width, height);
    for ((child, natural), main) in children.iter().zip(naturals).zip(sizes) {
        let natural_cross = if spec.direction == Direction::Row {
            natural.1
        } else {
            natural.0
        };
        let child_cross = if spec.align == Align::Stretch {
            cross_available
        } else {
            natural_cross.min(cross_available)
        };
        let cross = match spec.align {
            Align::Start | Align::Stretch => 0,
            Align::Center => cross_available.saturating_sub(child_cross) / 2,
            Align::End => cross_available.saturating_sub(child_cross),
        };
        let child_width = if spec.direction == Direction::Row {
            main
        } else {
            child_cross
        };
        let child_height = if spec.direction == Direction::Row {
            child_cross
        } else {
            main
        };
        let child_canvas = layout(child, child_width, child_height)?;
        let visible_cursor = cursor.saturating_sub(offset);
        if cursor + main > offset && visible_cursor < main_available {
            let skip = offset.saturating_sub(cursor);
            if spec.direction == Direction::Row {
                let clipped = crop(
                    &child_canvas,
                    skip,
                    0,
                    main.saturating_sub(skip)
                        .min(main_available - visible_cursor),
                    child_height,
                );
                canvas.blit(
                    &clipped,
                    visible_cursor,
                    cross,
                    clipped.width(),
                    clipped.height(),
                );
            } else {
                let clipped = crop(
                    &child_canvas,
                    0,
                    skip,
                    child_width,
                    main.saturating_sub(skip)
                        .min(main_available - visible_cursor),
                );
                canvas.blit(
                    &clipped,
                    cross,
                    visible_cursor,
                    clipped.width(),
                    clipped.height(),
                );
            }
        }
        cursor += main + gap;
        if around {
            cursor += gap;
        }
    }
    Ok(canvas)
}

fn distribute(sizes: &mut [usize], children: &[Node], mut amount: usize, grow: bool) {
    while amount > 0 {
        let mut changed = false;
        for (size, child) in sizes.iter_mut().zip(children) {
            let weight = match child {
                Node::Flex { spec, .. } => {
                    if grow {
                        spec.grow
                    } else {
                        spec.shrink
                    }
                },
                _ => usize::from(!grow),
            };
            for _ in 0..weight {
                if amount == 0 || (!grow && *size == 0) {
                    break;
                }
                *size = if grow {
                    *size + 1
                } else {
                    size.saturating_sub(1)
                };
                amount -= 1;
                changed = true;
            }
            if amount == 0 {
                break;
            }
        }
        if !changed {
            break;
        }
    }
}

fn focused_offset(children: &[Node], sizes: &[usize], viewport: usize) -> usize {
    let mut start = 0;
    for (child, size) in children.iter().zip(sizes) {
        if contains_focus(child) {
            return (start + size).saturating_sub(viewport);
        }
        start += size;
    }
    0
}

fn contains_focus(node: &Node) -> bool {
    match node {
        Node::Button { focused, .. } => *focused,
        Node::Flex { children, .. } => children.iter().any(contains_focus),
        Node::Text(_) => false,
    }
}

fn justify(justify: Justify, free: usize, count: usize) -> (usize, usize, bool) {
    match justify {
        Justify::Start => (0, 0, false),
        Justify::Center => (free / 2, 0, false),
        Justify::End => (free, 0, false),
        Justify::SpaceBetween if count > 1 => (0, free / (count - 1), false),
        Justify::SpaceAround if count > 0 => {
            let gap = free / count;
            (gap / 2, gap, false)
        },
        _ => (0, 0, false),
    }
}

fn crop(canvas: &Canvas, x: usize, y: usize, width: usize, height: usize) -> Canvas {
    let mut result = Canvas::new(width, height);
    for row in 0..height {
        for col in 0..width {
            if let Some(cell) = canvas.cells.get(y + row).and_then(|r| r.get(x + col)) {
                result.cells[row][col] = cell.clone();
            }
        }
    }
    result
}

fn text_canvas(
    text: &str,
    width: usize,
    height: usize,
    action: Option<ClickAction>,
) -> Result<Canvas, Error> {
    let lines = split_text_lines(text)?;
    let mut canvas = Canvas::new(width, height);
    for (y, line) in lines.iter().take(height).enumerate() {
        let mut x = 0;
        let mut active_sgr = String::new();
        let mut pending = String::new();
        let mut chars = line.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\u{1b}' {
                let mut sequence = String::from("\u{1b}");
                consume_ansi(&mut chars, &mut sequence)?;
                if sequence.starts_with("\u{1b}[") && sequence.ends_with('m') {
                    if sequence == "\u{1b}[0m" {
                        active_sgr.clear();
                    } else {
                        active_sgr.push_str(&sequence);
                    }
                } else {
                    pending.push_str(&sequence);
                }
                continue;
            }
            let cell_width = UnicodeWidthChar::width(ch).unwrap_or(0);
            if cell_width == 0 {
                pending.push(ch);
                continue;
            }
            if x + cell_width > width {
                break;
            }
            pending.push_str(&active_sgr);
            pending.push(ch);
            if !active_sgr.is_empty() {
                pending.push_str("\u{1b}[0m");
            }
            canvas.cells[y][x] = Cell {
                text: std::mem::take(&mut pending),
                action: action.clone(),
            };
            for continuation in 1..cell_width {
                canvas.cells[y][x + continuation].action = action.clone();
            }
            x += cell_width;
        }
        if !pending.is_empty() && x > 0 {
            canvas.cells[y][x - 1].text.push_str(&pending);
        }
    }
    Ok(canvas)
}

fn split_text_lines(text: &str) -> Result<Vec<&str>, Error> {
    if text.contains('\r') || text.contains('\t') {
        return Err(layout_error(
            "template text cannot contain tabs or carriage returns",
        ));
    }
    Ok(text.split('\n').collect())
}

fn visible_width(text: &str) -> usize {
    let mut plain = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            let _ = consume_ansi(&mut chars, &mut String::new());
        } else {
            plain.push(ch);
        }
    }
    UnicodeWidthStr::width(plain.as_str())
}

pub(super) fn consume_ansi(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    output: &mut String,
) -> Result<(), Error> {
    let Some(kind) = chars.next() else {
        return Err(layout_error("truncated ANSI escape"));
    };
    output.push(kind);
    match kind {
        '[' => loop {
            let Some(ch) = chars.next() else {
                return Err(layout_error("truncated ANSI CSI sequence"));
            };
            output.push(ch);
            if ('@'..='~').contains(&ch) {
                break;
            }
        },
        ']' => loop {
            let Some(ch) = chars.next() else {
                return Err(layout_error("truncated ANSI OSC sequence"));
            };
            output.push(ch);
            if ch == '\u{7}' {
                break;
            }
            if ch == '\u{1b}' && chars.peek() == Some(&'\\') {
                output.push(chars.next().unwrap());
                break;
            }
        },
        _ => {},
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain_button(label: &str, action: ClickAction, focused: bool) -> Node {
        Node::Button {
            action,
            focused,
            label: label.into(),
        }
    }

    #[test]
    fn flex_grow_allocates_remaining_cells() {
        let node = Node::Flex {
            spec: FlexSpec::default(),
            children: vec![
                Node::Text("a".into()),
                Node::Flex {
                    spec: FlexSpec {
                        grow: 1,
                        ..FlexSpec::default()
                    },
                    children: vec![Node::Text("b".into())],
                },
            ],
        };
        let canvas = layout(&node, 5, 1).unwrap();
        assert_eq!(canvas.width(), 5);
        assert_eq!(canvas.cells[0][0].text, "a");
        assert_eq!(canvas.cells[0][1].text, "b");
    }

    #[test]
    fn scroll_keeps_focused_button_visible() {
        let node = Node::Flex {
            spec: FlexSpec {
                overflow: Overflow::Scroll,
                ..FlexSpec::default()
            },
            children: vec![
                plain_button("one", ClickAction::SwitchTab(1), false),
                plain_button("two", ClickAction::SwitchTab(2), true),
            ],
        };
        let frame = layout(&node, 3, 1).unwrap().into_frame();
        assert_eq!(frame.lines[0], "two");
    }

    #[test]
    fn click_hitboxes_are_two_dimensional() {
        let node = Node::Flex {
            spec: FlexSpec {
                direction: Direction::Column,
                ..FlexSpec::default()
            },
            children: vec![
                plain_button("a", ClickAction::SwitchTab(1), false),
                plain_button("+", ClickAction::NewTab, false),
            ],
        };
        let frame = layout(&node, 2, 2).unwrap().into_frame();
        assert_eq!(frame.hitboxes[0][0], Some(ClickAction::SwitchTab(1)));
        assert_eq!(frame.hitboxes[1][0], Some(ClickAction::NewTab));
    }

    #[test]
    fn clipped_ansi_text_keeps_each_visible_cell_styled() {
        let canvas = text_canvas("\u{1b}[31mabc\u{1b}[0m", 3, 1, None).unwrap();
        let clipped = crop(&canvas, 1, 0, 1, 1).into_frame();
        assert!(clipped.lines[0].starts_with("\u{1b}[31m"));
        assert!(clipped.lines[0].ends_with("\u{1b}[0m"));
        assert!(clipped.lines[0].contains('b'));
    }
}
