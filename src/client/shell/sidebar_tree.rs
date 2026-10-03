use super::preferences::SidebarView;
use super::render::{put_text, ShellRenderState};
use super::*;
use crate::protocol::ClientShellAgent;
use crate::ui::{ResolvedToken, ResolvedTokenKind, RowStyles};
use ratatui::{
    layout::Alignment,
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

/// the view tabs in order, so rects, labels and hit testing cannot drift apart.
pub(super) const VIEW_TABS: [(SidebarView, &str); 2] = [
    (SidebarView::Spaces, " spaces"),
    (SidebarView::Agents, "agents"),
];
const VIEW_TAB_SEPARATOR: &str = " │ ";

// the view tabs, then the rule that closes them off
const SPACES_HEADER_ROWS: u16 = 2;

/// held back on every row so text never shifts as the selection moves; only the selected row draws in it.
const SELECTION_MARK_WIDTH: u16 = 1;
/// heavy where the tree's own rules are light: it answers "which row am I on".
const SELECTION_MARK: &str = "┃";
/// the margin every row opens with, so indicator columns line up and rules start in one place.
const ROW_INDENT: u16 = 1;

/// the expanded sidebar minus its separator column; every view fills it.
pub(super) fn sidebar_content_rect(area: Rect) -> Rect {
    let content = Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height);
    if content.width == 0 || content.height == 0 {
        return Rect::default();
    }
    content
}

/// hit rects for the view tabs in [`VIEW_TABS`] order; a tab with no room gets a zero-width rect.
pub(super) fn sidebar_view_tab_rects(content: Rect) -> [Rect; VIEW_TABS.len()] {
    let mut rects = [Rect::default(); VIEW_TABS.len()];
    if content.width == 0 || content.height == 0 {
        return rects;
    }
    let separator = render::display_width(VIEW_TAB_SEPARATOR);
    let mut offset = 0u16;
    for (index, (_, label)) in VIEW_TABS.iter().enumerate() {
        let width = render::display_width(label).min(content.width.saturating_sub(offset));
        rects[index] = Rect::new(content.x.saturating_add(offset), content.y, width, 1);
        offset = offset.saturating_add(width).saturating_add(separator);
    }
    rects
}

pub(super) fn render_view_tabs(
    buffer: &mut Buffer,
    content: Rect,
    view: SidebarView,
    palette: &Palette,
    hits: &mut ShellHitMap,
) {
    if content.width == 0 || content.height == 0 {
        return;
    }
    let active = Style::default()
        .fg(palette.text)
        .add_modifier(Modifier::BOLD);
    let inactive = Style::default().fg(palette.overlay0);
    let mut spans = Vec::with_capacity(VIEW_TABS.len() * 2);
    for (tab, label) in VIEW_TABS {
        if !spans.is_empty() {
            spans.push(Span::styled(
                VIEW_TAB_SEPARATOR,
                Style::default().fg(palette.surface1),
            ));
        }
        spans.push(Span::styled(
            label,
            if tab == view { active } else { inactive },
        ));
    }
    Paragraph::new(Line::from(spans))
        .render(Rect::new(content.x, content.y, content.width, 1), buffer);
    hits.sidebar_view_tabs.extend(
        VIEW_TABS
            .iter()
            .zip(sidebar_view_tab_rects(content))
            .filter(|(_, rect)| rect.width > 0)
            .map(|((tab, _), rect)| (rect, *tab)),
    );
}

/// the rows under the header and above the shared footer.
fn spaces_body_rect(content: Rect) -> Rect {
    if content.width == 0 || content.height <= SPACES_HEADER_ROWS {
        return Rect::default();
    }
    let body_y = content.y.saturating_add(SPACES_HEADER_ROWS);
    let footer_y = content.bottom().saturating_sub(1);
    Rect::new(
        content.x,
        body_y,
        content.width,
        footer_y.saturating_sub(body_y),
    )
}

/// enclosed forms say "which one"; past 20 Unicode stops enclosing and plain digits are left.
pub(super) fn space_number_label(number: usize) -> String {
    const FIRST_ENCLOSED: u32 = 0x2460;
    u32::try_from(number)
        .ok()
        .filter(|n| (1..=20).contains(n))
        .and_then(|n| char::from_u32(FIRST_ENCLOSED + n - 1))
        .map(String::from)
        .unwrap_or_else(|| number.to_string())
}

