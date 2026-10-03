use super::*;
use crate::config::SidebarPositionConfig;

const FILL: &str = "▓";

fn centered_config(
    position: SidebarPositionConfig,
    tab_bar: TabBarPositionConfig,
) -> ClientShellState {
    let mut config = ClientShellConfig::from_config(&Config::default());
    config.sidebar_position = position;
    config.tab_bar_position = tab_bar;
    config.sidebar_width = 26;
    let mut state = ClientShellState::new(config);
    state.sidebar_width = 26;
    state.set_endpoint_surface_reservation_supported(&ClientEndpointId::Local, true);
    state
}

/// the endpoint's half: a two-column tab laid out in the surface this client asked for.
fn endpoint_surface(
    state: &ClientShellState,
    cols: u16,
    rows: u16,
    zoomed: bool,
    focused: &str,
) -> (PaneSurfaceFrame, Option<SurfaceRect>) {
    let mut workspace = crate::workspace::Workspace::test_new("one");
    workspace.test_split(ratatui::layout::Direction::Horizontal);
    workspace.tabs[0].zoomed = zoomed;
    let mut app = crate::app::AppState::test_new();
    app.workspaces = vec![workspace];
    app.active = Some(0);
    let size = state.surface_size(cols, rows);
    let area = crate::ui::SurfaceArea::new(
        Rect::new(0, 0, size.cols, size.rows),
        state.surface_reserved_columns(cols),
    );
    let layout = crate::ui::compute_tab_surface_for(
        &app,
        &crate::terminal::TerminalRuntimeRegistry::new(),
        Some(crate::ui::TabSurfaceTarget {
            workspace_index: 0,
            tab_index: 0,
        }),
        area,
        false,
        crate::kitty_graphics::HostCellSize::default(),
    );
    assert!(
        layout.split_borders.is_empty() || !zoomed,
        "a zoomed tab offers no split to drag"
    );
    let mut buffer = Buffer::empty(area.rect);
    for cell in buffer.content.iter_mut() {
        cell.set_symbol(FILL);
    }
    let panes = layout
        .pane_infos
        .iter()
        .enumerate()
        .map(|(index, info)| {
            let pane_id = format!("pane_{}", index + 1);
            PaneSurfacePane {
                focused: pane_id == focused,
                pane_id,
                content_revision: 0,
                rect: info.rect.into(),
                inner_rect: info.inner_rect.into(),
                scrollbar_rect: None,
                scroll: None,
                mouse_reporting: false,
                sgr_pixel_mouse: false,
                alternate_screen_active: false,
                pixel_width: 0,
                pixel_height: 0,
            }
        })
        .collect();
    let mut frame = surface();
    frame.frame = FrameData::from_ratatui_buffer_with_hyperlinks(&buffer, None, &[]);
    frame.panes = panes;
    frame.splits = Vec::new();
    (frame, layout.reserved.map(Into::into))
}

fn compose_centered(
    position: SidebarPositionConfig,
    tab_bar: TabBarPositionConfig,
    zoomed: bool,
    focused: &str,
) -> (ClientShellState, Vec<String>) {
    let mut state = centered_config(position, tab_bar);
    let (pane_surface, reservation) = endpoint_surface(&state, 106, 20, zoomed, focused);
    let mut projected = snapshot();
    projected.focused_pane_id = Some(focused.to_owned());
    projected.panes[0].focused = focused == "pane_1";
    projected.panes.push(ClientShellPane {
        pane_id: "pane_2".into(),
        focused: focused == "pane_2",
        ..projected.panes[0].clone()
    });
    projected.surface_reservation = reservation;
    state.set_snapshot(Box::new(projected));
    state.set_pane_surface(pane_surface);
    let frame = state.compose(106, 20).expect("composed frame");
    let rows = frame_rows(&frame);
    (state, rows)
}

#[test]
fn centered_sidebar_sits_between_the_two_columns() {
    let (state, rows) = compose_centered(
        SidebarPositionConfig::Center,
        TabBarPositionConfig::Top,
        false,
        "pane_1",
    );
    let layout = state.layout(106, 20);

    // full width, because the sidebar no longer eats into one edge of it
    assert_eq!(layout.tab_bar, Rect::new(0, 0, 106, 1));
    assert_eq!(layout.terminal, Rect::new(0, 1, 106, 19));
    assert_eq!(layout.sidebar, Rect::new(40, 1, 26, 19));

    let mut panes: Vec<_> = state.hits.panes.iter().map(|hit| hit.rect).collect();
    panes.sort_by_key(|rect| rect.x);
    assert_eq!(panes[0], Rect::new(0, 1, 40, 19));
    assert_eq!(panes[1], Rect::new(66, 1, 40, 19));
    // the root split is the sidebar's to own, so it is no drag handle
    assert!(state.hits.pane_splits.is_empty());
    // the frame never paints over the sidebar sitting in its gutter
    for row in &rows[1..20] {
        let seam = row.chars().skip(40).take(26).collect::<String>();
        assert!(!seam.contains(FILL), "seam row {seam:?}");
    }
}

#[test]
fn centered_sidebar_falls_back_to_the_edge_without_two_columns() {
    let (state, _) = compose_centered(
        SidebarPositionConfig::Center,
        TabBarPositionConfig::Top,
        true,
        "pane_1",
    );
    let layout = state.layout(106, 20);

    assert_eq!(layout.sidebar, Rect::new(0, 0, 26, 20));
    assert_eq!(layout.terminal, Rect::new(26, 1, 80, 19));
    assert_eq!(state.hits.panes[0].rect, Rect::new(26, 1, 80, 19));
}

