mod tokens;

use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use self::tokens::{ResolvedToken, ResolvedTokenKind, SpaceTokenContext};
use super::scrollbar::{render_scrollbar, should_show_scrollbar};
use super::status::{state_icon, state_label, state_label_color};
use super::text::{display_width, display_width_u16, truncate_end};
use super::status::{
    agent_state_cells, agent_state_icon, state_dot, state_label, state_label_color,
};
use crate::app::state::{AgentPanelSort, Palette};
use crate::app::{AppState, Mode};
use crate::detect::AgentState;
use crate::terminal::TerminalRuntimeRegistry;

const WORKSPACE_SECTION_HEADER_ROWS: u16 = 2;
const AGENT_PANEL_HEADER_ROWS: u16 = 3;

pub(crate) struct AgentPanelEntry {
    pub ws_idx: usize,
    pub tab_idx: usize,
    pub pane_id: crate::layout::PaneId,
    pub primary_label: String,
    pub primary_tab_label: Option<String>,
    pub pane_label: Option<String>,
    pub terminal_title: Option<String>,
    pub terminal_title_stripped: Option<String>,
    pub agent_label: Option<String>,
    pub agent_kind_label: Option<String>,
    pub agent: Option<crate::detect::Agent>,
    pub state: AgentState,
    pub seen: bool,
    pub last_agent_state_change_seq: Option<u64>,
    pub state_labels: std::collections::HashMap<String, String>,
    pub tokens: std::collections::HashMap<String, String>,
}

const SIDEBAR_VIEW_TAB_SPACES: &str = " spaces";
const SIDEBAR_VIEW_TAB_AGENTS: &str = "agents";
const SIDEBAR_VIEW_TAB_SEPARATOR: &str = " │ ";

    if content.width == 0 || content.height == 0 {
    }

    }

}

fn render_sidebar_view_tabs(app: &AppState, frame: &mut Frame, content: Rect) {
    if content.width == 0 || content.height == 0 {
        return (Rect::default(), Rect::default());
        return;
    }

    let p = &app.palette;
    let active = Style::default().fg(p.text).add_modifier(Modifier::BOLD);
    let inactive = Style::default().fg(p.overlay0);
    frame.render_widget(
        Rect::new(content.x, content.y, content.width, 1),
    );
}

/// The expanded sidebar minus its right-hand separator column. Both views fill
/// it; only one is visible at a time.
pub(crate) fn sidebar_content_rect(area: Rect) -> Rect {
    let content = Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height);
    if content.width == 0 || content.height == 0 {
        return Rect::default();
    }

    content
}

fn agent_panel_sort_label(sort: AgentPanelSort) -> &'static str {
    match sort {
        AgentPanelSort::Spaces => "grouped",
        AgentPanelSort::Priority => "priority",
    }
}

pub(crate) fn agent_panel_toggle_rect(area: Rect, sort: AgentPanelSort) -> Rect {
    agent_panel_header_label_rect(area, agent_panel_sort_label(sort))
}

fn agent_panel_header_label_rect(area: Rect, label: &str) -> Rect {
    if area.width == 0 || area.height < 2 {
        return Rect::default();
    }

    let width = display_width_u16(label).min(area.width);
    Rect::new(
        area.x + area.width.saturating_sub(width),
        area.y + 1,
        width,
        1,
    )
}

fn active_agent_view_label(app: &AppState) -> Option<&str> {
    app.agent_view_override
        .as_ref()
        .map(|view| view.label.as_deref().unwrap_or("filtered"))
}

pub(crate) fn agent_panel_entries(app: &AppState) -> Vec<AgentPanelEntry> {
    agent_panel_entries_with_runtimes(app, None)
}

pub(crate) fn all_agent_panel_entries(app: &AppState) -> Vec<AgentPanelEntry> {
    collect_agent_panel_entries_with_runtimes(app, None)
}

pub(crate) fn agent_panel_entries_from(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
) -> Vec<AgentPanelEntry> {
    agent_panel_entries_with_runtimes(app, Some(terminal_runtimes))
}

fn agent_panel_entries_with_runtimes(
    app: &AppState,
    terminal_runtimes: Option<&TerminalRuntimeRegistry>,
) -> Vec<AgentPanelEntry> {
    let mut entries = collect_agent_panel_entries_with_runtimes(app, terminal_runtimes);
    crate::app::agent_view::apply_agent_view(app, &mut entries);
    entries
}

fn collect_agent_panel_entries_with_runtimes(
    app: &AppState,
    terminal_runtimes: Option<&TerminalRuntimeRegistry>,
) -> Vec<AgentPanelEntry> {
    let empty_runtimes;
    let terminal_runtimes = match terminal_runtimes {
        Some(terminal_runtimes) => terminal_runtimes,
        None => {
            empty_runtimes = TerminalRuntimeRegistry::new();
            &empty_runtimes
        }
    };

    app.workspaces
        .iter()
        .enumerate()
        .flat_map(|(ws_idx, ws)| {
            let multi_tab = ws.tabs.len() > 1;
            let workspace_label = ws.display_name_from(&app.terminals, terminal_runtimes);
            ws.pane_details(&app.terminals)
                .into_iter()
                .map(move |detail| {
                    let show_tab = multi_tab
                        || ws
                            .tabs
                            .get(detail.tab_idx)
                            .is_some_and(|tab| !tab.is_auto_named());
                    AgentPanelEntry {
                        ws_idx,
                        tab_idx: detail.tab_idx,
                        pane_id: detail.pane_id,
                        primary_label: workspace_label.clone(),
                        primary_tab_label: show_tab.then_some(detail.tab_label),
                        pane_label: detail.pane_label,
                        terminal_title: detail.terminal_title,
                        terminal_title_stripped: detail.terminal_title_stripped,
                        agent_label: Some(detail.agent_label),
                        agent_kind_label: detail.agent_kind_label,
                        agent: detail.agent,
                        state: detail.state,
                        seen: detail.seen,
                        last_agent_state_change_seq: detail.last_agent_state_change_seq,
                        state_labels: detail.state_labels,
                        tokens: detail.tokens,
                    }
                })
        })
        .collect()
}

pub(super) fn agent_panel_status_key(state: AgentState, seen: bool) -> &'static str {
    match (state, seen) {
        (AgentState::Idle, false) => "done",
        (AgentState::Idle, true) => "idle",
        (AgentState::Working, _) => "working",
        (AgentState::Blocked, _) => "blocked",
        (AgentState::Unknown, _) => "unknown",
    }
}

fn workspace_row_height(
    app: &AppState,
    ws: &crate::workspace::Workspace,
    indented: bool,
    suppress_branch: bool,
) -> u16 {
    let (state, seen) = ws.aggregate_state(&app.terminals);
    let label = if indented {
        grouped_child_display_label(
            &ws.display_name_from_terminals(&app.terminals),
            ws.branch().as_deref(),
            ws.custom_name.is_some(),
        )
    } else {
        ws.display_name_from_terminals(&app.terminals)
    };
    let token_values = ws.metadata_tokens.values();
    tokens::space_rows(
        &app.sidebar_spaces,
        SpaceTokenContext {
            workspace: &label,
            branch: ws.branch().as_deref(),
            state_text: state_label(state, seen),
            ahead_behind: ws.git_ahead_behind(),
            tokens: &token_values,
            suppress_git_details: indented,
            suppress_branch,
        },
    )
    .len()
    .max(1)
    .min(u16::MAX as usize) as u16
}

fn workspace_row_height_in_body(
    app: &AppState,
    workspace: &crate::workspace::Workspace,
    indented: bool,
    suppress_branch: bool,
    body_height: u16,
) -> u16 {
    workspace_row_height(app, workspace, indented, suppress_branch).min(body_height)
}

/// The branch rides on the agent rows, so the space above only carries it while
/// none are on screen: collapsed, or no agent under it at all.
fn space_shows_agent_rows(entries: &[WorkspaceListEntry], entry_idx: usize) -> bool {
    matches!(
        entries.get(entry_idx.saturating_add(1)),
        Some(WorkspaceListEntry::AgentPane { .. })
    )
}

/// A whole worktree group is one block, so no gap lands before an indented
/// child — neither between two children nor between a parent and its first one.
fn workspace_entry_gap(app: &AppState, entries: &[WorkspaceListEntry], entry_idx: usize) -> u16 {
    if entry_idx + 1 < entries.len() && !next_entry_is_indented_workspace(entries, entry_idx) {
        app.sidebar_spaces.row_gap
    } else {
        0
    }
}

fn workspace_attention_priority(state: AgentState, seen: bool) -> u8 {
    match (state, seen) {
        (AgentState::Blocked, _) => 4,
        (AgentState::Idle, false) => 3,
        (AgentState::Working, _) => 2,
        (AgentState::Idle, true) => 1,
        (AgentState::Unknown, _) => 0,
    }
}

fn space_aggregate_state(app: &AppState, key: &str) -> (AgentState, bool) {
    app.workspaces
        .iter()
        .filter(|ws| ws.worktree_space().is_some_and(|space| space.key == key))
        .map(|ws| ws.aggregate_state(&app.terminals))
        .max_by_key(|(state, seen)| workspace_attention_priority(*state, *seen))
        .unwrap_or((AgentState::Unknown, true))
}

pub(crate) fn workspace_parent_group_state(
    app: &AppState,
    ws_idx: usize,
) -> Option<(String, bool)> {
    let space = app.workspaces.get(ws_idx)?.worktree_space()?;
    if space.is_linked_worktree {
        return None;
    }
    let member_count = app
        .workspaces
        .iter()
        .filter(|ws| {
            ws.worktree_space()
                .is_some_and(|member| member.key == space.key)
        })
        .count();
    (member_count >= 2).then(|| {
        (
            space.key.clone(),
            app.collapsed_space_keys.contains(&space.key),
        )
    })
}

pub(crate) fn grouped_child_display_label(
    label: &str,
    branch: Option<&str>,
    has_custom_name: bool,
) -> String {
    if has_custom_name {
        return label.to_string();
    }
    let Some(branch) = branch else {
        return label.to_string();
    };
    branch
        .strip_prefix("worktree/")
        .unwrap_or(branch)
        .to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WorkspaceListEntry {
    Workspace {
        ws_idx: usize,
        indented: bool,
    },
    /// An agent row nested under its own space row in the spaces tree.
    AgentPane {
        ws_idx: usize,
        tab_idx: usize,
        pane_id: crate::layout::PaneId,
        under_indented: bool,
    },
}

/// True when the next *space* row is an indented group child, skipping any
/// nested agent rows in between so tree connectors stay correct.
pub(crate) fn next_entry_is_indented_workspace(entries: &[WorkspaceListEntry], idx: usize) -> bool {
    entries
        .iter()
        .skip(idx.saturating_add(1))
        .find_map(|entry| match entry {
            WorkspaceListEntry::Workspace { indented, .. } => Some(*indented),
            WorkspaceListEntry::AgentPane { .. } => None,
        })
        .unwrap_or(false)
}

pub(crate) fn normalized_workspace_scroll(app: &AppState, area: Rect, requested: usize) -> usize {
    let ws_area = workspace_list_rect(area);
    let body = workspace_list_body_rect(ws_area, false);
    if body.height == 0 {
        return requested;
    }

    let layout = TreeLayout::build(app, None);
    if layout.entries.is_empty() {
        0
    } else {
        requested.min(workspace_list_bottom_start(app, ws_area, &layout))
    }
}

/// Space rows only. Use this for workspace selection order, drag-reorder and
/// the mobile switcher; the sidebar's own geometry uses [`sidebar_tree_entries`].
pub(crate) fn workspace_list_entries(app: &AppState) -> Vec<WorkspaceListEntry> {
    workspace_list_entries_inner(app, false)
}

/// Every row the spaces view draws: space rows with their agent rows nested
/// underneath.
pub(crate) fn sidebar_tree_entries(app: &AppState) -> Vec<WorkspaceListEntry> {
    TreeLayout::build(app, None).entries
}

/// Like [`workspace_list_entries`] but always expands worktree groups, ignoring
/// `collapsed_space_keys`. The mobile switcher has no collapse affordance and
/// always shows the full worktree tree.
pub(crate) fn workspace_list_entries_expanded(app: &AppState) -> Vec<WorkspaceListEntry> {
    workspace_list_entries_inner(app, true)
}

fn workspace_list_entries_inner(app: &AppState, force_expanded: bool) -> Vec<WorkspaceListEntry> {
    let mut members_by_key = std::collections::HashMap::<String, Vec<usize>>::new();
    for (ws_idx, ws) in app.workspaces.iter().enumerate() {
        if let Some(space) = ws.worktree_space() {
            members_by_key
                .entry(space.key.clone())
                .or_default()
                .push(ws_idx);
        }
    }
    let grouped_keys = members_by_key
        .iter()
        .filter(|(_, members)| {
            members.len() >= 2
                && members.iter().any(|idx| {
                    app.workspaces
                        .get(*idx)
                        .and_then(|ws| ws.worktree_space())
                        .is_some_and(|space| !space.is_linked_worktree)
                })
        })
        .map(|(key, _)| key.clone())
        .collect::<std::collections::HashSet<_>>();

    let visible_group_idx = if matches!(app.mode, Mode::Navigate) {
        Some(app.selected)
    } else {
        app.active
    };
    let active_group = visible_group_idx.and_then(|idx| {
        app.workspaces
            .get(idx)
            .and_then(|ws| ws.worktree_space())
            .map(|space| space.key.clone())
    });

    let mut emitted_groups = std::collections::HashSet::<String>::new();
    let mut entries = Vec::new();
    for (ws_idx, ws) in app.workspaces.iter().enumerate() {
        let Some(space) = ws
            .worktree_space()
            .filter(|space| grouped_keys.contains(&space.key))
        else {
            entries.push(WorkspaceListEntry::Workspace {
                ws_idx,
                indented: false,
            });
            continue;
        };

        if !emitted_groups.insert(space.key.clone()) {
            continue;
        }

        let Some(members) = members_by_key.get(&space.key) else {
            continue;
        };
        let Some(parent_idx) = members.iter().copied().find(|idx| {
            app.workspaces
                .get(*idx)
                .and_then(|member| member.worktree_space())
                .is_some_and(|member_space| !member_space.is_linked_worktree)
        }) else {
            entries.push(WorkspaceListEntry::Workspace {
                ws_idx,
                indented: false,
            });
            continue;
        };
        let collapsed = !force_expanded && app.collapsed_space_keys.contains(&space.key);
        entries.push(WorkspaceListEntry::Workspace {
            ws_idx: parent_idx,
            indented: false,
        });

        if collapsed {
            if let Some(active_idx) = visible_group_idx
                .filter(|idx| *idx != parent_idx)
                .filter(|_| active_group.as_deref() == Some(space.key.as_str()))
            {
                entries.push(WorkspaceListEntry::Workspace {
                    ws_idx: active_idx,
                    indented: true,
                });
            }
        } else {
            for member_idx in members {
                if *member_idx == parent_idx {
                    continue;
                }
                entries.push(WorkspaceListEntry::Workspace {
                    ws_idx: *member_idx,
                    indented: true,
                });
            }
        }
    }
    entries
}

pub(crate) fn workspace_list_rect(area: Rect) -> Rect {
    sidebar_content_rect(area)
}

}

pub(crate) fn workspace_list_body_rect(area: Rect, has_scrollbar: bool) -> Rect {
    if area.width == 0 || area.height <= WORKSPACE_SECTION_HEADER_ROWS {
        return Rect::default();
    }

    let body_y = area.y.saturating_add(WORKSPACE_SECTION_HEADER_ROWS);
    let footer_y = area.y + area.height.saturating_sub(1);
    let body_height = footer_y.saturating_sub(body_y);
    let body_width = area.width.saturating_sub(u16::from(has_scrollbar));
    Rect::new(area.x, body_y, body_width, body_height)
}

/// Row height and trailing gap for one tree row. `None` when the row's
/// workspace is gone, matching the caller's skip behaviour.
fn tree_entry_metrics(
    app: &AppState,
    entries: &[WorkspaceListEntry],
    entry_idx: usize,
    agents: &std::collections::HashMap<(usize, crate::layout::PaneId), AgentPanelEntry>,
    body_height: u16,
) -> Option<(u16, u16)> {
    let gap = tree_entry_gap(app, entries, entry_idx);
    match entries.get(entry_idx)? {
        WorkspaceListEntry::Workspace { ws_idx, indented } => {
            let ws = app.workspaces.get(*ws_idx)?;
            Some((
                workspace_row_height_in_body(
                    app,
                    ws,
                    *indented,
                    space_shows_agent_rows(entries, entry_idx),
                    body_height,
                ),
                gap,
            ))
        }
        WorkspaceListEntry::AgentPane {
            ws_idx, pane_id, ..
        } => Some((
            nested_agent_row_height(app, agents.get(&(*ws_idx, *pane_id)), body_height),
            gap,
        )),
    }
}

/// A space and the agent rows nested under it are one block: the configured row
/// gap only lands after the last row of that block.
fn tree_entry_gap(app: &AppState, entries: &[WorkspaceListEntry], entry_idx: usize) -> u16 {
    if matches!(
        entries.get(entry_idx.saturating_add(1)),
        Some(WorkspaceListEntry::AgentPane { .. })
    ) {
        // sibling agents get a row for their own rule; a space and its first agent
        // stay glued, since the space row is already the boundary above them
        return u16::from(matches!(
            entries.get(entry_idx),
            Some(WorkspaceListEntry::AgentPane { .. })
        ));
    }
    if entries.get(entry_idx).is_none() {
        return 0;
    }
    workspace_entry_gap(app, entries, entry_idx)
}

/// Everything the spaces view needs about the tree, built once per pass.
/// Rebuilding it is the expensive part of sidebar layout, so callers that need
/// geometry more than once thread this through instead of recomputing.
pub(crate) struct TreeLayout {
    entries: Vec<WorkspaceListEntry>,
    agents: std::collections::HashMap<(usize, crate::layout::PaneId), AgentPanelEntry>,
}