#[derive(Clone, Copy)]
pub(super) enum TreeRow {
    Space(WorkspaceEntry),
    /// an agent row under its space; indices into the snapshot's workspaces and agents
    Agent {
        workspace: usize,
        agent: usize,
    },
}

/// every row the spaces view draws, built once per pass from the snapshot alone.
pub(super) struct SidebarTree {
    pub(super) rows: Vec<TreeRow>,
    pub(super) heights: Vec<u16>,
    pub(super) gaps: Vec<u16>,
    tokens: Vec<Vec<Vec<ResolvedToken>>>,
    discriminators: HashMap<usize, String>,
    one_tab_spaces: HashSet<usize>,
}

struct SnapshotIndex<'a> {
    workspace_by_id: HashMap<&'a str, usize>,
    tab_by_id: HashMap<&'a str, &'a ClientShellTab>,
    tab_count: HashMap<&'a str, usize>,
    pane_label: HashMap<&'a str, &'a str>,
}

impl<'a> SnapshotIndex<'a> {
    fn new(snapshot: &'a ClientShellSnapshot) -> Self {
        let mut tab_count = HashMap::<&str, usize>::new();
        for tab in &snapshot.tabs {
            *tab_count.entry(tab.workspace_id.as_str()).or_default() += 1;
        }
        Self {
            workspace_by_id: snapshot
                .workspaces
                .iter()
                .enumerate()
                .map(|(index, workspace)| (workspace.workspace_id.as_str(), index))
                .collect(),
            tab_by_id: snapshot
                .tabs
                .iter()
                .map(|tab| (tab.tab_id.as_str(), tab))
                .collect(),
            tab_count,
            pane_label: snapshot
                .panes
                .iter()
                .filter_map(|pane| Some((pane.pane_id.as_str(), pane.label.as_deref()?)))
                .collect(),
        }
    }

    /// a lone auto-named tab says nothing, so only a custom name or a sibling earns a label.
    fn tab_label(&self, agent: &ClientShellAgent) -> Option<&'a str> {
        let tab = self.tab_by_id.get(agent.tab_id.as_str())?;
        let count = self
            .tab_count
            .get(agent.workspace_id.as_str())
            .copied()
            .unwrap_or(0);
        (count > 1 || tab.custom_label).then_some(tab.label.as_str())
    }

    fn pane_label<'b>(&self, agent: &'b ClientShellAgent) -> Option<&'b str>
    where
        'a: 'b,
    {
        agent
            .title
            .as_deref()
            .or_else(|| self.pane_label.get(agent.pane_id.as_str()).copied())
    }
}

/// a worktree child is named after its branch, without the `worktree/` prefix herdr adds.
fn grouped_child_label(workspace: &ClientShellWorkspace) -> &str {
    if workspace.custom_label {
        return &workspace.label;
    }
    workspace
        .branch
        .as_deref()
        .map(|branch| branch.strip_prefix("worktree/").unwrap_or(branch))
        .unwrap_or(&workspace.label)
}

fn space_tokens(
    workspace: &ClientShellWorkspace,
    status: crate::api::schema::AgentStatus,
    indented: bool,
    suppress_branch: bool,
    config: &SpacesSidebarConfig,
) -> Vec<Vec<ResolvedToken>> {
    let label = if indented {
        grouped_child_label(workspace)
    } else {
        &workspace.label
    };
    let tokens = workspace.tokens.iter().cloned().collect::<HashMap<_, _>>();
    crate::ui::sidebar_space_rows(
        config,
        crate::ui::SpaceTokenContext {
            workspace: label,
            branch: workspace.branch.as_deref(),
            state_text: super::agent_sidebar::sidebar_status_text(status),
            ahead_behind: workspace.git_ahead_behind,
            tokens: &tokens,
            suppress_git_details: indented,
            suppress_branch,
        },
    )
}

