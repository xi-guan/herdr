use super::*;
use crate::client::endpoint::{
    ClientEndpointId, ClientEndpointStatus, ProfileId, SavedSshEndpoint,
};

fn agent_in(workspace: usize, status: AgentStatus) -> ClientShellAgent {
    ClientShellAgent {
        pane_id: format!("pane_{workspace}"),
        workspace_id: format!("ws_{workspace}"),
        tab_id: format!("tab_{workspace}"),
        name: None,
        display_agent: Some("pi".into()),
        agent: Some("pi".into()),
        title: None,
        terminal_title: None,
        terminal_title_stripped: None,
        agent_status: status,
        state_change_seq: 1,
        state_labels: Vec::new(),
        tokens: Vec::new(),
        focused: workspace == 1,
    }
}

// one space per status, each a single agent pane, so space order is the walk's order
fn spaces(statuses: &[AgentStatus]) -> ClientShellSnapshot {
    let mut projected = snapshot();
    let template = (
        projected.workspaces[0].clone(),
        projected.tabs[0].clone(),
        projected.panes[0].clone(),
    );
    projected.workspaces.clear();
    projected.tabs.clear();
    projected.panes.clear();
    for (index, status) in statuses.iter().enumerate() {
        let number = index + 1;
        let (mut workspace, mut tab, mut pane) = template.clone();
        workspace.workspace_id = format!("ws_{number}");
        workspace.active_tab_id = format!("tab_{number}");
        workspace.number = number;
        workspace.focused = number == 1;
        tab.tab_id = format!("tab_{number}");
        tab.workspace_id = format!("ws_{number}");
        tab.focused = number == 1;
        pane.pane_id = format!("pane_{number}");
        pane.workspace_id = format!("ws_{number}");
        pane.tab_id = format!("tab_{number}");
        pane.focused = number == 1;
        projected.workspaces.push(workspace);
        projected.tabs.push(tab);
        projected.panes.push(pane);
        projected.agents.push(agent_in(number, *status));
    }
    projected
}

fn shell_with(projected: ClientShellSnapshot) -> ClientShellState {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(projected));
    state.set_pane_surface(surface());
    state
}

fn focused_pane(outcome: &ClientShellInput) -> Option<String> {
    outcome.actions.iter().find_map(|action| match action {
        ClientShellAction::Endpoint { request, .. } => match &request.method {
            crate::api::schema::Method::PaneFocus(target) => Some(target.pane_id.clone()),
            _ => None,
        },
        _ => None,
    })
}

// the server moves focus; the next snapshot is what tells the client where it now sits
fn press(state: &mut ClientShellState) -> Option<String> {
    let mut outcome = ClientShellInput::default();
    state.record_binding(
        crate::input::KeybindMatch::Action(crate::input::KeybindAction::OpenNotificationTarget),
        &mut outcome,
    );
    let pane_id = focused_pane(&outcome)?;
    let mut moved = state.snapshot.as_deref().expect("snapshot").clone();
    moved.revision += 1;
    let workspace_id = pane_id.replace("pane", "ws");
    moved.focused_workspace_id = Some(workspace_id.clone());
    moved.focused_tab_id = Some(pane_id.replace("pane", "tab"));
    moved.focused_pane_id = Some(pane_id.clone());
    for workspace in &mut moved.workspaces {
        workspace.focused = workspace.workspace_id == workspace_id;
    }
    state.set_snapshot(Box::new(moved));
    Some(pane_id)
}

// reads agent state, not the toast: the toast is gone after a few seconds, the agent still waits
#[test]
fn custom_open_notification_key_goes_to_the_one_agent_waiting() {
    let mut config = Config::default();
    config.keys.open_notification_target = crate::config::BindingConfig::one("prefix+y");
    let mut state = shell_with(spaces(&[AgentStatus::Idle, AgentStatus::Blocked]));
    state.config.keybinds = ClientShellConfig::from_config(&config).keybinds;

    let outcome = state.handle_raw_events(vec![
        RawInputEvent::Key(crate::input::TerminalKey::new(
            KeyCode::Char('b'),
            KeyModifiers::CONTROL,
        )),
        RawInputEvent::Key(crate::input::TerminalKey::new(
            KeyCode::Char('y'),
            KeyModifiers::empty(),
        )),
    ]);

    assert_eq!(focused_pane(&outcome).as_deref(), Some("pane_2"));
    assert_eq!(state.mode, ClientShellMode::Terminal);
}