impl TreeLayout {
    fn build(app: &AppState, terminal_runtimes: Option<&TerminalRuntimeRegistry>) -> Self {
        // one pass over the panes feeds both the row list and the row content
        let collected = collect_agent_panel_entries_with_runtimes(app, terminal_runtimes);
        let mut panes_by_workspace =
            std::collections::HashMap::<usize, Vec<(usize, crate::layout::PaneId)>>::new();
        for entry in &collected {
            panes_by_workspace
                .entry(entry.ws_idx)
                .or_default()
                .push((entry.tab_idx, entry.pane_id));
        }
        let mut entries = Vec::with_capacity(collected.len() * 2);
        for entry in workspace_list_entries_inner(app, false) {
            let WorkspaceListEntry::Workspace { ws_idx, indented } = entry else {
                entries.push(entry);
                continue;
            };
            entries.push(WorkspaceListEntry::Workspace { ws_idx, indented });
            for (tab_idx, pane_id) in panes_by_workspace.get(&ws_idx).into_iter().flatten() {
                entries.push(WorkspaceListEntry::AgentPane {
                    ws_idx,
                    tab_idx: *tab_idx,
                    pane_id: *pane_id,
                    under_indented: indented,
                });
            }
        }

        Self {
            entries,
            agents: collected
                .into_iter()
                .map(|entry| ((entry.ws_idx, entry.pane_id), entry))
                .collect(),
        }
    }
}

fn workspace_list_visible_count(
    app: &AppState,
    area: Rect,
    scroll: usize,
    layout: &TreeLayout,
) -> usize {
    let body = workspace_list_body_rect(area, false);
    if body.width == 0 || body.height == 0 {
        return 0;
    }

    let mut used_rows = 0u16;
    let mut visible = 0usize;
    for entry_idx in scroll..layout.entries.len() {
        let Some((row_height, gap)) =
            tree_entry_metrics(app, &layout.entries, entry_idx, &layout.agents, body.height)
        else {
            continue;
        };
        if used_rows.saturating_add(row_height) > body.height {
            break;
        }
        used_rows = used_rows.saturating_add(row_height);
        visible += 1;
        used_rows = used_rows.saturating_add(gap).min(body.height);
    }
    visible
}

fn workspace_list_bottom_start(app: &AppState, area: Rect, layout: &TreeLayout) -> usize {
    let body = workspace_list_body_rect(area, false);
    let mut used_rows = 0u16;
    let mut start = layout.entries.len();
    for entry_idx in (0..layout.entries.len()).rev() {
        let Some((row_height, gap)) =
            tree_entry_metrics(app, &layout.entries, entry_idx, &layout.agents, body.height)
        else {
            continue;
        };
        let needed = row_height.saturating_add(gap);
        if used_rows.saturating_add(needed) > body.height {
            break;
        }
        used_rows = used_rows.saturating_add(needed);
        start = entry_idx;
    }
    start.min(layout.entries.len().saturating_sub(1))
}

pub(crate) fn workspace_list_scroll_metrics(
    app: &AppState,
    area: Rect,
) -> crate::pane::ScrollMetrics {
    workspace_list_scroll_metrics_with(app, area, &TreeLayout::build(app, None))
}

fn workspace_list_scroll_metrics_with(
    app: &AppState,
    area: Rect,
    layout: &TreeLayout,
) -> crate::pane::ScrollMetrics {
    let max_scroll = workspace_list_bottom_start(app, area, layout);
    let scroll = app.workspace_scroll.min(max_scroll);
    let viewport_rows = workspace_list_visible_count(app, area, scroll, layout);

    crate::pane::ScrollMetrics {
        offset_from_bottom: max_scroll.saturating_sub(scroll),
        max_offset_from_bottom: max_scroll,
        viewport_rows,
    }
}

pub(crate) fn workspace_list_scrollbar_rect(app: &AppState, area: Rect) -> Option<Rect> {
    workspace_list_scrollbar_rect_from(area, workspace_list_scroll_metrics(app, area))
}

fn workspace_list_scrollbar_rect_from(
    area: Rect,
    metrics: crate::pane::ScrollMetrics,
) -> Option<Rect> {
    let body = workspace_list_body_rect(area, true);
    (should_show_scrollbar(metrics) && body.width > 0 && body.height > 0).then_some(Rect::new(
        area.x + area.width.saturating_sub(1),
        body.y,
        1,
        body.height,
    ))
}

pub(crate) fn agent_panel_body_rect(area: Rect, has_scrollbar: bool) -> Rect {
    if area.width == 0 || area.height <= AGENT_PANEL_HEADER_ROWS {
        return Rect::default();
    }

    let body_y = area.y.saturating_add(AGENT_PANEL_HEADER_ROWS);
    // the new/menu footer row is shared with the spaces view
    let footer_y = area.y + area.height.saturating_sub(1);
    let body_height = footer_y.saturating_sub(body_y);
    let body_width = area.width.saturating_sub(u16::from(has_scrollbar));
    Rect::new(area.x, body_y, body_width, body_height)
}

/// Text that tells two otherwise identical sibling agent rows apart, keyed by
/// pane. Only spaces with a real collision get entries, so rows stay short
/// whenever the row already reads unambiguously.
///
/// Preference order per collision group: the pane's own label, then its stripped
/// terminal title, then a 1-based ordinal. A candidate is only used when it makes
/// every row in the group distinct; otherwise the whole group falls to ordinals.
fn nested_row_discriminators(
    app: &AppState,
    layout: &TreeLayout,
) -> std::collections::HashMap<(usize, crate::layout::PaneId), String> {
    let mut groups: std::collections::HashMap<(usize, String), Vec<crate::layout::PaneId>> =
        std::collections::HashMap::new();
    for entry in &layout.entries {
        let WorkspaceListEntry::AgentPane {
            ws_idx, pane_id, ..
        } = entry
        else {
            continue;
        };
        let Some(agent) = layout.agents.get(&(*ws_idx, *pane_id)) else {
            continue;
        };
        let text = tokens::rows_text(&resolved_nested_agent_rows(app, agent));
        groups.entry((*ws_idx, text)).or_default().push(*pane_id);
    }

    // once per collision group, not once per member: the candidates and their
    // distinctness are a property of the whole group
    let mut discriminators = std::collections::HashMap::new();
    for ((ws_idx, text), members) in &groups {
        if members.len() < 2 {
            continue;
        }
        let candidates = members
            .iter()
            .map(|member| {
                layout.agents.get(&(*ws_idx, *member)).and_then(|agent| {
                    agent
                        .pane_label
                        .as_deref()
                        .or(agent.terminal_title_stripped.as_deref())
                        .filter(|candidate| !text.contains(candidate))
                })
            })
            .collect::<Vec<_>>();
        let distinct = candidates.iter().all(Option::is_some)
            && candidates
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                == candidates.len();
        for (position, member) in members.iter().enumerate() {
            let label = if distinct {
                candidates[position].unwrap_or_default().to_string()
            } else {
                (position + 1).to_string()
            };
            discriminators.insert((*ws_idx, *member), label);
        }
    }
    discriminators
}

/// Spaces whose agent rows all carry the same tab label. There the tab tells the
/// rows apart from nothing, so it is dropped; a space whose agents really do sit
/// in different tabs keeps it.
fn spaces_with_one_tab_label(layout: &TreeLayout) -> std::collections::HashSet<usize> {
    use std::collections::hash_map::Entry;

    let mut first: std::collections::HashMap<usize, &Option<String>> =
        std::collections::HashMap::new();
    let mut mixed = std::collections::HashSet::new();
    for ((ws_idx, _), agent) in &layout.agents {
        match first.entry(*ws_idx) {
            Entry::Vacant(slot) => {
                slot.insert(&agent.primary_tab_label);
            }
            Entry::Occupied(slot) => {
                if *slot.get() != &agent.primary_tab_label {
                    mixed.insert(*ws_idx);
                }
            }
        }
    }
    first
        .into_keys()
        .filter(|ws_idx| !mixed.contains(ws_idx))
        .collect()
}

/// A space's ordinal. Enclosed forms say "which one" rather than "how many", which a
/// bare digit beside a name does not. Past 20 Unicode stops enclosing and plain digits
/// are all there is; the slot they are drawn in is sized once for the whole list, so
/// the names never shift as the count crosses ten.
fn space_number_label(number: usize) -> String {
    const FIRST_ENCLOSED: u32 = 0x2460;
    u32::try_from(number)
        .ok()
        .filter(|n| (1..=20).contains(n))
        .and_then(|n| char::from_u32(FIRST_ENCLOSED + n - 1))
        .map(String::from)
        .unwrap_or_else(|| number.to_string())
}

/// The rightmost column of an agent row, held back on every row so the text never
/// shifts as the selection moves. Only the focused row draws in it.
const SELECTION_MARK_WIDTH: u16 = 1;

/// Heavy, where the tree's own rules are light: this one answers "which row am I on",
/// which every other line in the sidebar has no part in.
const SELECTION_MARK: &str = "┃";

/// The rule marking the focused row. It wears the accent unmodified, exactly as the
/// focused pane's border does, because the two say the same thing and any lift toward
/// legibility washes the hue out until they no longer read as one signal. The fill
/// stays under it: taking it away leaves a notch and the row stops reading as
/// selected at all.
fn selection_mark_style(p: &Palette, fill: ratatui::style::Color) -> Style {
    Style::default().fg(p.accent).bg(fill)
}

/// A one-cell fill reads as a hairline, not as a selected row, so an agent with
/// nothing to say on its second line still gets a blank one.
const NESTED_AGENT_ROW_MIN_HEIGHT: usize = 2;

fn nested_agent_row_height(
    app: &AppState,
    entry: Option<&AgentPanelEntry>,
    body_height: u16,
) -> u16 {
    let rows = entry.map_or(1, |entry| resolved_nested_agent_rows(app, entry).len());
    (rows.max(NESTED_AGENT_ROW_MIN_HEIGHT).min(u16::MAX as usize) as u16).min(body_height)
}

fn resolved_nested_agent_rows(app: &AppState, entry: &AgentPanelEntry) -> Vec<Vec<ResolvedToken>> {
    let ws = app.workspaces.get(entry.ws_idx);
    let branch = ws.and_then(crate::workspace::Workspace::branch);
    // only a linked worktree child renders the grouped form, and only that form is
    // named after the branch; a top-level space keeps its own name and its branch
    let space_label = ws.map(|ws| {
        let label = ws.display_name_from_terminals(&app.terminals);
        if ws
            .worktree_space()
            .is_some_and(|space| space.is_linked_worktree)
        {
            grouped_child_display_label(&label, branch.as_deref(), ws.custom_name.is_some())
        } else {
            label
        }
    });
    tokens::nested_agent_rows(
        &app.sidebar_agents,
        entry,
        agent_state_text(entry),
        tokens::NestedContext {
            branch: branch.as_deref(),
            space_label: space_label.as_deref(),
        },
    )
}

fn agent_state_text(entry: &AgentPanelEntry) -> &str {
    entry
        .state_labels
        .get(agent_panel_status_key(entry.state, entry.seen))
        .map(String::as_str)
        .unwrap_or_else(|| state_label(entry.state, entry.seen))
}

fn resolved_agent_rows(app: &AppState, entry: &AgentPanelEntry) -> Vec<Vec<ResolvedToken>> {
    let label = entry
        .state_labels
        .get(agent_panel_status_key(entry.state, entry.seen))
        .map(String::as_str)
        .unwrap_or_else(|| state_label(entry.state, entry.seen));
    tokens::agent_rows(&app.sidebar_agents, entry, label)
}

pub(crate) fn agent_entry_height_in_body(
    app: &AppState,
    entry: &AgentPanelEntry,
    body_height: u16,
) -> u16 {
    (resolved_agent_rows(app, entry)
        .len()
        .max(1)
        .min(u16::MAX as usize) as u16)
        .min(body_height)
}

pub(crate) fn agent_entry_gap(app: &AppState, entry_idx: usize, entry_count: usize) -> u16 {
    if entry_idx + 1 < entry_count {
        app.sidebar_agents.row_gap
    } else {
        0
    }
}

fn agent_panel_visible_count_from(app: &AppState, area: Rect, scroll: usize) -> usize {
    let body = agent_panel_body_rect(area, false);
    if body.width == 0 || body.height == 0 {
        return 0;
    }

    let mut used_rows = 0u16;
    let mut visible = 0usize;
    let entries = agent_panel_entries(app);
    for (index, entry) in entries.iter().enumerate().skip(scroll) {
        let height = agent_entry_height_in_body(app, entry, body.height);
        if used_rows.saturating_add(height) > body.height {
            break;
        }
        used_rows = used_rows.saturating_add(height);
        visible += 1;
        used_rows = used_rows
            .saturating_add(agent_entry_gap(app, index, entries.len()))
            .min(body.height);
    }
    visible
}

fn agent_panel_bottom_start(app: &AppState, area: Rect) -> usize {
    let body = agent_panel_body_rect(area, false);
    let entries = agent_panel_entries(app);
    let mut used_rows = 0u16;
    let mut start = entries.len();
    for (index, entry) in entries.iter().enumerate().rev() {
        let gap = agent_entry_gap(app, index, entries.len());
        let needed = agent_entry_height_in_body(app, entry, body.height).saturating_add(gap);
        if used_rows.saturating_add(needed) > body.height {
            break;
        }
        used_rows = used_rows.saturating_add(needed);
        start = index;
    }
    start.min(entries.len().saturating_sub(1))
}

pub(crate) fn agent_panel_scroll_for_target(
    app: &AppState,
    area: Rect,
    current_scroll: usize,
    target: usize,
) -> usize {
    let max_scroll = agent_panel_bottom_start(app, area);
    if target < current_scroll {
        return target.min(max_scroll);
    }
    let mut scroll = current_scroll.min(max_scroll);
    while scroll < target {
        let visible = agent_panel_visible_count_from(app, area, scroll);
        if visible > 0 && target < scroll.saturating_add(visible) {
            break;
        }
        scroll += 1;
    }
    scroll.min(max_scroll)
}

pub(crate) fn agent_panel_scroll_metrics(app: &AppState, area: Rect) -> crate::pane::ScrollMetrics {
    let max_scroll = agent_panel_bottom_start(app, area);
    let scroll = app.agent_panel_scroll.min(max_scroll);
    let viewport_rows = agent_panel_visible_count_from(app, area, scroll);

    crate::pane::ScrollMetrics {
        offset_from_bottom: max_scroll.saturating_sub(scroll),
        max_offset_from_bottom: max_scroll,
        viewport_rows,
    }
}

pub(crate) fn agent_panel_scrollbar_rect(app: &AppState, area: Rect) -> Option<Rect> {
    let metrics = agent_panel_scroll_metrics(app, area);
    let body = agent_panel_body_rect(area, true);
    (should_show_scrollbar(metrics) && body.width > 0 && body.height > 0).then_some(Rect::new(
        area.x + area.width.saturating_sub(1),
        body.y,
        1,
        body.height,
    ))
}

pub(crate) fn compute_workspace_list_areas(
    app: &AppState,
    area: Rect,
) -> (
    Vec<crate::app::state::WorkspaceCardArea>,
    Vec<crate::app::state::SidebarAgentRowArea>,
) {
    let ws_area = workspace_list_rect(area);
    if ws_area == Rect::default() || app.sidebar_view != crate::app::state::SidebarView::Spaces {
        return (Vec::new(), Vec::new());
    }

    let layout = TreeLayout::build(app, None);
    let metrics = workspace_list_scroll_metrics_with(app, ws_area, &layout);
    let body = workspace_list_body_rect(ws_area, should_show_scrollbar(metrics));
    if body.width == 0 || body.height == 0 {
        return (Vec::new(), Vec::new());
    }

    let scroll = app.workspace_scroll;
    let mut row_y = body.y;
    let body_bottom = body.y + body.height;
    let mut cards = Vec::new();
    let mut agent_rows = Vec::new();

    let entries = &layout.entries;
    for entry_idx in scroll..entries.len() {
        let Some((row_height, gap)) =
            tree_entry_metrics(app, entries, entry_idx, &layout.agents, body.height)
        else {
            continue;
        };
        if row_y.saturating_add(row_height) > body_bottom {
            break;
        }
        let rect = Rect::new(body.x, row_y, body.width, row_height);
        match &entries[entry_idx] {
            WorkspaceListEntry::Workspace { ws_idx, indented } => {
                cards.push(crate::app::state::WorkspaceCardArea {
                    ws_idx: *ws_idx,
                    rect,
                    indented: *indented,
                });
            }
            WorkspaceListEntry::AgentPane {
                ws_idx,
                tab_idx,
                pane_id,
                under_indented,
            } => {
                agent_rows.push(crate::app::state::SidebarAgentRowArea {
                    ws_idx: *ws_idx,
                    tab_idx: *tab_idx,
                    pane_id: *pane_id,
                    rect,
                    under_indented: *under_indented,
                });
            }
        }
        row_y = row_y
            .saturating_add(row_height)
            .saturating_add(gap)
            .min(body_bottom);
    }

    (cards, agent_rows)
}

pub(crate) fn compute_workspace_card_areas(
    app: &AppState,
    area: Rect,
) -> Vec<crate::app::state::WorkspaceCardArea> {
    compute_workspace_list_areas(app, area).0
}

    }

        1,
        1,

/// Auto-scale sidebar width based on workspace identity + agent summary.
pub(crate) fn collapsed_sidebar_sections(area: Rect) -> (Rect, Option<u16>, Rect) {
    let content = Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height);
    if content.width == 0 || content.height == 0 {
        return (Rect::default(), None, Rect::default());
    }

    if content.height < 7 {
        return (content, None, Rect::default());
    }

    let total_h = content.height as usize;
    let ws_h = total_h.div_ceil(2);
    let detail_h = total_h.saturating_sub(ws_h + 1);
    if ws_h == 0 || detail_h == 0 {
        return (content, None, Rect::default());
    }

    let divider_y = content.y + ws_h as u16;
    let ws_area = Rect::new(content.x, content.y, content.width, ws_h as u16);
    let detail_area = Rect::new(content.x, divider_y + 1, content.width, detail_h as u16);
    (ws_area, Some(divider_y), detail_area)
}