fn nested_agent_tokens(
    index: &SnapshotIndex<'_>,
    workspace: &ClientShellWorkspace,
    agent: &ClientShellAgent,
    config: &crate::config::AgentsSidebarConfig,
) -> Vec<Vec<ResolvedToken>> {
    let labels = agent
        .state_labels
        .iter()
        .cloned()
        .collect::<HashMap<_, _>>();
    let tokens = agent.tokens.iter().cloned().collect::<HashMap<_, _>>();
    let state_text = labels
        .get(status_text(agent.agent_status))
        .map(String::as_str)
        .unwrap_or_else(|| super::agent_sidebar::sidebar_status_text(agent.agent_status));
    // only a linked worktree renders the grouped name, so only it can repeat its branch
    let space_label = if workspace
        .worktree
        .as_ref()
        .is_some_and(|worktree| worktree.is_linked_worktree)
    {
        grouped_child_label(workspace)
    } else {
        &workspace.label
    };
    crate::ui::sidebar_nested_agent_rows(
        config,
        crate::ui::AgentTokenContext {
            machine: None,
            workspace: &workspace.label,
            tab: index.tab_label(agent),
            pane: index.pane_label(agent),
            agent_label: agent
                .display_agent
                .as_deref()
                .or(agent.name.as_deref())
                .or(agent.agent.as_deref())
                .or(agent.title.as_deref()),
            terminal_title: agent.terminal_title.as_deref(),
            terminal_title_stripped: agent.terminal_title_stripped.as_deref(),
            canonical_agent: agent
                .agent
                .as_deref()
                .and_then(crate::detect::parse_agent_label),
            tokens: &tokens,
        },
        state_text,
        crate::ui::NestedContext {
            branch: workspace.branch.as_deref(),
            space_label: Some(space_label),
        },
    )
}

pub(super) fn build_tree(
    snapshot: &ClientShellSnapshot,
    collapsed_groups: &HashSet<String>,
    config: &ClientShellConfig,
) -> SidebarTree {
    let index = SnapshotIndex::new(snapshot);
    let mut agents_by_workspace = HashMap::<usize, Vec<usize>>::new();
    for (agent_index, agent) in snapshot.agents.iter().enumerate() {
        if let Some(workspace) = index.workspace_by_id.get(agent.workspace_id.as_str()) {
            agents_by_workspace
                .entry(*workspace)
                .or_default()
                .push(agent_index);
        }
    }

    let mut rows = Vec::with_capacity(snapshot.workspaces.len() + snapshot.agents.len());
    for entry in super::render::workspace_entries(snapshot, collapsed_groups) {
        rows.push(TreeRow::Space(entry));
        for agent in agents_by_workspace.get(&entry.index).into_iter().flatten() {
            rows.push(TreeRow::Agent {
                workspace: entry.index,
                agent: *agent,
            });
        }
    }

    let tokens = rows
        .iter()
        .enumerate()
        .map(|(position, row)| match *row {
            TreeRow::Space(entry) => {
                let workspace = &snapshot.workspaces[entry.index];
                // the branch rides on the agent rows, so the space only carries it without them
                let suppress_branch = matches!(rows.get(position + 1), Some(TreeRow::Agent { .. }));
                space_tokens(
                    workspace,
                    sidebar::displayed_workspace_status(snapshot, workspace, collapsed_groups),
                    entry.indented,
                    suppress_branch,
                    &config.spaces,
                )
            }
            TreeRow::Agent { workspace, agent } => nested_agent_tokens(
                &index,
                &snapshot.workspaces[workspace],
                &snapshot.agents[agent],
                &config.agents,
            ),
        })
        .collect::<Vec<_>>();
    let heights = tokens
        .iter()
        .map(|rows| rows.len().clamp(1, u16::MAX as usize) as u16)
        .collect();
    let gaps = rows
        .iter()
        .enumerate()
        .map(|(position, row)| match rows.get(position + 1) {
            // sibling agents get a row for their rule; a space and its first agent stay glued
            Some(TreeRow::Agent { .. }) => u16::from(matches!(row, TreeRow::Agent { .. })),
            // a worktree group is one block, so no gap opens before an indented child
            Some(TreeRow::Space(next)) if !next.indented => config.spaces.row_gap,
            Some(TreeRow::Space(_)) | None => 0,
        })
        .collect();

    let discriminators = nested_row_discriminators(snapshot, &index, &rows, &tokens);
    let one_tab_spaces = spaces_with_one_tab_label(snapshot, &index);
    SidebarTree {
        rows,
        heights,
        gaps,
        tokens,
        discriminators,
        one_tab_spaces,
    }
}

