use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::state::Palette;

use super::text::display_width_u16;
use super::widgets::panel_contrast_fg;
use crate::app::AppState;

/// The chip an inactive tab sits on. `surface0` is the tier for it, but a theme may
/// leave that to the terminal, and then an inactive tab has no chip at all — only
/// text floating beside the filled active one, which is what stops it reading as a
/// tab. `surface_dim` is the same wash the sidebar's focused row uses and is always
/// a concrete colour.
///
/// A chip is an area rather than text, so it needs far less separation from the bar
/// than a label needs from the chip — but it does need some, or the tab has no edges.
/// Taking that separation from the bar itself is what makes it the same amount in a
/// light theme as in a dark one; a fixed surface tier lands on either side of the bar
/// depending on the theme, and in some it lands on top of it.
fn tab_chip_bg(p: &Palette) -> Color {
    const FLOOR: f32 = 1.15;

    match super::status::resolve_rgb(p.panel_bg).zip(super::status::resolve_rgb(p.text)) {
        Some(_) => super::status::lift_until_legible(p.panel_bg, p.text, p.panel_bg, FLOOR),
        // the theme leaves both to the terminal, so there is nothing to blend from;
        // `surface_dim` is the concrete wash the sidebar's focused row already uses
        None => p.surface_dim,
    }
}

/// How a tab draws. Split out from the render loop so the contrast rules that keep
/// every tab legible can be checked against all themes rather than the one running.
/// The chip is passed in because it is the same for every tab on the bar and costs a
/// walk through the contrast maths to find.
fn tab_chip_style(active: bool, auto_named: bool, chip: Color, p: &Palette) -> Style {
    if active {
        let base = Style::default().fg(panel_contrast_fg(p)).bg(p.accent);
        if auto_named {
            base
        } else {
            base.add_modifier(Modifier::BOLD)
        }
    } else {
        // an unnamed tab has nothing to say and recedes by a tier; it used to also
        // carry DIM, which halved an already-muted tier into near-invisibility
        let quiet = if auto_named { p.overlay0 } else { p.overlay1 };
        Style::default()
            .fg(super::status::lift_until_legible(quiet, p.text, chip, 2.5))
            .bg(chip)
    }
}

const MIN_TAB_WIDTH: u16 = 8;
const NEW_TAB_WIDTH: u16 = 3;
const TAB_SCROLL_BUTTON_WIDTH: u16 = 3;
const ZOOM_INDICATOR: &str = "ZOOM";
// The narrowest overflowing tab strip worth keeping interactive: one
// minimum-width tab, both scroll controls, and the new-tab control.
const MIN_TAB_STRIP_WIDTH: u16 =
    MIN_TAB_WIDTH + NEW_TAB_WIDTH + TAB_SCROLL_BUTTON_WIDTH.saturating_mul(2);

#[derive(Debug, Clone, Default)]
pub(crate) struct TabBarView {
    pub scroll: usize,
    pub tab_hit_areas: Vec<Rect>,
    pub scroll_left_hit_area: Rect,
    pub scroll_right_hit_area: Rect,
    pub new_tab_hit_area: Rect,
}

/// Whether the only tab is carrying nothing but its own number. A lone tab is not a
/// choice, and a chip reading `1` costs a corner of the bar to say so; a name is
/// different, since you gave it one for a reason.
fn lone_tab_is_only_a_placeholder(ws: &crate::workspace::Workspace) -> bool {
    match ws.tabs.as_slice() {
        [only] => only.is_auto_named() && !only.zoomed,
        _ => false,
    }
}

fn tab_width(ws: &crate::workspace::Workspace, tab_idx: usize) -> u16 {
    display_width_u16(&tab_chrome_label(ws, tab_idx))
        .saturating_add(4)
        .max(MIN_TAB_WIDTH)
}

fn tab_chrome_label(ws: &crate::workspace::Workspace, tab_idx: usize) -> String {
    let name = ws
        .tab_display_name(tab_idx)
        .unwrap_or_else(|| (tab_idx + 1).to_string());
    if ws.tabs.get(tab_idx).is_some_and(|tab| tab.zoomed) {
        format!("{name} Z")
    } else {
        name
    }
}

#[derive(Clone, Copy)]
struct VisibleStatusSegment<'a> {
    text: &'a str,
    accent: bool,
}

fn visible_status_segments(app: &AppState) -> Vec<VisibleStatusSegment<'_>> {
    let zoomed = app
        .active
        .and_then(|index| app.workspaces.get(index))
        .is_some_and(|workspace| workspace.zoomed);
    app.tab_bar_right
        .iter()
        .filter_map(|segment| match segment {
            crate::app::state::TabBarStatusSegment::Zoom if zoomed => Some(VisibleStatusSegment {
                text: ZOOM_INDICATOR,
                accent: true,
            }),
            crate::app::state::TabBarStatusSegment::Text(Some(text))
                if display_width_u16(text) > 0 =>
            {
                Some(VisibleStatusSegment {
                    text,
                    accent: false,
                })
            }
            crate::app::state::TabBarStatusSegment::Zoom
            | crate::app::state::TabBarStatusSegment::Text(_) => None,
        })
        .collect()
}

fn tab_bar_status_width(app: &AppState) -> u16 {
    let segments = visible_status_segments(app);
    let content_width = segments.iter().fold(0_u16, |width, segment| {
        width.saturating_add(display_width_u16(segment.text))
    });
    let separators = u16::try_from(segments.len().saturating_sub(1)).unwrap_or(u16::MAX);
    content_width
        .saturating_add(display_width_u16(&app.tab_bar_right_separator).saturating_mul(separators))
}

fn tab_bar_status_area(app: &AppState, area: Rect) -> Option<Rect> {
    let width = tab_bar_status_width(app);
    if width == 0 {
        return None;
    }
    let reserved = width.saturating_add(1);
    (area.width.saturating_sub(reserved) >= MIN_TAB_STRIP_WIDTH)
        .then(|| Rect::new(area.x + area.width.saturating_sub(width), area.y, width, 1))
}