#[test]
fn left_sidebar_is_untouched_by_the_centered_option() {
    let (state, _) = compose_centered(
        SidebarPositionConfig::Left,
        TabBarPositionConfig::Top,
        false,
        "pane_1",
    );
    let layout = state.layout(106, 20);

    assert_eq!(layout.sidebar, Rect::new(0, 0, 26, 20));
    assert_eq!(layout.terminal, Rect::new(26, 1, 80, 19));
    assert_eq!(
        state.surface_size(106, 20),
        ClientSurfaceSize { cols: 80, rows: 19 }
    );
}

// the configuration this was ported for: the bar under the panes, the sidebar on the seam
#[test]
fn centered_sidebar_leaves_a_bottom_tab_bar_the_full_row() {
    let (state, _) = compose_centered(
        SidebarPositionConfig::Center,
        TabBarPositionConfig::Bottom,
        false,
        "pane_1",
    );
    let layout = state.layout(106, 20);

    assert_eq!(layout.tab_bar, Rect::new(1, 19, 105, 1));
    assert_eq!(layout.terminal, Rect::new(0, 0, 106, 19));
    assert_eq!(layout.sidebar, Rect::new(40, 0, 26, 19));
}

#[test]
fn centered_sidebar_asks_for_the_full_width_and_its_own_columns() {
    let mut state = centered_config(SidebarPositionConfig::Center, TabBarPositionConfig::Top);

    assert_eq!(
        state.surface_size(106, 20),
        ClientSurfaceSize {
            cols: 106,
            rows: 19
        }
    );
    let resize =
        crate::client::shell_runtime::client_shell_resize_message(&state, 106, 20, 8, 16, false);
    let ClientMessage::EndpointControl { kind, data } = resize else {
        panic!("a reservation cannot ride the frozen binary resize");
    };
    assert_eq!(kind, crate::protocol::endpoint::SURFACE_RESIZE_KIND);
    let resize: crate::protocol::endpoint::EndpointSurfaceResize =
        serde_json::from_str(&data).expect("reserving resize decodes");
    assert_eq!(resize.reserved_columns, 26);
    assert_eq!(resize.surface_size.cols, 106);

    // a collapsed sidebar has nothing to seat, and an endpoint without the capability is not asked
    state.sidebar_collapsed = true;
    assert_eq!(state.surface_reserved_columns(106), 0);
    state.sidebar_collapsed = false;
    state.set_endpoint_surface_reservation_supported(&ClientEndpointId::Local, false);
    assert!(matches!(
        crate::client::shell_runtime::client_shell_resize_message(&state, 106, 20, 8, 16, false),
        ClientMessage::ClientShellResize {
            surface_size: ClientSurfaceSize { cols: 80, rows: 19 },
            ..
        }
    ));
}

#[test]
fn clicking_and_selecting_beside_a_centered_sidebar_lands_in_the_right_pane() {
    let (mut state, _) = compose_centered(
        SidebarPositionConfig::Center,
        TabBarPositionConfig::Top,
        false,
        "pane_2",
    );
    let right = state
        .hits
        .panes
        .iter()
        .find(|hit| hit.pane_id == "pane_2")
        .expect("right pane hit")
        .clone();
    assert!(
        right.inner_rect.x >= 66,
        "right pane at {:?}",
        right.inner_rect
    );

    let mut mouse = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: right.inner_rect.x,
        row: right.inner_rect.y,
        modifiers: KeyModifiers::empty(),
    };
    let press = state.handle_raw_events(vec![RawInputEvent::Mouse(mouse)]);
    assert!(press.actions.iter().any(|action| matches!(
        action,
        ClientShellAction::Endpoint { request, .. }
            if matches!(&request.method,
                crate::api::schema::Method::PaneFocus(target) if target.pane_id == "pane_2")
    )));
    mouse.kind = MouseEventKind::Drag(MouseButton::Left);
    mouse.column += 2;
    state.handle_raw_events(vec![RawInputEvent::Mouse(mouse)]);
    mouse.kind = MouseEventKind::Up(MouseButton::Left);
    let release = state.handle_raw_events(vec![RawInputEvent::Mouse(mouse)]);
    assert!(matches!(
        &release.actions[..],
        [ClientShellAction::Endpoint { request, .. }]
            if matches!(&request.method,
                crate::api::schema::Method::PaneSelectionRead(params)
                    if params.pane_id == "pane_2"
                        && params.anchor == crate::api::schema::PaneTextPoint { row: 0, col: 0 }
                        && params.cursor == crate::api::schema::PaneTextPoint { row: 0, col: 2 })
    ));

    // a press inside the gutter belongs to the sidebar, never to a pane
    let seam = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 50,
        row: 5,
        modifiers: KeyModifiers::empty(),
    };
    let in_sidebar = state.handle_raw_events(vec![RawInputEvent::Mouse(seam)]);
    assert!(in_sidebar.requests.is_empty());
    assert!(!in_sidebar.actions.iter().any(|action| matches!(
        action,
        ClientShellAction::Endpoint { request, .. }
            if matches!(&request.method, crate::api::schema::Method::PaneFocus(_))
    )));
}
