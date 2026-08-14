//! Parsing Claude's `/usage` screen.
//!
//! The numbers are only reachable through that in-session command, so herdr reads
//! them the way a person would: run the screen, strip the drawing, pull the rows
//! out. Nothing here talks to a network or touches credentials — the agent already
//! holds those, and asking it is what keeps them where they are.

/// One limit window as the screen reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageWindow {
    pub label: String,
    pub percent: u8,
    /// The reset text verbatim. Claude words it differently per window ("12:50pm",
    /// "Aug 7 at 2pm"), and reformatting it here would only invent precision.
    pub resets: Option<String>,
}

/// Seconds from the Unix epoch to an RFC 3339 instant. Hand-rolled because the only
/// timestamps herdr reads are these, and a date crate is a large dependency to carry
/// for one field of one endpoint. Anything that does not match the shape the endpoint
/// sends is refused rather than guessed at.
fn epoch_seconds(stamp: &str) -> Option<i64> {
    fn field(text: &str, range: std::ops::Range<usize>) -> Option<i64> {
        text.get(range)?.parse().ok()
    }
    // days since 1970-01-01, by Howard Hinnant's days_from_civil
    fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
        let y = if m <= 2 { y - 1 } else { y };
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = y - era * 400;
        let mp = (m + 9) % 12;
        let doy = (153 * mp + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146097 + doe - 719468
    }

    let (date, rest) = stamp.split_once('T')?;
    let (year, month, day) = (field(date, 0..4)?, field(date, 5..7)?, field(date, 8..10)?);
    let (hour, minute, second) = (field(rest, 0..2)?, field(rest, 3..5)?, field(rest, 6..8)?);
    let civil = days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second;

    let offset = match rest.rfind(['+', '-']).map(|at| rest.split_at(at)) {
        Some((_, zone)) => {
            let sign = if zone.starts_with('-') { -1 } else { 1 };
            sign * (field(zone, 1..3)? * 3600 + field(zone, 4..6)?.max(0) * 60)
        }
        // no zone at all is refused; `Z` is the one spelling that means UTC
        None if rest.ends_with('Z') => 0,
        None => return None,
    };
    Some(civil - offset)
}

/// How long until a window resets, or `None` once it already has.
pub(crate) fn resets_in(stamp: &str, now: std::time::SystemTime) -> Option<std::time::Duration> {
    let now = now
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs()
        .try_into()
        .ok()?;
    let left: u64 = epoch_seconds(stamp)?.checked_sub(now)?.try_into().ok()?;
    Some(std::time::Duration::from_secs(left))
}

/// A window's reset, at the precision the wait deserves: minutes while you might
/// wait it out, whole hours once you would not. Hours all the way up, including the
/// weekly windows — days would be a second unit to read for no extra precision.
pub(crate) fn reset_label(left: std::time::Duration) -> String {
    let seconds = left.as_secs();
    if seconds >= 3600 {
        format!("{}h", seconds / 3600)
    } else {
        format!("{}m", seconds / 60)
    }
}

/// How often the figures are worth asking for. The endpoint rate-limits in a window
/// of a few minutes — it answered `Retry-After: 179` when tripped — so a shorter
/// interval buys refusals rather than freshness, and the session window is five hours
/// long, which a minute barely moves.
pub(crate) const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// How old a stored reading may be and still be worth showing. Far longer than the
/// poll interval on purpose: standing in for a blank bar is a different job from
/// being current, and a percentage of a five-hour window is still roughly true an
/// hour later — the countdown printed beside it is computed live either way.
const STALE_AFTER: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// Every window the screen showed, in the order it showed them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClaudeUsage {
    pub windows: Vec<UsageWindow>,
}

/// The endpoint Claude Code itself asks. Reaching it needs the credential Claude
/// already holds, and the answer concerns only the account that credential belongs
/// to — no third party learns anything it was not already being told.
const OAUTH_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const OAUTH_BETA: &str = "oauth-2025-04-20";
const CLAUDE_USER_AGENT: &str = "claude-code/2.1.0";

/// Claude's OAuth credential, wherever this platform keeps it.
#[cfg(target_os = "macos")]
fn oauth_token() -> Option<String> {
    let output = crate::noninteractive_process::command("security")
        .args([
            "find-generic-password",
            "-s",
            "Claude Code-credentials",
            "-w",
        ])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .and_then(|blob| access_token_from(&blob))
}