/// Collapsed sidebar: workspace glance on top, compact agent list below.
pub(super) fn render_sidebar_collapsed(app: &AppState, frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let is_navigating = matches!(app.mode, Mode::Navigate);

    let p = &app.palette;
    frame
        .buffer_mut()
        .set_style(area, Style::default().bg(p.sidebar_bg));
    let sep_style = if is_navigating {
        Style::default().fg(p.accent)
    } else {
        Style::default().fg(p.surface_dim)
    };
    let sep_x = area.x + area.width.saturating_sub(1);
    let buf = frame.buffer_mut();
    for y in area.y..area.y + area.height {
        buf[(sep_x, y)].set_symbol("│");
        buf[(sep_x, y)].set_style(sep_style);
    }

    let (ws_area, divider_y, detail_area) = collapsed_sidebar_sections(area);
    if ws_area == Rect::default() {
        render_sidebar_toggle(app, frame, area, true, p);
        return;
    }

    for (visible_idx, ws) in app.workspaces.iter().enumerate() {
        let y = ws_area.y + visible_idx as u16;
        if y >= ws_area.y + ws_area.height {
            break;
        }
        let (agg_state, agg_seen) = ws.aggregate_state(&app.terminals);
        let (icon, icon_style) = state_dot(agg_state, agg_seen, p);
        let is_selected = visible_idx == app.selected && is_navigating;
        let is_active = Some(visible_idx) == app.active;
        let row_style = if is_selected {
            Style::default().bg(p.surface0)
        } else if is_active {
            Style::default().bg(p.active_row_bg)
        } else {
            Style::default()
        };
        let num_style = if is_selected {
            Style::default().fg(p.overlay1).bg(p.surface0)
        } else if is_active {
            Style::default().fg(p.text).bg(p.active_row_bg)
        } else {
            Style::default().fg(p.overlay0)
        };

        if is_selected || is_active {
            let buf = frame.buffer_mut();
            for x in ws_area.x..ws_area.x + ws_area.width {
                buf[(x, y)].set_style(row_style);
            }
        }

        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("{:<2}", visible_idx + 1), num_style),
                Span::styled(icon, icon_style),
            ])),
            Rect::new(ws_area.x, y, ws_area.width, 1),
        );
    }

    if let Some(divider_y) = divider_y {
        let buf = frame.buffer_mut();
        let divider_color = if app.agent_view_override.is_some() {
            p.accent
        } else {
            p.surface_dim
        };
        for x in ws_area.x..ws_area.x + ws_area.width {
            buf[(x, divider_y)].set_symbol("─");
            buf[(x, divider_y)].set_style(Style::default().fg(divider_color));
        }
    }

    let detail_content_area = Rect::new(
        detail_area.x,
        detail_area.y,
        detail_area.width,
        detail_area.height.saturating_sub(1),
    );
    if detail_content_area != Rect::default() {
        for (detail_idx, detail) in agent_panel_entries(app).iter().enumerate() {
            let y = detail_content_area.y + detail_idx as u16;
            if y >= detail_content_area.y + detail_content_area.height {
                break;
            }
            let position = detail_idx + 1;
            let is_active = app.is_active_pane(detail.ws_idx, detail.tab_idx, detail.pane_id);
            let position_style = if is_active {
                Style::default().fg(p.text).bg(p.active_row_bg)
            } else {
                Style::default().fg(p.overlay0)
            };
            let (icon, icon_style) =
                agent_state_icon(detail.state, detail.seen, app.agent_spinner_frame, p);

            if is_active {
                let buf = frame.buffer_mut();
                for x in detail_content_area.x..detail_content_area.x + detail_content_area.width {
                    buf[(x, y)].set_style(Style::default().bg(p.active_row_bg));
                }
            }

            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(format!("{position:<2}"), position_style),
                    Span::styled(icon, icon_style),
                ])),
                Rect::new(detail_content_area.x, y, detail_content_area.width, 1),
            );
        }
    }

    render_sidebar_toggle(app, frame, area, true, p);
}

pub(crate) fn workspace_drop_slots(
    app: &AppState,
    cards: &[crate::app::state::WorkspaceCardArea],
    area: Rect,
) -> Vec<(crate::app::state::WorkspaceDropTarget, u16)> {
    if area.height == 0 || cards.is_empty() {
        return Vec::new();
    }
    let list_bottom = area.y + area.height.saturating_sub(1);
    let entries = workspace_list_entries(app);
    let entry_position = |ws_idx| {
        entries.iter().position(|entry| {
            matches!(
                entry,
                WorkspaceListEntry::Workspace {
                    ws_idx: entry_ws_idx,
                    ..
                } if *entry_ws_idx == ws_idx
            )
        })
    };
    let block_root_at = |entry_idx: usize| {
        entries[..=entry_idx]
            .iter()
            .rev()
            .find_map(|entry| match entry {
                WorkspaceListEntry::Workspace {
                    ws_idx,
                    indented: false,
                } => Some(*ws_idx),
                WorkspaceListEntry::Workspace { .. } | WorkspaceListEntry::AgentPane { .. } => None,
            })
    };

    let mut slots = Vec::new();
    let mut previous_root = None;
    for card in cards {
        let Some(entry_idx) = entry_position(card.ws_idx) else {
            continue;
        };
        let Some(root_idx) = block_root_at(entry_idx) else {
            continue;
        };
        if previous_root == Some(root_idx) {
            continue;
        }
        previous_root = Some(root_idx);
        if let Some(row) = card.rect.y.checked_sub(1).filter(|row| *row < list_bottom) {
            slots.push((
                crate::app::state::WorkspaceDropTarget::Before(root_idx),
                row,
            ));
        }
    }

    let Some(last) = cards.last() else {
        return slots;
    };
    let Some(last_entry_idx) = entry_position(last.ws_idx) else {
        return slots;
    };
    let next_entry = entries.get(last_entry_idx.saturating_add(1));
    if matches!(
        next_entry,
        Some(WorkspaceListEntry::Workspace { indented: true, .. })
    ) {
        return slots;
    }
    let target = match next_entry {
        Some(WorkspaceListEntry::Workspace { ws_idx, .. }) => {
            crate::app::state::WorkspaceDropTarget::Before(*ws_idx)
        }
        Some(WorkspaceListEntry::AgentPane { .. }) | None => {
            crate::app::state::WorkspaceDropTarget::End
        }
    };
    let row = last.rect.y.saturating_add(last.rect.height);
    if row < list_bottom
        && slots
            .last()
            .is_none_or(|(last_target, _)| *last_target != target)
    {
        slots.push((target, row));
    }
    slots
}

pub(crate) fn workspace_drop_indicator_row(
    app: &AppState,
    cards: &[crate::app::state::WorkspaceCardArea],
    area: Rect,
    target: crate::app::state::WorkspaceDropTarget,
) -> Option<u16> {
    workspace_drop_slots(app, cards, area)
        .into_iter()
        .find_map(|(candidate, row)| (candidate == target).then_some(row))
}

pub(super) fn render_sidebar(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
    area: Rect,
) {
    let p = &app.palette;
    frame
        .buffer_mut()
        .set_style(area, Style::default().bg(p.sidebar_bg));
    let is_navigating = matches!(app.mode, Mode::Navigate);
    let sep_style = if is_navigating {
        Style::default().fg(p.accent)
    } else {
        Style::default().fg(p.surface_dim)
    };

    let sep_x = area.x + area.width.saturating_sub(1);
    let buf = frame.buffer_mut();
    for y in area.y..area.y + area.height {
        buf[(sep_x, y)].set_symbol("│");
        buf[(sep_x, y)].set_style(sep_style);
    }


    let content = sidebar_content_rect(area);
    render_sidebar_view_tabs(app, frame, content);
    match app.sidebar_view {
        crate::app::state::SidebarView::Spaces => {
            render_workspace_list(app, terminal_runtimes, frame, content, is_navigating)
        }
        crate::app::state::SidebarView::Agents => {
            render_agent_detail(app, terminal_runtimes, frame, content)
        }
    }
    render_sidebar_footer(app, frame, content);
    render_sidebar_toggle(app, frame, area, false, p);
}

/// Per-token styles for one rendered row. `workspace` and `agent` are separate
/// because a nested agent row drops the workspace token, so the agent has to
/// carry the row's bright tier there or the whole row renders unreadable.
#[derive(Clone, Copy)]
struct RowStyles {
    state_text: Style,
    workspace: Style,
    agent: Style,
    /// Its own tier: the branch keeps one colour whether it sits on the space row
    /// or on the agent rows that took it over.
    branch: Style,
    secondary: Style,
    custom: Style,
}

fn resolved_token_spans(
    resolved: &[ResolvedToken],
    state_icon: (&str, Style),
    styles: RowStyles,
    p: &Palette,
    max_width: usize,
) -> Vec<Span<'static>> {
    let RowStyles {
        state_text: state_text_style,
        workspace: workspace_style,
        agent: agent_style,
        branch: branch_style,
        secondary: secondary_style,
        custom: custom_style,
    } = styles;
    let fixed_widths = resolved
        .iter()
        .map(|token| match &token.kind {
            ResolvedTokenKind::StateIcon => display_width(state_icon.0),
            ResolvedTokenKind::GitStatus { ahead, behind } => {
                usize::from(*ahead > 0) * display_width(&format!("↑{ahead}"))
                    + usize::from(*behind > 0) * display_width(&format!("↓{behind}"))
                    + usize::from(*ahead > 0 && *behind > 0)
            }
            _ => 0,
        })
        .collect::<Vec<_>>();
    let flexible_widths = resolved
        .iter()
        .map(|token| match &token.kind {
            ResolvedTokenKind::StateText(text)
            | ResolvedTokenKind::Workspace(text)
            | ResolvedTokenKind::Tab(text)
            | ResolvedTokenKind::Pane(text)
            | ResolvedTokenKind::Agent(text)
            | ResolvedTokenKind::TerminalTitle(text)
            | ResolvedTokenKind::Branch(text)
            | ResolvedTokenKind::Custom(text) => display_width(text),
            _ => 0,
        })
        .collect::<Vec<_>>();
    let minimum_width = |active: &[bool]| {
        let indices = active
            .iter()
            .enumerate()
            .filter_map(|(index, active)| active.then_some(index))
            .collect::<Vec<_>>();
        let content = indices
            .iter()
            .map(|index| fixed_widths[*index] + usize::from(flexible_widths[*index] > 0))
            .sum::<usize>();
        let separators = indices
            .windows(2)
            .map(|pair| display_width(tokens::separator(&resolved[pair[0]], &resolved[pair[1]])))
            .sum::<usize>();
        content + separators
    };
    let mut active = resolved.iter().map(|_| true).collect::<Vec<_>>();
    if minimum_width(&active) > max_width {
        for (index, width) in flexible_widths.iter().enumerate() {
            if *width > 0 {
                active[index] = false;
            }
        }
        for index in (0..resolved.len()).rev() {
            if flexible_widths[index] == 0 {
                continue;
            }
            active[index] = true;
            if minimum_width(&active) > max_width {
                active[index] = false;
            }
        }
    }
    let visible_indices = active
        .iter()
        .enumerate()
        .filter_map(|(index, active)| active.then_some(index))
        .collect::<Vec<_>>();
    let separator_width = visible_indices
        .windows(2)
        .map(|pair| display_width(tokens::separator(&resolved[pair[0]], &resolved[pair[1]])))
        .sum::<usize>();
    let fixed_width = visible_indices
        .iter()
        .map(|index| fixed_widths[*index])
        .sum::<usize>();
    let mut budgets = flexible_widths
        .iter()
        .enumerate()
        .map(|(index, width)| usize::from(active[index] && *width > 0))
        .collect::<Vec<_>>();
    let minimum = budgets.iter().sum::<usize>();
    let mut remaining = max_width
        .saturating_sub(separator_width + fixed_width)
        .saturating_sub(minimum);
    while remaining > 0 {
        let mut grew = false;
        for (budget, width) in budgets.iter_mut().zip(&flexible_widths) {
            if *budget > 0 && *budget < *width {
                *budget += 1;
                remaining -= 1;
                grew = true;
                if remaining == 0 {
                    break;
                }
            }
        }
        if !grew {
            break;
        }
    }
    let mut spans = Vec::new();
    for (position, index) in visible_indices.iter().copied().enumerate() {
        let token = &resolved[index];
        if position > 0 {
            let previous = &resolved[visible_indices[position - 1]];
            spans.push(Span::styled(
                tokens::separator(previous, token),
                Style::default().fg(p.overlay0).add_modifier(Modifier::DIM),
            ));
        }
        match &token.kind {
            ResolvedTokenKind::StateIcon => {
                spans.push(Span::styled(
                    state_icon.0.to_string(),
                    apply_token_style(state_icon.1, token.style),
                ));
            }
            ResolvedTokenKind::StateText(text) => {
                spans.push(Span::styled(
                    truncate_end(text, budgets[index]),
                    apply_token_style(state_text_style, token.style),
                ));
            }
            ResolvedTokenKind::Workspace(text) => {
                spans.push(Span::styled(
                    truncate_end(text, budgets[index]),
                    apply_token_style(workspace_style, token.style),
                ));
            }
            ResolvedTokenKind::Agent(text) => {
                spans.push(Span::styled(
                    truncate_end(text, budgets[index]),
                    apply_token_style(agent_style, token.style),
                ));
            }
            ResolvedTokenKind::Branch(text) => {
                spans.push(Span::styled(
                    truncate_end(text, budgets[index]),
                    apply_token_style(branch_style, token.style),
                ));
            }
            ResolvedTokenKind::Tab(text) | ResolvedTokenKind::Pane(text) => {
                spans.push(Span::styled(
                    truncate_end(text, budgets[index]),
                    apply_token_style(secondary_style, token.style),
                ));
            }
            ResolvedTokenKind::GitStatus { ahead, behind } => {
                if *ahead > 0 {
                    spans.push(Span::styled(
                        format!("↑{ahead}"),
                        apply_token_style(Style::default().fg(p.green), token.style),
                    ));
                }
                if *ahead > 0 && *behind > 0 {
                    spans.push(Span::styled(
                        " ",
                        apply_token_style(Style::default(), token.style),
                    ));
                }
                if *behind > 0 {
                    spans.push(Span::styled(
                        format!("↓{behind}"),
                        apply_token_style(Style::default().fg(p.red), token.style),
                    ));
                }
            }
            ResolvedTokenKind::TerminalTitle(text) | ResolvedTokenKind::Custom(text) => {
                spans.push(Span::styled(
                    truncate_end(text, budgets[index]),
                    apply_token_style(custom_style, token.style),
                ));
            }
        }
    }
    spans
}

fn apply_token_style(mut style: Style, patch: crate::config::SidebarTokenStyle) -> Style {
    if let Some(fg) = patch.fg {
        style = style.fg(fg.ratatui());
    }
    if let Some(bold) = patch.bold {
        style = if bold {
            style.add_modifier(Modifier::BOLD)
        } else {
            style.remove_modifier(Modifier::BOLD)
        };
    }
    if let Some(dim) = patch.dim {
        style = if dim {
            style.add_modifier(Modifier::DIM)
        } else {
            style.remove_modifier(Modifier::DIM)
        };
    }
    style
}