// Tabs win over status decoration on narrow rows. The extra reserved cell is
// the gap between the interactive strip and the right-aligned status entries.
pub(crate) fn tab_bar_content_area(app: &AppState, area: Rect) -> Rect {
    let reserved = tab_bar_status_area(app, area)
        .map(|status| status.width.saturating_add(1))
        .unwrap_or(0);
    Rect {
        width: area.width.saturating_sub(reserved),
        ..area
    }
}

fn layout_tab_hit_areas(ws: &crate::workspace::Workspace, area: Rect, scroll: usize) -> Vec<Rect> {
    let mut rects = vec![Rect::default(); ws.tabs.len()];
    if area.width == 0 || area.height == 0 {
        return rects;
    }

    let mut x = area.x;
    let right = area.x + area.width;
    for (idx, rect) in rects.iter_mut().enumerate().skip(scroll) {
        if x >= right {
            break;
        }
        let desired = tab_width(ws, idx);
        let remaining = right.saturating_sub(x);
        let width = desired.min(remaining).max(1);
        *rect = Rect::new(x, area.y, width, 1);
        x = x.saturating_add(width + 1);
    }
    rects
}

fn centered_tab_scroll(ws: &crate::workspace::Workspace, area: Rect) -> usize {
    let mut best_scroll = ws.active_tab;
    let mut best_distance = u16::MAX;
    let viewport_center = area.x.saturating_mul(2).saturating_add(area.width);

    for scroll in 0..=ws.active_tab {
        let rects = layout_tab_hit_areas(ws, area, scroll);
        let Some(active_rect) = rects.get(ws.active_tab).copied() else {
            continue;
        };
        if active_rect.width == 0 {
            continue;
        }

        let active_center = active_rect
            .x
            .saturating_mul(2)
            .saturating_add(active_rect.width);
        let distance = active_center.abs_diff(viewport_center);
        if distance <= best_distance {
            best_distance = distance;
            best_scroll = scroll;
        }
    }

    best_scroll
}

fn trailing_tab_controls_x(tab_hit_areas: &[Rect], fallback_x: u16) -> u16 {
    tab_hit_areas
        .iter()
        .rev()
        .find(|rect| rect.width > 0)
        .map(|rect| rect.x + rect.width)
        .unwrap_or(fallback_x)
}

fn max_tab_scroll(ws: &crate::workspace::Workspace, area: Rect) -> usize {
    (0..ws.tabs.len())
        .find(|&scroll| {
            layout_tab_hit_areas(ws, area, scroll)
                .last()
                .is_some_and(|rect| rect.width > 0)
        })
        .unwrap_or(0)
}

pub(crate) fn compute_tab_bar_view(
    ws: &crate::workspace::Workspace,
    area: Rect,
    current_scroll: usize,
    follow_active: bool,
    mouse_chrome: bool,
) -> TabBarView {
    if area.width == 0 || area.height == 0 {
        return TabBarView::default();
    }

    if lone_tab_is_only_a_placeholder(ws) {
        // nothing to switch to and nothing the chip could say; `+` is still an action,
        // and the bar itself stays because it carries the usage figures
        return TabBarView {
            scroll: 0,
            tab_hit_areas: vec![Rect::default(); ws.tabs.len()],
            scroll_left_hit_area: Rect::default(),
            scroll_right_hit_area: Rect::default(),
            new_tab_hit_area: if mouse_chrome {
                Rect::new(area.x, area.y, NEW_TAB_WIDTH.min(area.width), 1)
            } else {
                Rect::default()
            },
        };
    }

    if !mouse_chrome {
        let max_scroll = max_tab_scroll(ws, area);
        let scroll = if follow_active {
            centered_tab_scroll(ws, area).min(max_scroll)
        } else {
            current_scroll.min(max_scroll)
        };
        return TabBarView {
            scroll,
            tab_hit_areas: layout_tab_hit_areas(ws, area, scroll),
            scroll_left_hit_area: Rect::default(),
            scroll_right_hit_area: Rect::default(),
            new_tab_hit_area: Rect::default(),
        };
    }

    let area_right = area.x + area.width;
    let all_tabs_area = Rect::new(
        area.x,
        area.y,
        area.width.saturating_sub(NEW_TAB_WIDTH),
        area.height,
    );
    let all_tabs = layout_tab_hit_areas(ws, all_tabs_area, 0);
    let overflow = all_tabs.iter().any(|rect| rect.width == 0);
    if !overflow {
        let new_tab_x = trailing_tab_controls_x(&all_tabs, area.x);
        let new_tab_hit_area = Rect::new(
            new_tab_x,
            area.y,
            area_right.saturating_sub(new_tab_x).min(NEW_TAB_WIDTH),
            1,
        );
        return TabBarView {
            scroll: 0,
            tab_hit_areas: all_tabs,
            scroll_left_hit_area: Rect::default(),
            scroll_right_hit_area: Rect::default(),
            new_tab_hit_area,
        };
    }

    let left_hit_area = Rect::new(area.x, area.y, TAB_SCROLL_BUTTON_WIDTH.min(area.width), 1);
    let tab_area_x = left_hit_area.x + left_hit_area.width;
    let reserved_trailing_width = NEW_TAB_WIDTH.saturating_add(TAB_SCROLL_BUTTON_WIDTH);
    let tab_area_right = area_right.saturating_sub(reserved_trailing_width);
    let tab_area = Rect::new(
        tab_area_x,
        area.y,
        tab_area_right.saturating_sub(tab_area_x),
        area.height,
    );

    let max_scroll = max_tab_scroll(ws, tab_area);
    let scroll = if follow_active {
        centered_tab_scroll(ws, tab_area).min(max_scroll)
    } else {
        current_scroll.min(max_scroll)
    };
    let tab_hit_areas = layout_tab_hit_areas(ws, tab_area, scroll);
    let trailing_x = trailing_tab_controls_x(&tab_hit_areas, tab_area_x).min(tab_area_right);
    let right_hit_area = Rect::new(
        trailing_x,
        area.y,
        area_right
            .saturating_sub(trailing_x)
            .min(TAB_SCROLL_BUTTON_WIDTH),
        1,
    );
    let new_tab_x = right_hit_area.x + right_hit_area.width;
    let new_tab_hit_area = Rect::new(
        new_tab_x,
        area.y,
        area_right.saturating_sub(new_tab_x).min(NEW_TAB_WIDTH),
        1,
    );

    TabBarView {
        scroll,
        tab_hit_areas,
        scroll_left_hit_area: left_hit_area,
        scroll_right_hit_area: right_hit_area,
        new_tab_hit_area,
    }
}

