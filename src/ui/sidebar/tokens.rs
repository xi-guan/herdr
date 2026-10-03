use crate::config::{
    AgentSidebarToken, AgentsSidebarConfig, SidebarTokenStyle, SpaceSidebarToken,
    SpacesSidebarConfig,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedToken {
    pub kind: ResolvedTokenKind,
    pub style: SidebarTokenStyle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ResolvedTokenKind {
    StateIcon,
    StateText(String),
    Machine(String),
    Workspace(String),
    Tab(String),
    Pane(String),
    Agent(String),
    TerminalTitle(String),
    Branch(String),
    GitStatus { ahead: usize, behind: usize },
    Custom(String),
}

impl ResolvedTokenKind {
    fn text_value(&self) -> Option<&str> {
        match self {
            Self::StateText(value)
            | Self::Machine(value)
            | Self::Workspace(value)
            | Self::Tab(value)
            | Self::Pane(value)
            | Self::Agent(value)
            | Self::TerminalTitle(value)
            | Self::Branch(value)
            | Self::Custom(value) => Some(value),
            Self::StateIcon | Self::GitStatus { .. } => None,
        }
    }
}

impl ResolvedToken {
    fn new(kind: ResolvedTokenKind, style: SidebarTokenStyle) -> Self {
        Self { kind, style }
    }

    pub(crate) fn plain(kind: ResolvedTokenKind) -> Self {
        Self::new(kind, SidebarTokenStyle::default())
    }

    #[cfg(test)]
    pub(super) fn unstyled(kind: ResolvedTokenKind) -> Self {
        Self::new(kind, SidebarTokenStyle::default())
    }
}

pub(crate) struct AgentTokenContext<'a> {
    pub(crate) machine: Option<&'a str>,
    pub(crate) workspace: &'a str,
    pub(crate) tab: Option<&'a str>,
    pub(crate) pane: Option<&'a str>,
    pub(crate) agent_label: Option<&'a str>,
    pub(crate) terminal_title: Option<&'a str>,
    pub(crate) terminal_title_stripped: Option<&'a str>,
    pub(crate) canonical_agent: Option<crate::detect::Agent>,
    pub(crate) tokens: &'a std::collections::HashMap<String, String>,
}

/// presentation only: the API, detection and config all match on the agent's full name.
fn agent_row_label(label: &str) -> &str {
    match label {
        "claude" => "cc",
        "codex" => "cx",
        other => other,
    }
}

/// finding the real default branch would need a git read the render path cannot afford.
const UNREMARKABLE_BRANCHES: [&str; 2] = ["main", "master"];

/// what the parent space row already shows, so a nested agent row can skip repeating it.
#[derive(Clone, Copy)]
pub(crate) struct NestedContext<'a> {
    pub(crate) branch: Option<&'a str>,
    /// a worktree child is named after its own branch, which must not print again beneath it
    pub(crate) space_label: Option<&'a str>,
}

#[derive(Clone, Copy)]
enum AgentRowForm<'a> {
    /// every machine's agents panel, as upstream draws it
    Shared,
    /// the single-machine agents view
    Local,
    /// an agent row hanging under its space in the spaces tree
    Nested(NestedContext<'a>),
}

pub(crate) fn agent_rows(
    config: &AgentsSidebarConfig,
    context: AgentTokenContext<'_>,
    state_text: &str,
) -> Vec<Vec<ResolvedToken>> {
    resolve_agent_rows(config, context, state_text, AgentRowForm::Shared)
}

pub(crate) fn local_agent_rows(
    config: &AgentsSidebarConfig,
    context: AgentTokenContext<'_>,
    state_text: &str,
) -> Vec<Vec<ResolvedToken>> {
    resolve_agent_rows(config, context, state_text, AgentRowForm::Local)
}

/// the space row names the space, so `workspace` drops and its row merges into the next one.
pub(crate) fn nested_agent_rows(
    config: &AgentsSidebarConfig,
    context: AgentTokenContext<'_>,
    state_text: &str,
    parent: NestedContext<'_>,
) -> Vec<Vec<ResolvedToken>> {
    resolve_agent_rows(config, context, state_text, AgentRowForm::Nested(parent))
}