/// text telling identical sibling rows apart: the pane's label, then its title, else ordinals.
fn nested_row_discriminators(
    snapshot: &ClientShellSnapshot,
    index: &SnapshotIndex<'_>,
    rows: &[TreeRow],
    tokens: &[Vec<Vec<ResolvedToken>>],
) -> HashMap<usize, String> {
    let mut groups = HashMap::<(usize, String), Vec<usize>>::new();
    for (row, row_tokens) in rows.iter().zip(tokens) {
        if let TreeRow::Agent { workspace, agent } = *row {
            groups
                .entry((workspace, crate::ui::sidebar_rows_text(row_tokens)))
                .or_default()
                .push(agent);
        }
    }
    let mut discriminators = HashMap::new();
    for ((_, text), members) in &groups {
        if members.len() < 2 {
            continue;
        }
        let candidates = members
            .iter()
            .map(|member| {
                let agent = &snapshot.agents[*member];
                index
                    .pane_label(agent)
                    .or(agent.terminal_title_stripped.as_deref())
                    .filter(|candidate| !text.contains(candidate))
            })
            .collect::<Vec<_>>();
        // a candidate only counts when it makes every row in the group distinct
        let distinct = candidates.iter().all(Option::is_some)
            && candidates.iter().collect::<HashSet<_>>().len() == candidates.len();
        for (position, member) in members.iter().enumerate() {
            let label = match candidates[position].filter(|_| distinct) {
                Some(candidate) => candidate.to_owned(),
                None => (position + 1).to_string(),
            };
            discriminators.insert(*member, label);
        }
    }
    discriminators
}

/// spaces whose agents all share one tab label, where the tab tells the rows apart from nothing.
fn spaces_with_one_tab_label(
    snapshot: &ClientShellSnapshot,
    index: &SnapshotIndex<'_>,
) -> HashSet<usize> {
    let mut first = HashMap::<usize, Option<&str>>::new();
    let mut mixed = HashSet::new();
    for agent in &snapshot.agents {
        let Some(workspace) = index.workspace_by_id.get(agent.workspace_id.as_str()) else {
            continue;
        };
        let label = index.tab_label(agent);
        match first.get(workspace) {
            None => {
                first.insert(*workspace, label);
            }
            Some(existing) if *existing != label => {
                mixed.insert(*workspace);
            }
            Some(_) => {}
        }
    }
    first
        .into_keys()
        .filter(|workspace| !mixed.contains(workspace))
        .collect()
}

impl SidebarTree {
    /// the rows an agent paints: the height pass counts them before these trims and additions.
    fn painted_agent_rows(&self, position: usize) -> Vec<Vec<ResolvedToken>> {
        let mut rows = self.tokens[position].clone();
        let TreeRow::Agent { workspace, agent } = self.rows[position] else {
            return rows;
        };
        if self.one_tab_spaces.contains(&workspace) {
            for row in &mut rows {
                row.retain(|token| !matches!(token.kind, ResolvedTokenKind::Tab(_)));
            }
            rows.retain(|row| !row.is_empty());
        }
        // appended to the last row, so it never changes the row's height
        if let Some((extra, last)) = self.discriminators.get(&agent).zip(rows.last_mut()) {
            last.push(ResolvedToken::plain(ResolvedTokenKind::Pane(extra.clone())));
        }
        // a real row rather than bare height, so the fill covers it like any other
        rows.resize(rows.len().max(1), Vec::new());
        rows
    }
}

impl ClientShellState {
    /// agent navigation the client started scrolls the tree to that agent once focus lands.
    pub(super) fn reveal_agent_in_tree(&mut self, pane_id: &str) {
        if self.endpoints.len() == 1 && self.sidebar_view == SidebarView::Spaces {
            self.pending_tree_reveal = Some(TreeReveal::Agent(pane_id.to_owned()));
        }
    }
}

