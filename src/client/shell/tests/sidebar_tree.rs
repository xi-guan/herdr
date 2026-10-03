use super::*;
use crate::config::StatusIndicatorStyle;

fn agent(pane_id: &str, workspace_id: &str, agent: &str, status: AgentStatus) -> ClientShellAgent {
    ClientShellAgent {
        pane_id: pane_id.into(),
        workspace_id: workspace_id.into(),
        tab_id: "tab_1".into(),
        name: None,
        display_agent: None,
        agent: Some(agent.into()),
        title: None,
        terminal_title: None,
        terminal_title_stripped: None,
        agent_status: status,
        state_change_seq: 0,
        state_labels: Vec::new(),
        tokens: Vec::new(),
        focused: false,
    }
}

/// `count` spaces, each running one agent in `status` on its first tab.
fn spaces_with_agents(count: usize, status: AgentStatus) -> ClientShellSnapshot {
    let mut projected = snapshot();
    let template = projected.workspaces[0].clone();
    projected.workspaces = (1..=count)
        .map(|number| {
            let mut workspace = template.clone();
            workspace.workspace_id = format!("ws_{number}");
            workspace.number = number;
            workspace.label = format!("workspace-{number}");
            workspace.focused = number == 1;
            workspace.agent_status = status;
            workspace
        })
        .collect();
    projected.agents = (1..=count)
        .map(|number| {
            agent(
                &format!("pane_{number}"),
                &format!("ws_{number}"),
                "claude",
                status,
            )
        })
        .collect();
    projected
}

// the owner's agent rows: branch and title ride the name's row, so a nested agent costs one line
fn branch_rows() -> Config {
    use crate::config::AgentSidebarToken;
    let mut config = Config::default();
    config.ui.sidebar.agents.rows[1] = vec![
        AgentSidebarToken::Agent,
        AgentSidebarToken::Branch,
        AgentSidebarToken::TerminalTitleStripped,
    ];
    config
}

fn shell(projected: ClientShellSnapshot, config: Config) -> ClientShellState {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&config));
    state.set_snapshot(Box::new(projected));
    state.set_pane_surface(surface());
    state
}

fn symbol_at(frame: &FrameData, x: u16, y: u16) -> &str {
    frame.cells[usize::from(y) * usize::from(frame.width) + usize::from(x)]
        .symbol
        .as_str()
}

fn bar(status: AgentStatus, frame: u64) -> [String; 2] {
    agent_bar_cells(status, StatusIndicatorStyle::Dots, 2).map(|cell| cell.symbol(frame).into())
}

/// the two cells stack into one column the same width in every state and frame
#[test]
fn the_two_row_indicator_keeps_one_column_in_every_state() {
    let width =
        |cells: [String; 2]| cells.map(|cell| unicode_width::UnicodeWidthStr::width(cell.as_str()));

    let first = bar(AgentStatus::Working, 0);
    let later = bar(AgentStatus::Working, 5);
    assert_ne!(first, later, "a working agent has to turn");
    // every step moves: a repeated frame is the stall the crossings exist to fix
    let cycle = (0..spinner::RING_STEPS)
        .map(|frame| bar(AgentStatus::Working, frame))
        .collect::<Vec<_>>();
    for pair in cycle.windows(2) {
        assert_ne!(pair[0], pair[1], "the comet has to advance every step");
    }
    assert_ne!(
        cycle[spinner::RING_STEPS as usize - 1],
        cycle[0],
        "the loop has to close"
    );
    assert_eq!(width(first), [1, 1]);
    assert_eq!(width(later), [1, 1]);

    for (status, expected) in [
        (AgentStatus::Blocked, ["┃", "┃"]),
        (AgentStatus::Done, ["┃", "┃"]),
        // idle keeps the line and drops its weight
        (AgentStatus::Idle, ["│", "│"]),
        (AgentStatus::Unknown, ["┃", "┃"]),
    ] {
        let held = bar(status, 0);
        assert_eq!(held, bar(status, 9));
        assert_eq!(held, expected.map(str::to_string), "{status:?}");
        assert_eq!(width(held), [1, 1]);
    }
}

/// motion has to mean exactly one thing, and the column is one cell in every state
#[test]
fn only_a_working_agent_turns_and_every_state_stays_one_cell() {
    let width = |glyph: &str| unicode_width::UnicodeWidthStr::width(glyph);
    let glyph = |status, frame| agent_status_glyph(status, StatusIndicatorStyle::Dots, frame);

    let first = glyph(AgentStatus::Working, 0);
    let next = glyph(AgentStatus::Working, 1);
    assert_ne!(first, next);

    for status in [
        AgentStatus::Blocked,
        AgentStatus::Done,
        AgentStatus::Idle,
        AgentStatus::Unknown,
    ] {
        let held = glyph(status, 0);
        assert_eq!(held, glyph(status, 7));
        assert_eq!(width(held), 1, "{status:?}");
    }
    assert_eq!(width(first), 1);
}

/// the symbols style keeps its static glyphs everywhere, so nothing there ever turns
#[test]
fn symbols_style_keeps_the_static_glyphs() {
    for status in [
        AgentStatus::Blocked,
        AgentStatus::Working,
        AgentStatus::Done,
        AgentStatus::Idle,
        AgentStatus::Unknown,
    ] {
        let symbol = status_icon(status, StatusIndicatorStyle::Symbols);
        assert_eq!(
            agent_status_glyph(status, StatusIndicatorStyle::Symbols, 3),
            symbol
        );
        assert_eq!(
            space_status_glyph(status, StatusIndicatorStyle::Symbols),
            symbol
        );
        assert!(!status_turns(status, StatusIndicatorStyle::Symbols));
    }
}

