use super::AgentPanelEntry;
use crate::config::{
    AgentSidebarToken, AgentsSidebarConfig, SidebarTokenStyle, SpaceSidebarToken,
    SpacesSidebarConfig,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedToken {
    pub kind: ResolvedTokenKind,
    pub style: SidebarTokenStyle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ResolvedTokenKind {
    StateIcon,
    StateText(String),
    Workspace(String),
    Tab(String),
    Pane(String),
    Agent(String),
    TerminalTitle(String),
    Branch(String),
    GitStatus { ahead: usize, behind: usize },
    Custom(String),
}

impl ResolvedToken {
    fn new(kind: ResolvedTokenKind, style: SidebarTokenStyle) -> Self {
        Self { kind, style }
    }

    pub(super) fn plain(kind: ResolvedTokenKind) -> Self {
        Self::new(kind, SidebarTokenStyle::default())
    }

    #[cfg(test)]
    pub(super) fn unstyled(kind: ResolvedTokenKind) -> Self {
        Self::plain(kind)
    }
}

/// What an agent is called on its own row. The name that identifies a row is read
/// hundreds of times a day and never puzzled over, so the two agents in constant use
/// get the initials people already say out loud; the rest keep their name, which is
/// what a reader needs the first time they meet one.
///
/// Presentation only — `agent_label` stays the agent's name everywhere else, because
/// the API, detection and config all match on it.
fn agent_row_label(label: &str) -> String {
    match label {
        "claude" => "cc".to_string(),
        "codex" => "cx".to_string(),
        other => other.to_string(),
    }
}

/// The text a set of rows renders, for comparing two rows for sameness. Uses a
/// separator no label can contain so `["ab", "c"]` cannot collide with `["a", "bc"]`.
pub(super) fn rows_text(rows: &[Vec<ResolvedToken>]) -> String {
    rows.iter()
        .flatten()
        .map(|token| match &token.kind {
            ResolvedTokenKind::StateIcon => "",
            ResolvedTokenKind::GitStatus { .. } => "",
            ResolvedTokenKind::StateText(text)
            | ResolvedTokenKind::Workspace(text)
            | ResolvedTokenKind::Tab(text)
            | ResolvedTokenKind::Pane(text)
            | ResolvedTokenKind::Agent(text)
            | ResolvedTokenKind::TerminalTitle(text)
            | ResolvedTokenKind::Branch(text)
            | ResolvedTokenKind::Custom(text) => text.as_str(),
        })
        .collect::<Vec<_>>()
        .join("\u{1}")
}

/// What the parent space row already shows, so a nested row can skip repeating it.
#[derive(Clone, Copy)]
pub(super) struct NestedContext<'a> {
    pub branch: Option<&'a str>,
    /// The name the space row renders. A worktree child is named after its own
    /// branch, so without this the branch would be printed directly under itself.
    pub space_label: Option<&'a str>,
}

pub(super) fn agent_rows(
    config: &AgentsSidebarConfig,
    entry: &AgentPanelEntry,
    state_text: &str,
) -> Vec<Vec<ResolvedToken>> {
    resolve_agent_rows(config, entry, state_text, None)
}

/// Agent rows nested under their own space row in the spaces tree. The space
/// name is already on the parent row, so the `workspace` token resolves to
/// nothing and whatever else shared its row merges into the next row instead of
/// costing a line. A tab named after the branch is dropped for the same reason.
pub(super) fn nested_agent_rows(
    config: &AgentsSidebarConfig,
    entry: &AgentPanelEntry,
    state_text: &str,
    parent: NestedContext<'_>,
) -> Vec<Vec<ResolvedToken>> {
    resolve_agent_rows(config, entry, state_text, Some(parent))
}

fn resolve_agent_rows(
    config: &AgentsSidebarConfig,
    entry: &AgentPanelEntry,
    state_text: &str,
    nested: Option<NestedContext<'_>>,
) -> Vec<Vec<ResolvedToken>> {
    let mut rows: Vec<Vec<ResolvedToken>> = Vec::new();
    let mut carried: Vec<ResolvedToken> = Vec::new();
    for row in config.rows_for_agent(entry.agent) {
        let drops_workspace = nested.is_some()
            && row
                .iter()
                .any(|configured| matches!(configured.parts().0, AgentSidebarToken::Workspace));
        let resolved = row
            .iter()
            .filter_map(|configured| {
                let (token, style) = configured.parts();
                let kind = match token {
                    AgentSidebarToken::StateIcon => Some(ResolvedTokenKind::StateIcon),
                    AgentSidebarToken::StateText => {
                        Some(ResolvedTokenKind::StateText(state_text.to_string()))
                    }
                    AgentSidebarToken::Workspace if nested.is_some() => None,
                    AgentSidebarToken::Workspace => {
                        Some(ResolvedTokenKind::Workspace(entry.primary_label.clone()))
                    }
                    AgentSidebarToken::Tab => entry
                        .primary_tab_label
                        .clone()
                        // a tab named after the branch repeats what this row already carries
                        .filter(|label| {
                            nested.is_none_or(|parent| parent.branch != Some(label.as_str()))
                        })
                        .map(ResolvedTokenKind::Tab),
                    AgentSidebarToken::Pane => {
                        entry.pane_label.clone().map(ResolvedTokenKind::Pane)
                    }
                    AgentSidebarToken::Agent => entry
                        .agent_label
                        .as_deref()
                        .map(|label| ResolvedTokenKind::Agent(agent_row_label(label))),
                    // the flat list has no parent space, so there is no branch to name
                    AgentSidebarToken::Branch => nested
                        .and_then(|parent| parent.branch)
                        .filter(|branch| {
                            nested.is_none_or(|parent| parent.space_label != Some(*branch))
                        })
                        .map(|branch| ResolvedTokenKind::Branch(branch.to_string())),
                    AgentSidebarToken::TerminalTitle => entry
                        .terminal_title
                        .clone()
                        .map(ResolvedTokenKind::TerminalTitle),
                    AgentSidebarToken::TerminalTitleStripped => entry
                        .terminal_title_stripped
                        .clone()
                        .map(ResolvedTokenKind::TerminalTitle),
                    AgentSidebarToken::Custom(name) => entry
                        .tokens
                        .get(name)
                        .cloned()
                        .map(ResolvedTokenKind::Custom),
                    AgentSidebarToken::Styled { .. } => None,
                }?;
                Some(ResolvedToken::new(kind, style))
            })
            .collect::<Vec<_>>();
        if resolved.is_empty() {
            continue;
        }
        if drops_workspace {
            carried.extend(resolved);
            continue;
        }
        if carried.is_empty() {
            rows.push(resolved);
        } else {
            let mut merged = std::mem::take(&mut carried);
            merged.extend(resolved);
            rows.push(merged);
        }
    }
    if !carried.is_empty() {
        rows.push(carried);
    }
    rows
}

pub(super) struct SpaceTokenContext<'a> {
    pub workspace: &'a str,
    pub branch: Option<&'a str>,
    pub state_text: &'a str,
    pub ahead_behind: Option<(usize, usize)>,
    pub tokens: &'a std::collections::HashMap<String, String>,
    pub suppress_git_details: bool,
    /// Set when the agent rows below already name the branch.
    pub suppress_branch: bool,
}

