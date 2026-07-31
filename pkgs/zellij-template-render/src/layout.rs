//! Measures and paints layout trees into viewport-sized text and hitbox frames.

use minijinja::Error;
use unicode_width::UnicodeWidthChar;

use super::template::{Align, Basis, Direction, FlexSpec, Justify, Node, Overflow};
use super::{layout_error, Frame};

#[derive(Clone, Debug)]
pub(super) struct Canvas<A> {
    cells: Vec<Vec<Cell<A>>>,
}

#[derive(Clone, Debug)]
struct Cell<A> {
    text: String,
    action: Option<A>,
}

impl<A> Default for Cell<A> {
    fn default() -> Self {
        Self {
            text: " ".into(),
            action: None,
        }
    }
}

impl<A: Clone> Canvas<A> {
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

    fn blit(
        &mut self,
        child: &Canvas<A>,
        x: usize,
        y: usize,
        clip_width: usize,
        clip_height: usize,
    ) {
        self.blit_from(child, x, y, 0, 0, clip_width, clip_height);
    }

    fn blit_from(
        &mut self,
        child: &Canvas<A>,
        x: usize,
        y: usize,
        source_x: usize,
        source_y: usize,
        clip_width: usize,
        clip_height: usize,
    ) {
        for child_y in 0..clip_height {
            let Some(target_row) = self.cells.get_mut(y + child_y) else {
                break;
            };
            let Some(source_row) = child.cells.get(source_y + child_y) else {
                break;
            };
            for child_x in 0..clip_width {
                let Some(target) = target_row.get_mut(x + child_x) else {
                    break;
                };
                let Some(cell) = source_row.get(source_x + child_x) else {
                    break;
                };
                if !cell.text.is_empty() || cell.action.is_some() {
                    *target = cell.clone();
                }
            }
        }
    }

    pub(super) fn into_frame(self) -> Frame<A> {
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
        Frame {
            lines,
            hitboxes,
            refresh_after: None,
        }
    }
}
pub(super) fn layout<A: Clone>(
    node: &Node<A>,
    width: usize,
    height: usize,
) -> Result<Canvas<A>, Error> {
    match node {
        Node::Text(text) => text_canvas(text, width, height, None),
        Node::Button {
            action,
            focused: _,
            label,
        } => text_canvas(label, width, height, Some(action.clone())),
        Node::Flex { spec, children } => layout_flex(spec, children, width, height),
        Node::OnOverflow { .. } => Err(layout_error("OnOverflow must be a direct child of Flex")),
    }
}

fn natural_size<A>(node: &Node<A>) -> Result<(usize, usize), Error> {
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
            let gaps = spec.gap.saturating_mul(children.len().saturating_sub(1));
            Ok(match spec.direction {
                Direction::Row => (
                    sizes
                        .iter()
                        .map(|s| s.0)
                        .sum::<usize>()
                        .saturating_add(gaps),
                    sizes.iter().map(|s| s.1).max().unwrap_or(1),
                ),
                Direction::Column => (
                    sizes.iter().map(|s| s.0).max().unwrap_or(0),
                    sizes
                        .iter()
                        .map(|s| s.1)
                        .sum::<usize>()
                        .saturating_add(gaps),
                ),
            })
        },
        Node::OnOverflow { children } => {
            let sizes: Vec<_> = children
                .iter()
                .map(natural_size)
                .collect::<Result<_, _>>()?;
            Ok((
                sizes.iter().map(|size| size.0).sum(),
                sizes.iter().map(|size| size.1).max().unwrap_or(1),
            ))
        },
    }
}

