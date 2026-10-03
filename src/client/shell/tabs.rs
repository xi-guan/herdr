use super::*;
use crate::protocol::ClientShellUsageWindow;

const TAB_SCROLL_BUTTON_WIDTH: u16 = 3;
const MIN_TAB_STRIP_WIDTH: u16 =
    MIN_TAB_WIDTH + NEW_TAB_WIDTH + TAB_SCROLL_BUTTON_WIDTH.saturating_mul(2);

pub(crate) fn render_tab_bar(
    buffer: &mut Buffer,
    area: Rect,
    snapshot: &ClientShellSnapshot,
    config: &ClientShellConfig,
    tab_scroll: &mut usize,
    reveal_focused_tab: &mut bool,
    tab_drag_insert_index: Option<usize>,
    hits: &mut ShellHitMap,
) {
    let palette = &config.palette;
    buffer.set_style(area, Style::default().bg(palette.panel_bg));
    let tabs = snapshot
        .tabs
        .iter()
        .filter(|tab| Some(tab.workspace_id.as_str()) == snapshot.focused_workspace_id.as_deref())
        .collect::<Vec<_>>();
    let desired_widths = tabs
        .iter()
        .map(|tab| {
            let label = tab_label(tab);
            display_width(&label).saturating_add(4).max(MIN_TAB_WIDTH)
        })
        .collect::<Vec<_>>();
    let content = tab_bar_content_area(snapshot, area);
    let mouse_chrome = config.mouse_capture;
    let new_tab_width = if mouse_chrome { NEW_TAB_WIDTH } else { 0 };
    let desired_total = desired_widths
        .iter()
        .copied()
        .fold(0_u16, u16::saturating_add)
        .saturating_add(tabs.len().saturating_sub(1).min(u16::MAX as usize) as u16)
        .saturating_add(new_tab_width);
    let overflow =
        desired_total > content.width && (!mouse_chrome || content.width >= MIN_TAB_STRIP_WIDTH);
    let available = if overflow && mouse_chrome {
        content
            .width
            .saturating_sub(NEW_TAB_WIDTH)
            .saturating_sub(TAB_SCROLL_BUTTON_WIDTH.saturating_mul(2))
    } else {
        content.width.saturating_sub(new_tab_width)
    };
    let max_scroll = max_tab_scroll(&desired_widths, available);
    if !overflow {
        *tab_scroll = 0;
    } else if *reveal_focused_tab {
        if let Some(focused) = tabs.iter().position(|tab| tab.focused) {
            *tab_scroll = centered_tab_scroll(focused, &desired_widths, available).min(max_scroll);
        }
    } else {
        *tab_scroll = (*tab_scroll).min(max_scroll);
    }
    *reveal_focused_tab = false;

    let mut x = content.x;
    let tab_right = if overflow && mouse_chrome {
        hits.tab_scroll_left = Rect::new(
            content.x,
            content.y,
            TAB_SCROLL_BUTTON_WIDTH.min(content.width),
            1,
        );
        put_text(
            buffer,
            hits.tab_scroll_left.x,
            content.y,
            hits.tab_scroll_left.width,
            " < ",
            Style::default()
                .fg(if *tab_scroll > 0 {
                    palette.overlay1
                } else {
                    palette.overlay0
                })
                .bg(palette.surface0),
        );
        x = hits.tab_scroll_left.right();
        content
            .right()
            .saturating_sub(NEW_TAB_WIDTH + TAB_SCROLL_BUTTON_WIDTH)
    } else {
        content.right().saturating_sub(new_tab_width)
    };

    let mut first_visible = None;
    let mut last_visible = None;
    for (index, tab) in tabs.iter().enumerate().skip(*tab_scroll) {
        let name = tab_label(tab);
        let desired = desired_widths[index];
        let remaining = tab_right.saturating_sub(x);
        let width = desired.min(remaining);
        if width == 0 {
            break;
        }
        let rect = Rect::new(x, area.y, width, 1);
        let style = if tab.focused {
            let base = Style::default()
                .fg(panel_contrast_fg(palette))
                .bg(palette.accent);
            if tab.custom_label {
                base.add_modifier(Modifier::BOLD)
            } else {
                base
            }
        } else if tab.custom_label {
            Style::default().fg(palette.overlay1).bg(palette.surface0)
        } else {
            Style::default().fg(palette.overlay0).bg(palette.surface0)
        };
        let padding = width.saturating_sub(display_width(&name));
        let left = padding / 2;
        let text = format!(
            "{empty:left$}{name}{empty:right_padding$}",
            empty = "",
            left = left as usize,
            right_padding = padding.saturating_sub(left) as usize,
        );
        put_text(buffer, rect.x, rect.y, rect.width, &text, style);
        hits.tabs.push((rect, tab.tab_id.clone()));
        first_visible.get_or_insert(index);
        last_visible = Some(index);
        x = x.saturating_add(width + 1);
        if width < desired {
            break;
        }
    }

    if overflow && mouse_chrome {
        hits.tab_scroll_right = Rect::new(tab_right, area.y, TAB_SCROLL_BUTTON_WIDTH, 1);
        let can_scroll_right = *tab_scroll < max_scroll;
        put_text(
            buffer,
            hits.tab_scroll_right.x,
            area.y,
            hits.tab_scroll_right.width,
            " > ",
            Style::default()
                .fg(if can_scroll_right {
                    palette.overlay1
                } else {
                    palette.overlay0
                })
                .bg(palette.surface0),
        );
        hits.new_tab = Rect::new(
            hits.tab_scroll_right.right(),
            area.y,
            content
                .right()
                .saturating_sub(hits.tab_scroll_right.right())
                .min(NEW_TAB_WIDTH),
            1,
        );
    } else if mouse_chrome {
        hits.new_tab = Rect::new(
            x.min(content.right()),
            area.y,
            content.right().saturating_sub(x).min(NEW_TAB_WIDTH),
            1,
        );
    }
    if mouse_chrome {
        put_text(
            buffer,
            hits.new_tab.x,
            area.y,
            hits.new_tab.width,
            " + ",
            Style::default().fg(palette.overlay1).bg(palette.panel_bg),
        );
    }

    if first_visible.is_some_and(|index| index > 0) {
        let ellipsis_x = if hits.tab_scroll_left.width > 0 {
            hits.tab_scroll_left.right()
        } else {
            content.x
        };
        put_text(
            buffer,
            ellipsis_x,
            area.y,
            u16::from(ellipsis_x < content.right()),
            "…",
            Style::default().fg(palette.overlay0),
        );
    }
    if last_visible.is_some_and(|index| index + 1 < tabs.len()) {
        let ellipsis_x = if hits.tab_scroll_right.width > 0 {
            hits.tab_scroll_right.x.saturating_sub(1)
        } else {
            content.right().saturating_sub(1)
        };
        put_text(
            buffer,
            ellipsis_x,
            area.y,
            u16::from(ellipsis_x >= content.x && ellipsis_x < content.right()),
            "…",
            Style::default().fg(palette.overlay0),
        );
    }

    if let Some(insert_index) = tab_drag_insert_index {
        if let Some(indicator_x) = tab_drop_indicator_x(hits, &tabs, insert_index) {
            put_text(
                buffer,
                indicator_x.min(content.right().saturating_sub(1)),
                area.y,
                1,
                "│",
                Style::default().fg(palette.accent),
            );
        }
    }
    render_claude_usage(buffer, area, snapshot, palette, hits);
    render_tab_bar_status(buffer, area, snapshot, palette);
}

