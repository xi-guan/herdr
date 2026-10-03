use super::*;

fn shell_with(projected: ClientShellSnapshot) -> ClientShellState {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.config.confirm_close = true;
    state.set_snapshot(Box::new(projected));
    state.set_pane_surface(surface());
    state
}

fn worktree_group() -> ClientShellSnapshot {
    let mut projected = snapshot();
    projected.workspaces[0].worktree = Some(ClientShellWorktree {
        key: "repo-key".into(),
        label: "repo".into(),
        is_linked_worktree: false,
    });
    let mut linked = projected.workspaces[0].clone();
    linked.workspace_id = "ws_2".into();
    linked.number = 2;
    linked.focused = false;
    linked.worktree = Some(ClientShellWorktree {
        key: "repo-key".into(),
        label: "repo".into(),
        is_linked_worktree: true,
    });
    projected.workspaces.push(linked);
    projected
}

fn menu_labels(state: &mut ClientShellState, workspace_id: &str) -> Vec<&'static str> {
    state.open_workspace_context_menu(workspace_id.into(), 0, 0);
    let Some(ClientShellOverlay::ContextMenu(menu)) = state.overlay.as_ref() else {
        panic!("workspace context menu");
    };
    menu.items().iter().map(|item| item.label).collect()
}

fn hidden_workspace(outcome: &ClientShellInput) -> Option<String> {
    outcome.actions.iter().find_map(|action| match action {
        ClientShellAction::Endpoint { request, .. } => match &request.method {
            crate::api::schema::Method::WorkspaceHide(target) => Some(target.workspace_id.clone()),
            _ => None,
        },
        _ => None,
    })
}

#[test]
fn prefix_shift_s_puts_the_focused_space_away_without_confirmation() {
    let mut state = shell_with(snapshot());

    let outcome = state.handle_raw_events(vec![
        RawInputEvent::Key(crate::input::TerminalKey::new(
            KeyCode::Char('b'),
            KeyModifiers::CONTROL,
        )),
        RawInputEvent::Key(crate::input::TerminalKey::new(
            KeyCode::Char('S'),
            KeyModifiers::SHIFT,
        )),
    ]);

    assert_eq!(hidden_workspace(&outcome).as_deref(), Some("ws_1"));
    assert!(state.overlay.is_none(), "no confirmation to answer");
}

#[test]
fn every_space_menu_offers_hide_beside_close() {
    let mut state = shell_with(snapshot());
    assert_eq!(
        menu_labels(&mut state, "ws_1"),
        [
            "Rename",
            "Hide",
            "Close",
            "New worktree",
            "Open worktree..."
        ]
    );

    let mut plain = snapshot();
    plain.workspaces[0].branch = None;
    let mut state = shell_with(plain);
    assert_eq!(menu_labels(&mut state, "ws_1"), ["Rename", "Hide", "Close"]);

    let mut state = shell_with(worktree_group());
    assert_eq!(
        menu_labels(&mut state, "ws_1"),
        [
            "Rename",
            "Hide group",
            "Close group",
            "New worktree",
            "Open worktree...",
            "Collapse"
        ]
    );
    assert_eq!(
        menu_labels(&mut state, "ws_2"),
        ["Rename", "Hide", "Close", "Delete worktree checkout..."]
    );
}

// right-click is where closing a space already lives, so putting one away belongs beside it
#[test]
fn right_click_hide_puts_the_space_away_without_confirmation() {
    let mut state = shell_with(worktree_group());
    state.open_workspace_context_menu("ws_1".into(), 0, 0);
    let Some(ClientShellOverlay::ContextMenu(menu)) = state.overlay.as_ref() else {
        panic!("workspace context menu");
    };
    let hide_index = menu
        .items()
        .iter()
        .position(|item| item.action == ClientContextMenuAction::Hide)
        .expect("hide sits in the space menu");

    let mut outcome = ClientShellInput::default();
    state.activate_context_menu_item(hide_index, &mut outcome);

    assert_eq!(hidden_workspace(&outcome).as_deref(), Some("ws_1"));
    assert!(state.overlay.is_none(), "no confirmation to answer");
}