/// the tick is the whole cost of the animation, so it runs only while something turns
#[test]
fn the_spinner_tick_only_runs_while_an_agent_is_working() {
    let collapsed = || {
        let mut config = Config::default();
        config.ui.sidebar_start_collapsed = true;
        config
    };
    let now = std::time::Instant::now();

    let mut idle = shell(spaces_with_agents(1, AgentStatus::Idle), collapsed());
    idle.compose(106, 20).expect("idle sidebar");
    assert!(idle.tick_agent_spinner(now).is_none());
    assert_eq!(idle.agent_spinner_frame, 0);

    let mut working = shell(spaces_with_agents(1, AgentStatus::Working), collapsed());
    working.compose(106, 20).expect("working sidebar");
    assert!(working.tick_agent_spinner(now).is_some());
    assert_eq!(working.agent_spinner_frame, 1);
    // still inside the interval: the frame holds rather than free-running
    assert!(working.tick_agent_spinner(now).is_none());
    assert_eq!(working.agent_spinner_frame, 1);
    assert!(working
        .tick_agent_spinner(now + std::time::Duration::from_millis(200))
        .is_some());
    assert_eq!(working.agent_spinner_frame, 2);
}

/// nothing turning on screen costs no wake-up, no frame advance and no repaint
#[test]
fn a_sidebar_with_nothing_turning_never_repaints_on_the_tick() {
    let now = std::time::Instant::now();
    for (status, indicators) in [
        (AgentStatus::Idle, StatusIndicatorStyle::Dots),
        (AgentStatus::Blocked, StatusIndicatorStyle::Dots),
        // a working agent under the static glyphs has nothing to animate either
        (AgentStatus::Working, StatusIndicatorStyle::Symbols),
    ] {
        let mut config = Config::default();
        config.ui.sidebar_start_collapsed = true;
        config.ui.status_indicators = indicators;
        let mut state = shell(spaces_with_agents(3, status), config);
        state.compose(106, 20).expect("sidebar");

        assert!(state.hits.spinner_cells.is_empty(), "{status:?}");
        for elapsed in [0, 100, 250, 1000] {
            let at = now + std::time::Duration::from_millis(elapsed);
            assert!(state.tick_agent_spinner(at).is_none(), "{status:?}");
        }
        assert_eq!(state.agent_spinner_frame, 0);
        assert_eq!(state.agent_spinner_deadline, None);
        assert_eq!(
            state.timer_delay(now),
            std::time::Duration::from_millis(100)
        );
    }
}

/// a tick rewrites the turning marks alone, and lands where a full compose would
#[test]
fn a_spinner_tick_repaints_only_the_turning_cells() {
    let mut config = Config::default();
    config.ui.sidebar_start_collapsed = true;
    let mut projected = spaces_with_agents(3, AgentStatus::Idle);
    projected.agents[1].agent_status = AgentStatus::Working;
    let mut state = shell(projected, config);
    let before = state.compose(106, 20).expect("sidebar").frame;

    let repaint = state
        .tick_agent_spinner(std::time::Instant::now())
        .expect("one agent turns");
    assert_eq!(repaint.next.len(), 1);
    let row = &repaint.next[0];
    assert_eq!(row.cells.len(), 1);
    assert_eq!(row.cells[0].symbol, spinner::single_cell(1));
    assert_eq!(symbol_at(&before, row.x, row.y), spinner::single_cell(0));
    assert_eq!(
        repaint.shown[0].cells[0],
        before.cells[usize::from(row.y) * 106 + usize::from(row.x)]
    );

    let patched = apply_composed_surface_patch(
        &before,
        ClientComposedSurfacePatch {
            rows: repaint.next,
            cursor: before.cursor.clone(),
        },
    )
    .expect("patch fits");
    let after = state.compose(106, 20).expect("sidebar").frame;
    assert_eq!(patched.cells, after.cells);
}

#[test]
fn collapsed_sidebar_keeps_workspace_status_visible_for_two_digit_positions() {
    let mut config = Config::default();
    config.ui.sidebar_start_collapsed = true;
    let mut state = shell(spaces_with_agents(10, AgentStatus::Unknown), config);
    let frame = state.compose(106, 25).expect("collapsed sidebar");
    let (workspace_area, _, _) =
        render::sidebar::collapsed_sidebar_sections(Rect::new(0, 0, 4, 25));

    let tenth_row = workspace_area.y + 9;
    assert_eq!(symbol_at(&frame, workspace_area.x, workspace_area.y), "1");
    assert_eq!(
        symbol_at(&frame, workspace_area.x + 1, workspace_area.y),
        " "
    );
    // a dash rather than the indicator style's middle dot: this column says "no agent here"
    assert_eq!(
        symbol_at(&frame, workspace_area.x + 2, workspace_area.y),
        "–"
    );
    assert_eq!(symbol_at(&frame, workspace_area.x, tenth_row), "1");
    assert_eq!(symbol_at(&frame, workspace_area.x + 1, tenth_row), "0");
    assert_eq!(symbol_at(&frame, workspace_area.x + 2, tenth_row), "–");
}

#[test]
fn collapsed_sidebar_keeps_status_visible_for_two_digit_positions() {
    let mut config = Config::default();
    config.ui.sidebar_start_collapsed = true;
    let mut state = shell(spaces_with_agents(10, AgentStatus::Unknown), config);
    let frame = state.compose(106, 25).expect("collapsed sidebar");
    let (_, _, detail_area) = render::sidebar::collapsed_sidebar_sections(Rect::new(0, 0, 4, 25));

    let tenth_row = detail_area.y + 9;
    assert_eq!(symbol_at(&frame, detail_area.x, tenth_row), "1");
    assert_eq!(symbol_at(&frame, detail_area.x + 1, tenth_row), "0");
    assert_eq!(symbol_at(&frame, detail_area.x + 2, tenth_row), "–");
}

/// a working agent in the collapsed list turns where the dots style would draw its dot
#[test]
fn collapsed_sidebar_turns_a_working_agent() {
    let mut config = Config::default();
    config.ui.sidebar_start_collapsed = true;
    let mut state = shell(spaces_with_agents(2, AgentStatus::Working), config);
    let frame = state.compose(106, 25).expect("collapsed sidebar");
    let (workspace_area, _, detail_area) =
        render::sidebar::collapsed_sidebar_sections(Rect::new(0, 0, 4, 25));

    // spaces keep their dot; only the agent rows turn
    assert_eq!(
        symbol_at(&frame, workspace_area.x + 2, workspace_area.y),
        "●"
    );
    assert_eq!(
        symbol_at(&frame, detail_area.x + 2, detail_area.y),
        spinner::single_cell(0)
    );
    assert_eq!(state.hits.spinner_cells.len(), 2);
}