pub(crate) fn tab_bar_status_width(snapshot: &ClientShellSnapshot) -> u16 {
    let content = snapshot.tab_bar_right.iter().fold(0u16, |width, segment| {
        width.saturating_add(display_width(&segment.text))
    });
    let separators = snapshot.tab_bar_right.len().saturating_sub(1);
    content.saturating_add(
        display_width(&snapshot.tab_bar_right_separator)
            .saturating_mul(separators.min(u16::MAX as usize) as u16),
    )
}

fn tab_bar_status_area(snapshot: &ClientShellSnapshot, area: Rect) -> Option<Rect> {
    let width = tab_bar_status_width(snapshot);
    if width == 0 {
        return None;
    }
    let reserved = width.saturating_add(1);
    (area.width.saturating_sub(reserved) >= MIN_TAB_STRIP_WIDTH)
        .then(|| Rect::new(area.right().saturating_sub(width), area.y, width, 1))
}

fn tab_bar_content_area(snapshot: &ClientShellSnapshot, area: Rect) -> Rect {
    let reserved = tab_bar_status_area(snapshot, area)
        .map(|status| status.width.saturating_add(1))
        .unwrap_or(0);
    Rect {
        width: area.width.saturating_sub(reserved),
        ..area
    }
}

fn render_tab_bar_status(
    buffer: &mut Buffer,
    area: Rect,
    snapshot: &ClientShellSnapshot,
    palette: &Palette,
) {
    let Some(status) = tab_bar_status_area(snapshot, area) else {
        return;
    };
    let separator_width = display_width(&snapshot.tab_bar_right_separator);
    let mut x = status.x;
    for (index, segment) in snapshot.tab_bar_right.iter().enumerate() {
        if index > 0 && separator_width > 0 {
            put_text(
                buffer,
                x,
                area.y,
                separator_width,
                &snapshot.tab_bar_right_separator,
                Style::default().fg(palette.overlay0).bg(palette.panel_bg),
            );
            x = x.saturating_add(separator_width);
        }
        let width = display_width(&segment.text);
        let style = if segment.accent {
            Style::default()
                .fg(panel_contrast_fg(palette))
                .bg(palette.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(palette.overlay1).bg(palette.panel_bg)
        };
        put_text(buffer, x, area.y, width, &segment.text, style);
        x = x.saturating_add(width);
    }
}

// nerd font glyphs: octicons' filled sparkle marks whose figures these are, a clock the countdown
const USAGE_MARK: &str = "\u{f51b}";
const RESET_MARK: &str = "\u{f017}";

fn usage_window_label(label: &str) -> String {
    match label {
        "session" => "5h".to_owned(),
        "weekly_all" => "week".to_owned(),
        // nothing else on the bar is capitalised, and one capital among figures reads as emphasis
        other => other.to_lowercase(),
    }
}

fn resets_in(resets_at: i64, now: u64) -> Option<u64> {
    u64::try_from(resets_at.checked_sub(i64::try_from(now).ok()?)?).ok()
}

// minutes while it is worth waiting out, whole hours after; days would be a second unit to read
fn reset_label(left: u64) -> String {
    if left >= 3600 {
        format!("{}h", left / 3600)
    } else {
        format!("{}m", left / 60)
    }
}

// seconds until `reset_label(left)` would read differently
fn reset_label_changes_in(left: u64) -> u64 {
    left % if left >= 3600 { 3600 } else { 60 } + 1
}

// grouped on the countdown as written: the endpoint stamps windows that reset together apart
fn usage_groups(
    windows: &[ClientShellUsageWindow],
    now: u64,
) -> Vec<(Vec<&ClientShellUsageWindow>, Option<String>)> {
    let mut groups: Vec<(Vec<&ClientShellUsageWindow>, Option<String>)> = Vec::new();
    for window in windows {
        let reset = window
            .resets_at
            .and_then(|at| resets_in(at, now))
            .map(reset_label);
        match groups
            .last_mut()
            // an absent reset is not a time two windows can have in common
            .filter(|(_, last)| reset.is_some() && *last == reset)
        {
            Some((group, _)) => group.push(window),
            None => groups.push((vec![window], reset)),
        }
    }
    groups
}

// three figures: nobody acts on 5.81B against 5.82B, and the digits would cost a tab its place
fn compact_tokens(tokens: u64) -> String {
    match tokens {
        ..1_000 => tokens.to_string(),
        1_000..1_000_000 => format!("{:.1}k", tokens as f64 / 1e3),
        1_000_000..1_000_000_000 => format!("{:.1}M", tokens as f64 / 1e6),
        _ => format!("{:.1}B", tokens as f64 / 1e9),
    }
}

// quiet until nearly spent: a figure loud at 3% teaches you to stop reading it
fn usage_percent_color(percent: u8, palette: &Palette) -> ratatui::style::Color {
    match percent {
        90.. => palette.red,
        75..=89 => palette.peach,
        _ => palette.overlay0,
    }
}

fn render_claude_usage(
    buffer: &mut Buffer,
    area: Rect,
    snapshot: &ClientShellSnapshot,
    palette: &Palette,
    hits: &mut ShellHitMap,
) {
    let Some(usage) = snapshot
        .claude_usage
        .as_ref()
        .filter(|usage| !usage.windows.is_empty())
    else {
        return;
    };
    let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return;
    };
    let now = now.as_secs();
    let quiet = Style::default().fg(palette.overlay0);
    let divider = Style::default().fg(palette.surface1);
    // the brand's colour is what makes a mark this thin read as whose figures these are
    let mut segments = vec![(format!("{USAGE_MARK} "), Style::default().fg(palette.peach))];
    for (index, (group, reset)) in usage_groups(&usage.windows, now).into_iter().enumerate() {
        if index > 0 {
            segments.push((" │ ".to_owned(), divider));
        }
        for (position, window) in group.into_iter().enumerate() {
            if position > 0 {
                segments.push((", ".to_owned(), quiet));
            }
            segments.push((format!("{} ", usage_window_label(&window.label)), quiet));
            segments.push((
                format!("{}%", window.percent),
                Style::default().fg(usage_percent_color(window.percent, palette)),
            ));
        }
        // on the windows' own comma, so it reads as one more fact about the group
        if let Some(reset) = reset {
            segments.push((format!(", {reset} {RESET_MARK}"), quiet));
        }
    }
    // a rolling seven days rather than the endpoint's week, so it stands apart from `week`
    if let Some(tokens) = usage.seven_day_tokens {
        segments.push((" │ ".to_owned(), divider));
        segments.push((format!("7d {}", compact_tokens(tokens)), quiet));
    }

    let width = segments.iter().fold(0u16, |width, (text, _)| {
        width.saturating_add(display_width(text))
    });
    // the tabs own the bar: the figures take what is left and give way rather than push a tab off
    let taken = hits
        .tabs
        .iter()
        .map(|(rect, _)| *rect)
        .chain([hits.new_tab, hits.tab_scroll_right])
        .filter(|rect| rect.width > 0)
        .map(|rect| rect.right())
        .max()
        .unwrap_or(area.x);
    // one blank cell before the configured status, or before the edge when there is none
    let right = tab_bar_status_area(snapshot, area)
        .map_or(area.right(), |status| status.x)
        .saturating_sub(1);
    let Some(mut x) = right.checked_sub(width).filter(|x| *x > taken) else {
        return;
    };
    for (text, style) in &segments {
        let text_width = display_width(text);
        put_text(buffer, x, area.y, text_width, text, *style);
        x = x.saturating_add(text_width);
    }
    hits.claude_usage_repaint_at = usage
        .windows
        .iter()
        .filter_map(|window| resets_in(window.resets_at?, now))
        .map(reset_label_changes_in)
        .min()
        .map(|wait| {
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(now.saturating_add(wait))
        });
}