fn render_workspace_list(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
    area: Rect,
    is_navigating: bool,
) {
    let p = &app.palette;
    let dragged_ws_idx = match app.drag.as_ref().map(|drag| &drag.target) {
        Some(crate::app::state::DragTarget::WorkspaceReorder { source_ws_idx, .. }) => {
            Some(*source_ws_idx)
        }
        _ => None,
    };
    let insertion_row = match app.drag.as_ref().map(|drag| &drag.target) {
        Some(crate::app::state::DragTarget::WorkspaceReorder {
            drop_target: Some(drop_target),
            ..
        }) => workspace_drop_indicator_row(app, &app.view.workspace_card_areas, area, *drop_target),
        _ => None,
    };

    let list_bottom = area.y + area.height.saturating_sub(1);
        frame.render_widget(
        );
    }

    let layout = TreeLayout::build(app, Some(terminal_runtimes));
    let metrics = workspace_list_scroll_metrics_with(app, area, &layout);
    let scrollbar_rect = workspace_list_scrollbar_rect_from(area, metrics);
    let cards = &app.view.workspace_card_areas;
    let entries = workspace_list_entries(app);

    // a top-level space always starts a block, so the gap row directly above it
    // is where the separator goes; at row_gap 0 that row holds content instead
    let list_body = workspace_list_body_rect(area, should_show_scrollbar(metrics));
    // every top-level space carries the number the collapsed sidebar shows for it.
    // The slot is sized once from the whole list so the names all end in the same
    // column rather than shifting when the count crosses ten.
    let space_numbers = cards
        .iter()
        .filter(|card| !card.indented)
        .enumerate()
        .map(|(position, card)| (card.ws_idx, position + 1))
        .collect::<std::collections::HashMap<_, _>>();
    let number_width = space_numbers
        .values()
        .copied()
        .max()
        // sized from the label rather than the digits: an enclosed form is one cell
        // where its number would be two
        .map_or(0, |highest| {
            space_number_label(highest).chars().count() as u16
        });
    for card in cards {
        if app.sidebar_spaces.row_gap == 0 || card.indented || card.rect.width == 0 {
            continue;
        }
        let Some(rule_y) = card.rect.y.checked_sub(1) else {
            continue;
        };
        if rule_y < list_body.y || rule_y >= list_bottom {
            continue;
        }
        frame.render_widget(
            Paragraph::new(Span::styled(
                "─".repeat(card.rect.width as usize),
                Style::default().fg(p.space_rule()),
            )),
            Rect::new(card.rect.x, rule_y, card.rect.width, 1),
        );
    }

    for card in cards {
        let i = card.ws_idx;
        let ws = &app.workspaces[i];
        let row_y = card.rect.y;
        let row_height = card.rect.height;
        let selected = i == app.selected && is_navigating;
        let is_active = Some(i) == app.active;
        let is_dragged = dragged_ws_idx == Some(i);
        // the focused agent's own row carries the highlight, so painting the
        // space row too would mark the same thing twice
        let focused_agent_row_shown =
            app.view.sidebar_agent_row_areas.iter().any(|row| {
                row.ws_idx == i && app.is_active_pane(row.ws_idx, row.tab_idx, row.pane_id)
            });
        let agent_rows_shown = app
            .view
            .sidebar_agent_row_areas
            .iter()
            .any(|row| row.ws_idx == i);
        let highlighted = selected || (is_active && !focused_agent_row_shown) || is_dragged;
        let (agg_state, agg_seen) = ws.aggregate_state(&app.terminals);

        if highlighted {
            let bg = if selected {
                p.surface0
            } else if is_dragged {
                p.surface1
            } else {
                p.active_row_bg
            };
            let mark_style = selection_mark_style(p, bg);
            let buf = frame.buffer_mut();
            for y in row_y..row_y + row_height {
                if y >= list_bottom {
                    break;
                }
                for x in card.rect.x..card.rect.x + card.rect.width {
                    buf[(x, y)].set_style(Style::default().bg(bg));
                }
                // a space only carries the highlight when no agent row does, and it
                // has to be marked the same way those rows are
                if let Some(edge) = card
                    .rect
                    .x
                    .checked_add(card.rect.width)
                    .and_then(|right| right.checked_sub(SELECTION_MARK_WIDTH))
                {
                    buf[(edge, y)].set_symbol(SELECTION_MARK);
                    buf[(edge, y)].set_style(mark_style);
                }
            }
        }

        // the brightest tier in the tree: a space is the thing you scan for first,
        // and its chevron still carries the accent that marks it as interactive
        let name_style = Style::default().fg(p.text);
        let name_style = if selected || is_active || is_dragged {
            name_style.add_modifier(Modifier::BOLD)
        } else {
            name_style
        };

        let label = ws.display_name_from(&app.terminals, terminal_runtimes);
        let display_label = if card.indented {
            grouped_child_display_label(&label, ws.branch().as_deref(), ws.custom_name.is_some())
        } else {
            label
        };
        let parent_group = (!card.indented)
            .then(|| workspace_parent_group_state(app, i))
            .flatten();
        let space_entry_idx = entries.iter().position(|entry| {
            matches!(
                entry,
                WorkspaceListEntry::Workspace { ws_idx, .. } if *ws_idx == i
            )
        });
        let is_last_child = card.indented
            && space_entry_idx
                .is_none_or(|entry_idx| !next_entry_is_indented_workspace(&entries, entry_idx));
        // the same predicate on the same list layout used: `entries` here holds only
        // space rows, so asking it whether an agent row follows always says no
        let suppress_branch = layout
            .entries
            .iter()
            .position(|entry| {
                matches!(entry, WorkspaceListEntry::Workspace { ws_idx, .. } if *ws_idx == i)
            })
            .is_some_and(|entry_idx| space_shows_agent_rows(&layout.entries, entry_idx));
        let (display_state, display_seen) = parent_group
            .as_ref()
            .filter(|(_, collapsed)| *collapsed)
            .map(|(key, _)| space_aggregate_state(app, key))
            .unwrap_or((agg_state, agg_seen));
        // each agent row carries its own dot, so the space only speaks for them
        // while they are hidden: collapsed, or no agent under it at all
        // the dot keeps the near column: the agent rows hang their own indicator
        // under it, so it is the line the whole block is built on
        let state_icon = if agent_rows_shown {
            state_dot(AgentState::Unknown, false, p)
        } else {
            state_dot(display_state, display_seen, p)
        };
        // an ordinal is a handle rather than content and rides the far edge, where its
        // width can change without moving a single name
        let ordinal = space_numbers
            .get(&i)
            .filter(|_| number_width > 0)
            .map(|number| space_number_label(*number));
        let state_text_style = Style::default()
            .fg(state_label_color(display_state, display_seen, p))
            .add_modifier(Modifier::DIM);
        // colour says what kind of token this is, weight says whether it is active:
        // a branch that only turned mauve when focused sat on the agent tier otherwise
        let branch_style = Style::default().fg(p.mauve);
        let token_values = ws.metadata_tokens.values();
        let rows = tokens::space_rows(
            &app.sidebar_spaces,
            SpaceTokenContext {
                workspace: &display_label,
                branch: ws.branch().as_deref(),
                state_text: state_label(display_state, display_seen),
                ahead_behind: ws.git_ahead_behind(),
                tokens: &token_values,
                suppress_git_details: card.indented,
                suppress_branch,
            },
        );

        for (row_index, resolved) in rows.iter().enumerate() {
            if row_index as u16 >= row_height || row_y + row_index as u16 >= list_bottom {
                break;
            }
            let mut spans = Vec::new();
            // every block hangs from its own state-dot column: a top-level space
            // marks col 1, so its branch and children start there too
            let prefix_width = if card.indented {
                spans.push(Span::raw(" "));
                if row_index == 0 {
                    spans.push(Span::styled(
                        if is_last_child { "└─ " } else { "├─ " },
                        Style::default().fg(p.overlay0),
                    ));
                    4
                } else if is_last_child {
                    spans.push(Span::raw("   "));
                    4
                } else {
                    spans.push(Span::styled("│", Style::default().fg(p.overlay0)));
                    spans.push(Span::raw("  "));
                    4
                }
            } else if row_index == 0 {
                spans.push(Span::raw(" "));
                1
            } else {
                // the same indicator column an agent row would have. Only a space
                // with nothing running keeps a second row, and nothing running is
                // exactly what the structural tier means.
                spans.push(Span::raw(" "));
                spans.push(Span::styled("│", Style::default().fg(p.surface1)));
                spans.push(Span::raw(" "));
                3
            };
            // the selection rule's column plus the ordinal's slot, held back on every
            // row so a name never shifts as the highlight moves between rows
            let trailing_width = SELECTION_MARK_WIDTH + number_width;
            spans.extend(resolved_token_spans(
                resolved,
                state_icon,
                RowStyles {
                    state_text: state_text_style,
                    workspace: name_style,
                    agent: branch_style,
                    branch: branch_style,
                    secondary: branch_style,
                    custom: branch_style,
                },
                p,
                card.rect
                    .width
                    .saturating_sub(prefix_width + trailing_width) as usize,
            ));
            frame.render_widget(
                Paragraph::new(Line::from(spans)),
                Rect::new(card.rect.x, row_y + row_index as u16, card.rect.width, 1),
            );
        }

        if let Some(text) = ordinal.filter(|_| row_y < list_bottom) {
            let slot = card
                .rect
                .x
                .saturating_add(card.rect.width)
                .saturating_sub(SELECTION_MARK_WIDTH + number_width);
            frame.render_widget(
                // one tier up from the rules: an ordinal is a handle rather than
                // content, but a handle you cannot read is not one. DIM on top of
                // this tier is what made it unreadable.
                Paragraph::new(Span::styled(text, Style::default().fg(p.overlay0)))
                    .alignment(Alignment::Right),
                Rect::new(slot, row_y, number_width, 1),
            );
        }
    }

    render_nested_agent_rows(app, &layout, frame, list_bottom);

    if let Some(y) = insertion_row.filter(|y| *y < list_bottom) {
        let indicator_right = scrollbar_rect
            .map(|rect| rect.x)
            .unwrap_or(area.x + area.width);
        let buf = frame.buffer_mut();
        for x in area.x..indicator_right {
            buf[(x, y)].set_symbol("─");
            buf[(x, y)].set_style(Style::default().fg(p.accent));
        }
    }

    if let Some(track) = scrollbar_rect {
        render_scrollbar(frame, metrics, track, p.surface_dim, p.overlay0, "▕");
    }
}

/// The new/menu row at the bottom of the sidebar, shared by both views.
fn render_sidebar_footer(app: &AppState, frame: &mut Frame, content: Rect) {
    if !app.mouse_capture || content.height < 2 {
        return;
    }

    let p = &app.palette;
    frame.render_widget(
        Paragraph::new(Span::styled(" new", Style::default().fg(p.overlay0))),
        app.sidebar_new_button_rect(),
    );

    let menu_line = if app.global_menu_attention_badge_visible() {
        Line::from(vec![
            Span::styled(
                "● ",
                Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled("menu", Style::default().fg(p.overlay0)),
        ])
    } else {
        Line::from(vec![Span::styled("menu", Style::default().fg(p.overlay0))])
    };
    frame.render_widget(
        Paragraph::new(menu_line).alignment(Alignment::Center),
        app.global_launcher_rect(),
    );
}

        frame.render_widget(
        );

        } else {
        };
/// Agent rows nested under their space row. Geometry comes from the cached
/// areas so hit testing and rendering can never disagree.
fn render_nested_agent_rows(
    app: &AppState,
    layout: &TreeLayout,
    frame: &mut Frame,
    list_bottom: u16,
) {
    let areas = &app.view.sidebar_agent_row_areas;
    if areas.is_empty() {
        return;
    }

    let p = &app.palette;
    let entries = &layout.agents;
    let discriminators = nested_row_discriminators(app, layout);
    let one_tab = spaces_with_one_tab_label(layout);

    // one tier under the rule between spaces: same job, but a space boundary
    // outranks a boundary between two agents of the same space
    for pair in areas.windows(2) {
        let (row, next) = (&pair[0], &pair[1]);
        let rule_y = row.rect.y.saturating_add(row.rect.height);
        // a different space means the space rule already owns this boundary, and a
        // missing gap row means the rows are packed with nowhere to put one
        if row.ws_idx != next.ws_idx || rule_y >= list_bottom || rule_y >= next.rect.y {
            continue;
        }
        frame.render_widget(
            Paragraph::new(Span::styled(
                "─".repeat(row.rect.width as usize),
                Style::default().fg(p.surface1),
            )),
            Rect::new(row.rect.x, rule_y, row.rect.width, 1),
        );
    }

    for area in areas {
        let Some(entry) = entries.get(&(area.ws_idx, area.pane_id)) else {
            continue;
        };
        let mut rows = resolved_nested_agent_rows(app, entry);
        if one_tab.contains(&area.ws_idx) {
            for row in &mut rows {
                row.retain(|token| !matches!(token.kind, ResolvedTokenKind::Tab(_)));
            }
            rows.retain(|row| !row.is_empty());
        }
        // appended to the last row, so this never changes the row's height
        if let Some((extra, last)) = discriminators
            .get(&(area.ws_idx, area.pane_id))
            .zip(rows.last_mut())
        {
            last.push(ResolvedToken::plain(ResolvedTokenKind::Pane(extra.clone())));
        }
        // a real row rather than bare height, so the highlight and the row's own
        // indentation cover the blank line the same way they cover the content
        rows.resize(rows.len().max(NESTED_AGENT_ROW_MIN_HEIGHT), Vec::new());
        let is_active = app.is_active_pane(area.ws_idx, area.tab_idx, area.pane_id);
        let label_color = state_label_color(entry.state, entry.seen, p);
        // one tier below the space names above them, and a different hue from the
        // branch beside them, so the three kinds of name never read as one run
        let name_style = Style::default().fg(p.blue);
        let name_style = if is_active {
            name_style.add_modifier(Modifier::BOLD)
        } else {
            name_style
        };
        let status_style = if is_active {
            Style::default().fg(label_color)
        } else {
            Style::default().fg(label_color).add_modifier(Modifier::DIM)
        };
        // overlay0 already sits at ~3.4:1; DIM on top of it lands near 1.4:1
        let secondary_style = Style::default().fg(p.overlay0);
        let (indicator, indicator_style) =
            agent_state_cells(entry.state, entry.seen, app.agent_spinner_frame, p);

        if is_active {
            let mark_style = selection_mark_style(p, p.surface_dim);
            let buf = frame.buffer_mut();
            for y in area.rect.y..area.rect.y + area.rect.height {
                if y >= list_bottom {
                    break;
                }
                for x in area.rect.x..area.rect.x + area.rect.width {
                    // a tier under the indicator and the rules: the fill marks the
                    // row, and anything drawn on it has to stay readable on top
                    buf[(x, y)].set_style(Style::default().bg(p.surface_dim));
                }
                if let Some(edge) = area
                    .rect
                    .x
                    .checked_add(area.rect.width)
                    .and_then(|right| right.checked_sub(SELECTION_MARK_WIDTH))
                {
                    // a full-cell glyph rather than an eighth-block sliver: the
                    // sliver could be painted over by a long line of text
                    buf[(edge, y)].set_symbol(SELECTION_MARK);
                    buf[(edge, y)].set_style(mark_style);
                }
            }
        }

        for (row_index, resolved) in rows.iter().enumerate() {
            if row_index as u16 >= area.rect.height || area.rect.y + row_index as u16 >= list_bottom
            {
                break;
            }
            let mut spans = Vec::new();
            // flush left with the space rows: the expand chevron is what marks a
            // space row, so nesting needs neither connectors nor indentation
            let mut prefix_width = 1u16;
            spans.push(Span::raw(" "));
            if row_index > 0 {
                // the indicator owns this column on both rows, so the two of them
                // stack into one shape rather than a mark with a gap beneath it
                spans.push(Span::styled(indicator[1].clone(), indicator_style));
                spans.push(Span::raw(" "));
                prefix_width = prefix_width.saturating_add(2);
            }
            spans.extend(resolved_token_spans(
                resolved,
                (indicator[0].as_str(), indicator_style),
                RowStyles {
                    state_text: status_style,
                    workspace: name_style,
                    // the agent names this row, so it carries the bright tier
                    agent: name_style,
                    branch: Style::default().fg(p.mauve),
                    secondary: secondary_style,
                    custom: secondary_style,
                },
                p,
                area.rect
                    .width
                    .saturating_sub(prefix_width + SELECTION_MARK_WIDTH) as usize,
            ));
            frame.render_widget(
                Paragraph::new(Line::from(spans)),
                Rect::new(
                    area.rect.x,
                    area.rect.y + row_index as u16,
                    area.rect.width,
                    1,
                ),
            );
        }
    }
}

fn render_agent_detail(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
    area: Rect,
) {
    let p = &app.palette;

    if area.height < 3 {
        return;
    }

    );

    );
    let control_label = active_agent_view_label(app)
        .unwrap_or_else(|| agent_panel_sort_label(app.agent_panel_sort));
    let toggle_rect = agent_panel_header_label_rect(area, control_label);
    if toggle_rect != Rect::default() {
        let color = if app.agent_view_override.is_some() {
            p.accent
        } else {
            p.overlay0
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                control_label,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Right),
            toggle_rect,
        );
    }

    let details = agent_panel_entries_from(app, terminal_runtimes);
    let metrics = agent_panel_scroll_metrics(app, area);
    let scrollbar_rect = agent_panel_scrollbar_rect(app, area);
    let body = agent_panel_body_rect(area, should_show_scrollbar(metrics));
    if body == Rect::default() {
        return;
    }
    if details.is_empty() && app.agent_view_override.is_some() {
        frame.render_widget(
            Paragraph::new(" no matching agents")
                .style(Style::default().fg(p.overlay0).add_modifier(Modifier::DIM)),
            Rect::new(body.x, body.y, body.width, 1),
        );
        return;
    }

    let scroll = app.agent_panel_scroll.min(metrics.max_offset_from_bottom);
    let mut row_y = body.y;
    let body_bottom = body.y + body.height;
    for (index, detail) in details.iter().enumerate().skip(scroll) {
        let label_color = state_label_color(detail.state, detail.seen, p);
        let rows = resolved_agent_rows(app, detail);
        let height = (rows.len().max(1) as u16).min(body.height);
        if row_y.saturating_add(height) > body_bottom {
            break;
        }

        let is_active = app.is_active_pane(detail.ws_idx, detail.tab_idx, detail.pane_id);
        let row_style = if is_active {
            // same tier the tree gives the focused agent row
            Style::default().bg(p.active_row_bg)
        } else {
            Style::default()
        };
        let name_style = if is_active {
            Style::default().fg(p.text).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(p.subtext0).add_modifier(Modifier::BOLD)
        };
        let status_style = if is_active {
            Style::default().fg(label_color)
        } else {
            Style::default().fg(label_color).add_modifier(Modifier::DIM)
        };
        let agent_style = Style::default().fg(p.overlay0).add_modifier(Modifier::DIM);
        let state_icon = agent_state_icon(detail.state, detail.seen, app.agent_spinner_frame, p);

        for (row_index, resolved) in rows.iter().take(height as usize).enumerate() {
            let mut spans = vec![Span::raw(if row_index == 0 { " " } else { "   " })];
            spans.extend(resolved_token_spans(
                resolved,
                state_icon,
                RowStyles {
                    state_text: status_style,
                    workspace: name_style,
                    agent: agent_style,
                    branch: Style::default().fg(p.mauve),
                    secondary: agent_style,
                    custom: agent_style,
                },
                p,
                body.width
                    .saturating_sub(if row_index == 0 { 1 } else { 3 }) as usize,
            ));
            frame.render_widget(
                Paragraph::new(Line::from(spans)).style(row_style),
                Rect::new(body.x, row_y + row_index as u16, body.width, 1),
            );
        }
        row_y = row_y
            .saturating_add(height)
            .saturating_add(agent_entry_gap(app, index, details.len()))
            .min(body_bottom);
    }

    if let Some(track) = scrollbar_rect {
        render_scrollbar(frame, metrics, track, p.surface_dim, p.overlay0, "▕");
    }
}

pub(crate) fn collapsed_sidebar_toggle_rect(area: Rect) -> Rect {
    let bottom_y = area.y + area.height.saturating_sub(1);
    let content_w = area.width.saturating_sub(1);
    if content_w == 0 || area.height == 0 {
        return Rect::default();
    }
    let x = area.x + content_w / 2;
    Rect::new(x, bottom_y, 1, 1)
}

pub(crate) fn expanded_sidebar_toggle_rect(area: Rect) -> Rect {
    if area.width <= 1 || area.height == 0 {
        return Rect::default();
    }
    Rect::new(
        area.x + area.width.saturating_sub(2),
        area.y + area.height.saturating_sub(1),
        1,
        1,
    )
}

