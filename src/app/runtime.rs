use std::time::Instant;

#[cfg(test)]
use std::time::Duration;

use super::{
    auto_updates_enabled, background_update_check_enabled, App, AUTO_UPDATE_CHECK_INTERVAL,
    MIN_RENDER_INTERVAL,
};
use crate::events::AppEvent;
use crate::usage::UsageEvent;

// the refusal window is minutes long: retry well before the next read, doubling up to that interval
fn claude_usage_retry_delay(refusals: u32) -> std::time::Duration {
    const FIRST: u64 = 60;
    let cap = crate::usage::POLL_INTERVAL.as_secs();
    // clamped far below an overflowing shift: the ladder reaches the cap after two doublings
    let doublings = refusals.saturating_sub(1).min(8);
    std::time::Duration::from_secs((FIRST << doublings).min(cap))
}

fn retain_detached_process_after_wait(
    pid: u32,
    result: std::io::Result<Option<std::process::ExitStatus>>,
) -> bool {
    match result {
        Ok(None) => true,
        Ok(Some(_)) => false,
        Err(err) if err.kind() == std::io::ErrorKind::Interrupted => true,
        Err(err) => {
            tracing::warn!(pid, err = %err, "failed to reap detached process");
            false
        }
    }
}

impl App {
    pub(crate) fn reap_finished_detached_processes(&mut self) {
        self.detached_process_children
            .retain_mut(|child| retain_detached_process_after_wait(child.id(), child.try_wait()));
    }

    pub(crate) fn shutdown_terminal_runtime(&mut self, terminal_id: crate::terminal::TerminalId) {
        if let Some(runtime) = self.terminal_runtimes.remove(&terminal_id) {
            runtime.shutdown();
        }
    }

    pub(crate) fn shutdown_detached_terminal_runtimes(&mut self) {
        let terminal_ids = std::mem::take(&mut self.state.terminal_runtime_shutdowns);
        for terminal_id in terminal_ids {
            self.shutdown_terminal_runtime(terminal_id);
        }
    }

    pub(crate) fn sync_agent_metadata_deadline(&mut self) {
        self.agent_metadata_deadline = self.state.next_agent_metadata_expiry();
    }

    pub(crate) fn expire_due_metadata(&mut self, now: Instant) -> bool {
        let Some(deadline) = self
            .agent_metadata_deadline
            .filter(|deadline| now >= *deadline)
        else {
            return false;
        };
        self.expire_metadata_at(deadline, now);
        true
    }

    pub(crate) fn expire_metadata_at(&mut self, deadline: Instant, now: Instant) {
        let previous_toast = self.state.toast.clone();
        for update in self.state.expire_agent_metadata_at(deadline, now) {
            self.refresh_new_herdr_toast_context_for_update(&update, &previous_toast);
            self.emit_pane_state_update(&update);
        }
        let (panes, workspaces) = self.state.expire_metadata_tokens(now);
        for (ws_idx, pane_id) in panes {
            self.emit_pane_updated(ws_idx, pane_id);
        }
        for ws_idx in workspaces {
            self.emit_workspace_token_updated(ws_idx);
        }
        self.sync_agent_metadata_deadline();
    }

    pub(crate) fn can_render_now(&self, now: Instant) -> bool {
        match self.last_render_at {
            Some(last_render_at) => now.duration_since(last_render_at) >= MIN_RENDER_INTERVAL,
            None => true,
        }
    }

    pub(crate) fn can_present_now(&self, now: Instant) -> bool {
        match self.last_presentation_at {
            Some(last_presentation_at) => {
                now.duration_since(last_presentation_at) >= MIN_RENDER_INTERVAL
            }
            None => true,
        }
    }

    pub(crate) fn record_render_attempt(&mut self, now: Instant, presentation: bool) {
        self.last_render_at = Some(now);
        if presentation {
            self.last_presentation_at = Some(now);
        }
    }

    pub(crate) fn run_auto_update_check(&mut self) {
        if !background_update_check_enabled(
            self.policy.background_updates,
            self.update_version_check_enabled,
        ) {
            self.next_auto_update_check = None;
            return;
        }

        self.next_auto_update_check = self
            .state
            .update_available
            .is_none()
            .then_some(Instant::now() + AUTO_UPDATE_CHECK_INTERVAL);

        if self.state.update_available.is_some() {
            return;
        }

        let update_tx = self.event_tx.clone();
        std::thread::spawn(move || crate::update::auto_update(update_tx));
    }

    pub(crate) fn run_agent_manifest_update_check(&mut self) {
        if !background_update_check_enabled(
            self.policy.background_updates,
            self.update_manifest_check_enabled,
        ) {
            self.next_agent_manifest_update_check = None;
            return;
        }

        self.next_agent_manifest_update_check = Some(Instant::now() + AUTO_UPDATE_CHECK_INTERVAL);

        let manifest_update_tx = self.event_tx.clone();
        std::thread::spawn(move || crate::detect::manifest_update::auto_update(manifest_update_tx));
    }