/// the tree row a pending reveal asks for, once the snapshot can answer it.
fn resolve_reveal(
    snapshot: &ClientShellSnapshot,
    tree: &SidebarTree,
    pending: &mut Option<TreeReveal>,
) -> Option<usize> {
    let space_row = |workspace_id: &str| {
        tree.rows.iter().position(|row| {
            matches!(row, TreeRow::Space(entry)
                if snapshot.workspaces[entry.index].workspace_id == workspace_id)
        })
    };
    match pending.take()? {
        TreeReveal::Space(workspace_id) => space_row(&workspace_id),
        TreeReveal::Agent(pane_id) => {
            let agent = snapshot
                .agents
                .iter()
                .position(|agent| agent.pane_id == pane_id)?;
            if snapshot.focused_pane_id.as_deref() != Some(pane_id.as_str()) {
                *pending = Some(TreeReveal::Agent(pane_id));
                return None;
            }
            tree.rows
                .iter()
                .position(|row| {
                    matches!(row, TreeRow::Agent { agent: candidate, .. } if *candidate == agent)
                })
                // a hidden agent row still leaves its space to point at
                .or_else(|| space_row(&snapshot.agents[agent].workspace_id))
        }
    }
}

// overlay0, so a space boundary outranks the surface1 rule between agents
fn space_rule(buffer: &mut Buffer, x: u16, y: u16, width: u16, palette: &Palette) {
    put_text(
        buffer,
        x,
        y,
        width,
        &"─".repeat(width as usize),
        Style::default().fg(palette.overlay0),
    );
}

pub(super) fn render_spaces_view(
    buffer: &mut Buffer,
    content: Rect,
    snapshot: &ClientShellSnapshot,
    config: &ClientShellConfig,
    state: &mut ShellRenderState<'_>,
    hits: &mut ShellHitMap,
) {
    let palette = &config.palette;
    let body = spaces_body_rect(content);
    hits.workspace_body = body;
    if body.is_empty() {
        return;
    }
    let list_bottom = content.bottom().saturating_sub(1);
    let tree = build_tree(snapshot, state.collapsed_groups, config);

    let focus_moved = std::mem::take(state.reveal_focused_workspace);
    // a reveal the client asked for outranks following focus to its space
    let target = resolve_reveal(snapshot, &tree, state.pending_tree_reveal).or_else(|| {
        focus_moved
            .then(|| {
                tree.rows.iter().position(|row| {
                    matches!(row, TreeRow::Space(entry) if snapshot.workspaces[entry.index].focused)
                })
            })
            .flatten()
    });
    if let Some(target) = target {
        *state.workspace_scroll = super::scroll::list_scroll_start_to_reveal(
            &tree.heights,
            &tree.gaps,
            body.height,
            *state.workspace_scroll,
            target,
        );
    }
    let metrics = super::scroll::list_scroll_metrics(
        &tree.heights,
        &tree.gaps,
        body.height,
        *state.workspace_scroll,
    );
    hits.workspace_max_scroll = metrics.max_offset_from_bottom;
    hits.workspace_scroll_metrics = Some(metrics);
    *state.workspace_scroll = metrics
        .max_offset_from_bottom
        .saturating_sub(metrics.offset_from_bottom);
    let show_scrollbar = metrics.max_offset_from_bottom > 0;
    let list_width = body.width.saturating_sub(u16::from(show_scrollbar));

    // the tabs are a different kind of thing from the list under them, so they earn a space's rule
    if let Some(rule_y) = body.y.checked_sub(1).filter(|y| *y >= content.y) {
        space_rule(buffer, body.x, rule_y, list_width, palette);
    }

    let mut placed = Vec::<(usize, Rect)>::new();
    let mut y = body.y;
    for position in *state.workspace_scroll..tree.rows.len() {
        let height = tree.heights[position].min(body.height);
        if y.saturating_add(height) > body.bottom() {
            break;
        }
        placed.push((position, Rect::new(body.x, y, list_width, height)));
        y = y
            .saturating_add(height)
            .saturating_add(tree.gaps[position])
            .min(body.bottom());
    }

    if config.spaces.row_gap > 0 {
        for (position, rect) in &placed {
            if !matches!(tree.rows[*position], TreeRow::Space(entry) if !entry.indented)
                || rect.width == 0
            {
                continue;
            }
            // a top-level space starts a block, so the gap row above it holds the boundary
            if let Some(rule_y) = rect
                .y
                .checked_sub(1)
                .filter(|y| *y >= body.y && *y < list_bottom)
            {
                space_rule(buffer, rect.x, rule_y, rect.width, palette);
            }
        }
    }

    render_space_rows(
        buffer,
        snapshot,
        config,
        state,
        &tree,
        &placed,
        list_bottom,
        hits,
    );
    render_agent_rows(
        buffer,
        snapshot,
        config,
        state,
        &tree,
        &placed,
        list_bottom,
        hits,
    );

    if let Some(row) = state
        .workspace_drop_indicator_row
        .filter(|row| *row < list_bottom)
    {
        put_text(
            buffer,
            body.x,
            row,
            list_width,
            &"─".repeat(list_width as usize),
            Style::default().fg(palette.accent),
        );
    }

    if show_scrollbar && body.width > 1 {
        let track = Rect::new(body.right().saturating_sub(1), body.y, 1, body.height);
        hits.workspace_scrollbar = track;
        super::scroll::render_list_scrollbar(buffer, track, metrics, palette);
    }
}