fn render_sidebar_toggle(
    app: &AppState,
    frame: &mut Frame,
    area: Rect,
    collapsed: bool,
    p: &Palette,
) {
    let toggle_area = if collapsed {
        collapsed_sidebar_toggle_rect(area)
    } else {
        expanded_sidebar_toggle_rect(area)
    };
    if toggle_area == Rect::default() {
        return;
    }
    let icon = if collapsed { "»" } else { "«" };
    let icon_style = if collapsed && app.global_menu_attention_badge_visible() {
        Style::default().fg(p.accent).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(p.overlay0)
    };
    frame.render_widget(Paragraph::new(Span::styled(icon, icon_style)), toggle_area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{detect::Agent, layout::PaneId, workspace::Workspace};
    use ratatui::{backend::TestBackend, layout::Direction, Terminal};

    fn row_text(buffer: &ratatui::buffer::Buffer, row: u16, width: u16) -> String {
        (0..width)
            .map(|x| buffer[(x, row)].symbol())
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    fn find_symbol_x(buffer: &ratatui::buffer::Buffer, row: u16, width: u16, symbol: &str) -> u16 {
        (0..width)
            .find(|x| buffer[(*x, row)].symbol() == symbol)
            .unwrap_or_else(|| {
                panic!(
                    "missing symbol {symbol:?} in row {}",
                    row_text(buffer, row, width)
                )
            })
    }

    #[test]
    fn expanded_and_collapsed_sidebars_use_custom_background() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces.clear();
        app.active = None;
        app.palette.sidebar_bg = ratatui::style::Color::Rgb(12, 34, 56);
        let area = Rect::new(0, 0, 26, 20);

        let mut expanded = Terminal::new(TestBackend::new(26, 20)).unwrap();
        expanded
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        assert!(expanded
            .backend()
            .buffer()
            .content
            .iter()
            .all(|cell| cell.bg == app.palette.sidebar_bg));

        let mut collapsed = Terminal::new(TestBackend::new(26, 20)).unwrap();
        collapsed
            .draw(|frame| render_sidebar_collapsed(&app, frame, area))
            .unwrap();
        assert!(collapsed
            .backend()
            .buffer()
            .content
            .iter()
            .all(|cell| cell.bg == app.palette.sidebar_bg));
    }

    #[test]
    fn default_agent_rows_remove_redundant_state_text() {
        let mut app = crate::app::state::AppState::test_new();
        let workspace = Workspace::test_new("one");
        let pane_id = workspace.tabs[0].root_pane;
        app.workspaces = vec![workspace];
        app.ensure_test_terminals();
        app.active = Some(0);
        let terminal_id = app.workspaces[0].tabs[0].panes[&pane_id]
            .attached_terminal_id
            .clone();
        let terminal_state = app.terminals.get_mut(&terminal_id).unwrap();
        terminal_state.detected_agent = Some(Agent::Pi);
        terminal_state.state = AgentState::Working;

        app.sidebar_view = crate::app::state::SidebarView::Agents;
        let area = Rect::new(0, 0, 26, 20);
        let mut terminal = Terminal::new(TestBackend::new(26, 20)).unwrap();
        terminal
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let agent_area = sidebar_content_rect(area);
        let body = agent_panel_body_rect(agent_area, false);

        let first = row_text(buffer, body.y, 25);
        let second = row_text(buffer, body.y + 1, 25);
        assert!(first.contains("one"));
        assert_eq!(second, "   pi");
        assert!(!first.contains("working"));
        assert!(!second.contains("working"));

        let workspace_x = find_symbol_x(buffer, body.y, body.width, "o");
        let workspace_style = buffer[(workspace_x, body.y)].style();
        assert_eq!(workspace_style.fg, Some(app.palette.text));
        assert!(workspace_style.add_modifier.contains(Modifier::BOLD));
        assert!(!workspace_style.add_modifier.contains(Modifier::DIM));
        assert_eq!(workspace_style.bg, Some(app.palette.active_row_bg));

        let agent_x = find_symbol_x(buffer, body.y + 1, body.width, "p");
        let agent_style = buffer[(agent_x, body.y + 1)].style();
        assert_eq!(agent_style.fg, Some(app.palette.overlay0));
        assert!(agent_style.add_modifier.contains(Modifier::DIM));
        assert!(!agent_style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(agent_style.bg, Some(app.palette.active_row_bg));
    }

    #[test]
    fn occurrence_false_removes_default_workspace_bold_and_agent_dim() {
        let config: crate::config::Config = toml::from_str(
            r##"
[ui.sidebar.agents]
rows = [[{ token = "workspace", bold = false }, { token = "agent", dim = false }]]
"##,
        )
        .unwrap();
        let mut app = crate::app::state::AppState::test_new();
        app.sidebar_agents = config.ui.sidebar.agents;
        let workspace = Workspace::test_new("one");
        let pane_id = workspace.tabs[0].root_pane;
        app.workspaces = vec![workspace];
        app.ensure_test_terminals();
        app.active = Some(0);
        let terminal_id = app.workspaces[0].tabs[0].panes[&pane_id]
            .attached_terminal_id
            .clone();
        app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(Agent::Pi);

        app.sidebar_view = crate::app::state::SidebarView::Agents;

        let area = Rect::new(0, 0, 26, 20);
        let mut terminal = Terminal::new(TestBackend::new(26, 20)).unwrap();
        terminal
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let agent_area = sidebar_content_rect(area);
        let body = agent_panel_body_rect(agent_area, false);
        let buffer = terminal.backend().buffer();
        let workspace = buffer[(find_symbol_x(buffer, body.y, body.width, "o"), body.y)].style();
        let agent = buffer[(find_symbol_x(buffer, body.y, body.width, "p"), body.y)].style();

        assert_eq!(workspace.fg, Some(app.palette.text));
        assert!(!workspace.add_modifier.contains(Modifier::BOLD));
        assert_eq!(agent.fg, Some(app.palette.overlay0));
        assert!(!agent.add_modifier.contains(Modifier::DIM));
    }

    /// A worktree child is named after its own branch, so an agent hanging under it
    /// would otherwise print that branch directly beneath the identical space name.
    /// A top-level space keeps its own name, so its agents still carry the branch.
    #[test]
    fn worktree_child_agents_drop_a_branch_that_repeats_the_space_name() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_worktree_space("herdr", Some("repo-key"), "/repo/herdr"),
            workspace_with_worktree_space("feat-x", Some("repo-key"), "/repo/herdr-feat-x"),
        ];
        if let Some(space) = app.workspaces[1].worktree_space.as_mut() {
            space.is_linked_worktree = true;
        }
        app.workspaces[0].cached_git_branch = Some("main".into());
        app.workspaces[1].cached_git_branch = Some("feat-x".into());
        app.ensure_test_terminals();
        app.active = Some(0);
        for ws_idx in 0..2 {
            let pane_id = app.workspaces[ws_idx].tabs[0].root_pane;
            let terminal_id = app.workspaces[ws_idx].tabs[0].panes[&pane_id]
                .attached_terminal_id
                .clone();
            app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(Agent::Pi);
        }

        let area = Rect::new(0, 0, 30, 20);
        crate::ui::compute_view(&mut app, Rect::new(0, 0, 110, 20));
        let mut terminal = Terminal::new(TestBackend::new(30, 20)).unwrap();
        terminal
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let rows = &app.view.sidebar_agent_row_areas;

        let parent_agent = row_text(buffer, rows[0].rect.y, 30);
        assert!(parent_agent.contains("main"), "parent: {parent_agent:?}");
        let child_agent = row_text(buffer, rows[1].rect.y, 30);
        assert!(!child_agent.contains("feat-x"), "child: {child_agent:?}");
        assert!(child_agent.contains("pi"), "child: {child_agent:?}");
    }

    /// The branch lives on the agent rows now, so the space row above them drops to
    /// one line — and takes the branch back the moment those rows are hidden.
    #[test]
    fn space_row_drops_the_branch_line_while_agent_rows_carry_it() {
        let mut app = AppState::test_new();
        let workspace = Workspace::test_new("one");
        let pane_id = workspace.tabs[0].root_pane;
        app.workspaces = vec![workspace];
        app.workspaces[0].cached_git_branch = Some("feature-x".into());
        // ahead/behind keeps the space's second row alive after the branch is
        // dropped, which is exactly when a stale suppression check shows up
        app.workspaces[0].cached_git_ahead_behind = Some((1, 0));
        app.ensure_test_terminals();
        app.active = Some(0);
        let terminal_id = app.workspaces[0].tabs[0].panes[&pane_id]
            .attached_terminal_id
            .clone();
        app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(Agent::Pi);

        let area = Rect::new(0, 0, 26, 20);
        let (cards, agent_rows) = compute_workspace_list_areas(&app, area);
        assert_eq!(cards[0].rect.height, 1);
        let agent_row = agent_rows[0].rect.y;
        app.view.workspace_card_areas = cards;
        app.view.sidebar_agent_row_areas = agent_rows;
        let mut terminal = Terminal::new(TestBackend::new(26, 20)).unwrap();
        terminal
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let rendered = row_text(buffer, agent_row, 26);
        assert!(rendered.contains("feature-x"), "rendered: {rendered:?}");
        // the drawn space row has to agree with the height layout gave it, or the
        // branch comes back on screen while the card is still one row tall
        let space_row = row_text(buffer, app.view.workspace_card_areas[0].rect.y, 26);
        assert!(!space_row.contains("feature-x"), "space row: {space_row:?}");
        // the ahead count rides up rather than holding a line open by itself
        assert!(space_row.contains("↑1"), "space row: {space_row:?}");

        // with no agent under it there is nowhere else for the branch to live
        app.terminals.get_mut(&terminal_id).unwrap().detected_agent = None;
        let (bare_cards, _) = compute_workspace_list_areas(&app, area);
        assert_eq!(bare_cards[0].rect.height, 2);
    }

    /// An ordinal reads as "which one", and staying one cell wide is what keeps the
    /// slot it hangs in narrow; past the range Unicode encloses it has to widen, and
    /// the slot is sized from the highest number so no name shifts.
    #[test]
    fn space_ordinals_are_enclosed_while_unicode_still_encloses_them() {
        for (number, expected) in [(1, "①"), (9, "⑨"), (20, "⑳")] {
            let label = space_number_label(number);
            assert_eq!(label, expected);
            assert_eq!(unicode_width::UnicodeWidthStr::width(label.as_str()), 1);
        }
        assert_eq!(space_number_label(21), "21");
        // never truncated: a wrong number is worse than a wider column
        assert_eq!(space_number_label(120), "120");
    }

    /// A space with nothing running still reads as one block: its branch line hangs
    /// under the name in the column an agent row would have used for its indicator.
    #[test]
    fn a_space_without_agents_keeps_the_indicator_column_on_its_branch_row() {
        let mut app = AppState::test_new();
        app.workspaces = vec![Workspace::test_new("one")];
        app.workspaces[0].cached_git_branch = Some("feature-x".into());
        app.ensure_test_terminals();
        app.active = Some(0);

        let area = Rect::new(0, 0, 26, 20);
        let (cards, agent_rows) = compute_workspace_list_areas(&app, area);
        assert!(agent_rows.is_empty(), "no agent should be detected here");
        app.view.workspace_card_areas = cards;
        let card = app.view.workspace_card_areas[0].rect;
        assert_eq!(card.height, 2);

        let mut terminal = Terminal::new(TestBackend::new(26, 20)).unwrap();
        terminal
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let branch_row = row_text(buffer, card.y + 1, 26);

        assert!(branch_row.starts_with(" │ feature-x"), "{branch_row:?}");
        // the same tier the rules and an idle agent use: nothing here wants you
        assert_eq!(
            buffer[(card.x + 1, card.y + 1)].style().fg,
            Some(app.palette.surface1)
        );
    }

    /// The space dot is a fallback, not a duplicate: it aggregates its agents only
    /// while their own rows are hidden, and goes neutral once they are on screen.
    #[test]
    fn space_state_dot_defers_to_visible_agent_rows() {
        let mut app = AppState::test_new();
        let workspace = Workspace::test_new("one");
        let pane_id = workspace.tabs[0].root_pane;
        app.workspaces = vec![workspace];
        app.ensure_test_terminals();
        app.active = Some(0);
        let terminal_id = app.workspaces[0].tabs[0].panes[&pane_id]
            .attached_terminal_id
            .clone();
        let terminal = app.terminals.get_mut(&terminal_id).unwrap();
        terminal.detected_agent = Some(Agent::Pi);
        terminal.state = AgentState::Working;

        let area = Rect::new(0, 0, 26, 20);
        // the dot keeps the near column; the ordinal rides the far edge
        let expanded = space_row_text(&mut app, area);
        assert!(expanded.starts_with(" – one"), "expanded: {expanded:?}");
        assert!(expanded.contains('\u{2460}'), "expanded: {expanded:?}");

        // with its agent row gone the space has to speak for it again
        app.terminals.get_mut(&terminal_id).unwrap().detected_agent = None;
        let bare = space_row_text(&mut app, area);
        assert!(bare.contains('●'), "bare: {bare:?}");
    }

    fn space_row_text(app: &mut AppState, area: Rect) -> String {
        let (card_areas, agent_row_areas) = compute_workspace_list_areas(app, area);
        app.view.workspace_card_areas = card_areas;
        app.view.sidebar_agent_row_areas = agent_row_areas;
        let row = app.view.workspace_card_areas[0].rect.y;
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
        terminal
            .draw(|frame| render_sidebar(app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();

        let first = row_text(buffer, body.y, 25);
        let second = row_text(buffer, body.y + 1, 25);
        assert!(first.contains("one"));
        assert_eq!(second, "   pi");
        assert!(!first.contains("working"));
        assert!(!second.contains("working"));

        let workspace_x = find_symbol_x(buffer, body.y, body.width, "o");
        let workspace_style = buffer[(workspace_x, body.y)].style();
        assert_eq!(workspace_style.fg, Some(app.palette.text));
        assert!(workspace_style.add_modifier.contains(Modifier::BOLD));
        assert!(!workspace_style.add_modifier.contains(Modifier::DIM));
        assert_eq!(workspace_style.bg, Some(app.palette.active_row_bg));

        let agent_x = find_symbol_x(buffer, body.y + 1, body.width, "p");
        let agent_style = buffer[(agent_x, body.y + 1)].style();
        assert_eq!(agent_style.fg, Some(app.palette.overlay0));
        assert!(agent_style.add_modifier.contains(Modifier::DIM));
        assert!(!agent_style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(agent_style.bg, Some(app.palette.active_row_bg));
        row_text(terminal.backend().buffer(), row, area.width)
    }

    /// Exactly one filled row on screen: once the focused agent has its own row in
    /// the tree, the space row above it must stay unfilled or the same place is
    /// marked twice. The space row only takes the fill back when that row is gone.
    #[test]
    fn active_space_row_yields_its_fill_to_the_focused_agent_row() {
        let mut app = crate::app::state::AppState::test_new();
        let workspace = Workspace::test_new("one");
        let pane_id = workspace.tabs[0].root_pane;
        app.workspaces = vec![workspace];
        app.ensure_test_terminals();
        app.active = Some(0);
        app.mode = Mode::Terminal;
        let terminal_id = app.workspaces[0].tabs[0].panes[&pane_id]
            .attached_terminal_id
            .clone();
        app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(Agent::Pi);

        let area = Rect::new(0, 0, 26, 20);
        let (card_areas, agent_row_areas) = compute_workspace_list_areas(&app, area);
        app.view.workspace_card_areas = card_areas;
        app.view.sidebar_agent_row_areas = agent_row_areas;
        let space_row = app.view.workspace_card_areas[0].rect.y;
        let agent_row = app.view.sidebar_agent_row_areas[0].rect.y;

        let mut terminal = Terminal::new(TestBackend::new(26, 20)).unwrap();
        terminal
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let buffer = terminal.backend().buffer();

        let space = buffer[(find_symbol_x(buffer, space_row, 25, "o"), space_row)].style();
        assert_eq!(space.bg, Some(ratatui::style::Color::Reset));
        let agent = buffer[(find_symbol_x(buffer, agent_row, 25, "p"), agent_row)].style();
        assert_eq!(agent.bg, Some(app.palette.surface_dim));
    }

    #[test]
    fn default_space_workspace_style_tracks_active_state() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![Workspace::test_new("one"), Workspace::test_new("two")];
        app.active = Some(0);
        app.mode = Mode::Terminal;
        let area = Rect::new(0, 0, 26, 20);
        app.view.workspace_card_areas = compute_workspace_card_areas(&app, area);
        let first_row = app.view.workspace_card_areas[0].rect.y;
        let second_row = app.view.workspace_card_areas[1].rect.y;
        let mut terminal = Terminal::new(TestBackend::new(26, 20)).unwrap();
        terminal
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let buffer = terminal.backend().buffer();

        // spaces take the brightest tier; only weight separates active
        let active = buffer[(find_symbol_x(buffer, first_row, 25, "o"), first_row)].style();
        assert_eq!(active.fg, Some(app.palette.text));
        assert!(active.add_modifier.contains(Modifier::BOLD));
        assert!(!active.add_modifier.contains(Modifier::DIM));
        assert_eq!(active.bg, Some(app.palette.active_row_bg));

        let inactive = buffer[(find_symbol_x(buffer, second_row, 25, "t"), second_row)].style();
        assert_eq!(inactive.fg, Some(app.palette.text));
        assert!(!inactive
            .add_modifier
            .intersects(Modifier::BOLD | Modifier::DIM));
        assert_eq!(inactive.bg, Some(ratatui::style::Color::Reset));
    }

    #[test]
    fn navigate_selection_keeps_its_existing_background_beside_active_workspace() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![Workspace::test_new("one"), Workspace::test_new("two")];
        app.active = Some(0);
        app.selected = 1;
        app.mode = Mode::Navigate;
        let area = Rect::new(0, 0, 26, 20);
        app.view.workspace_card_areas = compute_workspace_card_areas(&app, area);
        let active_row = app.view.workspace_card_areas[0].rect.y;
        let selected_row = app.view.workspace_card_areas[1].rect.y;
        let mut terminal = Terminal::new(TestBackend::new(26, 20)).unwrap();
        terminal
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let buffer = terminal.backend().buffer();

        assert_eq!(
            buffer[(0, active_row)].bg,
            app.palette.active_row_bg,
            "active workspace should keep its dedicated background"
        );
        assert_eq!(
            buffer[(0, selected_row)].bg,
            app.palette.surface0,
            "navigate selection should keep its existing surface0 background"
        );
    }

    #[test]
    fn space_occurrence_style_applies_without_styling_separator() {
        let config: crate::config::Config = toml::from_str(
            r##"
[ui.sidebar.spaces]
rows = [[{ token = "$hype", fg = "#abcdef", bold = true, dim = false }, "workspace"]]
"##,
        )
        .unwrap();
        let mut app = crate::app::state::AppState::test_new();
        app.sidebar_spaces = config.ui.sidebar.spaces;
        app.workspaces = vec![Workspace::test_new("one")];
        app.active = Some(0);
        app.mode = Mode::Terminal;
        app.workspaces[0].metadata_tokens.patch(
            std::collections::HashMap::from([("hype".into(), Some("HI".into()))]),
            None,
            std::time::Instant::now(),
        );

        let area = Rect::new(0, 0, 26, 20);
        app.view.workspace_card_areas = compute_workspace_card_areas(&app, area);
        let row = app.view.workspace_card_areas[0].rect.y;
        let mut terminal = Terminal::new(TestBackend::new(26, 20)).unwrap();
        terminal
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let h = buffer[(find_symbol_x(buffer, row, 25, "H"), row)].style();
        let i = buffer[(find_symbol_x(buffer, row, 25, "I"), row)].style();
        let separator = buffer[(find_symbol_x(buffer, row, 25, "·"), row)].style();

        for style in [h, i] {
            assert_eq!(style.fg, Some(ratatui::style::Color::Rgb(0xab, 0xcd, 0xef)));
            assert!(style.add_modifier.contains(Modifier::BOLD));
            assert!(!style.add_modifier.contains(Modifier::DIM));
            assert_eq!(style.bg, Some(app.palette.active_row_bg));
        }
        assert_eq!(separator.fg, Some(app.palette.overlay0));
        assert!(separator.add_modifier.contains(Modifier::DIM));
        assert!(!separator.add_modifier.contains(Modifier::BOLD));
        assert_eq!(separator.bg, Some(app.palette.active_row_bg));
    }

    #[test]
    fn occurrence_foreground_flattens_composite_git_status_colors() {
        let config: crate::config::Config = toml::from_str(
            r##"[ui.sidebar.spaces]
rows = [[{ token = "git_status", fg = "#123456" }]]
"##,
        )
        .unwrap();
        let spans = resolved_token_spans(
            &[ResolvedToken {
                kind: ResolvedTokenKind::GitStatus {
                    ahead: 2,
                    behind: 1,
                },
                style: config.ui.sidebar.spaces.rows[0][0].parts().1,
            }],
            ("", Style::default()),
            RowStyles {
                state_text: Style::default(),
                workspace: Style::default(),
                agent: Style::default(),
                branch: Style::default(),
                secondary: Style::default(),
                custom: Style::default(),
            },
            &crate::app::state::AppState::test_new().palette,
            20,
        );

        assert_eq!(
            spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>(),
            "↑2 ↓1"
        );
        assert!(spans
            .iter()
            .all(|span| { span.style.fg == Some(ratatui::style::Color::Rgb(0x12, 0x34, 0x56)) }));
    }

    #[test]
    fn default_agent_row_gap_packs_rendering_and_scroll_geometry() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![Workspace::test_new("one"), Workspace::test_new("two")];
        app.ensure_test_terminals();
        for (workspace, agent) in app.workspaces.iter().zip([Agent::Pi, Agent::Claude]) {
            let pane_id = workspace.tabs[0].root_pane;
            let terminal_id = workspace.tabs[0].panes[&pane_id]
                .attached_terminal_id
                .clone();
            app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(agent);
        }
        app.sidebar_agents.rows = vec![vec![crate::config::AgentSidebarToken::Agent]];
        assert_eq!(app.sidebar_agents.row_gap, 0);

        // 3 header rows + 2 agent rows + the shared footer row
        let area = Rect::new(0, 0, 20, 6);
        let metrics = agent_panel_scroll_metrics(&app, area);
        let body = agent_panel_body_rect(area, false);
        let mut terminal = Terminal::new(TestBackend::new(20, 6)).unwrap();
        terminal
            .draw(|frame| render_agent_detail(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let buffer = terminal.backend().buffer();

        assert_eq!(metrics.viewport_rows, 2);
        assert_eq!(metrics.max_offset_from_bottom, 0);
        assert_eq!(row_text(buffer, body.y, body.width), " pi");
        // claude is shortened to its initials on a row
        assert_eq!(row_text(buffer, body.y + 1, body.width), " cc");
    }

    #[test]
    fn narrow_agent_rows_preserve_later_tab_tokens() {
        let mut app = crate::app::state::AppState::test_new();
        let mut workspace = Workspace::test_new("very-long-workspace-name");
        let tab_idx = workspace.test_add_tab(Some("logs"));
        let pane_id = workspace.tabs[tab_idx].root_pane;
        app.workspaces = vec![workspace];
        app.ensure_test_terminals();
        let terminal_id = app.workspaces[0].tabs[tab_idx].panes[&pane_id]
            .attached_terminal_id
            .clone();
        app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(Agent::Pi);

        app.sidebar_view = crate::app::state::SidebarView::Agents;

        let area = Rect::new(0, 0, 18, 20);
        let mut terminal = Terminal::new(TestBackend::new(18, 20)).unwrap();
        terminal
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let agent_area = sidebar_content_rect(area);
        let body = agent_panel_body_rect(agent_area, false);
        let first = row_text(buffer, body.y, 17);

        assert!(first.contains("logs"), "rendered row: {first:?}");
        assert!(first.contains('·'), "rendered row: {first:?}");
    }

    #[test]
    fn stripped_terminal_title_renders_with_unicode_width_truncation() {
        let mut app = crate::app::state::AppState::test_new();
        let workspace = Workspace::test_new("one");
        let pane_id = workspace.tabs[0].root_pane;
        app.workspaces = vec![workspace];
        app.ensure_test_terminals();
        let terminal_id = app.workspaces[0].tabs[0].panes[&pane_id]
            .attached_terminal_id
            .clone();
        let terminal = app.terminals.get_mut(&terminal_id).unwrap();
        terminal.detected_agent = Some(Agent::Claude);
        terminal.set_terminal_title(Some("⠋ 修复🙂标题很长".into()));
        app.sidebar_agents.rows = vec![vec![
            crate::config::AgentSidebarToken::TerminalTitleStripped,
        ]];

        app.sidebar_view = crate::app::state::SidebarView::Agents;

        let area = Rect::new(0, 0, 10, 12);
        let mut renderer = Terminal::new(TestBackend::new(10, 12)).unwrap();
        renderer
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let agent_area = sidebar_content_rect(area);
        let body = agent_panel_body_rect(agent_area, false);
        let rendered = row_text(renderer.backend().buffer(), body.y, 9);

        assert!(!rendered.contains('⠋'));
        assert!(rendered.contains('修') && rendered.contains('复'));

        let spans = resolved_token_spans(
            &[ResolvedToken::unstyled(ResolvedTokenKind::TerminalTitle(
                "修复🙂标题很长".into(),
            ))],
            ("", Style::default()),
            RowStyles {
                state_text: Style::default(),
                workspace: Style::default(),
                agent: Style::default(),
                branch: Style::default(),
                secondary: Style::default(),
                custom: Style::default(),
            },
            &app.palette,
            8,
        );
        let text = spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();
        assert!(display_width(&text) <= 8, "resolved title: {text:?}");
    }

    #[test]
    fn variable_agent_heights_pack_the_bottom_and_reveal_targets() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![
            Workspace::test_new("one"),
            Workspace::test_new("two"),
            Workspace::test_new("three"),
        ];
        app.ensure_test_terminals();
        for workspace in &app.workspaces {
            let pane_id = workspace.tabs[0].root_pane;
            let terminal_id = workspace.tabs[0].panes[&pane_id]
                .attached_terminal_id
                .clone();
            app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(Agent::Pi);
        }
        let first_pane = app.workspaces[0].tabs[0].root_pane;
        let first_terminal = app.workspaces[0].tabs[0].panes[&first_pane]
            .attached_terminal_id
            .clone();
        app.terminals
            .get_mut(&first_terminal)
            .unwrap()
            .metadata_tokens
            .patch(
                std::collections::HashMap::from([
                    ("a".into(), Some("a".into())),
                    ("b".into(), Some("b".into())),
                ]),
                None,
                std::time::Instant::now(),
            );
        app.sidebar_agents.rows = vec![
            vec![crate::config::AgentSidebarToken::Agent],
            vec![crate::config::AgentSidebarToken::Custom("a".into())],
            vec![crate::config::AgentSidebarToken::Custom("b".into())],
        ];
        let area = Rect::new(0, 0, 20, 6);

        let metrics = agent_panel_scroll_metrics(&app, area);
        assert_eq!(metrics.max_offset_from_bottom, 1);
        assert_eq!(agent_panel_scroll_for_target(&app, area, 0, 2), 1);
    }

    #[test]
    fn oversized_space_layout_is_clipped_to_the_section_body() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![Workspace::test_new("one"), Workspace::test_new("two")];
        app.sidebar_spaces.rows = vec![vec![crate::config::SpaceSidebarToken::Workspace]; 6];
        // body is 5 rows, so the 6-row space row must be clipped to it
        let area = Rect::new(0, 0, 20, 8);
        let workspace_area = workspace_list_rect(area);
        let body = workspace_list_body_rect(workspace_area, false);

        let metrics = workspace_list_scroll_metrics(&app, workspace_area);
        let (cards, _) = compute_workspace_list_areas(&app, area);

        assert_eq!(metrics.viewport_rows, 1);
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].ws_idx, 0);
        assert_eq!(cards[0].rect.height, body.height);
    }

    #[test]
    fn oversized_agent_override_is_clipped_to_the_panel_body() {
        let mut app = crate::app::state::AppState::test_new();
        let workspace = Workspace::test_new("one");
        let pane_id = workspace.tabs[0].root_pane;
        app.workspaces = vec![workspace];
        app.ensure_test_terminals();
        let terminal_id = app.workspaces[0].tabs[0].panes[&pane_id]
            .attached_terminal_id
            .clone();
        app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(Agent::Claude);
        app.sidebar_agents.rows_by_agent.insert(
            "claude".into(),
            vec![vec![crate::config::AgentSidebarToken::Agent]; 6],
        );
        let panel = Rect::new(0, 0, 20, 5);

        let metrics = agent_panel_scroll_metrics(&app, panel);

        assert_eq!(metrics.viewport_rows, 1);
        assert_eq!(metrics.max_offset_from_bottom, 0);
        let entry = agent_panel_entries(&app).pop().unwrap();
        assert_eq!(
            agent_entry_height_in_body(&app, &entry, agent_panel_body_rect(panel, false).height),
            agent_panel_body_rect(panel, false).height
        );
    }

    #[test]
    fn render_sidebar_toggle_draws_expanded_collapse_icon() {
        let app = crate::app::state::AppState::test_new();
        let area = Rect::new(0, 0, 26, 20);
        let mut terminal =
            Terminal::new(TestBackend::new(26, 20)).expect("test terminal should initialize");

        terminal
            .draw(|frame| render_sidebar_toggle(&app, frame, area, false, &app.palette))
            .expect("sidebar toggle should render");

        let toggle = expanded_sidebar_toggle_rect(area);
        assert_eq!(
            terminal.backend().buffer()[(toggle.x, toggle.y)].symbol(),
            "«"
        );
    }

    #[test]
    fn expanded_sidebar_toggle_sits_inside_sidebar_content() {
        let area = Rect::new(0, 0, 26, 20);
        let toggle = expanded_sidebar_toggle_rect(area);

        assert_eq!(toggle.x, area.x + area.width - 2);
        assert_eq!(toggle.y, area.y + area.height - 1);
    }

    #[test]
    fn agent_panel_tab_label_visibility_tracks_tab_identity() {
        let mut app = crate::app::state::AppState::test_new();
        let single_auto = Workspace::test_new("auto");
        let mut single_custom = Workspace::test_new("custom");
        single_custom.tabs[0].set_custom_name("focus".into());
        let mut multi = Workspace::test_new("multi");
        multi.test_add_tab(Some("logs"));

        app.workspaces = vec![single_auto, single_custom, multi];
        app.ensure_test_terminals();
        for (ws_idx, tab_idx, agent) in [
            (0, 0, Agent::Pi),
            (1, 0, Agent::Claude),
            (2, 0, Agent::Codex),
            (2, 1, Agent::Pi),
        ] {
            let pane_id = app.workspaces[ws_idx].tabs[tab_idx].root_pane;
            let terminal_id = app.workspaces[ws_idx].tabs[tab_idx].panes[&pane_id]
                .attached_terminal_id
                .clone();
            app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(agent);
        }

        let entries = agent_panel_entries(&app);
        let labels: Vec<_> = entries
            .iter()
            .map(|entry| {
                (
                    entry.primary_label.as_str(),
                    entry.primary_tab_label.as_deref(),
                )
            })
            .collect();

        assert_eq!(
            labels,
            [
                ("auto", None),
                ("custom", Some("focus")),
                ("multi", Some("1")),
                ("multi", Some("logs")),
            ]
        );
    }

    #[test]
    fn priority_agent_panel_sort_uses_attention_then_space_order() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![
            Workspace::test_new("one"),
            Workspace::test_new("two"),
            Workspace::test_new("three"),
            Workspace::test_new("four"),
        ];
        app.ensure_test_terminals();
        app.active = Some(0);
        app.selected = 0;
        app.agent_panel_sort = crate::app::state::AgentPanelSort::Priority;

        let set_state = |app: &mut crate::app::state::AppState, ws_idx: usize, state| {
            let pane = app.workspaces[ws_idx].tabs[0].root_pane;
            let terminal_id = app.workspaces[ws_idx].tabs[0].panes[&pane]
                .attached_terminal_id
                .clone();
            let terminal = app.terminals.get_mut(&terminal_id).unwrap();
            terminal.detected_agent = Some(Agent::Claude);
            terminal.state = state;
        };
        set_state(&mut app, 0, AgentState::Working);
        set_state(&mut app, 1, AgentState::Idle);
        set_state(&mut app, 2, AgentState::Working);
        set_state(&mut app, 3, AgentState::Blocked);

        let done_pane = app.workspaces[1].tabs[0].root_pane;
        app.workspaces[1].tabs[0]
            .panes
            .get_mut(&done_pane)
            .unwrap()
            .seen = false;

        let labels: Vec<String> = agent_panel_entries(&app)
            .into_iter()
            .map(|entry| entry.primary_label)
            .collect();

        assert_eq!(labels, ["four", "two", "one", "three"]);
    }

    #[test]
    fn collapsed_sidebar_numbers_grouped_agents_by_list_position() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![Workspace::test_new("one"), Workspace::test_new("two")];
        app.ensure_test_terminals();

        for ws_idx in 0..app.workspaces.len() {
            let pane = app.workspaces[ws_idx].tabs[0].root_pane;
            let terminal_id = app.workspaces[ws_idx].tabs[0].panes[&pane]
                .attached_terminal_id
                .clone();
            app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(Agent::Claude);
        }

        let area = Rect::new(0, 0, 4, 12);
        let (_, _, detail_area) = collapsed_sidebar_sections(area);
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height))
            .expect("test terminal should initialize");

        terminal
            .draw(|frame| render_sidebar_collapsed(&app, frame, area))
            .expect("collapsed sidebar should render");

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(detail_area.x, detail_area.y)].symbol(), "1");
        assert_eq!(buffer[(detail_area.x, detail_area.y + 1)].symbol(), "2");
    }

    /// Two agent panes in one workspace plus a second workspace, so the
    /// assertions can tell pane-level highlighting apart from workspace-level.
    fn collapsed_agent_app() -> (crate::app::state::AppState, PaneId, PaneId) {
        let mut app = crate::app::state::AppState::test_new();
        let mut first = Workspace::test_new("one");
        let second_pane = first.test_split(Direction::Horizontal);
        let first_pane = first.tabs[0].root_pane;
        app.workspaces = vec![first, Workspace::test_new("two")];
        app.ensure_test_terminals();

        let terminal_ids: Vec<_> = app
            .workspaces
            .iter()
            .flat_map(|ws| ws.tabs.iter())
            .flat_map(|tab| tab.panes.values())
            .map(|pane| pane.attached_terminal_id.clone())
            .collect();
        for terminal_id in terminal_ids {
            app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(Agent::Claude);
        }

        (app, first_pane, second_pane)
    }

    fn collapsed_agent_row_styles(
        app: &crate::app::state::AppState,
        area: Rect,
        detail_area: Rect,
        rows: u16,
    ) -> Vec<Vec<ratatui::style::Style>> {
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height))
            .expect("test terminal should initialize");
        terminal
            .draw(|frame| render_sidebar_collapsed(app, frame, area))
            .expect("collapsed sidebar should render");
        let buffer = terminal.backend().buffer();
        (0..rows)
            .map(|row| {
                (detail_area.x..detail_area.x + detail_area.width)
                    .map(|x| buffer[(x, detail_area.y + row)].style())
                    .collect()
            })
            .collect()
    }

    #[test]
    fn collapsed_sidebar_highlights_only_the_focused_agent_pane() {
        let (mut app, first_pane, second_pane) = collapsed_agent_app();
        app.active = Some(0);
        app.workspaces[0].tabs[0].layout.focus_pane(second_pane);
        assert!(app.is_active_pane(0, 0, second_pane));
        assert!(!app.is_active_pane(0, 0, first_pane));

        let area = Rect::new(0, 0, 4, 14);
        let (_, _, detail_area) = collapsed_sidebar_sections(area);
        let rows = collapsed_agent_row_styles(&app, area, detail_area, 3);

        let highlighted: Vec<_> = rows
            .iter()
            .filter(|cells| {
                cells
                    .iter()
                    .all(|style| style.bg == Some(app.palette.active_row_bg))
            })
            .collect();
        assert_eq!(
            highlighted.len(),
            1,
            "only the focused agent pane should be highlighted, across the whole row"
        );
        assert_eq!(highlighted[0][0].fg, Some(app.palette.text));

        let muted = rows
            .iter()
            .filter(|cells| cells[0].fg == Some(app.palette.overlay0))
            .count();
        assert_eq!(
            muted, 2,
            "the sibling pane in the active workspace and the other workspace stay muted"
        );
    }

    #[test]
    fn collapsed_sidebar_does_not_highlight_agents_without_active_workspace() {
        let (mut app, _, _) = collapsed_agent_app();
        app.active = None;

        let area = Rect::new(0, 0, 4, 14);
        let (_, _, detail_area) = collapsed_sidebar_sections(area);
        let rows = collapsed_agent_row_styles(&app, area, detail_area, 3);

        for cells in rows {
            assert_eq!(cells[0].fg, Some(app.palette.overlay0));
            for style in cells {
                assert_ne!(style.bg, Some(app.palette.active_row_bg));
            }
        }
    }

    #[test]
    fn collapsed_sidebar_keeps_workspace_status_visible_for_two_digit_positions() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = (1..=10)
            .map(|idx| Workspace::test_new(&format!("workspace-{idx}")))
            .collect();
        app.ensure_test_terminals();

        for ws_idx in 0..app.workspaces.len() {
            let pane = app.workspaces[ws_idx].tabs[0].root_pane;
            let terminal_id = app.workspaces[ws_idx].tabs[0].panes[&pane]
                .attached_terminal_id
                .clone();
            app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(Agent::Claude);
        }

        let area = Rect::new(0, 0, 4, 25);
        let (workspace_area, _, _) = collapsed_sidebar_sections(area);
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height))
            .expect("test terminal should initialize");

        terminal
            .draw(|frame| render_sidebar_collapsed(&app, frame, area))
            .expect("collapsed sidebar should render");

        let tenth_row = workspace_area.y + 9;
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(workspace_area.x, workspace_area.y)].symbol(), "1");
        assert_eq!(
            buffer[(workspace_area.x + 1, workspace_area.y)].symbol(),
            " "
        );
        // a dash rather than the indicator style's middle dot: this column says "no
        // agent here", and a middle dot disappears against the digits beside it
        assert_eq!(
            buffer[(workspace_area.x + 2, workspace_area.y)].symbol(),
            "–"
        );
        assert_eq!(buffer[(workspace_area.x, tenth_row)].symbol(), "1");
        assert_eq!(buffer[(workspace_area.x + 1, tenth_row)].symbol(), "0");
        assert_eq!(buffer[(workspace_area.x + 2, tenth_row)].symbol(), "–");
    }

    #[test]
    fn collapsed_sidebar_keeps_status_visible_for_two_digit_positions() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = (1..=10)
            .map(|idx| Workspace::test_new(&format!("workspace-{idx}")))
            .collect();
        app.ensure_test_terminals();

        for ws_idx in 0..app.workspaces.len() {
            let pane = app.workspaces[ws_idx].tabs[0].root_pane;
            let terminal_id = app.workspaces[ws_idx].tabs[0].panes[&pane]
                .attached_terminal_id
                .clone();
            app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(Agent::Claude);
        }

        let area = Rect::new(0, 0, 4, 25);
        let (_, _, detail_area) = collapsed_sidebar_sections(area);
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height))
            .expect("test terminal should initialize");

        terminal
            .draw(|frame| render_sidebar_collapsed(&app, frame, area))
            .expect("collapsed sidebar should render");

        let tenth_row = detail_area.y + 9;
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(detail_area.x, tenth_row)].symbol(), "1");
        assert_eq!(buffer[(detail_area.x + 1, tenth_row)].symbol(), "0");
        assert_eq!(buffer[(detail_area.x + 2, tenth_row)].symbol(), "–");
    }

    #[test]
    fn collapsed_sidebar_numbers_priority_agents_by_list_position() {
        let first = Workspace::test_new("one");
        let first_pane = first.tabs[0].root_pane;
        let mut second = Workspace::test_new("two");
        let second_pane = second.tabs[0].root_pane;
        let urgent_pane = second.test_split(ratatui::layout::Direction::Horizontal);

        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![first, second];
        app.ensure_test_terminals();
        app.agent_panel_sort = crate::app::state::AgentPanelSort::Priority;
        app.status_indicators = crate::config::StatusIndicatorStyle::Symbols;

        let set_state = |app: &mut crate::app::state::AppState, ws_idx: usize, pane_id, state| {
            let terminal_id = app.workspaces[ws_idx].tabs[0].panes[&pane_id]
                .attached_terminal_id
                .clone();
            let terminal = app.terminals.get_mut(&terminal_id).unwrap();
            terminal.detected_agent = Some(Agent::Claude);
            terminal.state = state;
        };
        set_state(&mut app, 0, first_pane, AgentState::Idle);
        set_state(&mut app, 1, second_pane, AgentState::Working);
        set_state(&mut app, 1, urgent_pane, AgentState::Blocked);
        app.workspaces[0].tabs[0]
            .panes
            .get_mut(&first_pane)
            .unwrap()
            .seen = false;

        assert_eq!(app.workspaces[1].public_pane_number(urgent_pane), Some(2));
        assert_eq!(agent_panel_entries(&app)[0].pane_id, urgent_pane);

        let area = Rect::new(0, 0, 4, 16);
        let (_, _, detail_area) = collapsed_sidebar_sections(area);
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height))
            .expect("test terminal should initialize");

        terminal
            .draw(|frame| render_sidebar_collapsed(&app, frame, area))
            .expect("collapsed sidebar should render");

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(detail_area.x, detail_area.y)].symbol(), "1");
        assert_eq!(buffer[(detail_area.x, detail_area.y + 1)].symbol(), "2");
        assert_eq!(buffer[(detail_area.x, detail_area.y + 2)].symbol(), "3");
        // the agent rows carry the same dot and the same colour the pane border does,
        // so the indicator style names the glyph everywhere except here
        assert_eq!(buffer[(detail_area.x + 2, detail_area.y)].symbol(), "●");
        assert_eq!(
            buffer[(detail_area.x + 2, detail_area.y)].style().fg,
            Some(app.palette.red)
        );
        assert_eq!(buffer[(detail_area.x + 2, detail_area.y + 1)].symbol(), "●");
        assert_eq!(
            buffer[(detail_area.x + 2, detail_area.y + 1)].style().fg,
            Some(app.palette.green)
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn all_workspaces_agent_panel_entries_use_live_root_runtime_cwd_for_workspace_label() {
        let unique = format!(
            "herdr-agent-panel-runtime-cwd-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let stale_cwd = root.join("issue-264-nix-support");
        let live_cwd = root.join("herdr");
        std::fs::create_dir_all(stale_cwd.join(".git")).unwrap();
        std::fs::create_dir_all(live_cwd.join(".git")).unwrap();

        let mut app = crate::app::state::AppState::test_new();
        let mut workspace = Workspace::test_new("stale-name");
        workspace.custom_name = None;
        workspace.identity_cwd = stale_cwd.clone();
        let pane = workspace.tabs[0].root_pane;

        app.workspaces = vec![workspace];
        app.ensure_test_terminals();
        let terminal_id = app.workspaces[0].tabs[0].panes[&pane]
            .attached_terminal_id
            .clone();
        let terminal = app.terminals.get_mut(&terminal_id).unwrap();
        terminal.cwd = stale_cwd;
        terminal.detected_agent = Some(Agent::Pi);
        app.active = Some(0);
        app.selected = 0;

        let (events, _) = tokio::sync::mpsc::channel(4);
        let runtime = crate::terminal::TerminalRuntime::spawn(
            pane,
            24,
            80,
            live_cwd.clone(),
            0,
            crate::terminal_theme::TerminalTheme::default(),
            None,
            crate::pane::PaneShellConfig::new("/bin/sh", crate::config::ShellModeConfig::NonLogin),
            &crate::pane::PaneLaunchEnv::default(),
            events,
            std::sync::Arc::new(tokio::sync::Notify::new()),
            std::sync::Arc::new(crate::render_signal::RenderSignal::new()),
        )
        .unwrap();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while runtime.cwd() != Some(live_cwd.clone()) && std::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        let mut runtime_registry = TerminalRuntimeRegistry::new();
        runtime_registry.insert(terminal_id, runtime);
        let entries = agent_panel_entries_from(&app, &runtime_registry);
        let primary_label = entries[0].primary_label.clone();

        for (_, runtime) in runtime_registry.drain() {
            runtime.shutdown();
        }
        let _ = std::fs::remove_dir_all(root);

        assert_eq!(primary_label, "herdr");
    }

    #[test]
    fn all_workspaces_agent_panel_entries_prefer_agent_names_for_agent_identity() {
        let mut app = crate::app::state::AppState::test_new();
        let workspace = Workspace::test_new("bridge");
        let first_pane = workspace.tabs[0].root_pane;

        app.workspaces = vec![workspace];
        app.ensure_test_terminals();
        let first_terminal_id = app.workspaces[0].tabs[0].panes[&first_pane]
            .attached_terminal_id
            .clone();
        app.terminals
            .get_mut(&first_terminal_id)
            .unwrap()
            .detected_agent = Some(Agent::Pi);
        app.terminals
            .get_mut(&first_terminal_id)
            .unwrap()
            .set_agent_name("planner".into());
        app.active = Some(0);
        app.selected = 0;

        let entries = agent_panel_entries(&app);
        assert_eq!(entries[0].primary_label, "bridge");
        assert_eq!(entries[0].agent_label.as_deref(), Some("planner"));
    }

    #[test]
    fn both_views_fill_the_sidebar_content_rect() {
        let area = Rect::new(0, 0, 20, 5);

        assert_eq!(sidebar_content_rect(area), Rect::new(0, 0, 19, 5));
        assert_eq!(workspace_list_rect(area), Rect::new(0, 0, 19, 5));
        assert_eq!(sidebar_content_rect(Rect::new(0, 0, 1, 5)), Rect::default());
    }

    #[test]
    fn view_tabs_sit_on_the_first_content_row() {

    }

    #[test]
        assert_eq!(spaces, Rect::new(0, 0, 7, 1));
        assert_eq!(agents, Rect::new(10, 0, 6, 1));

    }

    #[test]
    fn grouped_child_label_keeps_custom_workspace_name() {
        assert_eq!(
            grouped_child_display_label("renamed issue", Some("worktree/issue-137"), true),
            "renamed issue"
        );
    }

    #[test]
    fn grouped_child_label_uses_short_branch_for_auto_named_workspace() {
        assert_eq!(
            grouped_child_display_label("herdr-issue", Some("worktree/issue-137"), false),
            "issue-137"
        );
    }

    #[test]
    fn workspace_list_truncates_cjk_branch_without_panic() {
        let mut app = crate::app::state::AppState::test_new();
        let mut ws = Workspace::test_new("repo");
        ws.cached_git_branch = Some("feature/中文-分支-644".into());
        app.workspaces = vec![ws];
        app.active = Some(0);
        app.selected = 0;
        app.mode = Mode::Terminal;
        app.view.workspace_card_areas = vec![crate::app::state::WorkspaceCardArea {
            ws_idx: 0,
            rect: Rect::new(0, 1, 15, 2),
            indented: false,
        }];

        let mut terminal = Terminal::new(TestBackend::new(15, 6)).expect("test terminal");
        let runtimes = crate::terminal::TerminalRuntimeRegistry::new();

        terminal
            .draw(|frame| {
                render_workspace_list(&app, &runtimes, frame, Rect::new(0, 0, 15, 6), false)
            })
            .expect("workspace list should render");
    }

    fn workspace_with_worktree_space(
        name: &str,
        key: Option<&str>,
        checkout_key: &str,
    ) -> crate::workspace::Workspace {
        let mut ws = crate::workspace::Workspace::test_new(name);
        if let Some(key) = key {
            ws.worktree_space = Some(crate::workspace::WorktreeSpaceMembership {
                key: key.into(),
                label: "herdr".into(),
                repo_root: std::path::PathBuf::from("/repo/herdr"),
                checkout_path: std::path::PathBuf::from(checkout_key),
                is_linked_worktree: name != "main",
            });
        }
        ws
    }

    fn workspace_with_git_space(name: &str, key: &str) -> crate::workspace::Workspace {
        let mut ws = crate::workspace::Workspace::test_new(name);
        ws.cached_git_space = Some(crate::workspace::GitSpaceMetadata {
            key: key.into(),
            checkout_key: format!("/repo/{name}"),
            repo_name: "herdr".into(),
            repo_root: std::path::PathBuf::from(format!("/repo/{name}")),
            is_linked_worktree: false,
        });
        ws
    }

    #[test]
    fn desktop_worktree_tree_aligns_parents_and_marks_children() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_worktree_space("main", Some("repo-key"), "/repo/herdr"),
            workspace_with_worktree_space("issue", Some("repo-key"), "/repo/herdr-issue"),
            workspace_with_worktree_space("review", Some("repo-key"), "/repo/herdr-review"),
            Workspace::test_new("notes"),
        ];
        app.sidebar_spaces.rows = vec![vec![
            crate::config::SpaceSidebarToken::StateIcon,
            crate::config::SpaceSidebarToken::Workspace,
        ]];
        app.sidebar_spaces.row_gap = 0;
        let area = Rect::new(0, 0, 30, 20);
        app.view.workspace_card_areas = compute_workspace_card_areas(&app, area);
        let list_area = workspace_list_rect(area);

        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
        terminal
            .draw(|frame| {
                render_workspace_list(
                    &app,
                    &TerminalRuntimeRegistry::new(),
                    frame,
                    list_area,
                    false,
                )
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let cards = &app.view.workspace_card_areas;
        let parent_name_x = find_symbol_x(buffer, cards[0].rect.y, cards[0].rect.width, "m");
        let plain_name_x = find_symbol_x(buffer, cards[3].rect.y, cards[3].rect.width, "n");
        assert_eq!(parent_name_x, plain_name_x);
        assert_eq!(buffer[(cards[1].rect.x + 1, cards[1].rect.y)].symbol(), "├");
        assert_eq!(buffer[(cards[2].rect.x + 1, cards[2].rect.y)].symbol(), "└");
        // no chevron: the name owns the row all the way to its right edge
        assert_eq!(
            buffer[(cards[0].rect.x + cards[0].rect.width - 1, cards[0].rect.y)].symbol(),
            " "
        );
    }

    #[test]
    fn desktop_worktree_connector_uses_full_list_at_viewport_boundary() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_worktree_space("main", Some("repo-key"), "/repo/herdr"),
            workspace_with_worktree_space("issue", Some("repo-key"), "/repo/herdr-issue"),
            workspace_with_worktree_space("review", Some("repo-key"), "/repo/herdr-review"),
        ];
        app.sidebar_spaces.rows = vec![vec![crate::config::SpaceSidebarToken::Workspace]];
        app.sidebar_spaces.row_gap = 0;
        // 2 header rows + 2 visible space rows + the footer row, so the third
        // space is clipped and the connector must come from the full entry list
        let area = Rect::new(0, 0, 30, 5);
        app.view.workspace_card_areas = compute_workspace_card_areas(&app, area);
        assert_eq!(app.view.workspace_card_areas.len(), 2);
        let list_area = workspace_list_rect(area);

        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
        terminal
            .draw(|frame| {
                render_workspace_list(
                    &app,
                    &TerminalRuntimeRegistry::new(),
                    frame,
                    list_area,
                    false,
                )
            })
            .unwrap();

        let child = app.view.workspace_card_areas[1];
        assert_eq!(
            terminal.backend().buffer()[(child.rect.x + 1, child.rect.y)].symbol(),
            "├"
        );
    }

    #[test]
    fn parent_workspace_row_stays_clickable_when_grouped() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_worktree_space("main", Some("repo-key"), "/repo/herdr"),
            workspace_with_worktree_space("issue", Some("repo-key"), "/repo/herdr-issue"),
        ];
        app.sidebar_spaces.row_gap = 1;

        let (cards, headers) = compute_workspace_list_areas(&app, Rect::new(0, 0, 30, 20));

        assert!(headers.is_empty());
        assert_eq!(cards[0].ws_idx, 0);
        assert!(!cards[0].indented);
        assert_eq!(cards[1].ws_idx, 1);
        assert!(cards[1].indented);
        // the group is one block, so no gap opens between parent and first child
        assert_eq!(cards[1].rect.y, cards[0].rect.y + cards[0].rect.height);
    }

    #[test]
    fn space_row_gap_preserves_compact_worktree_children() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_worktree_space("main", Some("repo-key"), "/repo/herdr"),
            workspace_with_worktree_space("issue", Some("repo-key"), "/repo/herdr-issue"),
            workspace_with_worktree_space("review", Some("repo-key"), "/repo/herdr-review"),
            Workspace::test_new("notes"),
        ];
        app.sidebar_spaces.rows = vec![vec![crate::config::SpaceSidebarToken::Workspace]];
        app.sidebar_spaces.row_gap = 2;

        let (spacious, _) = compute_workspace_list_areas(&app, Rect::new(0, 0, 30, 30));
        // parent and both children are one block: gaps only land after it
        assert_eq!(
            spacious[1].rect.y,
            spacious[0].rect.y + spacious[0].rect.height
        );
        assert_eq!(
            spacious[2].rect.y,
            spacious[1].rect.y + spacious[1].rect.height
        );
        assert_eq!(
            spacious[3].rect.y,
            spacious[2].rect.y + spacious[2].rect.height + 2
        );
        let spacious_metrics = workspace_list_scroll_metrics(&app, Rect::new(0, 0, 30, 7));
        assert_eq!(spacious_metrics.viewport_rows, 3);
        assert_eq!(spacious_metrics.max_offset_from_bottom, 2);

        app.sidebar_spaces.row_gap = 0;
        let (packed, _) = compute_workspace_list_areas(&app, Rect::new(0, 0, 30, 30));
        assert!(packed
            .windows(2)
            .all(|pair| pair[1].rect.y == pair[0].rect.y + pair[0].rect.height));
        let packed_metrics = workspace_list_scroll_metrics(&app, Rect::new(0, 0, 30, 7));
        assert_eq!(packed_metrics.viewport_rows, 4);
        assert_eq!(packed_metrics.max_offset_from_bottom, 0);
    }

    #[test]
    fn packed_workspace_drag_indicator_overlays_an_internal_boundary() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            Workspace::test_new("a"),
            Workspace::test_new("b"),
            Workspace::test_new("c"),
        ];
        app.sidebar_spaces.rows = vec![vec![crate::config::SpaceSidebarToken::Workspace]];
        app.sidebar_spaces.row_gap = 0;
        let area = Rect::new(0, 0, 30, 20);
        app.view.workspace_card_areas = compute_workspace_card_areas(&app, area);
        let list_area = workspace_list_rect(area);
        let indicator_row = workspace_drop_indicator_row(
            &app,
            &app.view.workspace_card_areas,
            list_area,
            crate::app::state::WorkspaceDropTarget::Before(2),
        )
        .unwrap();
        assert_eq!(indicator_row, app.view.workspace_card_areas[1].rect.y);
        app.drag = Some(crate::app::state::DragState {
            target: crate::app::state::DragTarget::WorkspaceReorder {
                source_id: 0,
                source_ws_idx: 0,
                drop_target: Some(crate::app::state::WorkspaceDropTarget::Before(2)),
            },
        });

        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
        terminal
            .draw(|frame| {
                render_workspace_list(
                    &app,
                    &TerminalRuntimeRegistry::new(),
                    frame,
                    list_area,
                    false,
                )
            })
            .unwrap();

        assert_eq!(
            terminal.backend().buffer()[(list_area.x, indicator_row)].symbol(),
            "─"
        );
    }

    #[test]
    fn linked_only_worktree_members_do_not_form_parentless_group() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_worktree_space("issue", Some("repo-key"), "/repo/herdr-issue"),
            workspace_with_worktree_space("review", Some("repo-key"), "/repo/herdr-review"),
        ];

        let entries = workspace_list_entries(&app);

        assert_eq!(
            entries,
            vec![
                WorkspaceListEntry::Workspace {
                    ws_idx: 0,
                    indented: false
                },
                WorkspaceListEntry::Workspace {
                    ws_idx: 1,
                    indented: false
                },
            ]
        );
    }

    #[test]
    fn compact_space_group_scroll_clamps_when_all_entries_fit() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_worktree_space("main", Some("repo-key"), "/repo/herdr"),
            workspace_with_worktree_space("one", Some("repo-key"), "/repo/herdr-one"),
            workspace_with_worktree_space("two", Some("repo-key"), "/repo/herdr-two"),
        ];
        let area = Rect::new(0, 0, 30, 20);
        app.workspace_scroll = normalized_workspace_scroll(&app, area, 2);

        let (cards, headers) = compute_workspace_list_areas(&app, area);

        assert!(headers.is_empty());
        assert_eq!(app.workspace_scroll, 0);
        assert_eq!(cards.len(), 3);
        assert_eq!(cards[2].ws_idx, 2);
    }

    #[test]
    fn workspace_scroll_metrics_count_display_entries_not_raw_workspaces() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_worktree_space("main", Some("repo-key"), "/repo/herdr"),
            workspace_with_worktree_space("issue", Some("repo-key"), "/repo/herdr-issue"),
            Workspace::test_new("notes"),
        ];
        for workspace in &mut app.workspaces {
            workspace.cached_git_branch = Some("main".into());
        }
        app.collapsed_space_keys.insert("repo-key".into());
        app.active = None;
        app.mode = Mode::Terminal;

        let ws_area = Rect::new(0, 0, 30, 6);
        let metrics = workspace_list_scroll_metrics(&app, ws_area);

        assert_eq!(metrics.viewport_rows, 1);
        assert_eq!(metrics.max_offset_from_bottom, 1);
        assert_eq!(metrics.offset_from_bottom, 1);
    }

    #[test]
    fn workspace_scroll_offset_applies_to_group_children() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_worktree_space("main", Some("repo-key"), "/repo/herdr"),
            workspace_with_worktree_space("issue", Some("repo-key"), "/repo/herdr-issue"),
            Workspace::test_new("notes"),
        ];
        app.collapsed_space_keys.insert("repo-key".into());
        app.active = None;
        app.mode = Mode::Terminal;
        app.workspace_scroll = 1;

        let (cards, headers) = compute_workspace_list_areas(&app, Rect::new(0, 0, 30, 12));

        assert!(headers.is_empty());
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].ws_idx, 2);
    }

    #[test]
    fn workspace_list_entries_group_multiple_workspaces_in_same_git_space() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_worktree_space("main", Some("repo-key"), "/repo/herdr"),
            workspace_with_worktree_space("issue", Some("repo-key"), "/repo/herdr-issue"),
        ];

        assert_eq!(
            workspace_list_entries(&app),
            vec![
                WorkspaceListEntry::Workspace {
                    ws_idx: 0,
                    indented: false,
                },
                WorkspaceListEntry::Workspace {
                    ws_idx: 1,
                    indented: true,
                },
            ]
        );
    }

    #[test]
    fn workspace_list_entries_group_non_contiguous_explicit_members() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_worktree_space("main", Some("repo-key"), "/repo/herdr"),
            workspace_with_git_space("normal", "other-key"),
            workspace_with_worktree_space("issue", Some("repo-key"), "/repo/herdr-issue"),
        ];

        assert_eq!(
            workspace_list_entries(&app),
            vec![
                WorkspaceListEntry::Workspace {
                    ws_idx: 0,
                    indented: false,
                },
                WorkspaceListEntry::Workspace {
                    ws_idx: 2,
                    indented: true,
                },
                WorkspaceListEntry::Workspace {
                    ws_idx: 1,
                    indented: false,
                },
            ]
        );
    }

    #[test]
    fn workspace_list_entries_do_not_group_normal_git_workspaces() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_git_space("one", "repo-key"),
            workspace_with_git_space("two", "repo-key"),
        ];

        assert_eq!(
            workspace_list_entries(&app),
            vec![
                WorkspaceListEntry::Workspace {
                    ws_idx: 0,
                    indented: false,
                },
                WorkspaceListEntry::Workspace {
                    ws_idx: 1,
                    indented: false,
                },
            ]
        );
    }

    #[test]
    fn workspace_list_entries_do_not_auto_attach_normal_git_workspace_to_group() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_worktree_space("main", Some("repo-key"), "/repo/herdr"),
            workspace_with_git_space("scratch", "repo-key"),
            workspace_with_worktree_space("issue", Some("repo-key"), "/repo/herdr-issue"),
        ];

        assert_eq!(
            workspace_list_entries(&app),
            vec![
                WorkspaceListEntry::Workspace {
                    ws_idx: 0,
                    indented: false,
                },
                WorkspaceListEntry::Workspace {
                    ws_idx: 2,
                    indented: true,
                },
                WorkspaceListEntry::Workspace {
                    ws_idx: 1,
                    indented: false,
                },
            ]
        );
    }

    #[test]
    fn workspace_list_entries_leave_single_git_and_non_git_workspaces_flat() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_git_space("one", "repo-key"),
            workspace_with_worktree_space("notes", None, "/notes"),
        ];

        assert_eq!(
            workspace_list_entries(&app),
            vec![
                WorkspaceListEntry::Workspace {
                    ws_idx: 0,
                    indented: false,
                },
                WorkspaceListEntry::Workspace {
                    ws_idx: 1,
                    indented: false,
                },
            ]
        );
    }

    #[test]
    fn worktree_group_keeps_its_connectors_while_agent_rows_sit_flush_left() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_worktree_space("main", Some("repo-key"), "/repo/herdr"),
            workspace_with_worktree_space("c1", Some("repo-key"), "/repo/herdr-c1"),
            workspace_with_worktree_space("c2", Some("repo-key"), "/repo/herdr-c2"),
        ];
        for idx in 1..3 {
            if let Some(space) = app.workspaces[idx].worktree_space.as_mut() {
                space.is_linked_worktree = true;
            }
        }
        app.sidebar_spaces.rows = vec![vec![
            crate::config::SpaceSidebarToken::StateIcon,
            crate::config::SpaceSidebarToken::Workspace,
        ]];
        app.ensure_test_terminals();
        app.active = Some(0);
        for ws_idx in 0..3 {
            let pane_id = app.workspaces[ws_idx].tabs[0].root_pane;
            let terminal_id = app.workspaces[ws_idx].tabs[0].panes[&pane_id]
                .attached_terminal_id
                .clone();
            app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(Agent::Pi);
        }

        let area = Rect::new(0, 0, 26, 20);
        crate::ui::compute_view(&mut app, Rect::new(0, 0, 106, 20));
        let mut terminal = Terminal::new(TestBackend::new(26, 20)).unwrap();
        terminal
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let rows = (2..10)
            .map(|row| row_text(buffer, row, 26))
            .collect::<Vec<_>>();

        // space rows keep the group connectors, because a chevron sits on the parent
        // and on its worktree children alike and so cannot tell them apart
        assert!(rows[0].starts_with(" – main"), "rows: {rows:#?}");
        assert!(rows[3].starts_with(" ├─ – c1"), "rows: {rows:#?}");
        assert!(rows[6].starts_with(" └─ – c2"), "rows: {rows:#?}");
        // agent rows carry neither, so every one of them starts in the same column
        for row in [&rows[1], &rows[4], &rows[7]] {
            assert!(row.starts_with(" ┃ pi"), "rows: {rows:#?}");
        }
    }

    #[test]
    fn expanded_space_emits_its_agent_rows_under_the_space_row() {
        let mut app = AppState::test_new();
        let mut workspace = Workspace::test_new("one");
        let first_pane = workspace.tabs[0].root_pane;
        let second_tab = workspace.test_add_tab(Some("logs"));
        let second_pane = workspace.tabs[second_tab].root_pane;
        app.workspaces = vec![workspace];
        app.ensure_test_terminals();
        for (tab_idx, pane_id) in [(0, first_pane), (second_tab, second_pane)] {
            let terminal_id = app.workspaces[0].tabs[tab_idx].panes[&pane_id]
                .attached_terminal_id
                .clone();
            app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(Agent::Pi);
        }

        assert_eq!(
            sidebar_tree_entries(&app),
            vec![
                WorkspaceListEntry::Workspace {
                    ws_idx: 0,
                    indented: false,
                },
                WorkspaceListEntry::AgentPane {
                    ws_idx: 0,
                    tab_idx: 0,
                    pane_id: first_pane,
                    under_indented: false,
                },
                WorkspaceListEntry::AgentPane {
                    ws_idx: 0,
                    tab_idx: second_tab,
                    pane_id: second_pane,
                    under_indented: false,
                },
            ]
        );
    }

    #[test]
    fn nested_agent_rows_print_the_space_name_once() {
        let mut app = AppState::test_new();
        let mut workspace = Workspace::test_new("one");
        let first_pane = workspace.tabs[0].root_pane;
        let second_tab = workspace.test_add_tab(Some("logs"));
        let second_pane = workspace.tabs[second_tab].root_pane;
        app.workspaces = vec![workspace];
        app.ensure_test_terminals();
        app.active = Some(0);
        for (tab_idx, pane_id) in [(0, first_pane), (second_tab, second_pane)] {
            let terminal_id = app.workspaces[0].tabs[tab_idx].panes[&pane_id]
                .attached_terminal_id
                .clone();
            app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(Agent::Pi);
        }

        let area = Rect::new(0, 0, 26, 20);
        crate::ui::compute_view(&mut app, Rect::new(0, 0, 106, 20));
        let mut terminal = Terminal::new(TestBackend::new(26, 20)).unwrap();
        terminal
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let rendered = (0..20)
            .map(|row| row_text(buffer, row, 25))
            .collect::<Vec<_>>()
            .join("\n");

        assert_eq!(rendered.matches("one").count(), 1, "rendered:\n{rendered}");
        assert_eq!(rendered.matches("pi").count(), 2, "rendered:\n{rendered}");
        // agent rows hang on indentation alone; the chevron is what marks a space row
        assert!(!rendered.contains("├"), "rendered:\n{rendered}");
        assert!(!rendered.contains("└"), "rendered:\n{rendered}");
    }

    /// One space, one tab, two panes running the same agent — the default
    /// template renders both rows identically without a discriminator.
    fn app_with_colliding_sibling_agents(
    ) -> (AppState, crate::layout::PaneId, crate::layout::PaneId) {
        let mut app = AppState::test_new();
        let mut workspace = Workspace::test_new("one");
        let first = workspace.tabs[0].root_pane;
        let second = workspace.test_split(ratatui::layout::Direction::Horizontal);
        app.workspaces = vec![workspace];
        app.ensure_test_terminals();
        app.active = Some(0);
        for pane_id in [first, second] {
            let terminal_id = app.workspaces[0].tabs[0].panes[&pane_id]
                .attached_terminal_id
                .clone();
            app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(Agent::Pi);
        }
        (app, first, second)
    }

    fn rendered_sidebar(app: &mut AppState) -> String {
        let area = Rect::new(0, 0, 26, 20);
        crate::ui::compute_view(app, Rect::new(0, 0, 106, 20));
        let mut terminal = Terminal::new(TestBackend::new(26, 20)).unwrap();
        terminal
            .draw(|frame| render_sidebar(app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let buffer = terminal.backend().buffer();
        (0..20)
            .map(|row| row_text(buffer, row, 25))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Two rules, two weights: a space boundary separates unrelated work and has to
    /// read as the stronger break, so the rule between agents stays under it.
    #[test]
    fn the_rule_between_spaces_outranks_the_rule_between_agents() {
        let (mut app, _, _) = app_with_colliding_sibling_agents();
        app.workspaces.push(Workspace::test_new("two"));
        app.ensure_test_terminals();
        app.sidebar_spaces.row_gap = 1;

        let area = Rect::new(0, 0, 26, 24);
        crate::ui::compute_view(&mut app, Rect::new(0, 0, 106, 24));
        let mut terminal = Terminal::new(TestBackend::new(26, 24)).unwrap();
        terminal
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let buffer = terminal.backend().buffer();

        let agent_rows = &app.view.sidebar_agent_row_areas;
        let agent_rule = &buffer[(
            agent_rows[0].rect.x,
            agent_rows[0].rect.y + agent_rows[0].rect.height,
        )];
        assert_eq!(agent_rule.symbol(), "─");
        assert_eq!(agent_rule.style().fg, Some(app.palette.surface1));
        assert!(!agent_rule.style().add_modifier.contains(Modifier::DIM));

        let second_card = app.view.workspace_card_areas[1];
        let space_rule = &buffer[(second_card.rect.x, second_card.rect.y - 1)];
        assert_eq!(space_rule.symbol(), "─");
        assert_eq!(space_rule.style().fg, Some(app.palette.space_rule()));
        // it only has to outrank the rule between agents; how far it goes toward the
        // legible tier is a taste the constant carries
        assert_ne!(app.palette.space_rule(), app.palette.surface1);
    }

    /// Flush-left agent rows have no indentation left to group them, so a rule
    /// carries the boundary — one tier dimmer than the rule between spaces, which
    /// separates unrelated work and so has to read as the stronger break.
    #[test]
    fn sibling_agent_rows_are_separated_by_a_dimmer_rule() {
        let (mut app, _, _) = app_with_colliding_sibling_agents();

        let area = Rect::new(0, 0, 26, 20);
        crate::ui::compute_view(&mut app, Rect::new(0, 0, 106, 20));
        let mut terminal = Terminal::new(TestBackend::new(26, 20)).unwrap();
        terminal
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let rows = &app.view.sidebar_agent_row_areas;
        let rule_y = rows[0].rect.y + rows[0].rect.height;

        assert!(
            rule_y < rows[1].rect.y,
            "sibling rows left no gap for a rule"
        );
        let cell = &buffer[(rows[0].rect.x, rule_y)];
        assert_eq!(cell.symbol(), "─");
        assert_eq!(cell.style().fg, Some(app.palette.surface1));
        assert_ne!(app.palette.surface1, app.palette.overlay0);
    }

    #[test]
    fn identical_sibling_agent_rows_fall_back_to_ordinals() {
        let (mut app, _, _) = app_with_colliding_sibling_agents();

        let rendered = rendered_sidebar(&mut app);

        // the branch now sits between the name and the discriminator, and its text
        // depends on whatever branch the test checkout is on, so match the tail only
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
        let (mut app, first, second) = app_with_colliding_sibling_agents();
        for (pane_id, label) in [(first, "api"), (second, "ui")] {
            let terminal_id = app.workspaces[0].tabs[0].panes[&pane_id]
                .attached_terminal_id
                .clone();
            app.terminals.get_mut(&terminal_id).unwrap().manual_label = Some(label.into());
        }

        let rendered = rendered_sidebar(&mut app);

        assert!(rendered.contains("api"), "rendered:\n{rendered}");
        assert!(rendered.contains("ui"), "rendered:\n{rendered}");
        assert!(!rendered.contains("pi · 1"), "rendered:\n{rendered}");
    }

    #[test]
    fn distinguishable_sibling_agent_rows_get_no_discriminator() {
        let (mut app, first, _) = app_with_colliding_sibling_agents();
        let terminal_id = app.workspaces[0].tabs[0].panes[&first]
            .attached_terminal_id
            .clone();
        app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(Agent::Claude);

        let rendered = rendered_sidebar(&mut app);

        assert!(rendered.contains("cc"), "rendered:\n{rendered}");
        assert!(rendered.contains("pi"), "rendered:\n{rendered}");
        assert!(!rendered.contains("pi · 1"), "rendered:\n{rendered}");
        assert!(!rendered.contains("cc · 1"), "rendered:\n{rendered}");
    }

    #[test]
    fn collapsed_group_hides_inactive_children_but_keeps_active_visible() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_worktree_space("main", Some("repo-key"), "/repo/herdr"),
            workspace_with_worktree_space("issue", Some("repo-key"), "/repo/herdr-issue"),
        ];
        app.active = Some(1);
        app.mode = Mode::Terminal;
        app.collapsed_space_keys.insert("repo-key".into());

        assert_eq!(
            workspace_list_entries(&app),
            vec![
                WorkspaceListEntry::Workspace {
                    ws_idx: 0,
                    indented: false,
                },
                WorkspaceListEntry::Workspace {
                    ws_idx: 1,
                    indented: true,
                },
            ]
        );

        app.active = None;
        app.mode = Mode::Terminal;
        assert_eq!(
            workspace_list_entries(&app),
            vec![WorkspaceListEntry::Workspace {
                ws_idx: 0,
                indented: false,
            }]
        );
    }

    #[test]
    fn collapsed_group_keeps_selected_child_visible_in_navigate_mode() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_worktree_space("main", Some("repo-key"), "/repo/herdr"),
            workspace_with_worktree_space("issue", Some("repo-key"), "/repo/herdr-issue"),
        ];
        app.mode = Mode::Navigate;
        app.selected = 1;
        app.active = Some(1);
        app.collapsed_space_keys.insert("repo-key".into());

        assert_eq!(
            workspace_list_entries(&app),
            vec![
                WorkspaceListEntry::Workspace {
                    ws_idx: 0,
                    indented: false,
                },
                WorkspaceListEntry::Workspace {
                    ws_idx: 1,
                    indented: true,
                },
            ]
        );
    }
}