#[cfg(not(target_os = "macos"))]
fn oauth_token() -> Option<String> {
    let path = crate::integration::env::home_dir()
        .ok()?
        .join(".claude/.credentials.json");
    access_token_from(&std::fs::read_to_string(path).ok()?)
}

fn access_token_from(blob: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(blob).ok()?;
    parsed
        .get("claudeAiOauth")?
        .get("accessToken")?
        .as_str()
        .map(str::to_string)
}

/// Asks the endpoint and reads the windows out of its answer.
///
/// The token goes in on stdin rather than the command line: an argument is visible
/// to anything that can list processes, and this one opens an account.
fn fetch_usage(token: &str) -> Option<ClaudeUsage> {
    use std::io::Write;

    let mut child = crate::noninteractive_process::curl_command()
        // the status goes to stderr so it stays out of the body; without it a refusal
        // is indistinguishable from a timeout and every diagnosis is a guess
        .args([
            "-sfL",
            "--max-time",
            "10",
            "-w",
            "%{stderr}%{http_code}",
            "-K",
            "-",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .ok()?;
    {
        let mut stdin = child.stdin.take()?;
        let config = format!(
            "url = \"{OAUTH_USAGE_URL}\"\nheader = \"Authorization: Bearer {token}\"\n\
             header = \"anthropic-beta: {OAUTH_BETA}\"\nuser-agent = \"{CLAUDE_USER_AGENT}\"\n"
        );
        stdin.write_all(config.as_bytes()).ok()?;
    }
    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        tracing::warn!(
            http = %String::from_utf8_lossy(&output.stderr).trim(),
            curl = output.status.code().unwrap_or(-1),
            "claude usage endpoint refused"
        );
        return None;
    }
    let body = String::from_utf8_lossy(&output.stdout);
    let usage = parse_usage_response(&body)?;
    store(&body);
    Some(usage)
}

/// Pulls the windows out of the endpoint's answer. Entries without a usable
/// percentage are skipped rather than reported as zero, which would read as "plenty
/// left" for a window that is simply unknown.
fn parse_usage_response(body: &str) -> Option<ClaudeUsage> {
    let parsed: serde_json::Value = serde_json::from_str(body).ok()?;
    let entries = parsed.get("limits")?.as_array()?;
    let windows = entries
        .iter()
        .filter_map(|entry| {
            let percent = entry.get("percent")?.as_f64().filter(|p| p.is_finite())?;
            let label = entry
                .get("scope")
                .and_then(|scope| scope.get("model"))
                .and_then(|model| model.get("display_name"))
                .and_then(serde_json::Value::as_str)
                .or_else(|| entry.get("kind").and_then(serde_json::Value::as_str))
                .unwrap_or("limit");
            Some(UsageWindow {
                label: label.to_string(),
                percent: percent.round().clamp(0.0, 100.0) as u8,
                resets: entry
                    .get("resets_at")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
            })
        })
        .collect::<Vec<_>>();
    (!windows.is_empty()).then_some(ClaudeUsage { windows })
}

/// Where the last reading is kept between runs. The endpoint rate-limits, and a
/// restart that asks again immediately spends that budget on an answer herdr already
/// had — enough restarts in a row and it answers 429 to everyone, including the
/// agents' own tooling.
fn cache_path() -> std::path::PathBuf {
    // state rather than config: this is a reading herdr took, not a setting, and it
    // is not per-session because the figures belong to the account
    crate::config::state_dir().join("claude-usage.json")
}

fn now_secs() -> Option<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|since| since.as_secs())
}

/// A stored reading, if it is still fresh enough to stand in for asking again.
fn cached_from(raw: &str, now: u64, within: std::time::Duration) -> Option<ClaudeUsage> {
    let parsed: serde_json::Value = serde_json::from_str(raw).ok()?;
    let age = now.checked_sub(parsed.get("read_at")?.as_u64()?)?;
    (age <= within.as_secs()).then(|| parse_usage_response(raw))?
}

/// The last reading, if it is still fresh enough to stand in for asking again.
fn cached(within: std::time::Duration) -> Option<ClaudeUsage> {
    let raw = std::fs::read_to_string(cache_path()).ok()?;
    cached_from(&raw, now_secs()?, within)
}