/// a space as the fork's `Workspace::test_new` made one: named by hand, nothing running.
fn space(number: usize, label: &str) -> ClientShellWorkspace {
    let mut workspace = snapshot().workspaces[0].clone();
    workspace.workspace_id = format!("ws_{number}");
    workspace.active_tab_id = format!("tab_{number}");
    workspace.number = number;
    workspace.label = label.into();
    workspace.custom_label = true;
    workspace.branch = None;
    workspace.focused = number == 1;
    workspace.agent_status = AgentStatus::Unknown;
    workspace
}

fn tab(number: usize, workspace: usize, label: &str, custom_label: bool) -> ClientShellTab {
    ClientShellTab {
        tab_id: format!("tab_{number}"),
        workspace_id: format!("ws_{workspace}"),
        number,
        label: label.into(),
        custom_label,
        zoomed: false,
        focused: number == 1,
        agent_status: AgentStatus::Unknown,
    }
}

fn pane(number: usize, workspace: usize, tab: usize) -> ClientShellPane {
    ClientShellPane {
        pane_id: format!("pane_{number}"),
        workspace_id: format!("ws_{workspace}"),
        tab_id: format!("tab_{tab}"),
        label: None,
        cwd: Some("/repo".into()),
        foreground_cwd: Some("/repo".into()),
        focused: number == 1,
        right_click_passthrough: false,
    }
}

fn pi(number: usize, workspace: usize, tab: usize) -> ClientShellAgent {
    let mut agent = agent(
        &format!("pane_{number}"),
        &format!("ws_{workspace}"),
        "pi",
        AgentStatus::Unknown,
    );
    agent.tab_id = format!("tab_{tab}");
    agent.focused = number == 1;
    agent
}

/// spaces with one auto-named tab each, the first space focused on its first pane.
fn tree(
    workspaces: Vec<ClientShellWorkspace>,
    agents: Vec<ClientShellAgent>,
) -> ClientShellSnapshot {
    let mut projected = snapshot();
    projected.tabs = workspaces
        .iter()
        .map(|workspace| tab(workspace.number, workspace.number, "1", false))
        .collect();
    projected.panes = agents
        .iter()
        .map(|agent| {
            let number = agent
                .pane_id
                .trim_start_matches("pane_")
                .parse()
                .unwrap_or(1);
            let workspace = agent
                .workspace_id
                .trim_start_matches("ws_")
                .parse()
                .unwrap_or(1);
            let tab = agent.tab_id.trim_start_matches("tab_").parse().unwrap_or(1);
            pane(number, workspace, tab)
        })
        .collect();
    if projected.panes.is_empty() {
        projected.panes = vec![pane(1, 1, 1)];
    }
    projected.workspaces = workspaces;
    projected.agents = agents;
    projected
}

fn worktree(key: &str, linked: bool) -> Option<ClientShellWorktree> {
    Some(ClientShellWorktree {
        key: key.into(),
        label: "herdr".into(),
        is_linked_worktree: linked,
    })
}

fn row_text(frame: &FrameData, y: u16, width: u16) -> String {
    frame_rows(frame)[usize::from(y)]
        .chars()
        .take(usize::from(width))
        .collect::<String>()
        .trim_end()
        .to_owned()
}

fn rendered(frame: &FrameData, rows: u16, width: u16) -> String {
    (0..rows)
        .map(|y| row_text(frame, y, width))
        .collect::<Vec<_>>()
        .join("\n")
}

fn find_symbol_x(frame: &FrameData, y: u16, width: u16, symbol: &str) -> u16 {
    (0..width)
        .find(|x| symbol_at(frame, *x, y) == symbol)
        .unwrap_or_else(|| panic!("missing {symbol:?} in {:?}", row_text(frame, y, width)))
}

fn style_at(frame: &FrameData, x: u16, y: u16) -> ratatui::style::Style {
    frame.to_ratatui_buffer().expect("frame buffer")[(x, y)].style()
}

/// one space, one tab, two panes running the same agent: the default rows render alike.
fn colliding_siblings() -> ClientShellSnapshot {
    tree(vec![space(1, "one")], vec![pi(1, 1, 1), pi(2, 1, 1)])
}

fn compose_tree(
    projected: ClientShellSnapshot,
    config: Config,
    rows: u16,
) -> (ClientShellState, FrameData) {
    let mut state = shell(projected, config);
    let frame = state.compose(106, rows).expect("sidebar frame").frame;
    (state, frame)
}

#[test]
fn both_views_fill_the_sidebar_content_rect() {
    let area = Rect::new(0, 0, 20, 5);

    assert_eq!(
        super::super::sidebar_tree::sidebar_content_rect(area),
        Rect::new(0, 0, 19, 5)
    );
    assert_eq!(
        super::super::sidebar_tree::sidebar_content_rect(Rect::new(0, 0, 1, 5)),
        Rect::default()
    );
}

#[test]
fn view_tabs_sit_on_the_first_content_row() {
    let [spaces, agents] =
        super::super::sidebar_tree::sidebar_view_tab_rects(Rect::new(0, 0, 26, 5));

    assert_eq!(spaces, Rect::new(0, 0, 7, 1));
    assert_eq!(agents, Rect::new(10, 0, 6, 1));
}

/// a header too narrow for every tab keeps the ones that fit rather than drawing one off its label
#[test]
fn view_tabs_clip_to_the_header_width() {
    let [spaces, agents] =
        super::super::sidebar_tree::sidebar_view_tab_rects(Rect::new(0, 0, 19, 5));
    assert_eq!(spaces, Rect::new(0, 0, 7, 1));
    assert_eq!(agents, Rect::new(10, 0, 6, 1));

    let [spaces, agents] =
        super::super::sidebar_tree::sidebar_view_tab_rects(Rect::new(0, 0, 9, 5));
    assert_eq!(spaces, Rect::new(0, 0, 7, 1));
    assert_eq!(agents.width, 0);
}

/// an ordinal reads as "which one", and stays one cell wide while Unicode still encloses it
#[test]
fn space_ordinals_are_enclosed_while_unicode_still_encloses_them() {
    for (number, expected) in [(1, "①"), (9, "⑨"), (20, "⑳")] {
        let label = super::super::sidebar_tree::space_number_label(number);
        assert_eq!(label, expected);
        assert_eq!(unicode_width::UnicodeWidthStr::width(label.as_str()), 1);
    }
    assert_eq!(super::super::sidebar_tree::space_number_label(21), "21");
    // never truncated: a wrong number is worse than a wider column
    assert_eq!(super::super::sidebar_tree::space_number_label(120), "120");
}