fn tab_drop_indicator_x(
    app: &AppState,
    ws: &crate::workspace::Workspace,
    insert_idx: usize,
) -> Option<u16> {
    let mut visible_tabs = app
        .view
        .tab_hit_areas
        .iter()
        .enumerate()
        .filter(|(_, rect)| rect.width > 0);
    let first_visible = visible_tabs.clone().next()?;
    let last_visible = visible_tabs.next_back().unwrap_or(first_visible);

    if insert_idx == 0 {
        return Some(if first_visible.0 == 0 {
            first_visible.1.x
        } else {
            app.view.tab_scroll_left_hit_area.x + app.view.tab_scroll_left_hit_area.width
        });
    }

    if let Some((_, rect)) = app
        .view
        .tab_hit_areas
        .iter()
        .enumerate()
        .find(|(idx, rect)| *idx == insert_idx && rect.width > 0)
    {
        return Some(rect.x.saturating_sub(1));
    }

    if insert_idx >= ws.tabs.len() {
        return Some(if last_visible.0 + 1 >= ws.tabs.len() {
            last_visible.1.x + last_visible.1.width
        } else {
            app.view.tab_scroll_right_hit_area.x.saturating_sub(1)
        });
    }

    None
}

pub(super) fn render_tab_bar(app: &AppState, frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let Some(active_ws_idx) = app.active else {
        return;
    };
    let Some(ws) = app.workspaces.get(active_ws_idx) else {
        return;
    };
    let p = &app.palette;
    // one chip for the whole bar: it depends on nothing but the palette
    let chip = tab_chip_bg(p);

    frame.render_widget(
        Paragraph::new(" ".repeat(area.width as usize)).style(Style::default().bg(p.panel_bg)),
        area,
    );

    let first_visible_idx = app
        .view
        .tab_hit_areas
        .iter()
        .enumerate()
        .find(|(_, rect)| rect.width > 0)
        .map(|(idx, _)| idx);
    let last_visible_idx = app
        .view
        .tab_hit_areas
        .iter()
        .enumerate()
        .rev()
        .find(|(_, rect)| rect.width > 0)
        .map(|(idx, _)| idx);
    let can_scroll_left = app.view.tab_scroll_left_hit_area.width > 0 && app.tab_scroll > 0;
    let can_scroll_right = app.view.tab_scroll_right_hit_area.width > 0
        && last_visible_idx.is_some_and(|idx| idx + 1 < ws.tabs.len());

    if app.mouse_capture && app.view.tab_scroll_left_hit_area.width > 0 {
        let style = if can_scroll_left {
            Style::default().fg(p.overlay1).bg(chip)
        } else {
            Style::default()
                .fg(p.overlay0)
                .bg(chip)
                .add_modifier(Modifier::DIM)
        };
        frame.render_widget(
            Paragraph::new(" < ").style(style),
            app.view.tab_scroll_left_hit_area,
        );
    }

    if app.mouse_capture && app.view.tab_scroll_right_hit_area.width > 0 {
        let style = if can_scroll_right {
            Style::default().fg(p.overlay1).bg(chip)
        } else {
            Style::default()
                .fg(p.overlay0)
                .bg(chip)
                .add_modifier(Modifier::DIM)
        };
        frame.render_widget(
            Paragraph::new(" > ").style(style),
            app.view.tab_scroll_right_hit_area,
        );
    }

    for (idx, tab) in ws.tabs.iter().enumerate() {
        let Some(rect) = app.view.tab_hit_areas.get(idx).copied() else {
            break;
        };
        if rect.width == 0 {
            continue;
        }
        let style = tab_chip_style(idx == ws.active_tab, tab.is_auto_named(), chip, p);
        let width = rect.width as usize;
        let name = tab_chrome_label(ws, idx);
        // Pad by terminal columns, not chars, so wide glyphs stay centered.
        let padding = width.saturating_sub(display_width_u16(&name) as usize);
        let left = padding / 2;
        let text = format!(
            "{empty:left$}{name}{empty:right$}",
            empty = "",
            right = padding - left
        );
        frame.render_widget(Paragraph::new(text).style(style), rect);
    }

    if let Some(crate::app::state::DragState {
        target:
            crate::app::state::DragTarget::TabReorder {
                ws_idx,
                insert_idx: Some(insert_idx),
                ..
            },
    }) = &app.drag
    {
        if *ws_idx == active_ws_idx {
            if let Some(x) = tab_drop_indicator_x(app, ws, *insert_idx) {
                frame.buffer_mut()[(x.min(area.x + area.width.saturating_sub(1)), area.y)]
                    .set_symbol("│")
                    .set_style(Style::default().fg(p.accent));
            }
        }
    }

    if app.mouse_capture && app.view.new_tab_hit_area.width > 0 {
        frame.render_widget(
            Paragraph::new(" + ").style(Style::default().fg(p.overlay1)),
            app.view.new_tab_hit_area,
        );
    }

    if first_visible_idx.is_some_and(|idx| idx > 0) {
        let x = if app.mouse_capture && app.view.tab_scroll_left_hit_area.width > 0 {
            app.view.tab_scroll_left_hit_area.x + app.view.tab_scroll_left_hit_area.width
        } else {
            area.x
        };
        if x < area.x + area.width {
            frame.buffer_mut()[(x, area.y)]
                .set_symbol("…")
                .set_style(Style::default().fg(p.overlay0));
        }
    }
    if last_visible_idx.is_some_and(|idx| idx + 1 < ws.tabs.len()) {
        let content = tab_bar_content_area(app, area);
        let content_right = content.x + content.width;
        let x = if app.mouse_capture && app.view.tab_scroll_right_hit_area.width > 0 {
            app.view.tab_scroll_right_hit_area.x.saturating_sub(1)
        } else {
            content_right.saturating_sub(1)
        };
        if x >= area.x && x < area.x + area.width {
            frame.buffer_mut()[(x, area.y)]
                .set_symbol("…")
                .set_style(Style::default().fg(p.overlay0));
        }
    }

    render_claude_usage(app, frame, area);

    if let Some(status_area) = tab_bar_status_area(app, area) {
        let segments = visible_status_segments(app);
        let separator_width = display_width_u16(&app.tab_bar_right_separator);
        let mut x = status_area.x;
        for (index, segment) in segments.iter().enumerate() {
            if index > 0 && separator_width > 0 {
                let rect = Rect::new(x, area.y, separator_width, 1);
                frame.render_widget(
                    Paragraph::new(app.tab_bar_right_separator.as_str())
                        .style(Style::default().fg(p.overlay0).bg(p.panel_bg)),
                    rect,
                );
                x = x.saturating_add(separator_width);
            }

            let width = display_width_u16(segment.text);
            let rect = Rect::new(x, area.y, width, 1);
            let style = if segment.accent {
                Style::default()
                    .fg(panel_contrast_fg(p))
                    .bg(p.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(p.overlay1).bg(p.panel_bg)
            };
            frame.render_widget(Paragraph::new(segment.text).style(style), rect);
            x = x.saturating_add(width);
        }
    }
}

/// Shortens a window's name to something a one-row bar can carry. Anything the
/// endpoint names after a model keeps that name, since that is already short.
fn usage_window_label(label: &str) -> String {
    match label {
        "session" => "5h".to_string(),
        "weekly_all" => "week".to_string(),
        // a model's own capitalisation is the endpoint's; nothing else on this bar
        // is capitalised, and one capital in a row of figures reads as emphasis
        other => other.to_lowercase(),
    }
}

/// Whose figures these are. Not Anthropic's mark: no font ships it, and a terminal
/// cell holds one glyph. This is Octicons' filled sparkle, the shape editors have
/// settled on for "a model did this" — solid enough to carry the brand's colour,
/// where the plain asterisks are too thin to read as anything.
///
/// A Nerd Font private-use codepoint, so it needs one installed. Everything here
/// already assumes that; a box would be the sign it is missing.
const USAGE_MARK: &str = "";

/// How long the group has left. A clock rather than the circular arrow that usually
/// means "reset": the number before it is time remaining, and a refresh arrow reads as
/// something you could press. Font Awesome rather than the Octicons the mark above
/// comes from — Octicons draws a size larger than the figures beside it here, and one
/// oversized glyph in a row of numbers reads as emphasis.
const RESET_MARK: &str = "";

/// Windows that run down together, each group carrying the one countdown they share.
///
/// Grouped by the countdown as written, not by the timestamp behind it: the endpoint
/// stamps the weekly windows microseconds apart, so comparing the text it sends puts
/// two windows that reset at the same moment in separate groups and prints `reset 72h`
/// twice. A window with no reset to report never joins a group — an absent time is not
/// a time two windows can have in common.
fn usage_groups(
    windows: &[crate::usage::UsageWindow],
    now: std::time::SystemTime,
) -> Vec<(Vec<&crate::usage::UsageWindow>, Option<String>)> {
    let mut groups: Vec<(Vec<&crate::usage::UsageWindow>, Option<String>)> = Vec::new();
    for window in windows {
        let reset = window
            .resets
            .as_deref()
            .and_then(|stamp| crate::usage::resets_in(stamp, now))
            .map(crate::usage::reset_label);
        match groups
            .last_mut()
            .filter(|(_, last)| reset.is_some() && *last == reset)
        {
            Some((group, _)) => group.push(window),
            None => groups.push((vec![window], reset)),
        }
    }
    groups
}

/// A token count at the width a one-row bar can spare. Three figures is as much as a
/// number this size says: nobody acts on the difference between 5.81B and 5.82B, and
/// the digits it would take to tell them apart cost a tab its place.
fn compact_tokens(tokens: u64) -> String {
    match tokens {
        ..1_000 => tokens.to_string(),
        1_000..1_000_000 => format!("{:.1}k", tokens as f64 / 1e3),
        1_000_000..1_000_000_000 => format!("{:.1}M", tokens as f64 / 1e6),
        _ => format!("{:.1}B", tokens as f64 / 1e9),
    }
}

/// Quiet until the window is close to spent. A figure that is loud at 3% teaches you
/// to stop reading it, and then it is not there when it matters.
fn usage_percent_color(percent: u8, p: &Palette) -> Color {
    match percent {
        90.. => p.red,
        75..=89 => p.peach,
        _ => p.overlay0,
    }
}

/// Claude's limit windows, in the end of the bar the tabs never reach. They are only
/// otherwise visible by leaving what you are doing and asking Claude itself.
fn render_claude_usage(app: &AppState, frame: &mut Frame, area: Rect) {
    let Some(usage) = app.claude_usage.as_ref().filter(|u| !u.windows.is_empty()) else {
        return;
    };
    let p = &app.palette;

    let mut spans: Vec<Span<'static>> = vec![Span::styled(
        format!("{USAGE_MARK} "),
        // the brand's own colour is what makes the mark read as whose figures these
        // are; a glyph this thin cannot do it on weight
        Style::default().fg(p.peach),
    )];
    let now = std::time::SystemTime::now();
    // windows that reset together are one thing running down, so the countdown is
    // written once for the group rather than repeated after every figure
    for (group_idx, (group, reset)) in usage_groups(&usage.windows, now).iter().enumerate() {
        if group_idx > 0 {
            spans.push(Span::styled(" │ ", Style::default().fg(p.surface1)));
        }
        for (idx, window) in group.iter().enumerate() {
            if idx > 0 {
                spans.push(Span::styled(", ", Style::default().fg(p.overlay0)));
            }
            spans.push(Span::styled(
                format!("{} ", usage_window_label(&window.label)),
                Style::default().fg(p.overlay0),
            ));
            spans.push(Span::styled(
                format!("{}%", window.percent),
                Style::default().fg(usage_percent_color(window.percent, p)),
            ));
        }
        // the percent alone does not say whether it is worth waiting out. It joins the
        // group on the same comma the windows use, so the countdown reads as one more
        // thing about them rather than as something that ran into the last figure
        if let Some(label) = reset {
            spans.push(Span::styled(
                format!(", {label} {RESET_MARK}"),
                Style::default().fg(p.overlay0),
            ));
        }
    }

    // a percentage says how much of a limit is gone, never how much went through. Its
    // own group rather than beside `week`: this is a rolling seven days, not the
    // endpoint's window, and sharing a label would make it the wrong number
    if let Some(tokens) = app.claude_seven_day_tokens {
        spans.push(Span::styled(" │ ", Style::default().fg(p.surface1)));
        spans.push(Span::styled(
            format!("7d {}", compact_tokens(tokens)),
            Style::default().fg(p.overlay0),
        ));
    }

    let width = spans
        .iter()
        .map(|span| display_width_u16(span.content.as_ref()))
        .sum::<u16>();
    // the tabs own their end of the bar; the figures only take what is left over,
    // and say nothing at all rather than push a tab off the edge
    let taken = app
        .view
        .tab_hit_areas
        .iter()
        .chain(std::iter::once(&app.view.new_tab_hit_area))
        .filter(|rect| rect.width > 0)
        .map(|rect| rect.x.saturating_add(rect.width))
        .max()
        .unwrap_or(area.x);
    let right = if app.mouse_capture && app.view.tab_scroll_right_hit_area.width > 0 {
        app.view.tab_scroll_right_hit_area.x
    } else {
        area.x.saturating_add(area.width)
    };
    let Some(x) = right.checked_sub(width.saturating_add(1)) else {
        return;
    };
    if x <= taken {
        return;
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)),
        Rect::new(x, area.y, width, 1),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::AppState;
    use crate::workspace::Workspace;
    use ratatui::{backend::TestBackend, Terminal};

    fn buffer_row_text(buffer: &ratatui::buffer::Buffer, area: Rect, row: u16) -> String {
        (area.x..area.x + area.width)
            .map(|x| buffer[(x, row)].symbol())
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    fn usage(windows: &[(&str, u8)]) -> crate::usage::ClaudeUsage {
        crate::usage::ClaudeUsage {
            windows: windows
                .iter()
                .map(|(label, percent)| crate::usage::UsageWindow {
                    label: (*label).to_string(),
                    percent: *percent,
                    resets: None,
                })
                .collect(),
        }
    }

    fn window(label: &str, percent: u8, resets: Option<&str>) -> crate::usage::UsageWindow {
        crate::usage::UsageWindow {
            label: label.to_string(),
            percent,
            resets: resets.map(str::to_string),
        }
    }

    fn render_bar(app: &AppState, width: u16) -> String {
        let backend = TestBackend::new(width, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_tab_bar(app, frame, app.view.tab_bar_rect))
            .unwrap();
        buffer_row_text(terminal.backend().buffer(), app.view.tab_bar_rect, 0)
    }

    fn app_with_usage(width: u16, usage: crate::usage::ClaudeUsage) -> AppState {
        let mut app = AppState::test_new();
        app.workspaces = vec![Workspace::test_new("test")];
        app.active = Some(0);
        app.claude_usage = Some(usage);
        app.view.tab_bar_rect = Rect::new(0, 0, width, 1);
        let view = compute_tab_bar_view(&app.workspaces[0], app.view.tab_bar_rect, 0, true, false);
        app.view.tab_hit_areas = view.tab_hit_areas;
        app
    }

    /// The end of the bar is dead space no tab ever reaches, and these figures are
    /// otherwise only visible by leaving what you are doing to ask Claude for them.
    #[test]
    fn the_bar_carries_claude_usage_in_the_end_the_tabs_never_reach() {
        let app = app_with_usage(60, usage(&[("session", 3), ("weekly_all", 6)]));

        let row = render_bar(&app, 60);

        assert!(row.ends_with(" 5h 3% │ week 6%"), "bar row: {row:?}");
    }

    /// A percentage says how much of a limit is gone and never how much went through,
    /// and the endpoint has no count to give. This one is tallied from Claude's own
    /// transcripts, so it stands apart from the windows beside it.
    #[test]
    fn the_bar_carries_the_weeks_tokens_beside_the_limits() {
        let mut app = app_with_usage(70, usage(&[("session", 3), ("weekly_all", 6)]));
        app.claude_seven_day_tokens = Some(31_000_000);

        let row = render_bar(&app, 70);

        assert!(row.ends_with("│ 7d 31.0M"), "bar row: {row:?}");
    }

    /// The countdown belongs to the group, so it joins on the same comma the windows
    /// in it use. Without one it ran straight into the last percentage.
    #[test]
    fn a_countdown_joins_its_group_on_a_comma() {
        // the render path reads the wall clock, and a window that has already reset
        // reports no countdown to join, so the stamps have to outlive the test itself
        let mut app = app_with_usage(
            70,
            crate::usage::ClaudeUsage {
                windows: vec![
                    window("weekly_all", 24, Some("2126-08-06T12:00:00Z")),
                    window("Fable", 7, Some("2126-08-06T12:00:00.000603Z")),
                ],
            },
        );
        app.claude_seven_day_tokens = None;

        let row = render_bar(&app, 70);

        assert!(row.contains("week 24%, fable 7%, "), "bar row: {row:?}");
    }

    /// Nobody acts on the difference between 5.81B and 5.82B, and the digits it takes
    /// to tell them apart cost a tab its place on the bar.
    #[test]
    fn a_token_count_keeps_three_figures_and_no_more() {
        assert_eq!(compact_tokens(0), "0");
        assert_eq!(compact_tokens(999), "999");
        assert_eq!(compact_tokens(12_300), "12.3k");
        assert_eq!(compact_tokens(83_167_217), "83.2M");
        assert_eq!(compact_tokens(5_809_127_363), "5.8B");
    }

    /// Two windows that reset at the same moment are one thing running down, so the
    /// countdown belongs to the pair. The endpoint stamps them microseconds apart, and
    /// grouping on the text it sends printed `reset` after each of them.
    #[test]
    fn windows_that_reset_together_share_one_countdown() {
        let now = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_785_585_600);
        let windows = vec![
            window("session", 4, Some("2026-08-01T15:00:00Z")),
            window("weekly_all", 10, Some("2026-08-04T12:00:00.603277+00:00")),
            window("Fable", 2, Some("2026-08-04T12:00:00.603504+00:00")),
        ];

        let groups = usage_groups(&windows, now);

        assert_eq!(groups.len(), 2, "the weeklies belong together");
        assert_eq!(groups[0].1.as_deref(), Some("3h"));
        assert_eq!(groups[1].0.len(), 2);
        assert_eq!(groups[1].1.as_deref(), Some("72h"));
        // the endpoint capitalises a model name; nothing else on the bar is
        assert_eq!(usage_window_label(&groups[1].0[1].label), "fable");
    }

    /// A window with no reset to report cannot share one, however it is placed.
    #[test]
    fn a_window_without_a_reset_stands_on_its_own() {
        let now = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_785_585_600);
        let windows = vec![window("session", 4, None), window("weekly_all", 10, None)];

        let groups = usage_groups(&windows, now);

        assert_eq!(groups.len(), 2);
        assert!(groups.iter().all(|(_, reset)| reset.is_none()));
    }

    /// A figure that shouts at 3% teaches you to stop reading it, and then it is not
    /// there at 95%.
    #[test]
    fn a_usage_window_only_takes_a_colour_once_it_is_nearly_spent() {
        let p = crate::app::state::Palette::from_name("catppuccin").expect("theme resolves");

        assert_eq!(usage_percent_color(3, &p), p.overlay0);
        assert_eq!(usage_percent_color(74, &p), p.overlay0);
        assert_eq!(usage_percent_color(80, &p), p.peach);
        assert_eq!(usage_percent_color(95, &p), p.red);
    }

    /// The tabs own the bar. A narrow one has no room to spare, and dropping the
    /// figures is the only answer that does not push a tab off the edge.
    #[test]
    fn usage_gives_way_rather_than_crowding_the_tabs() {
        let app = app_with_usage(14, usage(&[("session", 3), ("weekly_all", 6)]));

        let row = render_bar(&app, 14);

        assert!(!row.contains('%'), "bar row: {row:?}");
    }

    #[test]
    fn tab_bar_marks_zoomed_tabs_without_renaming_them() {
        let mut app = AppState::test_new();
        let mut ws = Workspace::test_new("test");
        ws.tabs[0].zoomed = true;
        let custom_tab = ws.test_add_tab(Some("test"));
        ws.tabs[custom_tab].zoomed = true;

        app.workspaces = vec![ws];
        app.active = Some(0);
        app.view.tab_bar_rect = Rect::new(0, 0, 30, 1);
        let view = compute_tab_bar_view(&app.workspaces[0], app.view.tab_bar_rect, 0, true, false);
        app.view.tab_hit_areas = view.tab_hit_areas;

        let backend = TestBackend::new(30, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_tab_bar(&app, frame, app.view.tab_bar_rect))
            .unwrap();

        let row = buffer_row_text(terminal.backend().buffer(), app.view.tab_bar_rect, 0);
        assert!(row.contains(" 1 Z"), "tab row: {row:?}");
        assert!(row.contains(" test Z"), "tab row: {row:?}");
        assert_eq!(app.workspaces[0].tab_display_name(0).as_deref(), Some("1"));
        assert_eq!(
            app.workspaces[0].tab_display_name(custom_tab).as_deref(),
            Some("test")
        );
    }

    #[test]
    fn tab_bar_renders_ordered_status_entries_with_separator() {
        let mut app = AppState::test_new();
        let mut ws = Workspace::test_new("test");
        ws.tabs[0].zoomed = true;
        app.tab_bar_right = vec![
            crate::app::state::TabBarStatusSegment::Zoom,
            crate::app::state::TabBarStatusSegment::Text(Some("wintermute".into())),
            crate::app::state::TabBarStatusSegment::Text(Some("14:30".into())),
        ];
        app.tab_bar_right_separator = " · ".into();

        app.workspaces = vec![ws];
        app.active = Some(0);
        app.view.tab_bar_rect = Rect::new(0, 0, 60, 1);
        let content = tab_bar_content_area(&app, app.view.tab_bar_rect);
        let view = compute_tab_bar_view(&app.workspaces[0], content, 0, true, false);
        app.view.tab_hit_areas = view.tab_hit_areas.clone();

        let backend = TestBackend::new(60, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_tab_bar(&app, frame, app.view.tab_bar_rect))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let row = buffer_row_text(buffer, app.view.tab_bar_rect, 0);
        assert!(
            row.ends_with("ZOOM · wintermute · 14:30"),
            "tab row: {row:?}"
        );
        let status_x = 60 - display_width_u16("ZOOM · wintermute · 14:30");
        assert_eq!(buffer[(status_x, 0)].style().bg, Some(app.palette.accent));
        for rect in &view.tab_hit_areas {
            assert!(rect.x + rect.width <= content.x + content.width);
        }
    }

    #[test]
    fn hidden_status_entries_do_not_leave_dangling_separators() {
        let mut app = AppState::test_new();
        app.tab_bar_right = vec![
            crate::app::state::TabBarStatusSegment::Zoom,
            crate::app::state::TabBarStatusSegment::Text(None),
            crate::app::state::TabBarStatusSegment::Text(Some("wintermute".into())),
        ];
        app.tab_bar_right_separator = " | ".into();
        app.workspaces = vec![Workspace::test_new("test")];
        app.active = Some(0);
        app.view.tab_bar_rect = Rect::new(0, 0, 40, 1);
        let content = tab_bar_content_area(&app, app.view.tab_bar_rect);
        let view = compute_tab_bar_view(&app.workspaces[0], content, 0, true, false);
        app.view.tab_hit_areas = view.tab_hit_areas;

        let backend = TestBackend::new(40, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_tab_bar(&app, frame, app.view.tab_bar_rect))
            .unwrap();

        let row = buffer_row_text(terminal.backend().buffer(), app.view.tab_bar_rect, 0);
        assert!(row.ends_with("wintermute"), "tab row: {row:?}");
        assert!(!row.contains(" | "), "tab row: {row:?}");
    }

    #[test]
    fn status_reservation_keeps_a_minimum_width_tab_between_scroll_controls() {
        let mut app = AppState::test_new();
        app.tab_bar_right = vec![crate::app::state::TabBarStatusSegment::Text(Some(
            "x".into(),
        ))];
        let mut workspace = Workspace::test_new("test");
        workspace.test_add_tab(None);
        workspace.test_add_tab(None);
        app.workspaces = vec![workspace];
        app.active = Some(0);

        let too_narrow = Rect::new(0, 0, MIN_TAB_STRIP_WIDTH + 1, 1);
        assert_eq!(tab_bar_content_area(&app, too_narrow), too_narrow);

        let wide_enough = Rect::new(0, 0, MIN_TAB_STRIP_WIDTH + 2, 1);
        let content = tab_bar_content_area(&app, wide_enough);
        assert_eq!(content.width, MIN_TAB_STRIP_WIDTH);
        let view = compute_tab_bar_view(&app.workspaces[0], content, 0, true, true);
        assert!(view.tab_hit_areas[0].width >= MIN_TAB_WIDTH);
    }

    #[test]
    fn combined_status_entries_yield_to_tab_controls_on_narrow_rows() {
        let mut app = AppState::test_new();
        app.tab_bar_right = vec![
            crate::app::state::TabBarStatusSegment::Text(Some(
                "a-hostname-wider-than-the-whole-bar".into(),
            )),
            crate::app::state::TabBarStatusSegment::Text(Some("14:30".into())),
        ];
        // a named tab, because a lone auto-named one draws no chip to compete with
        let mut ws = Workspace::test_new("test");
        ws.tabs[0].set_custom_name("one".into());
        app.workspaces = vec![ws];
        app.active = Some(0);
        app.view.tab_bar_rect = Rect::new(0, 0, 30, 1);

        assert_eq!(
            tab_bar_content_area(&app, app.view.tab_bar_rect),
            app.view.tab_bar_rect
        );
        assert_eq!(tab_bar_status_area(&app, app.view.tab_bar_rect), None);

        let view = compute_tab_bar_view(
            &app.workspaces[0],
            tab_bar_content_area(&app, app.view.tab_bar_rect),
            0,
            true,
            true,
        );
        assert!(view.tab_hit_areas[0].width > 0);
        assert!(view.new_tab_hit_area.width > 0);
    }

    #[test]
    fn cjk_tab_labels_are_centered_by_display_width() {
        let mut app = AppState::test_new();
        let mut ws = Workspace::test_new("test");
        ws.tabs[0].set_custom_name("提交 herdr 的反馈".into());

        app.workspaces = vec![ws];
        app.active = Some(0);
        app.view.tab_bar_rect = Rect::new(0, 0, 30, 1);
        let view = compute_tab_bar_view(&app.workspaces[0], app.view.tab_bar_rect, 0, true, false);
        app.view.tab_hit_areas = view.tab_hit_areas;

        let backend = TestBackend::new(30, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_tab_bar(&app, frame, app.view.tab_bar_rect))
            .unwrap();

        // 17 display columns + 4 padding: two columns each side, wide glyphs
        // starting right after the left padding.
        let rect = app.view.tab_hit_areas[0];
        assert_eq!(rect.width, 21);
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(rect.x, rect.y)].symbol(), " ");
        assert_eq!(buffer[(rect.x + 1, rect.y)].symbol(), " ");
        assert_eq!(buffer[(rect.x + 2, rect.y)].symbol(), "提");
        assert_eq!(buffer[(rect.x + rect.width - 2, rect.y)].symbol(), " ");
        assert_eq!(buffer[(rect.x + rect.width - 1, rect.y)].symbol(), " ");
    }

    #[test]
    fn tab_labels_are_centered_in_their_cells() {
        let mut app = AppState::test_new();
        let mut ws = Workspace::test_new("test");
        ws.tabs[0].set_custom_name("omarchy".into());

        app.workspaces = vec![ws];
        app.active = Some(0);
        app.view.tab_bar_rect = Rect::new(0, 0, 30, 1);
        let view = compute_tab_bar_view(&app.workspaces[0], app.view.tab_bar_rect, 0, true, false);
        app.view.tab_hit_areas = view.tab_hit_areas;

        let backend = TestBackend::new(30, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_tab_bar(&app, frame, app.view.tab_bar_rect))
            .unwrap();

        let rect = app.view.tab_hit_areas[0];
        let buffer = terminal.backend().buffer();
        let cell: String = (rect.x..rect.x + rect.width)
            .map(|x| buffer[(x, rect.y)].symbol())
            .collect();
        assert_eq!(cell, "  omarchy  ");
    }

    #[test]
    fn active_auto_named_tab_keeps_readable_weight() {
        let mut app = AppState::test_new();
        let mut ws = Workspace::test_new("test");
        // a lone auto-named tab draws no chip at all, so there has to be a choice
        ws.test_add_tab(None);

        app.workspaces = vec![ws];
        app.active = Some(0);
        app.view.tab_bar_rect = Rect::new(0, 0, 30, 1);
        let view = compute_tab_bar_view(&app.workspaces[0], app.view.tab_bar_rect, 0, true, false);
        app.view.tab_hit_areas = view.tab_hit_areas;

        let backend = TestBackend::new(30, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_tab_bar(&app, frame, app.view.tab_bar_rect))
            .unwrap();

        let tab_rect = app.view.tab_hit_areas[0];
        let style = terminal.backend().buffer()[(tab_rect.x + 1, tab_rect.y)].style();

        assert_eq!(style.bg, Some(app.palette.accent));
        assert!(!style.add_modifier.contains(Modifier::DIM));
        assert!(!style.add_modifier.contains(Modifier::BOLD));
    }

    /// A lone tab is not a choice, so its chip says only what its number already
    /// implies. The bar itself stays: it carries the usage figures, and `+` is still
    /// something to press.
    #[test]
    fn a_lone_numbered_tab_leaves_the_bar_to_the_plus() {
        let mut app = AppState::test_new();
        app.workspaces = vec![Workspace::test_new("test")];
        app.active = Some(0);
        app.view.tab_bar_rect = Rect::new(0, 0, 30, 1);
        let view = compute_tab_bar_view(&app.workspaces[0], app.view.tab_bar_rect, 0, true, true);
        app.view.tab_hit_areas = view.tab_hit_areas;
        app.view.new_tab_hit_area = view.new_tab_hit_area;

        let row = render_bar(&app, 30);

        assert_eq!(row.trim(), "+", "bar row: {row:?}");
        // no chip means no way to click one, so the hit area has to go with it
        assert_eq!(app.view.tab_hit_areas[0].width, 0);

        // a name is different: you gave it one for a reason
        app.workspaces[0].tabs[0].set_custom_name("logs".into());
        let view = compute_tab_bar_view(&app.workspaces[0], app.view.tab_bar_rect, 0, true, true);
        app.view.tab_hit_areas = view.tab_hit_areas;
        app.view.new_tab_hit_area = view.new_tab_hit_area;

        assert!(render_bar(&app, 30).contains("logs"));
    }

    /// A tab you cannot see is a tab you cannot switch to. The inactive chip is the
    /// one that breaks: the active tab is filled with the accent and obvious in any
    /// theme, while the inactive one used to be muted text with DIM on top, sitting
    /// on a `surface0` that the terminal theme leaves to the terminal — so it had no
    /// chip and near-invisible ink.
    #[test]
    fn every_tab_stays_legible_on_its_own_chip() {
        const INK_FLOOR: f32 = 2.5;
        const CHIP_FLOOR: f32 = 1.1;
        let mut checked = 0;

        for name in crate::ui::status::THEME_NAMES {
            let p = crate::app::state::Palette::from_name(name).expect("named theme resolves");
            for auto_named in [true, false] {
                for active in [true, false] {
                    let style = tab_chip_style(active, auto_named, tab_chip_bg(&p), &p);
                    let (Some(ink), Some(chip)) = (style.fg, style.bg) else {
                        panic!("{name}: a tab always sets both colours");
                    };
                    assert!(!style.add_modifier.contains(Modifier::DIM));
                    if let Some(ratio) = crate::ui::status::contrast_ratio(ink, chip) {
                        checked += 1;
                        assert!(
                            ratio >= INK_FLOOR,
                            "{name}: {}{} label on its chip is {ratio:.2}:1",
                            if active { "active " } else { "inactive " },
                            if auto_named { "auto-named" } else { "named" }
                        );
                    }
                    // the chip has to separate from the bar it sits in, or the tab
                    // has no edges and reads as loose text
                    if let Some(ratio) = crate::ui::status::contrast_ratio(chip, p.panel_bg) {
                        assert!(
                            ratio >= CHIP_FLOOR,
                            "{name}: chip on the bar is {ratio:.2}:1"
                        );
                    }
                }
            }
        }

        assert!(checked > 50, "only {checked} pairs were resolvable");
    }

    #[test]
    fn zoom_marker_counts_toward_tab_width() {
        let mut ws = Workspace::test_new("test");
        ws.tabs[0].set_custom_name("abcdefgh".into());
        ws.tabs[0].zoomed = true;

        assert_eq!(tab_width(&ws, 0), 14);
    }

    #[test]
    fn tab_width_uses_display_width_for_cjk_labels() {
        let mut ws = Workspace::test_new("test");
        ws.tabs[0].set_custom_name("提交 herdr 的反馈".into());

        assert_eq!(
            tab_width(&ws, 0),
            display_width_u16("提交 herdr 的反馈") + 4
        );
    }

    #[test]
    fn tab_bar_renders_trailing_cjk_character() {
        let mut app = AppState::test_new();
        let mut ws = Workspace::test_new("test");
        ws.tabs[0].set_custom_name("提交 herdr 的反馈".into());

        app.active = Some(0);
        app.workspaces = vec![ws];
        app.view.tab_bar_rect = Rect::new(0, 0, 30, 1);
        let view = compute_tab_bar_view(&app.workspaces[0], app.view.tab_bar_rect, 0, true, false);
        app.view.tab_hit_areas = view.tab_hit_areas;

        let backend = TestBackend::new(30, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_tab_bar(&app, frame, app.view.tab_bar_rect))
            .unwrap();

        let row = buffer_row_text(terminal.backend().buffer(), app.view.tab_bar_rect, 0);
        assert!(row.contains('馈'), "tab row: {row:?}");
    }
}
