use super::*;
use crate::protocol::{
    ClientShellClaudeUsage, ClientShellTabStatusSegment, ClientShellUsageWindow,
};

// the render path reads the wall clock, so a reset has to outlive the test suite itself
const FAR_FUTURE: i64 = 4_910_000_000;

fn window(label: &str, percent: u8, resets_at: Option<i64>) -> ClientShellUsageWindow {
    ClientShellUsageWindow {
        label: label.to_owned(),
        percent,
        resets_at,
    }
}

fn with_usage(
    windows: Vec<ClientShellUsageWindow>,
    seven_day_tokens: Option<u64>,
) -> ClientShellSnapshot {
    let mut projected = snapshot();
    projected.claude_usage = Some(ClientShellClaudeUsage {
        windows,
        seven_day_tokens,
    });
    projected
}

fn shell_with(projected: ClientShellSnapshot) -> ClientShellState {
    let mut config = ClientShellConfig::from_config(&Config::default());
    config.mobile_width_threshold = 0;
    let mut state = ClientShellState::new(config);
    state.set_snapshot(Box::new(projected));
    state.set_pane_surface(surface());
    state
}

fn bar_row(state: &mut ClientShellState, cols: u16) -> String {
    let frame = state.compose(cols, 20).expect("tab bar frame");
    frame_rows(&frame)[0].trim_end().to_owned()
}

fn plain_windows() -> Vec<ClientShellUsageWindow> {
    vec![window("session", 3, None), window("weekly_all", 6, None)]
}

#[test]
fn the_bar_carries_claude_usage_in_the_end_the_tabs_never_reach() {
    let mut state = shell_with(with_usage(plain_windows(), None));

    let row = bar_row(&mut state, 106);

    assert!(
        row.ends_with("\u{f51b} 5h 3% │ week 6%"),
        "bar row: {row:?}"
    );
}

#[test]
fn the_bar_carries_the_weeks_tokens_beside_the_limits() {
    let mut state = shell_with(with_usage(plain_windows(), Some(31_000_000)));

    let row = bar_row(&mut state, 106);

    assert!(row.ends_with("week 6% │ 7d 31.0M"), "bar row: {row:?}");
}

// without the comma the countdown ran straight into the last percentage
#[test]
fn a_countdown_joins_its_group_on_a_comma() {
    let mut state = shell_with(with_usage(
        vec![
            window("weekly_all", 24, Some(FAR_FUTURE)),
            window("Fable", 7, Some(FAR_FUTURE)),
        ],
        None,
    ));

    let row = bar_row(&mut state, 106);

    assert!(row.contains("week 24%, fable 7%, "), "bar row: {row:?}");
    assert!(row.ends_with("h \u{f017}"), "bar row: {row:?}");
}

// the old bar drew over the configured status; the two now share the end of the bar
#[test]
fn usage_stays_clear_of_the_configured_status() {
    let mut projected = with_usage(plain_windows(), None);
    projected.tab_bar_right = vec![ClientShellTabStatusSegment {
        text: "host".into(),
        accent: false,
    }];
    let mut state = shell_with(projected);

    let row = bar_row(&mut state, 106);

    assert!(row.ends_with("week 6% host"), "bar row: {row:?}");
}

// dropping the figures is the only answer that does not push a tab off the edge
#[test]
fn usage_gives_way_rather_than_crowding_the_tabs() {
    let mut projected = with_usage(vec![window("session", 3, Some(FAR_FUTURE))], None);
    // a named tab, because a lone numbered one draws no chip to crowd
    projected.tabs[0].label = "one".into();
    projected.tabs[0].custom_label = true;
    let mut state = shell_with(projected);

    let row = bar_row(&mut state, 40);

    assert_eq!(state.hits.tabs.len(), 1, "the tab bar is still drawn");
    assert!(!row.contains('%'), "bar row: {row:?}");
    // and a segment nobody sees asks for no repaints
    assert_eq!(state.hits.claude_usage_repaint_at, None);
}

// the client has no once-a-minute repaint, so the countdown asks for one when its wording moves
#[test]
fn the_countdown_repaints_only_when_its_wording_would_change() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock after the epoch")
        .as_secs();
    // reads "2h" until fewer than two whole hours are left
    let resets_at = now as i64 + 2 * 3600 + 30;
    let mut state = shell_with(with_usage(
        vec![window("session", 3, Some(resets_at))],
        None,
    ));

    let row = bar_row(&mut state, 106);

    assert!(row.contains("5h 3%, 2h \u{f017}"), "bar row: {row:?}");
    let at = |secs: i64| std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs as u64);
    let change = resets_at - 2 * 3600 + 1;
    assert_eq!(state.hits.claude_usage_repaint_at, Some(at(change)));
    assert!(!state.tick_claude_usage_countdown(at(change - 1)));
    assert!(state.tick_claude_usage_countdown(at(change)));
    // once: the compose it asks for works out the next one
    assert!(!state.tick_claude_usage_countdown(at(change)));
}

#[test]
fn a_segment_without_a_reset_asks_for_no_repaints() {
    let mut state = shell_with(with_usage(plain_windows(), Some(31_000_000)));

    assert!(bar_row(&mut state, 106).contains("5h 3%"));
    assert_eq!(state.hits.claude_usage_repaint_at, None);
}

// the token figure alone is not worth a segment; the windows are what it sits beside
#[test]
fn a_tally_without_windows_draws_nothing() {
    let mut state = shell_with(with_usage(Vec::new(), Some(31_000_000)));

    assert!(!bar_row(&mut state, 106).contains("7d"));
}