fn layout_flex<A: Clone>(
    spec: &FlexSpec,
    children: &[Node<A>],
    width: usize,
    height: usize,
) -> Result<Canvas<A>, Error> {
    let indicator_index = children
        .iter()
        .position(|child| matches!(child, Node::OnOverflow { .. }));
    let Some(indicator_index) = indicator_index else {
        return layout_flex_children(spec, children, width, height);
    };
    if children
        .iter()
        .skip(indicator_index + 1)
        .any(|child| matches!(child, Node::OnOverflow { .. }))
    {
        return Err(layout_error("Flex accepts at most one OnOverflow child"));
    }
    let Node::OnOverflow {
        children: indicator_children,
    } = &children[indicator_index]
    else {
        unreachable!()
    };
    if spec.overflow != Overflow::Scroll {
        return Err(layout_error(
            "OnOverflow parent Flex must use overflow=\"scroll\"",
        ));
    }
    let normal = children
        .iter()
        .enumerate()
        .filter_map(|(index, child)| (index != indicator_index).then(|| child.clone()))
        .collect::<Vec<_>>();
    let main_available = if spec.direction == Direction::Row {
        width
    } else {
        height
    };
    let natural_main = normal
        .iter()
        .map(|child| {
            let natural = natural_size(child)?;
            Ok(match child {
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
        })
        .collect::<Result<Vec<usize>, Error>>()?
        .into_iter()
        .sum::<usize>()
        .saturating_add(spec.gap.saturating_mul(normal.len().saturating_sub(1)));
    if natural_main <= main_available {
        return layout_flex_children(spec, &normal, width, height);
    }

    let indicator_natural = natural_size(&Node::OnOverflow {
        children: indicator_children.clone(),
    })?;
    let indicator_main = if spec.direction == Direction::Row {
        indicator_natural.0
    } else {
        indicator_natural.1
    }
    .min(main_available);
    let indicator_gap = spec.gap.min(main_available.saturating_sub(indicator_main));
    let content_main = main_available.saturating_sub(indicator_main + indicator_gap);
    let (content_width, content_height) = if spec.direction == Direction::Row {
        (content_main, height)
    } else {
        (width, content_main)
    };
    let content = layout_flex_children(spec, &normal, content_width, content_height)?;
    let indicator_spec = FlexSpec::default();
    let (indicator_width, indicator_height) = if spec.direction == Direction::Row {
        (indicator_main, height)
    } else {
        (width, indicator_main)
    };
    let indicator_canvas = layout_flex_children(
        &indicator_spec,
        indicator_children,
        indicator_width,
        indicator_height,
    )?;
    let mut canvas = Canvas::new(width, height);
    canvas.blit(&content, 0, 0, content.width(), content.height());
    if spec.direction == Direction::Row {
        canvas.blit(
            &indicator_canvas,
            content_main + indicator_gap,
            0,
            indicator_width,
            indicator_height,
        );
    } else {
        canvas.blit(
            &indicator_canvas,
            0,
            content_main + indicator_gap,
            indicator_width,
            indicator_height,
        );
    }
    Ok(canvas)
}

fn layout_flex_children<A: Clone>(
    spec: &FlexSpec,
    children: &[Node<A>],
    width: usize,
    height: usize,
) -> Result<Canvas<A>, Error> {
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
    let fixed_gaps = spec.gap.saturating_mul(children.len().saturating_sub(1));
    let child_available = main_available.saturating_sub(fixed_gaps);
    let total: usize = sizes.iter().sum();
    if total < child_available {
        distribute(&mut sizes, children, child_available - total, true);
    } else if total > child_available && spec.overflow == Overflow::Normal {
        distribute(&mut sizes, children, total - child_available, false);
    }
    let content_size = sizes.iter().sum::<usize>().saturating_add(fixed_gaps);
    let offset = if spec.overflow == Overflow::Scroll && content_size > main_available {
        focused_offset(children, &sizes, spec.gap, main_available)
    } else {
        0
    };
    let free = main_available.saturating_sub(content_size);
    let (mut cursor, distributed_gap) = justify(spec.justify, free, children.len());
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
        if cursor.saturating_add(main) > offset && visible_cursor < main_available {
            let skip = offset.saturating_sub(cursor);
            if spec.direction == Direction::Row {
                canvas.blit_from(
                    &child_canvas,
                    visible_cursor,
                    cross,
                    skip,
                    0,
                    main.saturating_sub(skip)
                        .min(main_available - visible_cursor),
                    child_height,
                );
            } else {
                canvas.blit_from(
                    &child_canvas,
                    cross,
                    visible_cursor,
                    0,
                    skip,
                    child_width,
                    main.saturating_sub(skip)
                        .min(main_available - visible_cursor),
                );
            }
        }
        cursor = cursor
            .saturating_add(main)
            .saturating_add(spec.gap)
            .saturating_add(distributed_gap);
    }
    Ok(canvas)
}

fn distribute<A>(sizes: &mut [usize], children: &[Node<A>], mut amount: usize, grow: bool) {
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

fn focused_offset<A>(children: &[Node<A>], sizes: &[usize], gap: usize, viewport: usize) -> usize {
    let mut start = 0usize;
    for (child, size) in children.iter().zip(sizes) {
        if contains_focus(child) {
            return start.saturating_add(*size).saturating_sub(viewport);
        }
        start = start.saturating_add(*size).saturating_add(gap);
    }
    0
}

fn contains_focus<A>(node: &Node<A>) -> bool {
    match node {
        Node::Button { focused, .. } => *focused,
        Node::Flex { children, .. } | Node::OnOverflow { children } => {
            children.iter().any(contains_focus)
        },
        Node::Text(_) => false,
    }
}

fn justify(justify: Justify, free: usize, count: usize) -> (usize, usize) {
    match justify {
        Justify::Start => (0, 0),
        Justify::Center => (free / 2, 0),
        Justify::End => (free, 0),
        Justify::SpaceBetween if count > 1 => (0, free / (count - 1)),
        Justify::SpaceAround if count > 0 => {
            let gap = free / count;
            (gap / 2, gap)
        },
        _ => (0, 0),
    }
}

fn text_canvas<A: Clone>(
    text: &str,
    width: usize,
    height: usize,
    action: Option<A>,
) -> Result<Canvas<A>, Error> {
    let lines = split_text_lines(text)?;
    let mut canvas = Canvas::new(width, height);
    for (y, line) in lines.iter().take(height).enumerate() {
        if line.bytes().all(|byte| (0x20..0x7f).contains(&byte)) {
            for (x, byte) in line.bytes().take(width).enumerate() {
                canvas.cells[y][x] = Cell {
                    text: char::from(byte).to_string(),
                    action: action.clone(),
                };
            }
            continue;
        }
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
    let mut width = 0;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            let _ = consume_ansi(&mut chars, &mut String::new());
        } else {
            width += UnicodeWidthChar::width(ch).unwrap_or(0);
        }
    }
    width
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

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum TestAction {
        Switch(usize),
        New,
    }

    fn plain_button(label: &str, action: TestAction, focused: bool) -> Node<TestAction> {
        Node::Button {
            action,
            focused,
            label: label.into(),
        }
    }

    #[test]
    fn scroll_keeps_focused_button_visible() {
        let node = Node::Flex {
            spec: FlexSpec {
                gap: 2,
                overflow: Overflow::Scroll,
                ..FlexSpec::default()
            },
            children: vec![
                plain_button("one", TestAction::Switch(1), false),
                plain_button("two", TestAction::Switch(2), true),
            ],
        };
        let frame = layout(&node, 3, 1).unwrap().into_frame();
        assert_eq!(frame.lines[0], "two");
    }

    #[test]
    fn on_overflow_is_hidden_when_content_fits() {
        let node: Node<TestAction> = Node::Flex {
            spec: FlexSpec {
                overflow: Overflow::Scroll,
                ..FlexSpec::default()
            },
            children: vec![
                Node::Text("tabs".into()),
                Node::OnOverflow {
                    children: vec![Node::Text("v".into())],
                },
            ],
        };
        assert_eq!(layout(&node, 5, 1).unwrap().into_frame().lines[0], "tabs");
    }

    #[test]
    fn on_overflow_is_fixed_and_clickable_when_content_overflows() {
        let node = Node::Flex {
            spec: FlexSpec {
                overflow: Overflow::Scroll,
                ..FlexSpec::default()
            },
            children: vec![
                Node::Text("abcdef".into()),
                Node::OnOverflow {
                    children: vec![plain_button("v", TestAction::New, false)],
                },
            ],
        };
        let frame = layout(&node, 5, 1).unwrap().into_frame();
        assert_eq!(frame.lines[0], "abcdv");
        assert_eq!(frame.hitboxes[0][4], Some(TestAction::New));
    }

    #[test]
    fn click_hitboxes_are_two_dimensional() {
        let node = Node::Flex {
            spec: FlexSpec {
                direction: Direction::Column,
                ..FlexSpec::default()
            },
            children: vec![
                plain_button("a", TestAction::Switch(1), false),
                plain_button("+", TestAction::New, false),
            ],
        };
        let frame = layout(&node, 2, 2).unwrap().into_frame();
        assert_eq!(frame.hitboxes[0][0], Some(TestAction::Switch(1)));
        assert_eq!(frame.hitboxes[1][0], Some(TestAction::New));
    }

    #[test]
    fn clipped_ansi_text_keeps_each_visible_cell_styled() {
        let canvas: Canvas<TestAction> = text_canvas("\u{1b}[31mabc\u{1b}[0m", 3, 1, None).unwrap();
        let mut clipped = Canvas::new(1, 1);
        clipped.blit_from(&canvas, 0, 0, 1, 0, 1, 1);
        let clipped = clipped.into_frame();
        assert!(clipped.lines[0].starts_with("\u{1b}[31m"));
        assert!(clipped.lines[0].ends_with("\u{1b}[0m"));
        assert!(clipped.lines[0].contains('b'));
    }
}