#[test]
fn worktree_group_keeps_its_connectors_while_agent_rows_sit_flush_left() {
    let mut main = space(1, "main");
    main.worktree = worktree("repo-key", false);
    let mut c1 = space(2, "c1");
    c1.worktree = worktree("repo-key", true);
    let mut c2 = space(3, "c2");
    c2.worktree = worktree("repo-key", true);
    let mut config = Config::default();
    config.ui.sidebar.spaces.rows = vec![vec![
        crate::config::SpaceSidebarToken::StateIcon,
        crate::config::SpaceSidebarToken::Workspace,
    ]];
    let (_, frame) = compose_tree(
        tree(
            vec![main, c1, c2],
            vec![pi(1, 1, 1), pi(2, 2, 2), pi(3, 3, 3)],
        ),
        config,
        20,
    );
    let rows = (2..10).map(|y| row_text(&frame, y, 26)).collect::<Vec<_>>();

    // space rows keep the group connectors, since nothing else tells a child from its parent
    assert!(rows[0].starts_with(" – main"), "rows: {rows:#?}");
    assert!(rows[2].starts_with(" ├─ – c1"), "rows: {rows:#?}");
    assert!(rows[4].starts_with(" └─ – c2"), "rows: {rows:#?}");
    // agent rows carry neither, so every one of them starts in the same column
    for row in [&rows[1], &rows[3], &rows[5]] {
        assert!(row.starts_with(" ┃ pi"), "rows: {rows:#?}");
    }
}

/// the space dot aggregates its agents only while their own rows are hidden
#[test]
fn space_state_dot_defers_to_visible_agent_rows() {
    let mut one = space(1, "one");
    one.agent_status = AgentStatus::Working;
    let mut working = pi(1, 1, 1);
    working.agent_status = AgentStatus::Working;

    let (state, frame) = compose_tree(
        tree(vec![one.clone()], vec![working]),
        Config::default(),
        20,
    );
    let row = state.hits.workspaces[0].rect.y;
    // the dot keeps the near column; the ordinal rides the far edge
    let expanded = row_text(&frame, row, 26);
    assert!(expanded.starts_with(" – one"), "expanded: {expanded:?}");
    assert!(expanded.contains('\u{2460}'), "expanded: {expanded:?}");

    // with its agent row gone the space has to speak for it again
    let (state, frame) = compose_tree(tree(vec![one], Vec::new()), Config::default(), 20);
    let bare = row_text(&frame, state.hits.workspaces[0].rect.y, 26);
    assert!(bare.contains('●'), "bare: {bare:?}");
}

/// a space with nothing running hangs its branch in the column an agent's bar would use
#[test]
fn a_space_without_agents_keeps_the_indicator_column_on_its_branch_row() {
    let mut one = space(1, "one");
    one.branch = Some("feature-x".into());
    let (state, frame) = compose_tree(tree(vec![one], Vec::new()), Config::default(), 20);
    assert!(state.hits.agents.is_empty(), "no agent runs here");
    let card = state.hits.workspaces[0].rect;
    assert_eq!(card.height, 2);

    let branch_row = row_text(&frame, card.y + 1, 26);
    assert!(branch_row.starts_with(" │ feature-x"), "{branch_row:?}");
    // the tier the rules and an idle agent use: nothing here wants you
    assert_eq!(
        style_at(&frame, card.x + 1, card.y + 1).fg,
        Some(state.config.palette.surface1)
    );
}

/// the branch rides on the agent rows, and goes back to the space the moment they are hidden
#[test]
fn space_row_drops_the_branch_line_while_agent_rows_carry_it() {
    let mut one = space(1, "one");
    one.branch = Some("feature-x".into());
    // ahead/behind keeps the second row alive after the branch drops, where a stale check shows
    one.git_ahead_behind = Some((1, 0));

    let (state, frame) = compose_tree(
        tree(vec![one.clone()], vec![pi(1, 1, 1)]),
        branch_rows(),
        20,
    );
    let card = state.hits.workspaces[0].rect;
    assert_eq!(card.height, 1);
    let agent_row = row_text(&frame, state.hits.agents[0].0.y, 26);
    assert!(agent_row.contains("feature-x"), "rendered: {agent_row:?}");
    let space_row = row_text(&frame, card.y, 26);
    assert!(!space_row.contains("feature-x"), "space row: {space_row:?}");
    // the ahead count rides up rather than holding a line open by itself
    assert!(space_row.contains("↑1"), "space row: {space_row:?}");

    // with no agent under it there is nowhere else for the branch to live
    let (state, _) = compose_tree(tree(vec![one], Vec::new()), Config::default(), 20);
    assert_eq!(state.hits.workspaces[0].rect.height, 2);
}

#[test]
fn default_agent_rows_remove_redundant_state_text() {
    let mut working = pi(1, 1, 1);
    working.agent_status = AgentStatus::Working;
    let mut state = shell(
        tree(vec![space(1, "one")], vec![working]),
        Config::default(),
    );
    state.sidebar_view = preferences::SidebarView::Agents;
    let frame = state.compose(106, 20).expect("agents view").frame;
    let body = state.hits.agent_body;
    let palette = &state.config.palette;

    let first = row_text(&frame, body.y, 25);
    let second = row_text(&frame, body.y + 1, 25);
    assert!(first.contains("one"));
    assert_eq!(second, "   pi");
    assert!(!first.contains("working"));
    assert!(!second.contains("working"));

    let workspace = style_at(
        &frame,
        find_symbol_x(&frame, body.y, body.width, "o"),
        body.y,
    );
    assert_eq!(workspace.fg, Some(palette.text));
    assert!(workspace.add_modifier.contains(Modifier::BOLD));
    assert!(!workspace.add_modifier.contains(Modifier::DIM));
    assert_eq!(workspace.bg, Some(palette.active_row_bg));

    let agent = style_at(
        &frame,
        find_symbol_x(&frame, body.y + 1, body.width, "p"),
        body.y + 1,
    );
    assert_eq!(agent.fg, Some(palette.overlay0));
    assert!(agent.add_modifier.contains(Modifier::DIM));
    assert!(!agent.add_modifier.contains(Modifier::BOLD));
    assert_eq!(agent.bg, Some(palette.active_row_bg));
}

