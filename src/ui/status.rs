use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use super::text::display_width_u16;
use super::widgets::panel_contrast_fg;
use crate::{
    app::state::{CopyFeedback, Palette, ToastKind, ToastNotification},
    config::{StatusIndicatorStyle, ToastClipboardPosition, ToastHerdrPosition},
    detect::AgentState,
};

pub(crate) fn copy_feedback_rect(
    area: Rect,
    feedback: &CopyFeedback,
    offset_rows: u16,
    position: ToastClipboardPosition,
) -> Rect {
    if area.width == 0 || area.height == 0 {
        return Rect::default();
    }

    let content_width = feedback.message.len() as u16 + 4;
    let width = content_width.min(area.width);
    let height = 3u16.min(area.height);
    let x = match position {
        ToastClipboardPosition::TopLeft | ToastClipboardPosition::BottomLeft => area.x,
        ToastClipboardPosition::TopCenter | ToastClipboardPosition::BottomCenter => {
            area.x + area.width.saturating_sub(width) / 2
        }
        ToastClipboardPosition::TopRight | ToastClipboardPosition::BottomRight => {
            area.x + area.width.saturating_sub(width)
        }
    };
    let y = match position {
        ToastClipboardPosition::TopLeft
        | ToastClipboardPosition::TopCenter
        | ToastClipboardPosition::TopRight => area.y + offset_rows.min(area.height),
        ToastClipboardPosition::BottomLeft
        | ToastClipboardPosition::BottomCenter
        | ToastClipboardPosition::BottomRight => {
            area.y + area.height.saturating_sub(height + offset_rows)
        }
    };
    Rect::new(x, y, width, height)
}

pub(crate) fn toast_notification_rect(
    area: Rect,
    toast: &ToastNotification,
    offset_for_warning: bool,
    position: ToastHerdrPosition,
) -> Rect {
    let content_width = display_width_u16(&toast.title)
        .max(display_width_u16(&toast.context))
        .saturating_add(4);
    let width = content_width.saturating_add(2).min(area.width);
    let content_height = if toast.context.is_empty() { 1 } else { 2 };
    let height = (content_height + 2).min(area.height);
    let x = match position {
        ToastHerdrPosition::TopLeft | ToastHerdrPosition::BottomLeft => area.x,
        ToastHerdrPosition::TopRight | ToastHerdrPosition::BottomRight => {
            area.x + area.width.saturating_sub(width)
        }
    };
    let warning_offset = u16::from(offset_for_warning);
    let y = match position {
        ToastHerdrPosition::TopLeft | ToastHerdrPosition::TopRight => {
            area.y + warning_offset.min(area.height)
        }
        ToastHerdrPosition::BottomLeft | ToastHerdrPosition::BottomRight => {
            area.y + area.height.saturating_sub(height + warning_offset)
        }
    };
    Rect::new(x, y, width, height)
}

pub(super) fn render_toast_notification(
    frame: &mut Frame,
    area: Rect,
    toast: &ToastNotification,
    offset_for_warning: bool,
    position: ToastHerdrPosition,
    p: &Palette,
) {
    let dot_color = match toast.kind {
        ToastKind::NeedsAttention => p.red,
        ToastKind::Finished => p.blue,
        ToastKind::UpdateInstalled => p.accent,
    };
    let toast_area = toast_notification_rect(area, toast, offset_for_warning, position);

    frame.render_widget(Clear, toast_area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(p.overlay0))
        .style(Style::default().bg(p.panel_bg));
    let inner = block.inner(toast_area);
    frame.render_widget(block, toast_area);

    if inner.height < 1 {
        return;
    }

    let [title_row, context_row] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(inner);

    let title = Line::from(vec![
        Span::styled("●", Style::default().fg(dot_color)),
        Span::raw(" "),
        Span::styled(
            &toast.title,
            Style::default().fg(p.text).add_modifier(Modifier::BOLD),
        ),
    ]);
    let context = Line::from(vec![
        Span::styled("  ", Style::default().fg(p.overlay0)),
        Span::styled(&toast.context, Style::default().fg(p.overlay0)),
    ]);

    frame.render_widget(Paragraph::new(title), title_row);
    if !toast.context.is_empty() && inner.height >= 2 {
        frame.render_widget(Paragraph::new(context), context_row);
    }
}