    pub(crate) fn run_claude_usage_polls_if_due(&mut self, now: Instant) {
        // off in debug builds like the update checks, so tests never touch claude's credentials
        if !auto_updates_enabled(self.policy.background_updates) {
            return;
        }
        if self
            .next_claude_usage_poll
            .is_none_or(|deadline| now >= deadline)
        {
            self.next_claude_usage_poll = Some(now + crate::usage::POLL_INTERVAL);
            // only the first read of a run may come off disk; after that the point is whether it moved
            let stored = self.state.claude_usage.is_none();
            let usage_tx = self.event_tx.clone();
            std::thread::spawn(move || crate::usage::poll(usage_tx, stored));
        }
        if self
            .next_claude_tokens_poll
            .is_none_or(|deadline| now >= deadline)
        {
            self.next_claude_tokens_poll = Some(now + crate::usage::TOKEN_POLL_INTERVAL);
            let tokens_tx = self.event_tx.clone();
            std::thread::spawn(move || {
                let tokens = crate::usage::seven_day_tokens();
                tracing::debug!(?tokens, "read claude token tally");
                let _ = tokens_tx.blocking_send(AppEvent::ClaudeUsage(UsageEvent::Tokens(tokens)));
            });
        }
    }

    pub(crate) fn handle_claude_usage_event(&mut self, ev: UsageEvent) -> bool {
        let changed = match ev {
            UsageEvent::Read { usage, live } => {
                if live {
                    self.claude_usage_refusals = 0;
                }
                let changed = self.state.claude_usage.as_ref() != Some(&usage);
                self.state.claude_usage = Some(usage);
                changed
            }
            UsageEvent::Unavailable => {
                self.claude_usage_refusals = self.claude_usage_refusals.saturating_add(1);
                self.next_claude_usage_poll =
                    Some(Instant::now() + claude_usage_retry_delay(self.claude_usage_refusals));
                false
            }
            UsageEvent::Tokens(tokens) => {
                let changed = self.state.claude_seven_day_tokens != tokens;
                self.state.claude_seven_day_tokens = tokens;
                changed
            }
        };
        if changed {
            self.render_dirty.request_generic();
            self.render_notify.notify_one();
        }
        changed
    }

    pub(crate) fn next_headless_loop_deadline_with_git_refresh(
        &self,
        now: Instant,
        needs_render: bool,
        include_git_refresh: bool,
    ) -> Option<Instant> {
        let render_deadline = if needs_render {
            self.last_render_at
                .map(|last_render_at| last_render_at + MIN_RENDER_INTERVAL)
                .filter(|deadline| *deadline > now)
        } else {
            None
        };

        [
            self.config_diagnostic_deadline,
            self.toast_deadline,
            self.state.next_pending_agent_notification_deadline(),
            self.state.next_managed_agent_deadline(),
            include_git_refresh
                .then(|| self.git_refresh_deadline())
                .flatten(),
            // like git refresh, the claude polls only run while a client is attached to see them
            include_git_refresh
                .then_some(self.next_claude_usage_poll)
                .flatten(),
            include_git_refresh
                .then_some(self.next_claude_tokens_poll)
                .flatten(),
            self.next_auto_update_check,
            self.next_agent_manifest_update_check,
            self.agent_metadata_deadline,
            self.pending_agent_resume_deadline,
            self.session_save_deadline,
            self.next_tab_bar_status_deadline(),
            render_deadline,
        ]
        .into_iter()
        .flatten()
        .min()
    }

    #[cfg(test)]
    pub(crate) fn drain_internal_events(&mut self) -> bool {
        self.drain_internal_events_up_to(super::APP_EVENT_DRAIN_LIMIT)
            .1
    }

    #[cfg(test)]
    pub(crate) fn drain_all_internal_events(&mut self) -> bool {
        let mut changed = false;
        loop {
            let (had_event, batch_changed) =
                self.drain_internal_events_up_to(super::APP_EVENT_DRAIN_LIMIT);
            changed |= batch_changed;
            if !had_event {
                break;
            }
        }
        changed
    }

