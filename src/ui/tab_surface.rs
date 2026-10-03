use ratatui::{layout::Rect, Frame};

use super::panes::{compute_pane_infos_for_tab, render_panes, resize_tab_panes};
use crate::app::AppState;
use crate::layout::{PaneArea, PaneInfo, SplitBorder, TileLayout};
use crate::protocol::CursorState;
use crate::terminal::TerminalRuntimeRegistry;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TabSurfaceTarget {
    pub(crate) workspace_index: usize,
    pub(crate) tab_index: usize,
}

/// a pane surface and the columns a client keeps free inside it for its own chrome.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SurfaceArea {
    pub(crate) rect: Rect,
    pub(crate) reserved_columns: u16,
}

impl From<Rect> for SurfaceArea {
    fn from(rect: Rect) -> Self {
        Self {
            rect,
            reserved_columns: 0,
        }
    }
}

impl SurfaceArea {
    pub(crate) fn new(rect: Rect, reserved_columns: u16) -> Self {
        Self {
            rect,
            reserved_columns,
        }
    }

    // a reservation that would leave no pane column is not one the surface can honour
    pub(crate) fn reserved(self) -> u16 {
        if self.reserved_columns < self.rect.width {
            self.reserved_columns
        } else {
            0
        }
    }

    /// the surface with any reservation held at its left edge.
    pub(crate) fn unreserved(self) -> Rect {
        let reserved = self.reserved();
        Rect {
            x: self.rect.x.saturating_add(reserved),
            width: self.rect.width - reserved,
            ..self.rect
        }
    }

    /// where a layout's panes tile and which columns stay free: its root seam, else the left edge.
    pub(crate) fn layout_area(self, layout: &TileLayout, zoomed: bool) -> (PaneArea, Option<Rect>) {
        let reserved = self.reserved();
        if reserved == 0 {
            return (PaneArea::Whole(self.rect), None);
        }
        // a zoomed tab shows one pane, so there is no seam left to sit on
        if let Some((left, gutter, right)) = (!zoomed)
            .then(|| layout.gutter_areas(self.rect, reserved))
            .flatten()
        {
            return (PaneArea::Gutter { left, right }, Some(gutter));
        }
        self.edge_area()
    }

    /// panes beside a reservation held at the left edge, as when there is no seam to sit on.
    pub(crate) fn edge_area(self) -> (PaneArea, Option<Rect>) {
        let reserved = self.reserved();
        (
            PaneArea::Whole(self.unreserved()),
            (reserved > 0).then_some(Rect {
                width: reserved,
                ..self.rect
            }),
        )
    }

    pub(crate) fn tab_area(self, tab: &crate::workspace::Tab) -> (PaneArea, Option<Rect>) {
        self.layout_area(&tab.layout, tab.zoomed)
    }
}

pub(crate) struct TabSurfaceLayout {
    pub(crate) target: Option<TabSurfaceTarget>,
    pub(crate) pane_infos: Vec<PaneInfo>,
    pub(crate) split_borders: Vec<SplitBorder>,
    /// the box the panes tile in, which popups center on
    pub(crate) pane_bounds: Rect,
    /// columns of the surface left free for the client
    pub(crate) reserved: Option<Rect>,
}

#[derive(Clone, Copy)]
pub(crate) struct TabSurfaceView<'a> {
    pub(crate) target: Option<TabSurfaceTarget>,
    pub(crate) pane_infos: &'a [PaneInfo],
    pub(crate) split_borders: &'a [SplitBorder],
}

pub(crate) fn compute_tab_surface(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    area: impl Into<SurfaceArea>,
    resize_panes: bool,
    cell_size: crate::kitty_graphics::HostCellSize,
) -> TabSurfaceLayout {
    let target = app.active.and_then(|workspace_index| {
        let workspace = app.workspaces.get(workspace_index)?;
        Some(TabSurfaceTarget {
            workspace_index,
            tab_index: workspace.active_tab_index(),
        })
    });
    compute_tab_surface_for(
        app,
        terminal_runtimes,
        target,
        area,
        resize_panes,
        cell_size,
    )
}