/// The endpoint's own body with a timestamp beside it, so one parser serves both the
/// live answer and the stored one.
fn stamped(body: &str, read_at: u64) -> Option<String> {
    let mut parsed = serde_json::from_str::<serde_json::Value>(body).ok()?;
    parsed
        .as_object_mut()?
        .insert("read_at".into(), read_at.into());
    Some(parsed.to_string())
}

fn store(body: &str) {
    let Some(stamped) = now_secs().and_then(|read_at| stamped(body, read_at)) else {
        return;
    };
    let path = cache_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(err) = std::fs::write(&path, stamped) {
        tracing::debug!(err = %err, "could not cache claude usage");
    }
}

/// How many whole days the figure covers. Claude's stats screen counts today and the
/// six days behind it, and a figure under the same name has to mean the same span.
const TOKEN_DAYS: u64 = 7;

/// How often the tally is worth re-reading. The figure only moves when a day ends, so
/// asking more often than this spends work on the same number — and a quarter of an
/// hour is close enough to midnight that nobody catches the bar a day behind.
pub(crate) const TOKEN_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// Midnight UTC on a `YYYY-MM-DD` day, the form the tally writes its dates in.
fn day_start(date: &str) -> Option<i64> {
    epoch_seconds(&format!("{}T00:00:00Z", date.get(..10)?))
}

/// The span the figure covers, as `[from, until)` in epoch seconds.
///
/// Whole days, so it steps at midnight rather than sliding all day, and it stops at
/// today rather than reaching into it: Claude settles a day only once the day is over,
/// so counting the hours since midnight would put this figure above the one Claude
/// prints for the same week. UTC, like the dates in the tally.
fn token_window(now: u64) -> Option<(i64, i64)> {
    let today = now - now % 86_400;
    let from = today.checked_sub((TOKEN_DAYS - 1) * 86_400)?;
    Some((from.try_into().ok()?, today.try_into().ok()?))
}

/// The part of the window Claude has already written up, and the point its tally stops.
///
/// Days through `lastComputedDate` are final — Claude freezes a day once it settles it
/// and never revisits it — and they are worth more than a recount would be, because old
/// transcripts get pruned and no longer add up to what they did.
fn settled_tokens_from(raw: &str, (from, until): (i64, i64)) -> Option<(u64, i64)> {
    let parsed: serde_json::Value = serde_json::from_str(raw).ok()?;
    // a tally that does not say how far it got is taken at its word, which is what
    // herdr did before it counted anything itself: stale beats double-counted
    let settled = parsed
        .get("lastComputedDate")
        .and_then(serde_json::Value::as_str)
        .and_then(day_start)
        .map_or(until, |at| (at + 86_400).clamp(from, until));

    let mut total = 0u64;
    for day in parsed.get("dailyModelTokens")?.as_array()? {
        let within = day
            .get("date")
            .and_then(serde_json::Value::as_str)
            .and_then(day_start)
            .is_some_and(|at| (from..settled).contains(&at));
        if !within {
            continue;
        }
        let Some(models) = day.get("tokensByModel").and_then(|by| by.as_object()) else {
            continue;
        };
        for tokens in models.values() {
            total = total.saturating_add(tokens.as_u64().unwrap_or(0));
        }
    }
    Some((total, settled))
}

/// Tokens on one transcript line, ignoring lines outside the window or carrying no
/// reply.
///
/// The arithmetic Claude's own tally uses, arrived at by matching a settled day model
/// by model: input plus output, cache reads and cache writes left out, and no attempt
/// to merge the repeated entries a retried request leaves behind.
fn entry_tokens(line: &str, (from, until): (i64, i64)) -> u64 {
    let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else {
        return 0;
    };
    let within = entry
        .get("timestamp")
        .and_then(serde_json::Value::as_str)
        .and_then(epoch_seconds)
        .is_some_and(|at| (from..until).contains(&at));
    if !within {
        return 0;
    }
    let Some(usage) = entry.pointer("/message/usage") else {
        return 0;
    };
    ["input_tokens", "output_tokens"]
        .iter()
        .filter_map(|field| usage.get(field).and_then(serde_json::Value::as_u64))
        .sum()
}

