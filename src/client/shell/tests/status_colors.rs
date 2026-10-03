use super::*;

const STATUSES: [AgentStatus; 5] = [
    AgentStatus::Blocked,
    AgentStatus::Working,
    AgentStatus::Done,
    AgentStatus::Idle,
    AgentStatus::Unknown,
];

#[test]
fn every_status_colour_stays_visible_on_the_rows_it_marks() {
    const FLOOR: f32 = 1.5;
    let mut checked = 0;

    for name in crate::config::THEME_NAMES {
        let palette = Palette::from_name(name).expect("named theme resolves");
        // unknown is the theme's own overlay0, which this table does not pick
        for status in &STATUSES[..4] {
            let mark = status_color(*status, &palette);
            for (label, background) in [
                ("active row", palette.active_row_bg),
                ("panel", palette.panel_bg),
            ] {
                // an ANSI slot's real colour is the terminal's, so only concrete pairs are measured
                let (ratatui::style::Color::Rgb(..), ratatui::style::Color::Rgb(..)) =
                    (mark, background)
                else {
                    continue;
                };
                let Some(ratio) = crate::color::contrast_ratio(mark, background) else {
                    continue;
                };
                checked += 1;
                assert!(
                    ratio >= FLOOR,
                    "{name}: {status:?} on the {label} is {ratio:.2}:1"
                );
            }
        }
    }

    assert!(checked > 50, "only {checked} pairs were resolvable");
}

// the fork settled idle on the dim wash; grounding it on the active row darkened it to surface1
#[test]
fn idle_keeps_the_forks_grey_in_the_terminal_theme() {
    // the owner's terminal theme tiers, from [theme.custom]
    let mut palette = Palette::terminal();
    palette.surface1 = ratatui::style::Color::Rgb(74, 78, 108);
    palette.surface_dim = ratatui::style::Color::Rgb(47, 49, 65);
    assert_eq!(
        status_color(AgentStatus::Idle, &palette),
        ratatui::style::Color::Rgb(89, 92, 119)
    );
    palette.overlay0 = ratatui::style::Color::Rgb(136, 136, 136);
    assert_eq!(
        status_color(AgentStatus::Idle, &palette),
        ratatui::style::Color::Rgb(82, 85, 112)
    );
}

#[test]
fn no_two_agent_statuses_share_a_colour() {
    for name in crate::config::THEME_NAMES {
        let palette = Palette::from_name(name).expect("named theme resolves");
        // unknown marks "no agent here" and never shares a column with these
        let marks = STATUSES[..4]
            .iter()
            .map(|status| status_color(*status, &palette))
            .collect::<Vec<_>>();

        for first in 0..marks.len() {
            for second in first + 1..marks.len() {
                assert_ne!(
                    marks[first], marks[second],
                    "{name}: {:?} and {:?} share a colour",
                    STATUSES[first], STATUSES[second]
                );
            }
        }
    }
}

#[test]
fn an_agent_wears_one_colour_on_its_border_and_its_row() {
    for name in crate::config::THEME_NAMES {
        let palette = Palette::from_name(name).expect("named theme resolves");
        for status in STATUSES {
            if let Some(border) = crate::color::attention_color(status, &palette) {
                assert_eq!(status_color(status, &palette), border, "{name}: {status:?}");
            }
        }
    }
    let palette = Palette::catppuccin();
    assert_eq!(status_color(AgentStatus::Working, &palette), palette.peach);
    assert_eq!(status_color(AgentStatus::Done, &palette), palette.green);
    assert_eq!(status_color(AgentStatus::Blocked, &palette), palette.red);
}

#[test]
fn mobile_summary_paints_a_finished_agent_from_the_shared_table() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    // the client only calls an agent done once it has watched it work, out of sight
    for (revision, agent_status) in [(1, AgentStatus::Working), (2, AgentStatus::Idle)] {
        let mut projected = snapshot();
        projected.revision = revision;
        projected.agents.push(ClientShellAgent {
            pane_id: "pane_elsewhere".into(),
            workspace_id: "ws_1".into(),
            tab_id: "tab_1".into(),
            name: Some("pi".into()),
            display_agent: Some("pi".into()),
            agent: Some("pi".into()),
            title: None,
            terminal_title: None,
            terminal_title_stripped: None,
            agent_status,
            state_change_seq: revision,
            state_labels: Vec::new(),
            tokens: Vec::new(),
            focused: false,
        });
        state.set_snapshot(Box::new(projected));
    }
    let mut pane_surface = surface();
    pane_surface.projection_revision = 2;
    state.set_pane_surface(pane_surface);

    let frame = state.compose(44, 20).expect("mobile header");
    let buffer = frame.to_ratatui_buffer().expect("frame should reconstruct");
    let area = buffer.area;
    let done = (0..area.height)
        .find_map(|y| {
            (0..area.width.saturating_sub(3))
                .find(|&x| {
                    (0..4)
                        .map(|offset| buffer[(x + offset, y)].symbol())
                        .eq(["d", "o", "n", "e"])
                })
                .map(|x| (x, y))
        })
        .expect("the summary names the finished agent");

    assert_eq!(buffer[done].fg, state.config.palette.green);
}
