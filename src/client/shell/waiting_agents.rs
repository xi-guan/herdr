//! prefix+o: one press per agent waiting on you, walked in layout order.

use super::*;

/// blocked, or finished while you were away: read from agent state, so it outlives the toast
pub(super) fn panes_waiting_on_you(snapshot: &ClientShellSnapshot) -> Vec<&str> {
    // the server lists agents workspace by workspace, tab by tab, in layout order: the walk's order
    snapshot
        .agents
        .iter()
        .filter(|agent| {
            matches!(
                agent.agent_status,
                crate::api::schema::AgentStatus::Blocked | crate::api::schema::AgentStatus::Done
            )
        })
        .map(|agent| agent.pane_id.as_str())
        .collect()
}

/// the one after the focused pane, wrapping; from the top when the focused pane is not waiting
pub(super) fn next_pane_waiting_on_you(snapshot: &ClientShellSnapshot) -> Option<String> {
    let waiting = panes_waiting_on_you(snapshot);
    let next = snapshot
        .focused_pane_id
        .as_deref()
        .and_then(|focused| waiting.iter().position(|pane_id| *pane_id == focused))
        .map_or(0, |idx| (idx + 1) % waiting.len());
    waiting.get(next).map(|pane_id| (*pane_id).to_owned())
}

impl ClientShellState {
    /// the single entry point for the waiting walk; nothing waiting leaves everything as it was
    pub(super) fn walk_to_next_waiting_agent(&mut self, outcome: &mut ClientShellInput) {
        // layout order is one machine's; the walk covers the Local machine only
        if !self.active_endpoint_id.is_local() {
            return;
        }
        let Some(pane_id) = self.snapshot.as_deref().and_then(next_pane_waiting_on_you) else {
            return;
        };
        self.reveal_agent_in_tree(&pane_id);
        self.push_endpoint_method(
            crate::api::schema::Method::PaneFocus(crate::api::schema::PaneTarget { pane_id }),
            outcome,
        );
        if self.visible_notification.take().is_some() {
            self.promote_queued_notification(std::time::Instant::now());
            outcome.repaint = true;
        }
    }
}