/// Whether a session file could hold an entry inside the window at all. Transcripts are
/// only ever appended to, so one last written before the window opened cannot, and on a
/// long-lived account that skips nearly every file.
fn touched_within(file: &std::fs::DirEntry, from: i64) -> bool {
    file.metadata()
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|since| i64::try_from(since.as_secs()).ok())
        .is_none_or(|at| at >= from)
}

/// Tokens in the window counted out of Claude's transcripts, for the days its tally has
/// not settled yet.
fn transcript_tokens(dir: &std::path::Path, window: (i64, i64)) -> u64 {
    use std::io::BufRead as _;

    if window.0 >= window.1 {
        return 0;
    }
    let Ok(projects) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut total = 0u64;
    for session in projects
        .flatten()
        .filter_map(|project| std::fs::read_dir(project.path()).ok())
        .flat_map(std::iter::IntoIterator::into_iter)
        .flatten()
    {
        let path = session.path();
        if path.extension().is_none_or(|ext| ext != "jsonl") || !touched_within(&session, window.0)
        {
            continue;
        }
        let Ok(file) = std::fs::File::open(&path) else {
            continue;
        };
        // read by line rather than whole: one session can run for days, and the file it
        // leaves behind is far larger than the handful of figures wanted out of it
        for line in std::io::BufReader::new(file)
            .lines()
            .map_while(Result::ok)
            // parsing every user turn and tool result costs more than the tally is worth
            .filter(|line| line.contains("\"usage\""))
        {
            total = total.saturating_add(entry_tokens(&line, window));
        }
    }
    total
}

/// Tokens Claude Code recorded over the last seven days, matching its own stats screen.
///
/// Two sources, because neither is enough alone. Claude's daily tally is authoritative
/// for the days it has settled, but it only settles them when it next recomputes, which
/// can be a day later — reading it alone left the bar a day behind. The transcripts
/// cover that gap, but only the recent part of it: they get pruned, so older days no
/// longer add up to what the tally froze.
pub(crate) fn seven_day_tokens() -> Option<u64> {
    let home = crate::integration::env::home_dir().ok()?;
    let window = token_window(now_secs()?)?;
    let raw = std::fs::read_to_string(home.join(".claude/stats-cache.json")).ok()?;
    let (settled, through) = settled_tokens_from(&raw, window)?;
    let counted = transcript_tokens(&home.join(".claude/projects"), (through, window.1));
    Some(settled.saturating_add(counted))
}