#[test]
fn default_agent_row_gap_packs_rendering_and_scroll_geometry() {
    let mut claude = pi(2, 2, 2);
    claude.agent = Some("claude".into());
    let mut config = Config::default();
    config.ui.sidebar.agents.rows = vec![vec![crate::config::AgentSidebarToken::Agent]];
    assert_eq!(config.ui.sidebar.agents.row_gap, 0);
    // a 20-cell content rect: the separator column takes the sidebar's last cell
    config.ui.sidebar_width = 21;
    let mut state = shell(
        tree(
            vec![space(1, "one"), space(2, "two")],
            vec![pi(1, 1, 1), claude],
        ),
        config,
    );
    state.sidebar_view = preferences::SidebarView::Agents;

    // 3 header rows + 2 agent rows + the shared footer row
    let frame = state.compose(106, 6).expect("agents view").frame;
    let metrics = state.hits.agent_scroll_metrics.expect("agent metrics");
    let body = state.hits.agent_body;

    assert_eq!(metrics.viewport_rows, 2);
    assert_eq!(metrics.max_offset_from_bottom, 0);
    assert_eq!(row_text(&frame, body.y, body.width), " pi");
    // claude is shortened to its initials on a row
    assert_eq!(row_text(&frame, body.y + 1, body.width), " cc");
}

#[test]
fn identical_sibling_agent_rows_fall_back_to_ordinals() {
    let (_, frame) = compose_tree(colliding_siblings(), Config::default(), 20);
    let rendered = rendered(&frame, 20, 25);

    let rows = rendered
        .lines()
        .filter(|line| line.contains("pi"))
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 2, "rendered:\n{rendered}");
    // the focused row keeps a selection rule in its last column, past the text
    let tail = |row: &str| row.trim_end().trim_end_matches('┃').trim_end().to_string();
    assert!(tail(rows[0]).ends_with("· 1"), "rendered:\n{rendered}");
    assert!(tail(rows[1]).ends_with("· 2"), "rendered:\n{rendered}");
}

#[test]
fn identical_sibling_agent_rows_prefer_pane_labels_over_ordinals() {
    let mut projected = colliding_siblings();
    for (pane, label) in projected.panes.iter_mut().zip(["api", "ui"]) {
        pane.label = Some(label.into());
    }
    let (_, frame) = compose_tree(projected, Config::default(), 20);
    let rendered = rendered(&frame, 20, 25);

    assert!(rendered.contains("api"), "rendered:\n{rendered}");
    assert!(rendered.contains("ui"), "rendered:\n{rendered}");
    assert!(!rendered.contains("pi · 1"), "rendered:\n{rendered}");
}

#[test]
fn distinguishable_sibling_agent_rows_get_no_discriminator() {
    let mut projected = colliding_siblings();
    projected.agents[0].agent = Some("claude".into());
    let (_, frame) = compose_tree(projected, Config::default(), 20);
    let rendered = rendered(&frame, 20, 25);

    assert!(rendered.contains("cc"), "rendered:\n{rendered}");
    assert!(rendered.contains("pi"), "rendered:\n{rendered}");
    assert!(!rendered.contains("pi · 1"), "rendered:\n{rendered}");
    assert!(!rendered.contains("cc · 1"), "rendered:\n{rendered}");
}

#[test]
fn nested_agent_rows_print_the_space_name_once() {
    let mut projected = tree(vec![space(1, "one")], vec![pi(1, 1, 1), pi(2, 1, 2)]);
    projected.tabs = vec![tab(1, 1, "1", false), tab(2, 1, "logs", true)];
    let (_, frame) = compose_tree(projected, Config::default(), 20);
    let rendered = rendered(&frame, 20, 25);

    assert_eq!(rendered.matches("one").count(), 1, "rendered:\n{rendered}");
    assert_eq!(rendered.matches("pi").count(), 2, "rendered:\n{rendered}");
    // agent rows hang on indentation alone
    assert!(!rendered.contains("├"), "rendered:\n{rendered}");
    assert!(!rendered.contains("└"), "rendered:\n{rendered}");
}

/// a worktree child is named after its branch, which must not print again right beneath it
#[test]
fn worktree_child_agents_drop_a_branch_that_repeats_the_space_name() {
    let mut herdr = space(1, "herdr");
    herdr.worktree = worktree("repo-key", true);
    herdr.branch = Some("release".into());
    let mut child = space(2, "feat-x");
    child.worktree = worktree("repo-key", true);
    child.branch = Some("feat-x".into());
    let mut config = branch_rows();
    config.ui.sidebar_width = 30;
    let mut state = shell(
        tree(vec![herdr, child], vec![pi(1, 1, 1), pi(2, 2, 2)]),
        config,
    );
    let frame = state.compose(110, 20).expect("sidebar").frame;
    let rows = &state.hits.agents;

    let parent_agent = row_text(&frame, rows[0].0.y, 30);
    assert!(parent_agent.contains("release"), "parent: {parent_agent:?}");
    let child_agent = row_text(&frame, rows[1].0.y, 30);
    assert!(!child_agent.contains("feat-x"), "child: {child_agent:?}");
    assert!(child_agent.contains("pi"), "child: {child_agent:?}");
}

/// a space boundary separates unrelated work, so its rule outranks the one between agents
#[test]
fn the_rule_between_spaces_outranks_the_rule_between_agents() {
    let mut projected = colliding_siblings();
    projected.workspaces.push(space(2, "two"));
    projected.tabs.push(tab(2, 2, "1", false));
    let mut config = Config::default();
    config.ui.sidebar.spaces.row_gap = 1;
    let (state, frame) = compose_tree(projected, config, 24);
    let palette = &state.config.palette;

    let agent_rows = &state.hits.agents;
    let (x, y) = (agent_rows[0].0.x + 1, agent_rows[0].0.bottom());
    assert_eq!(symbol_at(&frame, x, y), "─");
    let agent_rule = style_at(&frame, x, y);
    assert_eq!(agent_rule.fg, Some(palette.surface1));
    assert!(!agent_rule.add_modifier.contains(Modifier::DIM));

    let second_card = state.hits.workspaces[1].rect;
    assert_eq!(symbol_at(&frame, second_card.x, second_card.y - 1), "─");
    assert_eq!(
        style_at(&frame, second_card.x, second_card.y - 1).fg,
        Some(palette.overlay0)
    );
    assert_ne!(palette.overlay0, palette.surface1);
}