    #[cfg(test)]
    fn drain_internal_events_up_to(&mut self, limit: usize) -> (bool, bool) {
        let mut had_event = false;
        let mut changed = false;
        for _ in 0..limit {
            let Ok(ev) = self.event_rx.try_recv() else {
                break;
            };
            had_event = true;
            changed |= self.handle_internal_event_with_render_impact(ev);
        }
        (had_event, changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Workspace;

    #[test]
    fn hidden_render_attempt_keeps_presentation_cadence_available() {
        let (mut app, _) = test_app_with_pane();
        let initial_presentation = Instant::now();
        app.record_render_attempt(initial_presentation, true);

        let hidden_attempt = initial_presentation + MIN_RENDER_INTERVAL;
        app.record_render_attempt(hidden_attempt, false);
        let foreground_echo = hidden_attempt + Duration::from_millis(1);

        assert!(!app.can_render_now(foreground_echo));
        assert!(app.can_present_now(foreground_echo));
    }

    // retrying at the first delay forever would keep the refusal window tripped
    #[test]
    fn a_refused_read_is_retried_sooner_than_the_next_scheduled_one() {
        let delay = |refusals| claude_usage_retry_delay(refusals).as_secs();

        assert_eq!(delay(1), 60);
        assert_eq!(delay(2), 120);
        assert_eq!(delay(4), 300);
        // never longer than the next scheduled read, and never overflowing
        assert_eq!(delay(9), 300);
        assert_eq!(delay(u32::MAX), 300);
    }

    #[test]
    fn the_test_policy_never_reaches_for_claude_usage() {
        let mut app = test_app();

        app.run_claude_usage_polls_if_due(Instant::now());

        assert_eq!(app.next_claude_usage_poll, None);
        assert_eq!(app.next_claude_tokens_poll, None);
    }

    // a deadline nobody acts on while detached would wake the loop in a spin
    #[test]
    fn claude_poll_deadlines_only_wake_the_loop_while_a_client_is_attached() {
        let mut app = test_app();
        let now = Instant::now();
        app.next_claude_usage_poll = Some(now + Duration::from_secs(5));
        app.next_claude_tokens_poll = Some(now + Duration::from_secs(10));

        assert_eq!(
            app.next_headless_loop_deadline_with_git_refresh(now, false, false),
            None
        );
        assert_eq!(
            app.next_headless_loop_deadline_with_git_refresh(now, false, true),
            Some(now + Duration::from_secs(5))
        );
    }

    #[test]
    fn claude_usage_events_land_in_state_and_only_a_change_redraws() {
        let mut app = test_app();
        let usage = crate::usage::ClaudeUsage {
            windows: vec![crate::usage::UsageWindow {
                label: "five_hour".into(),
                percent: 3,
                resets_at: Some(4_000_000_000),
            }],
        };
        let read = |live| {
            AppEvent::ClaudeUsage(UsageEvent::Read {
                usage: usage.clone(),
                live,
            })
        };
        app.claude_usage_refusals = 2;

        assert!(app.handle_internal_event_with_render_impact(read(false)));
        assert_eq!(app.state.claude_usage.as_ref(), Some(&usage));
        // a stored reading is no reason to forget the endpoint just refused
        assert_eq!(app.claude_usage_refusals, 2);
        assert!(!app.handle_internal_event_with_render_impact(read(true)));
        assert_eq!(app.claude_usage_refusals, 0);

        let tokens = |tokens| AppEvent::ClaudeUsage(UsageEvent::Tokens(tokens));
        assert!(app.handle_internal_event_with_render_impact(tokens(Some(31_000_000))));
        assert_eq!(app.state.claude_seven_day_tokens, Some(31_000_000));
        assert!(!app.handle_internal_event_with_render_impact(tokens(Some(31_000_000))));
        assert!(app.handle_internal_event_with_render_impact(tokens(None)));
        assert_eq!(app.state.claude_seven_day_tokens, None);
    }

    #[test]
    fn a_refusal_brings_the_next_usage_read_forward() {
        let mut app = test_app();
        let before = Instant::now();
        app.next_claude_usage_poll = Some(before + crate::usage::POLL_INTERVAL);

        app.handle_internal_event(AppEvent::ClaudeUsage(UsageEvent::Unavailable));

        assert_eq!(app.claude_usage_refusals, 1);
        let next = app.next_claude_usage_poll.expect("a retry is scheduled");
        assert!(next >= before + Duration::from_secs(60));
        assert!(next < before + crate::usage::POLL_INTERVAL);
    }

    #[test]
    fn interrupted_detached_process_wait_keeps_child_for_retry() {
        let interrupted = std::io::Error::new(std::io::ErrorKind::Interrupted, "test interrupt");

        assert!(retain_detached_process_after_wait(42, Err(interrupted)));
    }

    fn test_app() -> super::super::App {
        super::super::App::new(
            &crate::config::Config::default(),
            crate::app::AppPolicy::TEST,
            None,
            tokio::sync::mpsc::unbounded_channel().1,
            crate::api::EventHub::default(),
        )
    }

    fn test_app_with_pane() -> (super::super::App, crate::layout::PaneId) {
        let mut app = super::super::App::new(
            &crate::config::Config::default(),
            crate::app::AppPolicy::TEST,
            None,
            tokio::sync::mpsc::unbounded_channel().1,
            crate::api::EventHub::default(),
        );
        let ws = Workspace::test_new("test");
        let pane_id = ws.tabs[0].root_pane;
        app.state.workspaces.push(ws);
        app.state.active = Some(0);
        app.state.view.pane_infos.push(crate::layout::PaneInfo {
            id: pane_id,
            rect: ratatui::layout::Rect::new(0, 0, 80, 24),
            inner_rect: ratatui::layout::Rect::new(0, 0, 80, 24),
            scrollbar_rect: None,
            borders: ratatui::widgets::Borders::NONE,
            is_focused: true,
        });
        (app, pane_id)
    }
}