/// Reads the figures on a worker thread and hands them back through the event loop.
///
/// Nothing is reported when there is no credential to read: a blank bar says less
/// than a wrong number, and the alternative — driving Claude's own `/usage` screen in
/// a throwaway terminal — costs ten seconds and a second PTY per read.
pub(crate) fn poll(events: tokio::sync::mpsc::Sender<crate::events::AppEvent>, stored: bool) {
    // the stored reading stands in only while there is nothing in hand: a restart is
    // not a reason to spend a request. Letting it answer the scheduled reads as well
    // froze the figures — every poll was served from a file nothing ever refreshed.
    if let Some(usage) = stored.then(|| cached(POLL_INTERVAL)).flatten() {
        tracing::debug!(windows = usage.windows.len(), "reused stored claude usage");
        let _ =
            events.blocking_send(crate::events::AppEvent::ClaudeUsageRead { usage, live: false });
        return;
    }

    match oauth_token().and_then(|token| fetch_usage(&token)) {
        Some(usage) => {
            tracing::info!(windows = usage.windows.len(), "read claude usage");
            let _ = events
                .blocking_send(crate::events::AppEvent::ClaudeUsageRead { usage, live: true });
        }
        None => {
            tracing::warn!("claude usage is unavailable");
            // the refusal still shortens the next attempt, but a reading from within
            // the hour beats blanking the bar over a rate limit that lasts minutes
            let _ = events.blocking_send(crate::events::AppEvent::ClaudeUsageUnavailable);
            if let Some(usage) = cached(STALE_AFTER) {
                let _ = events
                    .blocking_send(crate::events::AppEvent::ClaudeUsageRead { usage, live: false });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shaped after the endpoint's own answer: a scoped weekly entry names its model
    /// under `scope`, the plain windows only carry a `kind`.
    const BODY: &str = r#"{
      "limits": [
        { "kind": "five_hour", "percent": 3.4, "resets_at": "2026-08-01T12:50:00Z" },
        { "kind": "seven_day", "percent": 6.0, "resets_at": "2026-08-07T14:00:00Z" },
        { "kind": "weekly_scoped", "percent": 1.2,
          "scope": { "model": { "display_name": "Fable" } } }
      ]
    }"#;

    #[test]
    fn reads_every_window_the_endpoint_reports() {
        let usage = parse_usage_response(BODY).expect("three windows");

        assert_eq!(usage.windows.len(), 3);
        assert_eq!(usage.windows[0].label, "five_hour");
        assert_eq!(usage.windows[0].percent, 3);
        assert_eq!(
            usage.windows[0].resets.as_deref(),
            Some("2026-08-01T12:50:00Z")
        );
        // a scoped entry is named after its model rather than its kind
        assert_eq!(usage.windows[2].label, "Fable");
        assert_eq!(usage.windows[2].percent, 1);
    }

    /// An entry without a usable percentage is dropped rather than reported as zero,
    /// which would read as "plenty left" for a window that is simply unknown.
    #[test]
    fn windows_without_a_figure_are_left_out() {
        let usage = parse_usage_response(
            r#"{"limits":[{"kind":"five_hour"},{"kind":"seven_day","percent":9}]}"#,
        )
        .expect("one usable window");

        assert_eq!(usage.windows.len(), 1);
        assert_eq!(usage.windows[0].percent, 9);
    }

    /// A body that never arrived, or one from a contract that moved on, has to come
    /// back empty rather than half-read.
    #[test]
    fn an_unfamiliar_body_yields_nothing() {
        assert!(parse_usage_response("").is_none());
        assert!(parse_usage_response("{}").is_none());
        assert!(parse_usage_response(r#"{"limits":[]}"#).is_none());
    }

    /// The endpoint rate-limits. Every restart used to spend a request re-asking for
    /// figures herdr had just been told, and a run of restarts earned a 429 for
    /// everything on the account — so a fresh reading stands in for asking again.
    #[test]
    fn a_fresh_reading_answers_instead_of_the_endpoint() {
        const TEN_MINUTES: std::time::Duration = std::time::Duration::from_secs(600);
        let stored = stamped(BODY, 1_000).expect("a body can be stamped");

        let fresh = cached_from(&stored, 1_300, TEN_MINUTES).expect("still fresh");
        assert_eq!(fresh.windows.len(), 3);
        assert_eq!(fresh.windows[0].percent, 3);

        // past the window it is worth asking again, and a body without a stamp is
        // one herdr did not write, so its age is unknown rather than zero
        assert!(cached_from(&stored, 1_900, TEN_MINUTES).is_none());
        assert!(cached_from(BODY, 1_300, TEN_MINUTES).is_none());
        // a clock that moved backwards must not read as an age of zero
        assert!(cached_from(&stored, 900, TEN_MINUTES).is_none());
    }

    /// The endpoint's own spelling, and the two others RFC 3339 allows for the same
    /// instant. A timestamp read wrong would show a reset that already passed.
    #[test]
    fn a_reset_is_read_at_the_instant_the_endpoint_means() {
        let noon = 1_785_585_600; // 2026-08-01T12:00:00Z

        assert_eq!(epoch_seconds("2026-08-01T12:00:00Z"), Some(noon));
        assert_eq!(
            epoch_seconds("2026-08-01T12:00:00.603254+00:00"),
            Some(noon)
        );
        // an offset moves the instant, it does not decorate it
        assert_eq!(epoch_seconds("2026-08-01T14:00:00+02:00"), Some(noon));
        assert_eq!(epoch_seconds("2026-08-01T09:30:00-02:30"), Some(noon));
        // a leap day, where a wrong civil-days formula shows up
        assert_eq!(
            epoch_seconds("2024-02-29T00:00:00Z"),
            Some(1_709_164_800),
            "leap day"
        );
        assert!(epoch_seconds("").is_none());
        assert!(epoch_seconds("2026-08-01 12:00:00").is_none());
    }

    /// A window that has already reset has nothing left to wait for, and reporting a
    /// wrapped-around number would read as a full window ahead of you.
    #[test]
    fn a_window_that_already_reset_reports_nothing() {
        let now = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_785_585_600);

        assert!(resets_in("2026-08-01T11:59:00Z", now).is_none());
        assert_eq!(
            resets_in("2026-08-01T12:22:00Z", now).map(|left| left.as_secs()),
            Some(22 * 60)
        );
    }

    /// Minutes while you might wait it out, hours once you would not, and silence for
    /// a window whose reset is days away.
    #[test]
    fn a_reset_is_worded_at_the_precision_the_wait_deserves() {
        let label = |secs| reset_label(std::time::Duration::from_secs(secs));

        assert_eq!(label(22 * 60), "22m");
        assert_eq!(label(0), "0m");
        assert_eq!(label(3 * 3600 + 40 * 60), "3h");
        // a weekly window too: hours are the one unit, however many there are
        assert_eq!(label(3 * 86_400), "72h");
    }

    /// Shaped after Claude's own `stats-cache.json`: a row per day, a figure per model
    /// within it, days going back further than the window asks for, and a row for the
    /// day in progress that the tally has not settled.
    const TALLY: &str = r#"{
      "version": 4,
      "lastComputedDate": "2026-08-01",
      "dailyModelTokens": [
        { "date": "2026-07-25", "tokensByModel": { "claude-opus-5": 7000 } },
        { "date": "2026-07-26", "tokensByModel": { "claude-opus-5": 4000 } },
        { "date": "2026-07-27", "tokensByModel": { "claude-opus-5": 6000,
                                                   "claude-fable-5": 300 } },
        { "date": "2026-08-01", "tokensByModel": { "claude-opus-5": 4000 } },
        { "date": "2026-08-02", "tokensByModel": { "claude-opus-5": 9999 } }
      ]
    }"#;

    /// 2026-08-02T12:00:00Z, so the window runs from 2026-07-27 up to but not into
    /// 2026-08-02.
    const MIDDAY: u64 = 1_785_672_000;

    /// Every model counts, and the window covers the six whole days behind today — the
    /// same span Claude's own screen adds up, since a figure that disagreed with the
    /// one Claude prints for the same week would only be read as a bug.
    #[test]
    fn the_tally_covers_the_whole_days_behind_today() {
        let window = token_window(MIDDAY).expect("a window is in range");
        let (tokens, _) = settled_tokens_from(TALLY, window).expect("a tally is readable");

        assert_eq!(tokens, 6000 + 300 + 4000);
        // a day earlier and 2026-07-26 comes into range with it
        let earlier = token_window(1_785_585_600).expect("a window is in range");
        assert_eq!(
            settled_tokens_from(TALLY, earlier).map(|(tokens, _)| tokens),
            Some(4000 + 6000 + 300)
        );
    }

    /// The day in progress is left out. Claude settles a day only once it is over, so
    /// adding the hours since midnight would put this figure above the one Claude
    /// prints — the opposite of the mismatch it is meant to avoid.
    #[test]
    fn today_is_left_out_however_much_of_it_the_tally_already_holds() {
        let window = token_window(MIDDAY).expect("a window is in range");

        assert_eq!(
            window.1, 1_785_628_800,
            "the window stops at midnight today"
        );
        let (tokens, _) = settled_tokens_from(TALLY, window).expect("a tally is readable");
        assert!(tokens < 9999 + 6000, "the 2026-08-02 row stayed out");
    }

    /// Days the tally has not settled are its own arithmetic redone, not a second
    /// reading of the same rows: counting a day the tally already holds would double it.
    #[test]
    fn counting_takes_over_where_the_tally_stopped() {
        let window = token_window(MIDDAY).expect("a window is in range");
        let (tokens, through) = settled_tokens_from(TALLY, window).expect("a tally is readable");

        // settled through 2026-08-01, so the count picks up at midnight on 2026-08-02
        assert_eq!(tokens, 6000 + 300 + 4000);
        assert_eq!(through, 1_785_628_800);

        // and a tally settled through yesterday leaves nothing to count
        let caught_up = TALLY.replace("2026-08-01\",", "2026-08-02\",");
        assert_eq!(
            settled_tokens_from(&caught_up, window).map(|(_, through)| through),
            Some(window.1)
        );
    }

    /// Only replies inside the window count, and only the two fields the tally adds up:
    /// the cache figures dwarf them, so letting one through would show a wrong figure
    /// rather than a missing one.
    #[test]
    fn a_counted_entry_takes_input_and_output_from_within_the_window_only() {
        let window = (1_785_628_800, 1_785_715_200);
        let entry = |stamp: &str| {
            format!(
                r#"{{"timestamp":"{stamp}","message":{{"usage":{{"input_tokens":12,
                   "output_tokens":30,"cache_read_input_tokens":900000,
                   "cache_creation_input_tokens":5000}}}}}}"#
            )
        };

        assert_eq!(entry_tokens(&entry("2026-08-02T09:15:00.482Z"), window), 42);
        assert_eq!(entry_tokens(&entry("2026-08-01T23:59:59Z"), window), 0);
        assert_eq!(entry_tokens(&entry("2026-08-03T00:00:00Z"), window), 0);
        assert_eq!(
            entry_tokens(r#"{"timestamp":"2026-08-02T09:15:00Z"}"#, window),
            0
        );
        assert_eq!(entry_tokens("not json", window), 0);
    }

    /// Every session under every project counts, and nothing else does: the directory
    /// holds sidecar files too, and one of them parsing as a reply would inflate a
    /// figure whose whole point is agreeing with Claude's.
    #[test]
    fn counting_walks_every_session_and_only_the_sessions() {
        let root = std::env::temp_dir().join(format!("herdr-tally-{}", std::process::id()));
        let projects = root.join("projects");
        let reply = |stamp: &str, input: u64, output: u64| {
            format!(
                r#"{{"timestamp":"{stamp}","requestId":"req","message":{{"id":"msg",
                   "model":"claude-opus-5","usage":{{"input_tokens":{input},
                   "output_tokens":{output},"cache_read_input_tokens":900000}}}}}}"#
            )
            .replace('\n', "")
        };
        let write = |path: std::path::PathBuf, body: String| {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("a project directory is writable");
            }
            std::fs::write(path, body).expect("a session file is writable");
        };

        write(
            projects.join("-home-me-one/aaa.jsonl"),
            [
                r#"{"timestamp":"2026-08-02T09:00:00Z","type":"user"}"#.to_string(),
                reply("2026-08-02T09:15:00.482Z", 12, 30),
                reply("2026-08-01T23:59:59Z", 5000, 5000),
                reply("2026-08-03T00:00:00Z", 5000, 5000),
            ]
            .join("\n"),
        );
        write(
            projects.join("-home-me-two/bbb.jsonl"),
            reply("2026-08-02T22:10:00Z", 100, 7),
        );
        write(
            projects.join("-home-me-two/notes.md"),
            reply("2026-08-02T22:10:00Z", 9, 9),
        );

        let window = (1_785_628_800, 1_785_715_200);
        assert_eq!(transcript_tokens(&projects, window), 42 + 107);
        // and a tally that already reaches today leaves nothing to walk
        assert_eq!(transcript_tokens(&projects, (window.1, window.1)), 0);

        std::fs::remove_dir_all(&root).ok();
    }

    /// A file Claude has not written yet, or one whose shape moved on, leaves the bar
    /// without a token figure rather than showing a zero the account never spent.
    #[test]
    fn a_tally_that_cannot_be_read_reports_nothing() {
        let window = token_window(MIDDAY).expect("a window is in range");

        assert!(settled_tokens_from("", window).is_none());
        assert!(settled_tokens_from("{}", window).is_none());
        assert_eq!(
            settled_tokens_from(r#"{"dailyModelTokens":[]}"#, window).map(|(tokens, _)| tokens),
            Some(0)
        );
    }

    /// A tally that does not say how far it got is taken whole. Counting the window
    /// again beside it would add the same day twice, and a figure that is stale by a
    /// day is the smaller of the two errors.
    #[test]
    fn a_tally_without_a_settled_date_is_taken_at_its_word() {
        let window = token_window(MIDDAY).expect("a window is in range");
        let undated = TALLY.replace(r#""lastComputedDate": "2026-08-01","#, "");

        assert_eq!(
            settled_tokens_from(&undated, window),
            Some((6000 + 300 + 4000, window.1))
        );
    }

    /// The credential blob is Claude's, not herdr's, and only one field of it is
    /// wanted; anything else in there stays where it is.
    #[test]
    fn only_the_access_token_is_taken_from_the_credential_blob() {
        let blob = r#"{"claudeAiOauth":{"accessToken":"tok","refreshToken":"secret"}}"#;
        assert_eq!(access_token_from(blob).as_deref(), Some("tok"));
        assert!(access_token_from("{}").is_none());
        assert!(access_token_from("not json").is_none());
    }
}