pub(super) fn render_copy_feedback(
    frame: &mut Frame,
    area: Rect,
    feedback: &CopyFeedback,
    offset_rows: u16,
    position: ToastClipboardPosition,
    p: &Palette,
) {
    let feedback_area = copy_feedback_rect(area, feedback, offset_rows, position);
    if feedback_area.is_empty() {
        return;
    }

    frame.render_widget(Clear, feedback_area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(p.green))
        .style(Style::default().bg(p.panel_bg));
    let inner = block.inner(feedback_area);
    frame.render_widget(block, feedback_area);

    if inner.height == 0 {
        return;
    }

    let text = Line::from(vec![
        Span::styled("●", Style::default().fg(p.green).bg(p.panel_bg)),
        Span::raw(" "),
        Span::styled(
            &feedback.message,
            Style::default()
                .fg(p.text)
                .bg(p.panel_bg)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(Paragraph::new(text), inner);
}

pub(super) fn render_config_diagnostic(frame: &mut Frame, area: Rect, message: &str, p: &Palette) {
    let style = Style::default()
        .fg(panel_contrast_fg(p))
        .bg(p.yellow)
        .add_modifier(Modifier::BOLD);

    for (row, line) in message
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(area.height as usize)
        .enumerate()
    {
        let text = format!(" {line} ");
        let width = (text.len() as u16).min(area.width);
        let notif_area = Rect::new(
            area.x + area.width.saturating_sub(width),
            area.y + row as u16,
            width,
            1,
        );

        frame.render_widget(Clear, notif_area);
        frame.render_widget(Paragraph::new(Span::styled(text, style)), notif_area);
    }
}


/// Two stacked braille cells are a 2x8 dot grid; a short comet walking its
/// perimeter turns without ever leaving the one column the indicator owns. Every
/// other state fills both cells, so the column is the same width in every state
/// and nothing beside it shifts as an agent starts or stops.
const RING_LEFT: [u8; 4] = [0x01, 0x02, 0x04, 0x40];
const RING_RIGHT: [u8; 4] = [0x08, 0x10, 0x20, 0x80];
const RING_STEPS: u64 = 18;

/// Which columns the comet lights at each step, and how far down it is. The two
/// steps that light both columns are the top and bottom of the loop: without them
/// the comet turns around in place and the ends read as a stall.
fn ring_step(step: u64) -> (bool, bool, usize) {
    match step {
        0..=7 => (true, false, step as usize),
        8 => (true, true, 7),
        9..=16 => (false, true, (16 - step) as usize),
        _ => (true, true, 0),
    }
}

fn ring_cells(frame: u64) -> [String; 2] {
    let mut masks = [0u8; 2];
    for tail in 0..RING_COMET {
        let step = (frame + RING_STEPS - tail) % RING_STEPS;
        let (left, right, pos) = ring_step(step);
        if left {
            masks[pos / 4] |= RING_LEFT[pos % 4];
        }
        if right {
            masks[pos / 4] |= RING_RIGHT[pos % 4];
        }
    }
    masks.map(|mask| {
        char::from_u32(0x2800 + u32::from(mask))
            .unwrap_or(' ')
            .to_string()
    })
}

/// The two cells an agent row's indicator owns, top first.
pub(super) fn agent_state_cells(
    state: AgentState,
    seen: bool,
    frame: u64,
    p: &Palette,
) -> ([String; 2], Style) {
    let (glyph, style) = state_dot(state, seen, p);
        ["\u{2502}".to_string(), "\u{2502}".to_string()]
    } else {
        // box-drawing verticals join across the row boundary into one unbroken line,
        // and carry far less weight than a filled block for the same column
        ["\u{2503}".to_string(), "\u{2503}".to_string()]
    };
    (cells, style)
}

/// The indicator for an agent row: turning while it works, a static dot otherwise.
pub(super) fn agent_state_icon(
    state: AgentState,
    seen: bool,
    frame: u64,
    p: &Palette,
) -> (&'static str, Style) {
    let (glyph, style) = state_dot(state, seen, p);
    if matches!(state, AgentState::Working) {
        return (SPINNER[(frame % SPINNER.len() as u64) as usize], style);
    }
    (glyph, style)
}

    const STEPS: u8 = 8;

    };
    for step in 0..=STEPS {
            return candidate;
        }
    }
}

        // the agents themselves signal work in this tier, and a row that disagrees
        // with the pane it points at reads as two different things happening
        // one axis: colour means the row wants you, grey means it is done wanting you.
        // `seen` is false when the agent finished while you were looking elsewhere,
        // which is the completion you still have to read.
        // a dash reads as "no agent here" at a glance; a middle dot disappears