pub(super) fn space_rows(
    config: &SpacesSidebarConfig,
    context: SpaceTokenContext<'_>,
) -> Vec<Vec<ResolvedToken>> {
    let mut rows: Vec<Vec<ResolvedToken>> = Vec::new();
    for row in &config.rows {
        // whatever shared the branch's row rides up to the row above instead of
        // costing a line of its own once the branch itself is gone
        let held_branch = context.suppress_branch
            && row
                .iter()
                .any(|configured| matches!(configured.parts().0, SpaceSidebarToken::Branch));
        {
            let resolved = row
                .iter()
                .filter_map(|configured| {
                    let (token, style) = configured.parts();
                    let kind = match token {
                        SpaceSidebarToken::StateIcon => Some(ResolvedTokenKind::StateIcon),
                        SpaceSidebarToken::StateText => {
                            Some(ResolvedTokenKind::StateText(context.state_text.to_string()))
                        }
                        SpaceSidebarToken::Workspace => {
                            Some(ResolvedTokenKind::Workspace(context.workspace.to_string()))
                        }
                        SpaceSidebarToken::Branch
                            if !context.suppress_git_details && !context.suppress_branch =>
                        {
                            context
                                .branch
                                .map(|branch| ResolvedTokenKind::Branch(branch.to_string()))
                        }
                        SpaceSidebarToken::Branch => None,
                        SpaceSidebarToken::GitStatus if !context.suppress_git_details => context
                            .ahead_behind
                            .filter(|(ahead, behind)| *ahead > 0 || *behind > 0)
                            .map(|(ahead, behind)| ResolvedTokenKind::GitStatus { ahead, behind }),
                        SpaceSidebarToken::GitStatus => None,
                        SpaceSidebarToken::Custom(name) => context
                            .tokens
                            .get(name)
                            .cloned()
                            .map(ResolvedTokenKind::Custom),
                        SpaceSidebarToken::Styled { .. } => None,
                    }?;
                    Some(ResolvedToken::new(kind, style))
                })
                .collect::<Vec<_>>();
            if resolved.is_empty() {
                continue;
            }
            match rows.last_mut().filter(|_| held_branch) {
                Some(previous) => previous.extend(resolved),
                None => rows.push(resolved),
            }
        }
    }
    rows
}