fn resolve_agent_rows(
    config: &AgentsSidebarConfig,
    context: AgentTokenContext<'_>,
    state_text: &str,
    form: AgentRowForm<'_>,
) -> Vec<Vec<ResolvedToken>> {
    let nested = match form {
        AgentRowForm::Nested(parent) => Some(parent),
        AgentRowForm::Shared | AgentRowForm::Local => None,
    };
    let shorten = !matches!(form, AgentRowForm::Shared);
    let mut rows: Vec<Vec<ResolvedToken>> = Vec::new();
    let mut carried: Vec<ResolvedToken> = Vec::new();
    for row in config.rows_for_agent(context.canonical_agent) {
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
                    AgentSidebarToken::Machine => context
                        .machine
                        .map(|value| ResolvedTokenKind::Machine(value.to_string())),
                    AgentSidebarToken::Workspace if nested.is_some() => None,
                    AgentSidebarToken::Workspace => {
                        Some(ResolvedTokenKind::Workspace(context.workspace.to_string()))
                    }
                    AgentSidebarToken::Tab => context
                        .tab
                        // a tab named after the branch repeats what the space row carries
                        .filter(|label| nested.is_none_or(|parent| parent.branch != Some(*label)))
                        .map(|value| ResolvedTokenKind::Tab(value.to_string())),
                    AgentSidebarToken::Pane => context
                        .pane
                        .map(|value| ResolvedTokenKind::Pane(value.to_string())),
                    AgentSidebarToken::Agent => context
                        .agent_label
                        .map(|value| ResolvedTokenKind::Agent(value.to_string())),
                    // the flat lists have no parent space, so there is no branch to name
                    AgentSidebarToken::Branch => nested
                        .and_then(|parent| parent.branch)
                        .filter(|branch| {
                            nested.is_none_or(|parent| parent.space_label != Some(*branch))
                        })
                        .filter(|branch| !UNREMARKABLE_BRANCHES.contains(branch))
                        .map(|branch| ResolvedTokenKind::Branch(branch.to_string())),
                    AgentSidebarToken::TerminalTitle => context
                        .terminal_title
                        .map(|value| ResolvedTokenKind::TerminalTitle(value.to_string())),
                    AgentSidebarToken::TerminalTitleStripped => context
                        .terminal_title_stripped
                        .map(|value| ResolvedTokenKind::TerminalTitle(value.to_string())),
                    AgentSidebarToken::Custom(name) => context
                        .tokens
                        .get(name)
                        .cloned()
                        .map(ResolvedTokenKind::Custom),
                    AgentSidebarToken::Styled { .. } => None,
                }?;
                let style = kind
                    .text_value()
                    .map_or(Some(style), |value| configured.style_for_value(value))?;
                // rules matched the full name above; only the drawn text is shortened
                let kind = match kind {
                    ResolvedTokenKind::Agent(label) if shorten => {
                        ResolvedTokenKind::Agent(agent_row_label(&label).to_string())
                    }
                    kind => kind,
                };
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

pub(crate) struct SpaceTokenContext<'a> {
    pub(crate) workspace: &'a str,
    pub(crate) branch: Option<&'a str>,
    pub(crate) state_text: &'a str,
    pub(crate) ahead_behind: Option<(usize, usize)>,
    pub(crate) tokens: &'a std::collections::HashMap<String, String>,
    pub(crate) suppress_git_details: bool,
    /// set when the agent rows below already name the branch
    pub(crate) suppress_branch: bool,
}