/// flush-left agent rows have no indentation to group them, so a dimmer rule carries the boundary
#[test]
fn sibling_agent_rows_are_separated_by_a_dimmer_rule() {
    let (state, frame) = compose_tree(colliding_siblings(), Config::default(), 20);
    let palette = &state.config.palette;
    let rows = &state.hits.agents;
    let rule_y = rows[0].0.bottom();

    assert!(rule_y < rows[1].0.y, "sibling rows left no gap for a rule");
    // the rule starts in the indicator's column, not left of it
    let x = rows[0].0.x + 1;
    assert_eq!(symbol_at(&frame, x, rule_y), "─");
    assert_eq!(style_at(&frame, x, rule_y).fg, Some(palette.surface1));
    assert_ne!(palette.surface1, palette.overlay0);
}

/// exactly one filled row: once the focused agent has its own row, the space row stays unfilled
#[test]
fn active_space_row_yields_its_fill_to_the_focused_agent_row() {
    let (state, frame) = compose_tree(
        tree(vec![space(1, "one")], vec![pi(1, 1, 1)]),
        Config::default(),
        20,
    );
    let space_row = state.hits.workspaces[0].rect.y;
    let agent_row = state.hits.agents[0].0.y;

    let space = style_at(&frame, find_symbol_x(&frame, space_row, 25, "o"), space_row);
    assert_eq!(space.bg, Some(ratatui::style::Color::Reset));
    let agent = style_at(&frame, find_symbol_x(&frame, agent_row, 25, "p"), agent_row);
    assert_eq!(agent.bg, Some(state.config.palette.surface_dim));
}

#[test]
fn default_space_workspace_style_tracks_active_state() {
    let (state, frame) = compose_tree(
        tree(vec![space(1, "one"), space(2, "two")], Vec::new()),
        Config::default(),
        20,
    );
    let palette = &state.config.palette;
    let first_row = state.hits.workspaces[0].rect.y;
    let second_row = state.hits.workspaces[1].rect.y;

    // spaces take the brightest tier; only weight separates active
    let active = style_at(&frame, find_symbol_x(&frame, first_row, 25, "o"), first_row);
    assert_eq!(active.fg, Some(palette.text));
    assert!(active.add_modifier.contains(Modifier::BOLD));
    assert!(!active.add_modifier.contains(Modifier::DIM));
    assert_eq!(active.bg, Some(palette.active_row_bg));

    let inactive = style_at(
        &frame,
        find_symbol_x(&frame, second_row, 25, "t"),
        second_row,
    );
    assert_eq!(inactive.fg, Some(palette.text));
    assert!(!inactive
        .add_modifier
        .intersects(Modifier::BOLD | Modifier::DIM));
    assert_eq!(inactive.bg, Some(ratatui::style::Color::Reset));
}

#[test]
fn navigate_selection_keeps_its_existing_background_beside_active_workspace() {
    let mut state = shell(
        tree(vec![space(1, "one"), space(2, "two")], Vec::new()),
        Config::default(),
    );
    state.mode = ClientShellMode::Navigate;
    state.navigate_workspace_id = state.navigation_target(&ClientEndpointId::Local, "ws_2");
    let frame = state.compose(106, 20).expect("sidebar").frame;
    let palette = &state.config.palette;
    let active_row = state.hits.workspaces[0].rect.y;
    let selected_row = state.hits.workspaces[1].rect.y;

    assert_eq!(
        style_at(&frame, 0, active_row).bg,
        Some(palette.active_row_bg),
        "active workspace should keep its dedicated background"
    );
    assert_eq!(
        style_at(&frame, 0, selected_row).bg,
        Some(palette.surface0),
        "navigate selection should keep its existing surface0 background"
    );
}

#[test]
fn space_occurrence_style_applies_without_styling_separator() {
    let config: Config = toml::from_str(
        r##"
[ui.sidebar.spaces]
rows = [[{ token = "$hype", fg = "#abcdef", bold = true, dim = false }, "workspace"]]
"##,
    )
    .expect("sidebar config");
    let mut one = space(1, "one");
    one.tokens = vec![("hype".into(), "HI".into())];
    let (state, frame) = compose_tree(tree(vec![one], Vec::new()), config, 20);
    let palette = &state.config.palette;
    let row = state.hits.workspaces[0].rect.y;
    let h = style_at(&frame, find_symbol_x(&frame, row, 25, "H"), row);
    let i = style_at(&frame, find_symbol_x(&frame, row, 25, "I"), row);
    let separator = style_at(&frame, find_symbol_x(&frame, row, 25, "·"), row);

    for style in [h, i] {
        assert_eq!(style.fg, Some(ratatui::style::Color::Rgb(0xab, 0xcd, 0xef)));
        assert!(style.add_modifier.contains(Modifier::BOLD));
        assert!(!style.add_modifier.contains(Modifier::DIM));
        assert_eq!(style.bg, Some(palette.active_row_bg));
    }
    assert_eq!(separator.fg, Some(palette.overlay0));
    assert!(separator.add_modifier.contains(Modifier::DIM));
    assert!(!separator.add_modifier.contains(Modifier::BOLD));
    assert_eq!(separator.bg, Some(palette.active_row_bg));
}

#[test]
fn expanded_sidebar_workspace_rows_lead_with_their_state() {
    let mut one = space(1, "one");
    one.branch = Some("main".into());
    let mut state = shell(tree(vec![one], Vec::new()), Config::default());
    state.mode = ClientShellMode::Navigate;
    state.navigate_workspace_id = state.navigation_target(&ClientEndpointId::Local, "ws_1");
    let frame = state.compose(80, 20).expect("sidebar").frame;
    let card = state.hits.workspaces[0].rect;
    let line = |y: u16| {
        frame_rows(&frame)[usize::from(y)]
            .chars()
            .skip(usize::from(card.x))
            .take(usize::from(card.width))
            .collect::<String>()
    };
    let (line1, line2) = (line(card.y), line(card.y + 1));

    // the dot leads the row; the ordinal rides the far edge
    assert!(line1.starts_with(" – one"), "line1: {line1:?}");
    assert!(line1.contains('\u{2460}'), "line1: {line1:?}");
    // the branch hangs under the name in an agent's bar column; the far column holds the rule
    assert_eq!(line2.trim_end().trim_end_matches('┃').trim_end(), " │ main");
}