#[allow(clippy::too_many_arguments)] // one painting pass over shared layout; splitting it hides nothing
fn render_space_rows(
    buffer: &mut Buffer,
    snapshot: &ClientShellSnapshot,
    config: &ClientShellConfig,
    state: &ShellRenderState<'_>,
    tree: &SidebarTree,
    placed: &[(usize, Rect)],
    list_bottom: u16,
    hits: &mut ShellHitMap,
) {
    let palette = &config.palette;
    // ordinals count the top-level rows on screen; the slot is sized once so no name shifts
    let space_numbers = placed
        .iter()
        .filter(|(position, _)| matches!(tree.rows[*position], TreeRow::Space(entry) if !entry.indented))
        .enumerate()
        .map(|(ordinal, (position, _))| (*position, ordinal + 1))
        .collect::<HashMap<_, _>>();
    let number_width = space_numbers.values().copied().max().map_or(0, |highest| {
        space_number_label(highest).chars().count() as u16
    });

    for (position, rect) in placed {
        let TreeRow::Space(entry) = tree.rows[*position] else {
            continue;
        };
        let workspace = &snapshot.workspaces[entry.index];
        let selected = state.selected_workspace_id.is_some_and(|target| {
            target.matches(state.active_endpoint_id, &workspace.workspace_id)
        });
        let is_active = workspace.focused;
        let is_dragged = state.dragged_workspace_id == Some(workspace.workspace_id.as_str());
        let space_agent_rows = || {
            placed
                .iter()
                .filter_map(|(other, _)| match tree.rows[*other] {
                    TreeRow::Agent { workspace, agent } if workspace == entry.index => Some(agent),
                    _ => None,
                })
        };
        // the focused agent's own row carries the highlight; marking the space too says it twice
        let focused_agent_row_shown =
            space_agent_rows().any(|agent| snapshot.agents[agent].focused);
        let agent_rows_shown = space_agent_rows().next().is_some();
        let highlighted = selected || (is_active && !focused_agent_row_shown) || is_dragged;

        if highlighted {
            let fill = if selected {
                palette.surface0
            } else if is_dragged {
                palette.surface1
            } else {
                palette.active_row_bg
            };
            for y in rect.y..rect.bottom().min(list_bottom) {
                for x in rect.x..rect.right() {
                    buffer[(x, y)].set_style(Style::default().bg(fill));
                }
                // a space only carries the highlight when no agent row does, and is marked alike
                if let Some(edge) = rect.right().checked_sub(SELECTION_MARK_WIDTH) {
                    buffer[(edge, y)].set_symbol(SELECTION_MARK);
                    buffer[(edge, y)].set_style(Style::default().fg(palette.accent).bg(fill));
                }
            }
        }

        // the brightest tier in the tree: a space is the thing you scan for first
        let name_style = Style::default().fg(palette.text);
        let name_style = if selected || is_active || is_dragged {
            name_style.add_modifier(Modifier::BOLD)
        } else {
            name_style
        };
        let status =
            sidebar::displayed_workspace_status(snapshot, workspace, state.collapsed_groups);
        // each agent row carries its own mark, so the space only speaks for them while they are hidden
        let icon_status = if agent_rows_shown {
            crate::api::schema::AgentStatus::Unknown
        } else {
            status
        };
        let state_icon = (
            space_status_glyph(icon_status, config.status_indicators),
            Style::default().fg(status_color(icon_status, palette)),
        );
        // colour says what kind of token it is; a branch keeps one colour on any row
        let branch_style = Style::default().fg(palette.mauve);
        let styles = RowStyles {
            state_text: Style::default()
                .fg(status_text_color(status, palette))
                .add_modifier(Modifier::DIM),
            workspace: name_style,
            agent: branch_style,
            branch: branch_style,
            secondary: branch_style,
            custom: branch_style,
            separator: tree_separator_style(palette),
        };
        let is_last_child = entry.indented && entry.last_child;

        for (row_index, resolved) in tree.tokens[*position].iter().enumerate() {
            let y = rect.y + row_index as u16;
            if row_index as u16 >= rect.height || y >= list_bottom {
                break;
            }
            let indent = " ".repeat(ROW_INDENT as usize);
            let mut spans = vec![Span::raw(indent)];
            // every block hangs from its own mark column, so its branch and children start there too
            let prefix_width = if entry.indented {
                if row_index == 0 {
                    spans.push(Span::styled(
                        if is_last_child { "└─ " } else { "├─ " },
                        Style::default().fg(palette.overlay0),
                    ));
                } else if is_last_child {
                    spans.push(Span::raw("   "));
                } else {
                    spans.push(Span::styled("│", Style::default().fg(palette.overlay0)));
                    spans.push(Span::raw("  "));
                }
                ROW_INDENT + 3
            } else if row_index == 0 {
                ROW_INDENT
            } else {
                // the column an agent row's bar would have, in the tier that means nothing runs
                spans.push(Span::styled("│", Style::default().fg(palette.surface1)));
                spans.push(Span::raw(" "));
                ROW_INDENT + 2
            };
            let trailing_width = SELECTION_MARK_WIDTH + number_width;
            spans.extend(
                crate::ui::styled_token_spans(
                    resolved,
                    state_icon,
                    styles,
                    palette,
                    rect.width.saturating_sub(prefix_width + trailing_width) as usize,
                    crate::ui::sidebar_separator_tree,
                )
                .spans,
            );
            Paragraph::new(Line::from(spans)).render(Rect::new(rect.x, y, rect.width, 1), buffer);
        }

        if let Some(number) = space_numbers
            .get(position)
            .filter(|_| number_width > 0 && rect.y < list_bottom)
        {
            let slot = rect
                .right()
                .saturating_sub(SELECTION_MARK_WIDTH + number_width);
            // one tier up from the rules: a handle you cannot read is not one
            Paragraph::new(Span::styled(
                space_number_label(*number),
                Style::default().fg(palette.overlay0),
            ))
            .alignment(Alignment::Right)
            .render(Rect::new(slot, rect.y, number_width, 1), buffer);
        }

        hits.workspaces.push(WorkspaceHit {
            rect: *rect,
            endpoint_id: ClientEndpointId::Local,
            workspace_id: workspace.workspace_id.clone(),
            indented: entry.indented,
            // the arrow is gone; the key still marks a group's parent for drag and drop
            group_toggle: sidebar::parent_group_key(snapshot, entry.index)
                .map(|key| (Rect::default(), key)),
        });
    }
}