pub(crate) fn compute_tab_surface_for(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    target: Option<TabSurfaceTarget>,
    area: impl Into<SurfaceArea>,
    resize_panes: bool,
    cell_size: crate::kitty_graphics::HostCellSize,
) -> TabSurfaceLayout {
    let area = area.into();
    let tab = target.and_then(|target| {
        app.workspaces
            .get(target.workspace_index)?
            .tabs
            .get(target.tab_index)
    });
    let (pane_area, reserved) = match tab {
        Some(tab) => area.tab_area(tab),
        None => area.edge_area(),
    };
    let split_borders = tab
        .map(|tab| {
            if tab.zoomed {
                Vec::new()
            } else {
                tab.layout.splits_in(pane_area)
            }
        })
        .unwrap_or_default();
    let pane_infos = target.map_or_else(Vec::new, |target| {
        compute_pane_infos_for_tab(
            app,
            terminal_runtimes,
            target.workspace_index,
            target.tab_index,
            pane_area,
            resize_panes,
            cell_size,
        )
    });

    TabSurfaceLayout {
        target,
        pane_infos,
        split_borders,
        pane_bounds: pane_area.bounds(),
        reserved,
    }
}

/// size a tab's runtimes for its own shape, so a background tab is already right when shown.
pub(crate) fn resize_tab_surface(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    workspace_index: usize,
    tab_index: usize,
    area: impl Into<SurfaceArea>,
    cell_size: crate::kitty_graphics::HostCellSize,
) {
    let Some(tab) = app
        .workspaces
        .get(workspace_index)
        .and_then(|workspace| workspace.tabs.get(tab_index))
    else {
        return;
    };
    resize_tab_panes(
        app,
        terminal_runtimes,
        workspace_index,
        tab,
        area.into().tab_area(tab).0,
        cell_size,
    );
}

pub(crate) fn render_tab_surface(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    surface: TabSurfaceView<'_>,
    frame: &mut Frame,
) {
    render_panes(
        app,
        terminal_runtimes,
        frame,
        surface.target,
        surface.pane_infos,
        surface.split_borders,
    );
}

pub(crate) fn tab_surface_hyperlinks(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    surface: TabSurfaceView<'_>,
) -> Vec<((u16, u16), String, String)> {
    let Some(ws_idx) = surface.target.map(|target| target.workspace_index) else {
        return Vec::new();
    };
    if app.workspaces.get(ws_idx).is_none() {
        return Vec::new();
    }

    let mut links = Vec::new();
    for info in surface.pane_infos {
        if let Some(runtime) = app.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, info.id)
        {
            links.extend(runtime.visible_hyperlinks(info.inner_rect));
        }
    }
    links
}