// several of them is still a jump, one per press: picking from a list was the extra step
#[test]
fn open_notification_key_walks_the_waiting_agents_one_press_at_a_time() {
    let mut state = shell_with(spaces(&[
        AgentStatus::Idle,
        AgentStatus::Blocked,
        AgentStatus::Blocked,
    ]));

    assert_eq!(
        press(&mut state).as_deref(),
        Some("pane_2"),
        "first press takes the first one"
    );
    assert_eq!(
        press(&mut state).as_deref(),
        Some("pane_3"),
        "pressing again advances"
    );
    assert_eq!(
        press(&mut state).as_deref(),
        Some("pane_2"),
        "and wraps round"
    );
}

// blocked panes stay blocked after you look at them, so the walk steps past the one you sit in
#[test]
fn open_notification_key_stays_put_when_only_one_waits() {
    let mut state = shell_with(spaces(&[AgentStatus::Idle, AgentStatus::Blocked]));

    for _ in 0..2 {
        assert_eq!(press(&mut state).as_deref(), Some("pane_2"));
    }

    let snapshot = state.snapshot.as_deref().expect("snapshot");
    assert_eq!(snapshot.focused_workspace_id.as_deref(), Some("ws_2"));
    assert_eq!(snapshot.focused_pane_id.as_deref(), Some("pane_2"));
}

// finished-while-you-were-away counts as waiting; finished-and-read does not
#[test]
fn waiting_covers_blocked_and_unread_completions_only() {
    let projected = spaces(&[AgentStatus::Working, AgentStatus::Done, AgentStatus::Idle]);

    let waiting = crate::client::shell::waiting_agents::panes_waiting_on_you(&projected);

    assert_eq!(waiting.len(), 1, "waiting: {waiting:?}");
    assert_eq!(waiting[0], "pane_2");
    assert!(!waiting.contains(&"pane_1"));
    assert!(!waiting.contains(&"pane_3"));
}

#[test]
fn open_notification_key_does_nothing_when_nobody_waits() {
    let mut state = shell_with(spaces(&[AgentStatus::Idle, AgentStatus::Working]));
    state.config.toast_delivery = crate::config::ToastDelivery::Herdr;
    state.config.toast_delay_seconds = 0;
    state.receive_notification(
        &ClientEndpointId::Local,
        SemanticNotification {
            kind: SemanticNotificationKind::Custom,
            title: "deploy finished".into(),
            body: None,
            sound: None,
            agent: None,
            workspace_id: None,
            tab_id: None,
            pane_id: None,
            position: None,
        },
        std::time::Instant::now(),
    );
    assert!(state.visible_notification.is_some());

    let mut outcome = ClientShellInput::default();
    state.record_binding(
        crate::input::KeybindMatch::Action(crate::input::KeybindAction::OpenNotificationTarget),
        &mut outcome,
    );

    assert!(outcome.actions.is_empty(), "the toast jump is gone too");
    assert!(state.visible_notification.is_some(), "the toast stays up");
}

#[test]
fn the_walk_answers_the_toast_it_lands_on() {
    let mut state = shell_with(spaces(&[AgentStatus::Idle, AgentStatus::Blocked]));
    state.config.toast_delivery = crate::config::ToastDelivery::Herdr;
    state.config.toast_delay_seconds = 0;
    state.receive_notification(
        &ClientEndpointId::Local,
        SemanticNotification {
            kind: SemanticNotificationKind::NeedsAttention,
            title: "pi needs attention".into(),
            body: None,
            sound: None,
            agent: Some("pi".into()),
            workspace_id: Some("ws_2".into()),
            tab_id: Some("tab_2".into()),
            pane_id: Some("pane_2".into()),
            position: None,
        },
        std::time::Instant::now(),
    );
    assert!(state.visible_notification.is_some());

    assert_eq!(press(&mut state).as_deref(), Some("pane_2"));
    assert!(state.visible_notification.is_none());
}

#[test]
fn open_notification_key_walks_the_local_machine_only() {
    let mut state = shell_with(spaces(&[AgentStatus::Idle, AgentStatus::Blocked]));
    let profile = SavedSshEndpoint {
        id: ProfileId::parse("0123456789abcdef0123456789abcdef").unwrap(),
        label: "Build".into(),
        target: "dev@build.example".into(),
        session: "agents".into(),
        enabled: true,
    };
    let remote = ClientEndpointId::Ssh(profile.id.clone());
    state.set_endpoint_catalog(&[profile]);
    state.set_endpoint_status(&remote, ClientEndpointStatus::Online);
    let mut remote_snapshot = spaces(&[AgentStatus::Idle, AgentStatus::Blocked]);
    remote_snapshot.boot_id = "remote-boot".into();
    state.set_endpoint_snapshot(&remote, Box::new(remote_snapshot));
    assert!(state.activate_endpoint_projection(&remote));

    let mut outcome = ClientShellInput::default();
    state.record_binding(
        crate::input::KeybindMatch::Action(crate::input::KeybindAction::OpenNotificationTarget),
        &mut outcome,
    );

    assert!(outcome.actions.is_empty());
}