fn tree_separator_style(palette: &Palette) -> Style {
    Style::default()
        .fg(palette.overlay0)
        .add_modifier(Modifier::DIM)
}

#[allow(clippy::too_many_arguments)] // one painting pass over shared layout; splitting it hides nothing
fn render_agent_rows(
    buffer: &mut Buffer,
    snapshot: &ClientShellSnapshot,
    config: &ClientShellConfig,
    state: &ShellRenderState<'_>,
    tree: &SidebarTree,
    placed: &[(usize, Rect)],
    list_bottom: u16,
    hits: &mut ShellHitMap,
) {
    let palette = &config.palette;
    let agent_rows = placed
        .iter()
        .filter_map(|(position, rect)| match tree.rows[*position] {
            TreeRow::Agent { workspace, agent } => Some((*position, workspace, agent, *rect)),
            TreeRow::Space(_) => None,
        })
        .collect::<Vec<_>>();

    // one tier under the rule between spaces: a space boundary outranks one between two agents
    for pair in agent_rows.windows(2) {
        let ((_, workspace, _, rect), (_, next_workspace, _, next)) = (pair[0], pair[1]);
        let rule_y = rect.bottom();
        if workspace != next_workspace || rule_y >= list_bottom || rule_y >= next.y {
            continue;
        }
        // it starts in the bar's own column, so the bar reads as one edge down the list
        let width = rect.width.saturating_sub(ROW_INDENT);
        put_text(
            buffer,
            rect.x.saturating_add(ROW_INDENT),
            rule_y,
            width,
            &"─".repeat(width as usize),
            Style::default().fg(palette.surface1),
        );
    }

    for (position, _, agent_index, rect) in agent_rows {
        let agent = &snapshot.agents[agent_index];
        let rows = tree.painted_agent_rows(position);
        let is_active = agent.focused;
        let label_color = status_text_color(agent.agent_status, palette);
        // a tier below the space names above, in another hue from the branch beside it
        let name_style = Style::default().fg(palette.blue);
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
        // overlay0 already sits near 3.4:1, and DIM on top of it lands near 1.4:1
        let secondary_style = Style::default().fg(palette.overlay0);
        let bar = agent_bar_cells(agent.agent_status, config.status_indicators, rows.len());
        let bar_style = Style::default().fg(status_color(agent.agent_status, palette));
        let styles = RowStyles {
            state_text: status_style,
            workspace: name_style,
            agent: name_style,
            branch: Style::default().fg(palette.mauve),
            secondary: secondary_style,
            custom: secondary_style,
            separator: tree_separator_style(palette),
        };

        if is_active {
            for y in rect.y..rect.bottom().min(list_bottom) {
                // the margin and the mark's column stay out: the fill runs from the content to the accent
                let fill_end = rect.right().saturating_sub(SELECTION_MARK_WIDTH);
                for x in rect.x.saturating_add(ROW_INDENT)..fill_end {
                    buffer[(x, y)].set_style(Style::default().bg(palette.surface_dim));
                }
                if let Some(edge) = rect.right().checked_sub(SELECTION_MARK_WIDTH) {
                    buffer[(edge, y)].set_symbol(SELECTION_MARK);
                    // the glyph's colour only, or a block would print beside the panel edge
                    buffer[(edge, y)].set_fg(palette.accent);
                }
            }
        }

        let top = bar[0].symbol(state.spinner_frame);
        let lower = bar[1].symbol(state.spinner_frame);
        for (row_index, resolved) in rows.iter().enumerate() {
            let y = rect.y + row_index as u16;
            if row_index as u16 >= rect.height || y >= list_bottom {
                break;
            }
            // flush left with the space rows: nesting needs neither connectors nor indentation
            let mut spans = vec![Span::raw(" ".repeat(ROW_INDENT as usize))];
            let mut prefix_width = ROW_INDENT;
            if row_index > 0 {
                // the bar owns this column on every row, so the rows stack into one shape
                spans.push(Span::styled(lower.to_string(), bar_style));
                spans.push(Span::raw(" "));
                prefix_width += 2;
                record_turning(hits, bar[1], rect.x + ROW_INDENT, y, rect);
            }
            let styled = crate::ui::styled_token_spans(
                resolved,
                (top.as_ref(), bar_style),
                styles,
                palette,
                rect.width
                    .saturating_sub(prefix_width + SELECTION_MARK_WIDTH) as usize,
                crate::ui::sidebar_separator_tree,
            );
            if let Some(column) = styled.state_icon_column {
                record_turning(
                    hits,
                    bar[0],
                    rect.x.saturating_add(prefix_width).saturating_add(column),
                    y,
                    rect,
                );
            }
            spans.extend(styled.spans);
            Paragraph::new(Line::from(spans)).render(Rect::new(rect.x, y, rect.width, 1), buffer);
        }

        hits.agents.push((rect, agent.pane_id.clone()));
    }
}

fn record_turning(hits: &mut ShellHitMap, cell: BarCell, x: u16, y: u16, rect: Rect) {
    if let (BarCell::Turning(glyph), true) = (cell, x < rect.right()) {
        hits.spinner_cells.push(spinner::SpinnerCell {
            x,
            y,
            glyph,
            cell: None,
        });
    }
}