pub(crate) fn space_rows(
    config: &SpacesSidebarConfig,
    context: SpaceTokenContext<'_>,
) -> Vec<Vec<ResolvedToken>> {
    let mut rows: Vec<Vec<ResolvedToken>> = Vec::new();
    for row in &config.rows {
        // what shared the branch's row rides up a line rather than holding one open alone
        let held_branch = context.suppress_branch
            && row
                .iter()
                .any(|configured| matches!(configured.parts().0, SpaceSidebarToken::Branch));
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
                let style = kind
                    .text_value()
                    .map_or(Some(style), |value| configured.style_for_value(value))?;
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
    rows
}

/// the text a set of rows renders, joined on a byte no label holds so `ab|c` never equals `a|bc`.
pub(crate) fn rows_text(rows: &[Vec<ResolvedToken>]) -> String {
    rows.iter()
        .flatten()
        .map(|token| token.kind.text_value().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\u{1}")
}

/// a branch has its own colour and a title is a phrase, so a dot beside either only costs width.
pub(crate) fn separator_tree(previous: &ResolvedToken, current: &ResolvedToken) -> &'static str {
    // a pane name qualifies whatever it trails, and the dot is what says so
    if matches!(current.kind, ResolvedTokenKind::Pane(_)) {
        return " · ";
    }
    if matches!(
        previous.kind,
        ResolvedTokenKind::StateIcon | ResolvedTokenKind::Branch(_)
    ) || matches!(
        current.kind,
        ResolvedTokenKind::Branch(_)
            | ResolvedTokenKind::GitStatus { .. }
            | ResolvedTokenKind::TerminalTitle(_)
    ) {
        " "
    } else {
        " · "
    }
}

pub(crate) fn separator(previous: &ResolvedToken, current: &ResolvedToken) -> &'static str {
    if matches!(previous.kind, ResolvedTokenKind::StateIcon)
        || matches!(current.kind, ResolvedTokenKind::GitStatus { .. })
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

    struct Entry {
        workspace: String,
        tab: Option<String>,
        pane: Option<String>,
        agent_label: Option<String>,
        terminal_title: Option<String>,
        terminal_title_stripped: Option<String>,
        canonical_agent: Option<crate::detect::Agent>,
        tokens: std::collections::HashMap<String, String>,
    }

    fn entry() -> Entry {
        Entry {
            workspace: "repo".into(),
            tab: None,
            pane: None,
            agent_label: Some("pi".into()),
            terminal_title: None,
            terminal_title_stripped: None,
            canonical_agent: Some(crate::detect::Agent::Pi),
            tokens: std::collections::HashMap::new(),
        }
    }

    fn context(entry: &Entry) -> AgentTokenContext<'_> {
        AgentTokenContext {
            machine: None,
            workspace: &entry.workspace,
            tab: entry.tab.as_deref(),
            pane: entry.pane.as_deref(),
            agent_label: entry.agent_label.as_deref(),
            terminal_title: entry.terminal_title.as_deref(),
            terminal_title_stripped: entry.terminal_title_stripped.as_deref(),
            canonical_agent: entry.canonical_agent,
            tokens: &entry.tokens,
        }
    }

    #[test]
    fn conditional_styles_merge_first_match_and_keep_missing_values_absent() {
        let config: AgentsSidebarConfig = toml::from_str(r##"
rows = [["state_icon", { token = "machine", fg = "#fff", bold = true, dim = true, rules = [{ equals = "Local", fg = "#f00", bold = false }, { contains = "Loc", fg = "#0f0", dim = false }] }], [{ token = "$missing", rules = [{ equals = "", bold = true }] }]]
"##).unwrap();
        let entry = entry();
        for (machine, color, bold, dim) in [
            ("Local", (255, 0, 0), false, true),
            ("Localhost", (0, 255, 0), true, false),
            ("local", (255, 255, 255), true, true),
        ] {
            let mut context = context(&entry);
            context.machine = Some(machine);
            let rows = agent_rows(&config, context, "working");
            assert_eq!(rows.len(), 1);
            assert_eq!(
                rows[0][0],
                ResolvedToken::unstyled(ResolvedTokenKind::StateIcon)
            );
            let token = &rows[0][1];
            assert_eq!(token.kind, ResolvedTokenKind::Machine(machine.into()));
            assert_eq!(
                token.style.fg.unwrap().ratatui(),
                ratatui::style::Color::Rgb(color.0, color.1, color.2)
            );
            assert_eq!(token.style.bold, Some(bold));
            assert_eq!(token.style.dim, Some(dim));
        }
        assert_eq!(agent_rows(&config, context(&entry), "working")[0].len(), 1);
    }

    #[test]
    fn conditional_style_survives_truncation_and_removes_theme_modifiers() {
        use ratatui::style::{Color, Modifier, Style};
        let config: AgentsSidebarConfig = toml::from_str(r##"
rows = [[{ token = "workspace", rules = [{ equals = "long-workspace-name", fg = "#f00", bold = false, dim = false }] }]]
"##).unwrap();
        let mut entry = entry();
        entry.workspace = "long-workspace-name".into();
        let rows = agent_rows(&config, context(&entry), "working");
        let theme = Style::default()
            .fg(Color::Blue)
            .add_modifier(Modifier::BOLD | Modifier::DIM);
        for width in [4, 40] {
            let spans = super::super::resolved_token_spans(
                &rows[0],
                ("*", theme),
                theme,
                theme,
                theme,
                theme,
                &super::super::Palette::catppuccin(),
                width,
            );
            assert_eq!(spans.len(), 1);
            assert!(super::super::display_width(&spans[0].content) <= width);
            assert_eq!(spans[0].style.fg, Some(Color::Rgb(255, 0, 0)));
            assert!(!spans[0]
                .style
                .add_modifier
                .intersects(Modifier::BOLD | Modifier::DIM));
            assert!(spans[0]
                .style
                .sub_modifier
                .contains(Modifier::BOLD | Modifier::DIM));
        }
    }

    #[test]
    fn custom_numeric_rules_resolve_in_agent_overrides_and_space_rows() {
        let config: crate::config::SidebarConfig = toml::from_str(
            r#"
[agents]
rows = [["workspace"]]
[agents.rows_by_agent]
pi = [[{ token = "$load", rules = [{ gt = 80, bold = true }, { gt = 50, dim = true }] }]]
[spaces]
rows = [[{ token = "$load", rules = [{ lt = 50, dim = true }] }]]
"#,
        )
        .unwrap();
        let mut entry = entry();
        for (value, bold, dim) in [
            ("90", Some(true), None),
            ("60", None, Some(true)),
            ("20", None, None),
            ("90%", None, None),
        ] {
            entry.tokens.insert("load".into(), value.into());
            let rows = agent_rows(&config.agents, context(&entry), "working");
            assert_eq!(rows[0][0].kind, ResolvedTokenKind::Custom(value.into()));
            assert_eq!(rows[0][0].style.bold, bold);
            assert_eq!(rows[0][0].style.dim, dim);
            let spaces = space_rows(
                &config.spaces,
                SpaceTokenContext {
                    workspace: "repo",
                    branch: None,
                    state_text: "working",
                    ahead_behind: None,
                    suppress_git_details: false,
                    tokens: &entry.tokens,
                    suppress_branch: false,
                },
            );
            assert_eq!(spaces[0][0].style.dim, (value == "20").then_some(true));
        }
    }

    #[test]
    fn conditional_hide_removes_tokens_and_empty_rows() {
        let config: crate::config::SidebarConfig = toml::from_str(
            r##"
[agents]
rows = [[{ token = "machine", fg = "#61afef", rules = [{ equals = "Local", hide = true }] }, "agent"]]
[agents.rows_by_agent]
pi = [[{ token = "$load", rules = [{ lt = 50, hide = true }] }], ["agent"]]
[spaces]
rows = [[{ token = "$load", rules = [{ lt = 50, hide = true }] }], ["workspace"]]
"##,
        ).unwrap();
        let encoded = toml::to_string(&config).unwrap();
        assert!(encoded.contains("hide = true"));
        let config: crate::config::SidebarConfig = toml::from_str(&encoded).unwrap();
        let mut entry = entry();
        entry.canonical_agent = None;
        for (machine, count) in [("Local", 1), ("Remote", 2)] {
            let mut ctx = context(&entry);
            ctx.machine = Some(machine);
            let rows = agent_rows(&config.agents, ctx, "working");
            assert_eq!(rows[0].len(), count);
            assert_eq!(
                rows[0].last().unwrap().kind,
                ResolvedTokenKind::Agent("pi".into())
            );
        }
        entry.canonical_agent = Some(crate::detect::Agent::Pi);
        for (value, count) in [("20", 1), ("90", 2)] {
            entry.tokens.insert("load".into(), value.into());
            assert_eq!(
                agent_rows(&config.agents, context(&entry), "working").len(),
                count
            );
            let rows = space_rows(
                &config.spaces,
                SpaceTokenContext {
                    workspace: "repo",
                    branch: None,
                    state_text: "working",
                    ahead_behind: None,
                    suppress_git_details: false,
                    tokens: &entry.tokens,
                    suppress_branch: false,
                },
            );
            assert_eq!(rows.len(), count);
        }
    }

    #[test]
    fn conditional_hide_preserves_first_match_wins() {
        for first in ["hide = false", "bold = true"] {
            let config: AgentsSidebarConfig = toml::from_str(&format!(
                "rows = [[{{ token = 'agent', rules = [{{ equals = 'pi', {first} }}, {{ contains = '', hide = true }}] }}]]"
            )).unwrap();
            let entry = entry();
            let rows = agent_rows(&config, context(&entry), "working");
            assert_eq!(rows[0][0].kind, ResolvedTokenKind::Agent("pi".into()));
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

        let rows = agent_rows(&config, context(&entry), "working");

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
    fn machine_token_only_resolves_for_multi_machine_rows() {
        let entry = entry();
        let config = AgentsSidebarConfig {
            rows: vec![vec![
                AgentSidebarToken::Machine,
                AgentSidebarToken::Workspace,
            ]],
            ..Default::default()
        };

        assert_eq!(
            agent_rows(&config, context(&entry), "working"),
            vec![vec![ResolvedToken::unstyled(ResolvedTokenKind::Workspace(
                "repo".into()
            ))]]
        );

        let mut remote_context = context(&entry);
        remote_context.machine = Some("Build");
        assert_eq!(
            agent_rows(&config, remote_context, "working"),
            vec![vec![
                ResolvedToken::unstyled(ResolvedTokenKind::Machine("Build".into())),
                ResolvedToken::unstyled(ResolvedTokenKind::Workspace("repo".into())),
            ]]
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
            agent_rows(&config, context(&entry), "deep in the mines"),
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
            agent_rows(&config, context(&entry), "working"),
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
            agent_rows(&config, context(&pi), "working"),
            vec![vec![ResolvedToken::unstyled(ResolvedTokenKind::Agent(
                "renamed pi".into()
            ))]]
        );

        pi.canonical_agent = None;
        assert_eq!(
            agent_rows(&config, context(&pi), "working"),
            vec![vec![ResolvedToken::unstyled(ResolvedTokenKind::Workspace(
                "repo".into()
            ))]]
        );
    }

    #[test]
    fn nested_agent_rows_drop_workspace_and_merge_its_row_forward() {
        let config = AgentsSidebarConfig::default();
        let entry = entry();

        assert_eq!(
            nested_agent_rows(
                &config,
                context(&entry),
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

    // the owner's rows: branch and title ride the name's row
    fn branch_rows() -> AgentsSidebarConfig {
        let mut config = AgentsSidebarConfig::default();
        config.rows[1] = vec![
            AgentSidebarToken::Agent,
            AgentSidebarToken::Branch,
            AgentSidebarToken::TerminalTitleStripped,
        ];
        config
    }

    /// the title shares the name's row so a nested agent costs one line
    #[test]
    fn nested_agent_rows_put_the_terminal_title_on_the_agent_row() {
        let mut entry = entry();
        entry.terminal_title_stripped = Some("reviewing the sidebar".into());

        assert_eq!(
            nested_agent_rows(
                &branch_rows(),
                context(&entry),
                "working",
                NestedContext {
                    branch: None,
                    space_label: None
                }
            ),
            vec![vec![
                ResolvedToken::unstyled(ResolvedTokenKind::StateIcon),
                ResolvedToken::unstyled(ResolvedTokenKind::Agent("pi".into())),
                ResolvedToken::unstyled(ResolvedTokenKind::TerminalTitle(
                    "reviewing the sidebar".into()
                )),
            ]]
        );
    }

    #[test]
    fn nested_agent_rows_keep_the_tab_label_that_shared_the_workspace_row() {
        let mut entry = entry();
        entry.tab = Some("1".into());

        assert_eq!(
            nested_agent_rows(
                &AgentsSidebarConfig::default(),
                context(&entry),
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
        entry.tab = Some("release".into());

        assert_eq!(
            nested_agent_rows(
                &branch_rows(),
                context(&entry),
                "working",
                NestedContext {
                    branch: Some("release"),
                    space_label: None,
                }
            ),
            vec![vec![
                ResolvedToken::unstyled(ResolvedTokenKind::StateIcon),
                ResolvedToken::unstyled(ResolvedTokenKind::Agent("pi".into())),
                ResolvedToken::unstyled(ResolvedTokenKind::Branch("release".into())),
            ]]
        );
    }

    #[test]
    fn nested_agent_rows_keep_a_tab_that_differs_from_the_parent_branch() {
        let mut entry = entry();
        entry.tab = Some("review".into());

        assert_eq!(
            nested_agent_rows(
                &branch_rows(),
                context(&entry),
                "working",
                NestedContext {
                    branch: Some("release"),
                    space_label: None,
                }
            ),
            vec![vec![
                ResolvedToken::unstyled(ResolvedTokenKind::StateIcon),
                ResolvedToken::unstyled(ResolvedTokenKind::Tab("review".into())),
                ResolvedToken::unstyled(ResolvedTokenKind::Agent("pi".into())),
                ResolvedToken::unstyled(ResolvedTokenKind::Branch("release".into())),
            ]]
        );
    }

    /// the default branch is the absence of news, so the title takes those columns instead
    #[test]
    fn nested_agent_rows_drop_a_branch_nobody_needs_told_about() {
        let entry = entry();
        for branch in ["main", "master"] {
            assert_eq!(
                nested_agent_rows(
                    &AgentsSidebarConfig::default(),
                    context(&entry),
                    "working",
                    NestedContext {
                        branch: Some(branch),
                        space_label: None,
                    }
                ),
                vec![vec![
                    ResolvedToken::unstyled(ResolvedTokenKind::StateIcon),
                    ResolvedToken::unstyled(ResolvedTokenKind::Agent("pi".into())),
                ]],
                "{branch}"
            );
        }
    }

    #[test]
    fn flat_agent_rows_keep_a_tab_named_after_the_branch() {
        let mut entry = entry();
        entry.tab = Some("main".into());

        let rows = local_agent_rows(&AgentsSidebarConfig::default(), context(&entry), "working");

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
        let entry = entry();

        assert_eq!(
            nested_agent_rows(
                &config,
                context(&entry),
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
        let entry = entry();
        assert_eq!(
            local_agent_rows(&AgentsSidebarConfig::default(), context(&entry), "working"),
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

    /// only the drawn name shortens: the shared rows other machines use keep it whole
    #[test]
    fn local_and_nested_rows_call_claude_and_codex_by_their_initials() {
        let mut entry = entry();
        for (full, short) in [("claude", "cc"), ("codex", "cx"), ("pi", "pi")] {
            entry.agent_label = Some(full.into());
            let config = AgentsSidebarConfig {
                rows: vec![vec![AgentSidebarToken::Agent]],
                ..Default::default()
            };
            let nested = NestedContext {
                branch: None,
                space_label: None,
            };
            let agent = |rows: Vec<Vec<ResolvedToken>>| rows[0][0].kind.clone();
            assert_eq!(
                agent(local_agent_rows(&config, context(&entry), "working")),
                ResolvedTokenKind::Agent(short.into())
            );
            assert_eq!(
                agent(nested_agent_rows(
                    &config,
                    context(&entry),
                    "working",
                    nested
                )),
                ResolvedTokenKind::Agent(short.into())
            );
            assert_eq!(
                agent(agent_rows(&config, context(&entry), "working")),
                ResolvedTokenKind::Agent(full.into())
            );
        }
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