/// a working agent's bar turns in place, one row a spinner and two rows the stacked ring
#[test]
fn a_working_agent_turns_its_bar_in_the_tree() {
    let mut working = pi(1, 1, 1);
    working.agent_status = AgentStatus::Working;
    let (state, frame) = compose_tree(
        tree(vec![space(1, "one")], vec![working.clone()]),
        Config::default(),
        20,
    );
    let row = state.hits.agents[0].0;
    assert_eq!(symbol_at(&frame, row.x + 1, row.y), spinner::single_cell(0));
    assert_eq!(state.hits.spinner_cells.len(), 1);

    let mut config = Config::default();
    config.ui.sidebar.agents.rows = vec![
        vec![
            crate::config::AgentSidebarToken::StateIcon,
            crate::config::AgentSidebarToken::Agent,
        ],
        vec![crate::config::AgentSidebarToken::StateText],
    ];
    let (state, frame) = compose_tree(tree(vec![space(1, "one")], vec![working]), config, 20);
    let row = state.hits.agents[0].0;
    let [top, bottom] = spinner::ring_cells(0);
    assert_eq!(symbol_at(&frame, row.x + 1, row.y), top.to_string());
    assert_eq!(symbol_at(&frame, row.x + 1, row.y + 1), bottom.to_string());
    assert_eq!(state.hits.spinner_cells.len(), 2);
}

#[test]
fn clicking_view_tabs_switches_sidebar_views() {
    let mut state = shell(
        tree(vec![space(1, "one")], vec![pi(1, 1, 1)]),
        Config::default(),
    );
    state.compose(106, 20).expect("sidebar");
    let [spaces, agents] =
        super::super::sidebar_tree::sidebar_view_tab_rects(Rect::new(0, 0, 25, 20));
    let click = |state: &mut ClientShellState, column, row| {
        state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::empty(),
        })])
    };

    assert!(click(&mut state, agents.x, agents.y).repaint);
    assert_eq!(state.sidebar_view, preferences::SidebarView::Agents);
    state.compose(106, 20).expect("agents view");
    assert!(state.hits.workspaces.is_empty());

    click(&mut state, spaces.x + 1, spaces.y);
    assert_eq!(state.sidebar_view, preferences::SidebarView::Spaces);
}

/// the tabs are mouse-only, so a view saved without mouse capture would have no way back
#[test]
fn the_agents_view_is_restored_only_with_mouse_capture() {
    for (mouse_capture, expected) in [
        (true, preferences::SidebarView::Agents),
        (false, preferences::SidebarView::Spaces),
    ] {
        let mut config = ClientShellConfig::from_config(&Config::default());
        config.mouse_capture = mouse_capture;
        config.preferences.sidebar_view = Some(preferences::SidebarView::Agents);
        assert_eq!(ClientShellState::new(config).sidebar_view, expected);
    }
}

fn click_at(state: &mut ClientShellState, column: u16, row: u16) -> ClientShellInput {
    state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::empty(),
    })])
}

fn focused_pane_request(outcome: &ClientShellInput) -> Option<&str> {
    outcome.actions.iter().find_map(|action| match action {
        ClientShellAction::Endpoint { request, .. } => match &request.method {
            crate::api::schema::Method::PaneFocus(target) => Some(target.pane_id.as_str()),
            _ => None,
        },
        _ => None,
    })
}

/// the server confirms the focus the client asked for, as a fresh snapshot and surface pair
fn confirm_focus(state: &mut ClientShellState, mut projected: ClientShellSnapshot, pane: usize) {
    let pane_id = format!("pane_{pane}");
    let agent = projected
        .agents
        .iter()
        .find(|agent| agent.pane_id == pane_id)
        .expect("focused agent")
        .clone();
    projected.revision = 2;
    projected.focused_pane_id = Some(pane_id.clone());
    projected.focused_tab_id = Some(agent.tab_id.clone());
    projected.focused_workspace_id = Some(agent.workspace_id.clone());
    for workspace in &mut projected.workspaces {
        workspace.focused = workspace.workspace_id == agent.workspace_id;
    }
    for candidate in &mut projected.agents {
        candidate.focused = candidate.pane_id == pane_id;
    }
    state.set_snapshot(Box::new(projected));
    let mut frame = surface();
    frame.projection_revision = 2;
    state.set_pane_surface(frame);
}

/// one space with `count` tabs, an agent in each, focused on the first.
fn space_with_agent_tabs(count: usize) -> ClientShellSnapshot {
    let mut projected = tree(
        vec![space(1, "one")],
        (1..=count).map(|number| pi(number, 1, number)).collect(),
    );
    projected.tabs = (1..=count)
        .map(|number| {
            if number == 1 {
                tab(1, 1, "1", false)
            } else {
                tab(number, 1, &format!("tab-{number}"), true)
            }
        })
        .collect();
    projected
}

#[test]
fn clicking_a_nested_agent_row_focuses_its_pane() {
    let mut claude = pi(2, 1, 2);
    claude.agent = Some("claude".into());
    let mut projected = tree(vec![space(1, "solo")], vec![pi(1, 1, 1), claude]);
    projected.tabs = vec![tab(1, 1, "1", false), tab(2, 1, "logs", true)];
    let mut state = shell(projected, Config::default());
    state.mode = ClientShellMode::Navigate;
    state.navigate_workspace_id = state.navigation_target(&ClientEndpointId::Local, "ws_1");
    state.compose(106, 20).expect("sidebar");
    let second_row = state
        .hits
        .agents
        .iter()
        .find(|(_, pane_id)| pane_id == "pane_2")
        .map(|(rect, _)| *rect)
        .expect("second agent row");

    let outcome = click_at(&mut state, second_row.x + 4, second_row.y);

    assert_eq!(focused_pane_request(&outcome), Some("pane_2"));
    assert_eq!(state.mode, ClientShellMode::Terminal);
    assert!(state.navigate_workspace_id.is_none());
}