pub(super) fn state_icon_symbol(
    state: AgentState,
    seen: bool,
    indicator_style: StatusIndicatorStyle,
) -> &'static str {
    match (indicator_style, state, seen) {
        (StatusIndicatorStyle::Dots, AgentState::Blocked, _) => "●",
        (StatusIndicatorStyle::Dots, AgentState::Working, _) => "●",
        (StatusIndicatorStyle::Dots, AgentState::Idle, false) => "●",
        (StatusIndicatorStyle::Dots, AgentState::Idle, true) => "○",
        (StatusIndicatorStyle::Dots, AgentState::Unknown, _) => "·",
        (StatusIndicatorStyle::Symbols, AgentState::Blocked, _) => "×",
        (StatusIndicatorStyle::Symbols, AgentState::Working, _) => "◐",
        (StatusIndicatorStyle::Symbols, AgentState::Idle, false) => "✓",
        (StatusIndicatorStyle::Symbols, AgentState::Idle, true) => "○",
        (StatusIndicatorStyle::Symbols, AgentState::Unknown, _) => "·",
    }
}

pub(super) fn state_icon(
    state: AgentState,
    seen: bool,
    indicator_style: StatusIndicatorStyle,
    p: &Palette,
) -> (&'static str, Style) {
    (
        state_icon_symbol(state, seen, indicator_style),
        Style::default().fg(state_label_color(state, seen, p)),
    )
}

pub(super) fn state_label(state: AgentState, seen: bool) -> &'static str {
    match (state, seen) {
        (AgentState::Blocked, _) => "blocked",
        (AgentState::Working, _) => "working",
        (AgentState::Idle, false) => "done",
        (AgentState::Idle, true) => "idle",
        (AgentState::Unknown, _) => "idle",
    }
}

pub(super) fn state_label_color(state: AgentState, seen: bool, p: &Palette) -> Color {
    match (state, seen) {
        (AgentState::Blocked, _) => p.red,
        (AgentState::Working, _) => p.yellow,
        (AgentState::Idle, false) => p.teal,
        (AgentState::Idle, true) => p.green,
        (AgentState::Unknown, _) => p.overlay0,
    }
}

/// Every palette the sidebar can draw with, so a contrast rule covers all of them
/// rather than only the one the author happened to be running.
#[cfg(test)]
pub(crate) const THEME_NAMES: [&str; 18] = [
    "catppuccin",
    "catppuccin-latte",
    "terminal",
    "tokyo-night",
    "tokyo-night-day",
    "dracula",
    "nord",
    "gruvbox",
    "gruvbox-light",
    "one-dark",
    "one-light",
    "solarized",
    "solarized-light",
    "kanagawa",
    "kanagawa-lotus",
    "rose-pine",
    "rose-pine-dawn",
    "vesper",
];

/// ANSI slots are whatever the terminal decided, so only their conventional values
/// are knowable here. `Reset` is the terminal's own colour and has none at all.
pub(crate) fn resolve_rgb(color: Color) -> Option<(u8, u8, u8)> {
    match color {
        Color::Rgb(r, g, b) => Some((r, g, b)),
        Color::Black => Some((0, 0, 0)),
        Color::Red => Some((128, 0, 0)),
        Color::Green => Some((0, 128, 0)),
        Color::Yellow => Some((128, 128, 0)),
        Color::Blue => Some((0, 0, 128)),
        Color::Magenta => Some((128, 0, 128)),
        Color::Cyan => Some((0, 128, 128)),
        Color::Gray => Some((192, 192, 192)),
        Color::DarkGray => Some((128, 128, 128)),
        Color::LightRed => Some((255, 0, 0)),
        Color::LightGreen => Some((0, 255, 0)),
        Color::LightYellow => Some((255, 255, 0)),
        Color::LightBlue => Some((0, 0, 255)),
        Color::LightMagenta => Some((255, 0, 255)),
        Color::LightCyan => Some((0, 255, 255)),
        Color::White => Some((255, 255, 255)),
        _ => None,
    }
}