pub(crate) fn tab_surface_cursor(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    surface: TabSurfaceView<'_>,
) -> Option<CursorState> {
    let ws_idx = surface.target?.workspace_index;
    let info = surface.pane_infos.iter().find(|info| info.is_focused)?;
    if !app.pane_exposes_host_cursor(ws_idx, info.id) {
        return None;
    }
    let runtime = app.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, info.id)?;
    if runtime.synchronized_output_active() {
        return None;
    }
    let scrolled_back = super::panes::pane_is_scrolled_back(runtime);
    let reveal = app.reveal_hidden_cursor_for_cjk_ime
        && (!app.cjk_ime_agent_filter_configured || {
            let detected = app
                .workspaces
                .get(ws_idx)
                .and_then(|ws| ws.terminal_id(info.id))
                .and_then(|terminal_id| app.terminals.get(terminal_id))
                .and_then(|terminal| terminal.detected_agent);
            detected.is_some_and(|agent| app.cjk_ime_agents.contains(&agent))
        });

    if let Some(cursor) = runtime.cursor_state(info.inner_rect, true) {
        let visible = if reveal {
            !scrolled_back
        } else {
            cursor.visible && !scrolled_back
        };
        Some(CursorState {
            x: cursor.x,
            y: cursor.y,
            visible,
            shape: if reveal && visible {
                app.cjk_ime_cursor_shape
            } else {
                cursor.shape
            },
        })
    } else if reveal && !scrolled_back {
        Some(CursorState {
            x: info.inner_rect.x,
            y: info.inner_rect.y,
            visible: true,
            shape: app.cjk_ime_cursor_shape,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Workspace;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Direction;
    use ratatui::Terminal;

    #[tokio::test]
    async fn explicit_surface_layout_drives_render_cursor_and_hyperlinks() {
        let uri = "https://example.com/surface";
        let mut workspace = Workspace::test_new("shell-workspace");
        let left = workspace.tabs[0].root_pane;
        let right = workspace.test_split(Direction::Horizontal);
        workspace.insert_test_runtime(
            left,
            crate::terminal::TerminalRuntime::test_with_screen_bytes(
                20,
                8,
                format!("\x1b]8;;{uri}\x1b\\LEFT\x1b]8;;\x1b\\").as_bytes(),
            ),
        );
        workspace.insert_test_runtime(
            right,
            crate::terminal::TerminalRuntime::test_with_screen_bytes(20, 8, b"RIGHT"),
        );

        let mut app = AppState::test_new();
        app.workspaces = vec![workspace];
        app.active = Some(0);
        app.selected = 0;

        let full_area = Rect::new(0, 0, 106, 20);
        let area = full_area;
        let surface = compute_tab_surface(
            &app,
            &TerminalRuntimeRegistry::new(),
            area,
            false,
            crate::kitty_graphics::HostCellSize::default(),
        );
        assert_eq!(surface.pane_infos.len(), 2);
        assert!(!surface.split_borders.is_empty());

        app.view.terminal_area = Rect::new(9, 8, 7, 6);
        app.view.pane_infos.clear();

        let surface_view = TabSurfaceView {
            target: surface.target,
            pane_infos: &surface.pane_infos,
            split_borders: &surface.split_borders,
        };
        let mut terminal =
            Terminal::new(TestBackend::new(full_area.width, full_area.height)).unwrap();
        terminal
            .draw(|frame| {
                render_tab_surface(&app, &TerminalRuntimeRegistry::new(), surface_view, frame)
            })
            .unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("LEFT"), "surface: {rendered:?}");
        assert!(rendered.contains("RIGHT"), "surface: {rendered:?}");
        assert!(!rendered.contains("shell-workspace"));

        let links = tab_surface_hyperlinks(&app, &TerminalRuntimeRegistry::new(), surface_view);
        assert!(links
            .iter()
            .any(|(_, symbol, link)| { symbol == "L" && link == uri }));
        assert!(tab_surface_cursor(&app, &TerminalRuntimeRegistry::new(), surface_view,).is_some());
    }

    fn app_with(workspace: Workspace) -> AppState {
        let mut app = AppState::test_new();
        app.workspaces = vec![workspace];
        app.active = Some(0);
        app.selected = 0;
        app
    }

    fn surface_for(app: &AppState, tab_index: usize, area: SurfaceArea) -> TabSurfaceLayout {
        compute_tab_surface_for(
            app,
            &TerminalRuntimeRegistry::new(),
            Some(TabSurfaceTarget {
                workspace_index: 0,
                tab_index,
            }),
            area,
            false,
            crate::kitty_graphics::HostCellSize::default(),
        )
    }

    const SURFACE: Rect = Rect::new(0, 0, 106, 19);

    #[test]
    fn a_two_column_tab_tiles_either_side_of_the_reserved_seam() {
        let mut workspace = Workspace::test_new("one");
        workspace.test_split(Direction::Horizontal);
        let app = app_with(workspace);

        let surface = surface_for(&app, 0, SurfaceArea::new(SURFACE, 26));

        assert_eq!(surface.reserved, Some(Rect::new(40, 0, 26, 19)));
        let rects: Vec<_> = surface.pane_infos.iter().map(|info| info.rect).collect();
        assert_eq!(
            rects,
            vec![Rect::new(0, 0, 40, 19), Rect::new(66, 0, 40, 19)]
        );
        // the seam is the client's, so the root split offers no drag handle
        assert!(surface.split_borders.is_empty());
        assert_eq!(surface.pane_bounds, SURFACE);
    }

    #[test]
    fn a_tab_without_a_seam_keeps_the_reservation_at_the_edge() {
        let edge = Some(Rect::new(0, 0, 26, 19));
        let tiled_right_of_it = |surface: &TabSurfaceLayout| {
            surface
                .pane_infos
                .iter()
                .all(|info| info.rect.x >= 26 && info.rect.right() <= 106)
        };

        let single = app_with(Workspace::test_new("single"));
        let surface = surface_for(&single, 0, SurfaceArea::new(SURFACE, 26));
        assert_eq!(surface.reserved, edge);
        assert_eq!(surface.pane_infos[0].rect, Rect::new(26, 0, 80, 19));

        let mut stacked = Workspace::test_new("stacked");
        stacked.test_split(Direction::Vertical);
        let stacked = app_with(stacked);
        let surface = surface_for(&stacked, 0, SurfaceArea::new(SURFACE, 26));
        assert_eq!(surface.reserved, edge);
        assert!(tiled_right_of_it(&surface));

        let mut zoomed = Workspace::test_new("zoomed");
        zoomed.test_split(Direction::Horizontal);
        zoomed.tabs[0].zoomed = true;
        let zoomed = app_with(zoomed);
        let surface = surface_for(&zoomed, 0, SurfaceArea::new(SURFACE, 26));
        assert_eq!(surface.reserved, edge);
        assert_eq!(surface.pane_infos[0].rect, Rect::new(26, 0, 80, 19));

        // too narrow to keep 20 columns a side once the sidebar is taken out
        let mut narrow = Workspace::test_new("narrow");
        narrow.test_split(Direction::Horizontal);
        let narrow = app_with(narrow);
        let surface = surface_for(&narrow, 0, SurfaceArea::new(Rect::new(0, 0, 60, 19), 26));
        assert_eq!(surface.reserved, Some(Rect::new(0, 0, 26, 19)));
        assert!(surface.pane_infos.iter().all(|info| info.rect.x >= 26));
    }

    #[test]
    fn no_reservation_leaves_the_surface_whole() {
        let mut workspace = Workspace::test_new("one");
        workspace.test_split(Direction::Horizontal);
        let app = app_with(workspace);

        let surface = surface_for(&app, 0, SURFACE.into());

        assert_eq!(surface.reserved, None);
        assert_eq!(surface.pane_infos[0].rect.x, 0);
        assert_eq!(surface.split_borders.len(), 1);
    }

    // a background tab sized for a shape it will not show is resized the moment it is shown
    #[tokio::test]
    async fn background_tabs_are_sized_for_their_own_shape() {
        let mut workspace = Workspace::test_new("mixed");
        let columns_tab = 0;
        workspace.test_split(Direction::Horizontal);
        let single_tab = workspace.test_add_tab(Some("single"));
        let mut runtimes = Vec::new();
        for tab_index in [columns_tab, single_tab] {
            for pane in workspace.tabs[tab_index].layout.pane_ids() {
                workspace.tabs[tab_index].runtimes.insert(
                    pane,
                    crate::terminal::TerminalRuntime::test_with_screen_bytes(10, 5, b""),
                );
                runtimes.push((tab_index, pane));
            }
        }
        let app = app_with(workspace);
        let area = SurfaceArea::new(SURFACE, 26);
        for tab_index in [columns_tab, single_tab] {
            resize_tab_surface(
                &app,
                &TerminalRuntimeRegistry::new(),
                0,
                tab_index,
                area,
                crate::kitty_graphics::HostCellSize::default(),
            );
        }

        for (tab_index, pane) in runtimes {
            let resized = app.workspaces[0].tabs[tab_index].runtimes[&pane].current_size();
            let shown = surface_for(&app, tab_index, area)
                .pane_infos
                .into_iter()
                .find(|info| info.id == pane)
                .expect("pane laid out");
            // the hidden size is exactly the one the tab gets once it is the one on screen
            assert_eq!(
                resized,
                (shown.inner_rect.height, shown.inner_rect.width),
                "tab {tab_index}"
            );
        }
    }
}