fn tab_drop_indicator_x(
    hits: &ShellHitMap,
    tabs: &[&ClientShellTab],
    insert_index: usize,
) -> Option<u16> {
    let visible = hits
        .tabs
        .iter()
        .filter_map(|(rect, tab_id)| {
            tabs.iter()
                .position(|tab| tab.tab_id == *tab_id)
                .map(|index| (index, *rect))
        })
        .collect::<Vec<_>>();
    let (first_index, first_rect) = *visible.first()?;
    let (last_index, last_rect) = *visible.last()?;
    if insert_index == 0 {
        return Some(if first_index == 0 {
            first_rect.x
        } else {
            hits.tab_scroll_left.right()
        });
    }
    if let Some((_, rect)) = visible.iter().find(|(index, _)| *index == insert_index) {
        return Some(rect.x.saturating_sub(1));
    }
    if insert_index >= tabs.len() {
        return Some(if last_index + 1 >= tabs.len() {
            last_rect.right()
        } else {
            hits.tab_scroll_right.x.saturating_sub(1)
        });
    }
    None
}

fn centered_tab_scroll(focused: usize, widths: &[u16], available: u16) -> usize {
    let mut best = focused;
    let mut best_distance = u16::MAX;
    for start in 0..=focused {
        let before = widths
            .iter()
            .copied()
            .enumerate()
            .skip(start)
            .take(focused.saturating_sub(start))
            .fold(0u16, |width, (_, tab)| width.saturating_add(tab + 1));
        if before >= available {
            continue;
        }
        let focused_width = widths[focused].min(available.saturating_sub(before));
        let center = before.saturating_mul(2).saturating_add(focused_width);
        let distance = center.abs_diff(available);
        if distance <= best_distance {
            best_distance = distance;
            best = start;
        }
    }
    best
}