/// WCAG relative luminance and contrast ratio. `None` when either colour is one the
/// terminal owns, which is the honest answer rather than a guessed number.
pub(crate) fn contrast_ratio(a: Color, b: Color) -> Option<f32> {
    fn luminance((r, g, b): (u8, u8, u8)) -> f32 {
        fn channel(value: u8) -> f32 {
            let value = f32::from(value) / 255.0;
            if value <= 0.03928 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
    }

    let (first, second) = (luminance(resolve_rgb(a)?), luminance(resolve_rgb(b)?));
    let (lighter, darker) = if first >= second {
        (first, second)
    } else {
        (second, first)
    };
    Some((lighter + 0.05) / (darker + 0.05))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ToastClipboardPosition, ToastHerdrPosition};

    fn toast() -> ToastNotification {
        ToastNotification {
            kind: ToastKind::Finished,
            title: "done".to_string(),
            context: "workspace".to_string(),
            position: None,
            target: None,
        }
    }

    fn feedback() -> CopyFeedback {
        CopyFeedback {
            message: "copied to clipboard".to_string(),
        }
    }

    /// Anything the sidebar draws on the focused row's fill has to stay visible on
    /// it. This is the rule the indicator broke when the fill and the idle mark were
    /// handed the same palette entry: a ratio of exactly 1.0, invisible by
    /// construction, and nothing in the suite noticed.
    #[test]
    fn every_indicator_stays_visible_on_the_row_it_marks() {
        const FLOOR: f32 = 1.5;
        let mut checked = 0;

        for name in THEME_NAMES {
            let palette = Palette::from_name(name).expect("named theme should resolve");
            for (state, seen) in [
                (AgentState::Blocked, true),
                (AgentState::Working, true),
                (AgentState::Idle, false),
                (AgentState::Idle, true),
                (AgentState::Unknown, true),
            ] {
                let mark = state_dot(state, seen, &palette)
                    .1
                    .fg
                    .expect("a state mark always sets a colour");
                for (label, background) in [
                    ("focused row", palette.surface_dim),
                    ("panel", palette.panel_bg),
                ] {
                    let Some(ratio) = contrast_ratio(mark, background) else {
                        continue;
                    };
                    checked += 1;
                    assert!(
                        ratio >= FLOOR,
                        "{name}: {state:?}/{seen} on the {label} is {ratio:.2}:1"
                    );
                }
            }
        }

        assert!(checked > 50, "only {checked} pairs were resolvable");
    }

    /// Colour is the axis the sidebar reads by, so two states sharing one entry
    /// erases a distinction silently — which is how mauve and overlay0 both became
    /// grey in the terminal theme and made a branch look like an agent.
    #[test]
    fn no_two_agent_states_share_a_colour() {
        for name in THEME_NAMES {
            let palette = Palette::from_name(name).expect("named theme should resolve");
            // Unknown is the space row's "no agent here" mark and never shares a
            // column with these, so it is free to reuse a colour.
            let marks = [
                (AgentState::Blocked, true),
                (AgentState::Working, true),
                (AgentState::Idle, false),
                (AgentState::Idle, true),
            ]
            .map(|(state, seen)| state_dot(state, seen, &palette).1.fg);

            for (first, second) in
                (0..marks.len()).flat_map(|i| (i + 1..marks.len()).map(move |j| (i, j)))
            {
                assert_ne!(
                    marks[first], marks[second],
                    "{name}: states {first} and {second} share a colour"
                );
            }
        }
    }

    /// The two cells stack into one column: the same width in every state and every
    /// frame, so an agent starting or stopping never nudges the text beside it.
    #[test]
    fn the_two_row_indicator_keeps_one_column_in_every_state() {
        let palette = Palette::catppuccin();
        let width = |cells: [String; 2]| {
            cells.map(|cell| unicode_width::UnicodeWidthStr::width(cell.as_str()))
        };

        let first = agent_state_cells(AgentState::Working, true, 0, &palette).0;
        let later = agent_state_cells(AgentState::Working, true, 5, &palette).0;
        assert_ne!(first, later, "a working agent has to turn");
        // every step moves: a repeated frame is the stall the crossings exist to fix
        let cycle = (0..RING_STEPS)
            .map(|frame| agent_state_cells(AgentState::Working, true, frame, &palette).0)
            .collect::<Vec<_>>();
        for pair in cycle.windows(2) {
            assert_ne!(pair[0], pair[1], "the comet has to advance every step");
        }
        assert_ne!(
            cycle[RING_STEPS as usize - 1],
            cycle[0],
            "the loop has to close"
        );
        assert_eq!(width(first), [1, 1]);
        assert_eq!(width(later), [1, 1]);

        for (state, seen, expected) in [
            (AgentState::Blocked, true, ["┃", "┃"]),
            (AgentState::Idle, false, ["┃", "┃"]),
            // idle keeps the line and drops its weight
            (AgentState::Idle, true, ["│", "│"]),
            (AgentState::Unknown, true, ["┃", "┃"]),
        ] {
            let held = agent_state_cells(state, seen, 0, &palette).0;
            assert_eq!(held, agent_state_cells(state, seen, 9, &palette).0);
            assert_eq!(held, expected.map(str::to_string), "{state:?} {seen}");
            assert_eq!(width(held), [1, 1]);
        }
    }

    /// The indicator column is one cell wide in every state, and only the working
    /// state moves — motion has to mean exactly one thing.
    #[test]
    fn only_a_working_agent_turns_and_every_state_stays_one_cell() {
        let palette = Palette::catppuccin();
        let width = |glyph: &str| unicode_width::UnicodeWidthStr::width(glyph);

        let first = agent_state_icon(AgentState::Working, true, 0, &palette).0;
        let next = agent_state_icon(AgentState::Working, true, 1, &palette).0;
        assert_ne!(first, next);

        for (state, seen) in [
            (AgentState::Blocked, true),
            (AgentState::Idle, false),
            (AgentState::Idle, true),
            (AgentState::Unknown, true),
        ] {
            let held = agent_state_icon(state, seen, 0, &palette).0;
            assert_eq!(held, agent_state_icon(state, seen, 7, &palette).0);
            assert_eq!(width(held), 1, "{state:?} {seen}");
        }
        assert_eq!(width(first), 1);
    }

    #[test]
    fn state_icons_support_dot_and_distinct_symbol_styles() {
        let palette = Palette::catppuccin();
        for (indicator_style, expected_symbols) in [
            (StatusIndicatorStyle::Dots, ["●", "●", "●", "○", "·"]),
            (StatusIndicatorStyle::Symbols, ["×", "◐", "✓", "○", "·"]),
        ] {
            for ((state, seen, color), expected_symbol) in [
                (AgentState::Blocked, true, palette.red),
                (AgentState::Working, true, palette.yellow),
                (AgentState::Idle, false, palette.teal),
                (AgentState::Idle, true, palette.green),
                (AgentState::Unknown, true, palette.overlay0),
            ]
            .into_iter()
            .zip(expected_symbols)
            {
                let (actual_symbol, style) = state_icon(state, seen, indicator_style, &palette);
                assert_eq!(actual_symbol, expected_symbol);
                assert_eq!(display_width_u16(actual_symbol), 1);
                assert_eq!(style.fg, Some(color));
            }
        }
    }

    #[test]
    fn toast_rect_uses_configured_corner() {
        let area = Rect::new(10, 20, 100, 40);
        let toast = toast();

        let top_left = toast_notification_rect(area, &toast, false, ToastHerdrPosition::TopLeft);
        assert_eq!(top_left.x, area.x);
        assert_eq!(top_left.y, area.y);

        let top_right = toast_notification_rect(area, &toast, false, ToastHerdrPosition::TopRight);
        assert_eq!(top_right.x + top_right.width, area.x + area.width);
        assert_eq!(top_right.y, area.y);

        let bottom_left =
            toast_notification_rect(area, &toast, false, ToastHerdrPosition::BottomLeft);
        assert_eq!(bottom_left.x, area.x);
        assert_eq!(bottom_left.y + bottom_left.height, area.y + area.height);

        let bottom_right =
            toast_notification_rect(area, &toast, false, ToastHerdrPosition::BottomRight);
        assert_eq!(bottom_right.x + bottom_right.width, area.x + area.width);
        assert_eq!(bottom_right.y + bottom_right.height, area.y + area.height);
    }

    #[test]
    fn toast_rect_uses_display_width_for_cjk_labels() {
        let area = Rect::new(0, 0, 100, 20);
        let toast = ToastNotification {
            kind: ToastKind::NeedsAttention,
            title: "重构用户认证模块".to_string(),
            context: "提交 herdr 的反馈".to_string(),
            position: None,
            target: None,
        };

        let rect = toast_notification_rect(area, &toast, false, ToastHerdrPosition::TopRight);

        let expected_content_width =
            display_width_u16(&toast.title).max(display_width_u16(&toast.context)) + 6;
        assert_eq!(rect.width, expected_content_width);
        assert_eq!(rect.x + rect.width, area.x + area.width);
    }

    #[test]
    fn copy_feedback_rect_uses_configured_position() {
        let area = Rect::new(10, 20, 100, 40);
        let feedback = feedback();

        let top_center = copy_feedback_rect(area, &feedback, 0, ToastClipboardPosition::TopCenter);
        assert_eq!(top_center.y, area.y);
        assert_eq!(
            top_center.x,
            area.x + area.width.saturating_sub(top_center.width) / 2
        );

        let bottom_center =
            copy_feedback_rect(area, &feedback, 0, ToastClipboardPosition::BottomCenter);
        assert_eq!(bottom_center.y + bottom_center.height, area.y + area.height);
        assert_eq!(
            bottom_center.x,
            area.x + area.width.saturating_sub(bottom_center.width) / 2
        );
    }
}