pub(super) fn separator(previous: &ResolvedToken, current: &ResolvedToken) -> &'static str {
    // a branch carries its own colour, so a dot in front of it only costs width
    if matches!(previous.kind, ResolvedTokenKind::StateIcon)
        || matches!(
            current.kind,
            ResolvedTokenKind::Branch(_) | ResolvedTokenKind::GitStatus { .. }
        )
    {
        " "
    } else {
        " · "
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AgentSidebarToken, SpaceSidebarToken};
    use crate::detect::AgentState;

    fn entry() -> AgentPanelEntry {
        AgentPanelEntry {
            ws_idx: 0,
            tab_idx: 0,
            pane_id: crate::layout::PaneId::from_raw(1),
            primary_label: "repo".into(),
            primary_tab_label: None,
            pane_label: None,
            terminal_title: None,
            terminal_title_stripped: None,
            agent_label: Some("pi".into()),
            agent_kind_label: Some("pi".into()),
            agent: Some(crate::detect::Agent::Pi),
            state: AgentState::Working,
            seen: true,
            last_agent_state_change_seq: None,
            state_labels: std::collections::HashMap::new(),
            tokens: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn missing_custom_tokens_elide_rows_and_separators() {
        let entry = entry();
        let config = AgentsSidebarConfig {
            rows: vec![
                vec![
                    AgentSidebarToken::StateIcon,
                    AgentSidebarToken::Custom("missing".into()),
                ],
                vec![AgentSidebarToken::Custom("missing".into())],
                vec![AgentSidebarToken::Agent],
            ],
            ..Default::default()
        };

        let rows = agent_rows(&config, &entry, "working");

        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0],
            vec![ResolvedToken::unstyled(ResolvedTokenKind::StateIcon)]
        );
        assert_eq!(
            rows[1],
            vec![ResolvedToken::unstyled(ResolvedTokenKind::Agent(
                "pi".into()
            ))]
        );
    }

    #[test]
    fn state_text_and_arbitrary_values_are_independent_tokens() {
        let mut entry = entry();
        entry
            .tokens
            .insert("summary".into(), "reviewing auth".into());
        let config = AgentsSidebarConfig {
            rows: vec![vec![
                AgentSidebarToken::StateText,
                AgentSidebarToken::Custom("summary".into()),
            ]],
            ..Default::default()
        };

        assert_eq!(
            agent_rows(&config, &entry, "deep in the mines"),
            vec![vec![
                ResolvedToken::unstyled(ResolvedTokenKind::StateText("deep in the mines".into())),
                ResolvedToken::unstyled(ResolvedTokenKind::Custom("reviewing auth".into())),
            ]]
        );
    }

    #[test]
    fn terminal_title_builtins_are_distinct_from_custom_tokens() {
        let mut entry = entry();
        entry.terminal_title = Some("⠋ raw title".into());
        entry.terminal_title_stripped = Some("raw title".into());
        entry
            .tokens
            .insert("terminal_title".into(), "custom title".into());
        let config = AgentsSidebarConfig {
            rows: vec![vec![
                AgentSidebarToken::TerminalTitle,
                AgentSidebarToken::TerminalTitleStripped,
                AgentSidebarToken::Custom("terminal_title".into()),
            ]],
            ..Default::default()
        };

        assert_eq!(
            agent_rows(&config, &entry, "working"),
            vec![vec![
                ResolvedToken::unstyled(ResolvedTokenKind::TerminalTitle("⠋ raw title".into())),
                ResolvedToken::unstyled(ResolvedTokenKind::TerminalTitle("raw title".into())),
                ResolvedToken::unstyled(ResolvedTokenKind::Custom("custom title".into())),
            ]]
        );
    }

    #[test]
    fn known_agent_override_replaces_default_rows() {
        let mut config = AgentsSidebarConfig {
            rows: vec![vec![AgentSidebarToken::Workspace]],
            ..Default::default()
        };
        config
            .rows_by_agent
            .insert("pi".into(), vec![vec![AgentSidebarToken::Agent]]);
        let mut pi = entry();
        pi.agent_label = Some("renamed pi".into());

        assert_eq!(
            agent_rows(&config, &pi, "working"),
            vec![vec![ResolvedToken::unstyled(ResolvedTokenKind::Agent(
                "renamed pi".into()
            ))]]
        );

        pi.agent = None;
        assert_eq!(
            agent_rows(&config, &pi, "working"),
            vec![vec![ResolvedToken::unstyled(ResolvedTokenKind::Workspace(
                "repo".into()
            ))]]
        );
    }

    #[test]
    fn nested_agent_rows_drop_workspace_and_merge_its_row_forward() {
        let config = AgentsSidebarConfig::default();

        assert_eq!(
            nested_agent_rows(
                &config,
                &entry(),
                "working",
                NestedContext {
                    branch: None,
                    space_label: None
                }
            ),
            vec![vec![
                ResolvedToken::unstyled(ResolvedTokenKind::StateIcon),
                ResolvedToken::unstyled(ResolvedTokenKind::Agent("pi".into())),
            ]]
        );
    }

    /// The title is what the agent is doing; sharing the name's row truncates it to
    /// nothing, so it gets the second line the tree already reserves for it.
    #[test]
    fn nested_agent_rows_give_the_terminal_title_its_own_row() {
        let mut entry = entry();
        entry.terminal_title_stripped = Some("reviewing the sidebar".into());

        assert_eq!(
            nested_agent_rows(
                &AgentsSidebarConfig::default(),
                &entry,
                "working",
                NestedContext {
                    branch: None,
                    space_label: None
                }
            ),
            vec![
                vec![
                    ResolvedToken::unstyled(ResolvedTokenKind::StateIcon),
                    ResolvedToken::unstyled(ResolvedTokenKind::Agent("pi".into())),
                ],
                vec![ResolvedToken::unstyled(ResolvedTokenKind::TerminalTitle(
                    "reviewing the sidebar".into()
                ))],
            ]
        );
    }

    #[test]
    fn nested_agent_rows_keep_the_tab_label_that_shared_the_workspace_row() {
        let mut entry = entry();
        entry.primary_tab_label = Some("1".into());

        assert_eq!(
            nested_agent_rows(
                &AgentsSidebarConfig::default(),
                &entry,
                "working",
                NestedContext {
                    branch: None,
                    space_label: None
                }
            ),
            vec![vec![
                ResolvedToken::unstyled(ResolvedTokenKind::StateIcon),
                ResolvedToken::unstyled(ResolvedTokenKind::Tab("1".into())),
                ResolvedToken::unstyled(ResolvedTokenKind::Agent("pi".into())),
            ]]
        );
    }

    #[test]
    fn nested_agent_rows_drop_a_tab_named_after_the_parent_branch() {
        let mut entry = entry();
        entry.primary_tab_label = Some("main".into());

        assert_eq!(
            nested_agent_rows(
                &AgentsSidebarConfig::default(),
                &entry,
                "working",
                NestedContext {
                    branch: Some("main"),
                    space_label: None,
                }
            ),
            vec![vec![
                ResolvedToken::unstyled(ResolvedTokenKind::StateIcon),
                ResolvedToken::unstyled(ResolvedTokenKind::Agent("pi".into())),
                ResolvedToken::unstyled(ResolvedTokenKind::Branch("main".into())),
            ]]
        );
    }

    #[test]
    fn nested_agent_rows_keep_a_tab_that_differs_from_the_parent_branch() {
        let mut entry = entry();
        entry.primary_tab_label = Some("review".into());

        assert_eq!(
            nested_agent_rows(
                &AgentsSidebarConfig::default(),
                &entry,
                "working",
                NestedContext {
                    branch: Some("main"),
                    space_label: None,
                }
            ),
            vec![vec![
                ResolvedToken::unstyled(ResolvedTokenKind::StateIcon),
                ResolvedToken::unstyled(ResolvedTokenKind::Tab("review".into())),
                ResolvedToken::unstyled(ResolvedTokenKind::Agent("pi".into())),
                ResolvedToken::unstyled(ResolvedTokenKind::Branch("main".into())),
            ]]
        );
    }

    #[test]
    fn flat_agent_rows_keep_a_tab_named_after_the_branch() {
        let mut entry = entry();
        entry.primary_tab_label = Some("main".into());

        let rows = agent_rows(&AgentsSidebarConfig::default(), &entry, "working");

        assert!(
            rows[0].contains(&ResolvedToken::unstyled(ResolvedTokenKind::Tab(
                "main".into()
            )))
        );
    }

    #[test]
    fn nested_agent_rows_drop_a_row_that_only_held_the_workspace() {
        let config = AgentsSidebarConfig {
            rows: vec![
                vec![AgentSidebarToken::Workspace],
                vec![AgentSidebarToken::StateIcon, AgentSidebarToken::Agent],
            ],
            ..Default::default()
        };

        assert_eq!(
            nested_agent_rows(
                &config,
                &entry(),
                "working",
                NestedContext {
                    branch: None,
                    space_label: None
                }
            ),
            vec![vec![
                ResolvedToken::unstyled(ResolvedTokenKind::StateIcon),
                ResolvedToken::unstyled(ResolvedTokenKind::Agent("pi".into())),
            ]]
        );
    }

    #[test]
    fn flat_agent_rows_still_render_the_workspace() {
        assert_eq!(
            agent_rows(&AgentsSidebarConfig::default(), &entry(), "working"),
            vec![
                vec![
                    ResolvedToken::unstyled(ResolvedTokenKind::StateIcon),
                    ResolvedToken::unstyled(ResolvedTokenKind::Workspace("repo".into())),
                ],
                vec![ResolvedToken::unstyled(ResolvedTokenKind::Agent(
                    "pi".into()
                ))],
            ]
        );
    }

    #[test]
    fn grouped_children_suppress_all_builtin_git_details() {
        let config = SpacesSidebarConfig::default();

        assert_eq!(
            space_rows(
                &config,
                SpaceTokenContext {
                    workspace: "feature",
                    branch: Some("worktree/feature"),
                    state_text: "idle",
                    ahead_behind: Some((2, 1)),
                    tokens: &std::collections::HashMap::new(),
                    suppress_git_details: true,
                    suppress_branch: false,
                },
            ),
            vec![vec![
                ResolvedToken::unstyled(ResolvedTokenKind::StateIcon),
                ResolvedToken::unstyled(ResolvedTokenKind::Workspace("feature".into())),
            ]]
        );
    }

    #[test]
    fn workspace_custom_token_can_replace_git_specific_details() {
        let tokens = std::collections::HashMap::from([("jj_status".into(), "2 changes".into())]);
        let config = SpacesSidebarConfig {
            rows: vec![vec![SpaceSidebarToken::Custom("jj_status".into())]],
            ..Default::default()
        };

        assert_eq!(
            space_rows(
                &config,
                SpaceTokenContext {
                    workspace: "repo",
                    branch: None,
                    state_text: "idle",
                    ahead_behind: None,
                    tokens: &tokens,
                    suppress_git_details: false,
                    suppress_branch: false,
                },
            ),
            vec![vec![ResolvedToken::unstyled(ResolvedTokenKind::Custom(
                "2 changes".into()
            ))]]
        );
    }
}