#[test]
fn previous_agent_in_spaces_view_scrolls_the_tree_instead_of_the_agent_panel() {
    let projected = space_with_agent_tabs(20);
    let mut state = shell(projected.clone(), Config::default());
    state.compose(80, 14).expect("sidebar");

    let mut outcome = ClientShellInput::default();
    state.record_binding(
        crate::input::KeybindMatch::Action(crate::input::KeybindAction::PreviousAgent),
        &mut outcome,
    );
    assert_eq!(focused_pane_request(&outcome), Some("pane_20"));
    assert_eq!(state.sidebar_view, preferences::SidebarView::Spaces);
    assert_eq!(state.agent_scroll, 0);

    // nothing moves until the server says the focus landed
    state.compose(80, 14).expect("sidebar before focus");
    assert_eq!(state.workspace_scroll, 0);
    confirm_focus(&mut state, projected, 20);
    state.compose(80, 14).expect("sidebar after focus");
    assert!(state.workspace_scroll > 0);
    assert!(state
        .hits
        .agents
        .iter()
        .any(|(_, pane_id)| pane_id == "pane_20"));
    assert_eq!(state.agent_scroll, 0);
}

#[test]
fn focusing_an_agent_in_a_collapsed_worktree_group_reveals_its_row() {
    let mut parent = space(1, "parent");
    parent.worktree = worktree("repo-key", false);
    let mut child = space(2, "child");
    child.worktree = worktree("repo-key", true);
    let projected = tree(vec![parent, child], vec![pi(1, 1, 1), pi(2, 2, 2)]);
    let mut state = shell(projected.clone(), Config::default());
    state.collapsed_groups.insert("repo-key".into());
    state.compose(80, 14).expect("sidebar");
    assert!(!state
        .hits
        .agents
        .iter()
        .any(|(_, pane_id)| pane_id == "pane_2"));

    let mut outcome = ClientShellInput::default();
    state.record_binding(
        crate::input::KeybindMatch::Action(crate::input::KeybindAction::FocusAgent(1)),
        &mut outcome,
    );
    assert_eq!(focused_pane_request(&outcome), Some("pane_2"));
    confirm_focus(&mut state, projected, 2);
    state.compose(80, 14).expect("sidebar after focus");

    // the group stays folded: a collapsed group still shows its focused member
    assert!(state.collapsed_groups.contains("repo-key"));
    assert!(state
        .hits
        .agents
        .iter()
        .any(|(_, pane_id)| pane_id == "pane_2"));
}

#[test]
fn ensuring_visibility_terminates_when_the_sidebar_has_no_room_for_rows() {
    let projected = space_with_agent_tabs(4);
    let mut state = shell(projected.clone(), Config::default());
    // no body rows at all: an unbounded scroll-until-visible loop would spin forever here
    state.reveal_workspace("ws_1");
    state.compose(80, 2).expect("tiny sidebar");
    state.reveal_agent_in_tree("pane_4");
    confirm_focus(&mut state, projected, 4);
    state.compose(80, 2).expect("tiny sidebar after focus");
    assert_eq!(state.workspace_scroll, 0);
}

/// navigation counts tree rows, so a space below a long agent block still scrolls into view
#[test]
fn navigating_to_a_space_below_the_agents_scrolls_it_into_view() {
    let mut projected = space_with_agent_tabs(12);
    projected.workspaces.push(space(2, "two"));
    projected.tabs.push(tab(13, 2, "1", false));
    let mut state = shell(projected, Config::default());
    state.compose(80, 14).expect("sidebar");
    assert!(!state
        .hits
        .workspaces
        .iter()
        .any(|hit| hit.workspace_id == "ws_2"));

    for key in [&[0x02][..], b"w", b"\x1b[B"] {
        state.handle_input_bytes(key);
    }
    assert_eq!(state.mode, ClientShellMode::Navigate);
    state.compose(80, 14).expect("sidebar while navigating");

    assert!(state
        .hits
        .workspaces
        .iter()
        .any(|hit| hit.workspace_id == "ws_2"));
    assert!(state.workspace_scroll > 0);
}

/// a wheel notch moves one tree row, agent rows included
#[test]
fn a_wheel_notch_scrolls_the_tree_by_one_row() {
    let mut state = shell(space_with_agent_tabs(12), Config::default());
    state.compose(80, 14).expect("sidebar");
    let body = state.hits.workspace_body;

    let outcome = state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: body.x,
        row: body.y,
        modifiers: KeyModifiers::empty(),
    })]);
    assert!(outcome.repaint);
    assert_eq!(state.workspace_scroll, 1);
    state.compose(80, 14).expect("scrolled sidebar");
    // the space row is above the top now, and the first agent leads the list
    assert!(state.hits.workspaces.is_empty());
    assert_eq!(state.hits.agents[0].0.y, body.y);
    assert_eq!(state.hits.agents[0].1, "pane_1");
}

/// prefix+o is navigation the client started, so the tree follows it to the waiting agent
#[test]
fn walking_to_a_waiting_agent_reveals_its_row() {
    let mut projected = space_with_agent_tabs(20);
    projected.agents[19].agent_status = AgentStatus::Blocked;
    let mut state = shell(projected.clone(), Config::default());
    state.compose(80, 14).expect("sidebar");
    assert!(!state
        .hits
        .agents
        .iter()
        .any(|(_, pane_id)| pane_id == "pane_20"));

    let mut outcome = ClientShellInput::default();
    state.record_binding(
        crate::input::KeybindMatch::Action(crate::input::KeybindAction::OpenNotificationTarget),
        &mut outcome,
    );
    assert_eq!(focused_pane_request(&outcome), Some("pane_20"));
    state.compose(80, 14).expect("sidebar before focus");
    assert_eq!(state.workspace_scroll, 0);

    confirm_focus(&mut state, projected, 20);
    state.compose(80, 14).expect("sidebar after focus");
    assert!(state.workspace_scroll > 0);
    assert!(state
        .hits
        .agents
        .iter()
        .any(|(_, pane_id)| pane_id == "pane_20"));
}