fn max_tab_scroll(widths: &[u16], available: u16) -> usize {
    let Some((&last, preceding)) = widths.split_last() else {
        return 0;
    };
    let mut start = preceding.len();
    let mut used = u32::from(last);
    // Keep the longest fully visible suffix, not merely a sliver of the last tab.
    // An oversized last tab must still be reachable at the start of the strip.
    for width in preceding.iter().rev() {
        let required = used + 1 + u32::from(*width);
        if required > u32::from(available) {
            break;
        }
        used = required;
        start -= 1;
    }
    start
}

fn tab_label(tab: &ClientShellTab) -> String {
    if tab.zoomed {
        format!("{} Z", tab.label)
    } else {
        tab.label.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::max_tab_scroll;
    use super::{
        compact_tokens, reset_label, reset_label_changes_in, resets_in, usage_groups,
        usage_percent_color, usage_window_label, ClientShellUsageWindow,
    };

    // 2026-08-01T12:00:00Z; every helper below takes the clock as an argument
    const NOON: u64 = 1_785_585_600;

    fn window(label: &str, percent: u8, resets_at: Option<i64>) -> ClientShellUsageWindow {
        ClientShellUsageWindow {
            label: label.to_owned(),
            percent,
            resets_at,
        }
    }

    #[test]
    fn a_reset_is_worded_at_the_precision_the_wait_deserves() {
        assert_eq!(reset_label(22 * 60), "22m");
        assert_eq!(reset_label(0), "0m");
        assert_eq!(reset_label(3 * 3600 + 40 * 60), "3h");
        // a weekly window too: hours are the one unit, however many there are
        assert_eq!(reset_label(3 * 86_400), "72h");
    }

    // the bar has no clock of its own, so it repaints exactly when the wording would move
    #[test]
    fn a_countdown_repaints_the_moment_its_wording_would_change() {
        for left in [
            0u64,
            59,
            60,
            61,
            22 * 60,
            3599,
            3600,
            3601,
            13_200,
            3 * 86_400,
        ] {
            let wait = reset_label_changes_in(left);
            assert_eq!(reset_label(left - (wait - 1)), reset_label(left), "{left}");
            assert!(
                left < wait || reset_label(left - wait) != reset_label(left),
                "{left}"
            );
        }
    }

    // a wrapped-around number would read as a full window ahead of you
    #[test]
    fn a_window_that_already_reset_reports_nothing() {
        let now = NOON as i64;

        assert_eq!(resets_in(now - 60, NOON), None);
        assert_eq!(resets_in(now + 22 * 60, NOON), Some(22 * 60));
    }

    // the endpoint stamps windows that reset together apart, and each printed its own countdown
    #[test]
    fn windows_that_reset_together_share_one_countdown() {
        let now = NOON as i64;
        let windows = vec![
            window("session", 4, Some(now + 3 * 3600)),
            window("weekly_all", 10, Some(now + 3 * 86_400)),
            window("Fable", 2, Some(now + 3 * 86_400 + 1)),
        ];

        let groups = usage_groups(&windows, NOON);

        assert_eq!(groups.len(), 2, "the weeklies belong together");
        assert_eq!(groups[0].1.as_deref(), Some("3h"));
        assert_eq!(groups[1].0.len(), 2);
        assert_eq!(groups[1].1.as_deref(), Some("72h"));
        // the endpoint capitalises a model name; nothing else on the bar is
        assert_eq!(usage_window_label(&groups[1].0[1].label), "fable");
    }

    #[test]
    fn a_window_without_a_reset_stands_on_its_own() {
        let windows = vec![window("session", 4, None), window("weekly_all", 10, None)];

        let groups = usage_groups(&windows, NOON);

        assert_eq!(groups.len(), 2);
        assert!(groups.iter().all(|(_, reset)| reset.is_none()));
    }

    #[test]
    fn the_plain_windows_get_the_short_names() {
        assert_eq!(usage_window_label("session"), "5h");
        assert_eq!(usage_window_label("weekly_all"), "week");
        assert_eq!(usage_window_label("weekly_scoped"), "weekly_scoped");
    }

    #[test]
    fn a_token_count_keeps_three_figures_and_no_more() {
        assert_eq!(compact_tokens(0), "0");
        assert_eq!(compact_tokens(999), "999");
        assert_eq!(compact_tokens(12_300), "12.3k");
        assert_eq!(compact_tokens(83_167_217), "83.2M");
        assert_eq!(compact_tokens(5_809_127_363), "5.8B");
    }

    #[test]
    fn a_usage_window_only_takes_a_colour_once_it_is_nearly_spent() {
        let p = crate::app::state::Palette::from_name("catppuccin").expect("theme resolves");

        assert_eq!(usage_percent_color(3, &p), p.overlay0);
        assert_eq!(usage_percent_color(74, &p), p.overlay0);
        assert_eq!(usage_percent_color(75, &p), p.peach);
        assert_eq!(usage_percent_color(89, &p), p.peach);
        assert_eq!(usage_percent_color(90, &p), p.red);
        assert_eq!(usage_percent_color(95, &p), p.red);
    }

    #[test]
    fn trailing_scroll_limit_accounts_for_full_widths_and_separators() {
        for (widths, available, expected) in [
            (&[][..], 0, 0),
            (&[8, 13][..], 0, 1),
            (&[8, 13][..], 1, 1),
            (&[8, 13][..], 12, 1),
            (&[8, 13][..], 21, 1),
            (&[8, 13][..], 22, 0),
            (&[8, 13][..], 30, 0),
            (&[8, u16::MAX][..], u16::MAX, 1),
        ] {
            assert_eq!(
                max_tab_scroll(widths, available),
                expected,
                "widths={widths:?}, available={available}"
            );
        }
    }
}
