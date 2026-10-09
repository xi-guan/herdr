//! claude's limit windows, fetched from its oauth usage endpoint with claude's own token, and its token tally

/// what a background usage read tells the app; one app event carries all of it
#[derive(Debug)]
pub enum UsageEvent {
    Read {
        usage: ClaudeUsage,
        /// false when it came off disk, which is no reason to forget the endpoint just refused
        live: bool,
    },
    /// the endpoint rate-limits for minutes, so a refusal is worth retrying before the next read
    Unavailable,
    /// `None` once the tally is unreadable, which blanks the figure rather than keeping a stale one
    Tokens(Option<u64>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageWindow {
    pub label: String,
    pub percent: u8,
    // unix seconds, so whoever draws it derives the countdown and the stored value holds still
    pub resets_at: Option<i64>,
}

// hand-rolled: a date crate is too much to carry for one field, and any other shape is refused, not guessed
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

// the endpoint answered `Retry-After: 179` when tripped, so asking more often only buys refusals
pub(crate) const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5 * 60);

// a reading this old still beats a blank bar while a refusal that lasts minutes runs out
const STALE_AFTER: std::time::Duration = std::time::Duration::from_secs(60 * 60);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClaudeUsage {
    pub windows: Vec<UsageWindow>,
}

// the endpoint claude code itself asks; the answer only concerns the account the token belongs to
const OAUTH_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const OAUTH_BETA: &str = "oauth-2025-04-20";
const CLAUDE_USER_AGENT: &str = "claude-code/2.1.0";

fn oauth_token() -> Option<String> {
    access_token_from(&crate::platform::claude_credentials()?)
}

fn access_token_from(blob: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(blob).ok()?;
    parsed
        .get("claudeAiOauth")?
        .get("accessToken")?
        .as_str()
        .map(str::to_string)
}

// the token goes in on stdin: an argument is visible to anything that can list processes
fn fetch_usage(token: &str) -> Option<ClaudeUsage> {
    use std::io::Write;

    let mut child = crate::noninteractive_process::curl_command()
        // the status goes to stderr so a refusal can be told apart from a timeout
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

// an entry without a usable percentage is skipped: a zero would read as "plenty left"
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
                resets_at: entry
                    .get("resets_at")
                    .and_then(serde_json::Value::as_str)
                    .and_then(epoch_seconds),
            })
        })
        .collect::<Vec<_>>();
    (!windows.is_empty()).then_some(ClaudeUsage { windows })
}

// kept between runs: a restart that asks again spends the rate limit the agents themselves share
fn cache_path() -> std::path::PathBuf {
    // state rather than config, and not per-session, because the figures belong to the account
    crate::config::state_dir().join("claude-usage.json")
}

fn now_secs() -> Option<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|since| since.as_secs())
}

fn cached_from(raw: &str, now: u64, within: std::time::Duration) -> Option<ClaudeUsage> {
    let parsed: serde_json::Value = serde_json::from_str(raw).ok()?;
    let age = now.checked_sub(parsed.get("read_at")?.as_u64()?)?;
    (age <= within.as_secs()).then(|| parse_usage_response(raw))?
}

fn cached(within: std::time::Duration) -> Option<ClaudeUsage> {
    let raw = std::fs::read_to_string(cache_path()).ok()?;
    cached_from(&raw, now_secs()?, within)
}

// the endpoint's own body with a stamp beside it, so one parser reads both the live and stored answer
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

// today and the six days behind it: the span claude's own stats screen adds up
const TOKEN_DAYS: u64 = 7;

// the figure only moves when a day ends, so reading it more often spends work on the same number
pub(crate) const TOKEN_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(15 * 60);

// whole UTC days up to but not into today: claude settles a day only once it is over
fn token_window(now: u64) -> Option<(i64, i64)> {
    let today = now - now % 86_400;
    let from = today.checked_sub((TOKEN_DAYS - 1) * 86_400)?;
    Some((from.try_into().ok()?, today.try_into().ok()?))
}

// claude's own arithmetic, matched against a settled day: input, output and both cache figures, no dedup
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
    [
        "input_tokens",
        "output_tokens",
        "cache_read_input_tokens",
        "cache_creation_input_tokens",
    ]
    .iter()
    .filter_map(|field| usage.get(field).and_then(serde_json::Value::as_u64))
    .sum()
}

// transcripts are append-only, so one last written before the window opened holds nothing in it
fn touched_within(file: &std::fs::DirEntry, from: i64) -> bool {
    file.metadata()
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|since| i64::try_from(since.as_secs()).ok())
        .is_none_or(|at| at >= from)
}

// claude reads no more than this off a transcript's end, so what lies before it never reaches its figure
const TRANSCRIPT_TAIL: u64 = 100 * 1024 * 1024;

// claude's cut too: the first line in the tail is dropped whole, even when the cut fell on its start
fn transcript_tail(path: &std::path::Path, tail: u64) -> Option<std::io::BufReader<std::fs::File>> {
    use std::io::{BufRead as _, Seek as _};

    let file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    let mut reader = std::io::BufReader::new(file);
    if size > tail {
        reader.seek(std::io::SeekFrom::Start(size - tail)).ok()?;
        reader.read_until(b'\n', &mut Vec::new()).ok()?;
    }
    Some(reader)
}

// none when the transcripts cannot be read: a zero the account never spent would say more
fn transcript_tokens(dir: &std::path::Path, window: (i64, i64)) -> Option<u64> {
    use std::io::BufRead as _;

    if window.0 >= window.1 {
        return Some(0);
    }
    let projects = std::fs::read_dir(dir).ok()?;
    let mut total = 0u64;
    for session in projects
        .flatten()
        .filter_map(|project| std::fs::read_dir(project.path()).ok())
        .flat_map(std::iter::IntoIterator::into_iter)
        .flatten()
        // a subagent's transcript sits under the session that spawned it, and claude counts it too
        .flat_map(|entry| {
            let subagents = std::fs::read_dir(entry.path().join("subagents"));
            std::iter::once(entry).chain(subagents.into_iter().flatten().flatten())
        })
    {
        let path = session.path();
        if path.extension().is_none_or(|ext| ext != "jsonl") || !touched_within(&session, window.0)
        {
            continue;
        }
        let Some(reader) = transcript_tail(&path, TRANSCRIPT_TAIL) else {
            continue;
        };
        // by line: one session can run for days and leave a file far larger than the figures wanted
        for line in reader
            .lines()
            .map_while(Result::ok)
            // parsing every user turn and tool result costs more than the tally is worth
            .filter(|line| line.contains("\"usage\""))
        {
            total = total.saturating_add(entry_tokens(&line, window));
        }
    }
    Some(total)
}

// transcripts only: claude stopped refreshing its own stats-cache.json tally
pub(crate) fn seven_day_tokens() -> Option<u64> {
    let home = crate::integration::env::home_dir().ok()?;
    transcript_tokens(&home.join(".claude/projects"), token_window(now_secs()?)?)
}

pub(crate) fn poll(events: tokio::sync::mpsc::Sender<crate::events::AppEvent>, stored: bool) {
    let send = |ev| {
        let _ = events.blocking_send(crate::events::AppEvent::ClaudeUsage(ev));
    };
    // the stored reading answers only the first read of a run; answering every poll froze the figures
    if let Some(usage) = stored.then(|| cached(POLL_INTERVAL)).flatten() {
        tracing::debug!(windows = usage.windows.len(), "reused stored claude usage");
        send(UsageEvent::Read { usage, live: false });
        return;
    }

    match oauth_token().and_then(|token| fetch_usage(&token)) {
        Some(usage) => {
            tracing::info!(windows = usage.windows.len(), "read claude usage");
            send(UsageEvent::Read { usage, live: true });
        }
        None => {
            tracing::warn!("claude usage is unavailable");
            // the refusal still shortens the next attempt, but an hour-old reading beats a blank bar
            send(UsageEvent::Unavailable);
            if let Some(usage) = cached(STALE_AFTER) {
                send(UsageEvent::Read { usage, live: false });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // shaped after the endpoint's answer: a scoped weekly names its model, plain windows carry a `kind`
    const BODY: &str = r#"{
      "limits": [
        { "kind": "session", "percent": 3.4, "resets_at": "2026-08-01T12:50:00Z" },
        { "kind": "weekly_all", "percent": 6.0, "resets_at": "2026-08-07T14:00:00Z" },
        { "kind": "weekly_scoped", "percent": 1.2,
          "scope": { "model": { "display_name": "Fable" } } }
      ]
    }"#;

    #[test]
    fn reads_every_window_the_endpoint_reports() {
        let usage = parse_usage_response(BODY).expect("three windows");

        assert_eq!(usage.windows.len(), 3);
        assert_eq!(usage.windows[0].label, "session");
        assert_eq!(usage.windows[0].percent, 3);
        // 2026-08-01T12:50:00Z, stored as the instant rather than the text
        assert_eq!(usage.windows[0].resets_at, Some(1_785_588_600));
        // a scoped entry is named after its model rather than its kind
        assert_eq!(usage.windows[2].label, "Fable");
        assert_eq!(usage.windows[2].percent, 1);
        assert_eq!(usage.windows[2].resets_at, None);
    }

    #[test]
    fn windows_without_a_figure_are_left_out() {
        let usage = parse_usage_response(
            r#"{"limits":[{"kind":"session"},{"kind":"weekly_all","percent":9}]}"#,
        )
        .expect("one usable window");

        assert_eq!(usage.windows.len(), 1);
        assert_eq!(usage.windows[0].percent, 9);
    }

    #[test]
    fn an_unfamiliar_body_yields_nothing() {
        assert!(parse_usage_response("").is_none());
        assert!(parse_usage_response("{}").is_none());
        assert!(parse_usage_response(r#"{"limits":[]}"#).is_none());
    }

    // every restart used to spend a request, and a run of them earned a 429 for the whole account
    #[test]
    fn a_fresh_reading_answers_instead_of_the_endpoint() {
        const TEN_MINUTES: std::time::Duration = std::time::Duration::from_secs(600);
        let stored = stamped(BODY, 1_000).expect("a body can be stamped");

        let fresh = cached_from(&stored, 1_300, TEN_MINUTES).expect("still fresh");
        assert_eq!(fresh.windows.len(), 3);
        assert_eq!(fresh.windows[0].percent, 3);

        // past the window, or without a stamp herdr wrote, its age is unknown rather than zero
        assert!(cached_from(&stored, 1_900, TEN_MINUTES).is_none());
        assert!(cached_from(BODY, 1_300, TEN_MINUTES).is_none());
        // a clock that moved backwards must not read as an age of zero
        assert!(cached_from(&stored, 900, TEN_MINUTES).is_none());
    }

    // a timestamp read wrong would show a reset that already passed
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

    // counting the hours since midnight would put this figure above the one claude prints
    #[test]
    fn the_window_is_the_whole_days_behind_today() {
        // 2026-08-02T12:00:00Z: from 2026-07-27 up to but not into 2026-08-02
        assert_eq!(
            token_window(1_785_672_000),
            Some((1_785_110_400, 1_785_628_800))
        );
    }

    // claude's stats count the cache too, and it dwarfs input and output, so leaving it out is orders off
    #[test]
    fn a_counted_entry_takes_every_token_kind_from_within_the_window_only() {
        let window = (1_785_628_800, 1_785_715_200);
        let entry = |stamp: &str| {
            format!(
                r#"{{"timestamp":"{stamp}","message":{{"usage":{{"input_tokens":12,
                   "output_tokens":30,"cache_read_input_tokens":900000,
                   "cache_creation_input_tokens":5000}}}}}}"#
            )
        };

        assert_eq!(
            entry_tokens(&entry("2026-08-02T09:15:00.482Z"), window),
            905_042
        );
        assert_eq!(entry_tokens(&entry("2026-08-01T23:59:59Z"), window), 0);
        assert_eq!(entry_tokens(&entry("2026-08-03T00:00:00Z"), window), 0);
        assert_eq!(
            entry_tokens(r#"{"timestamp":"2026-08-02T09:15:00Z"}"#, window),
            0
        );
        assert_eq!(entry_tokens("not json", window), 0);
    }

    // the directory holds sidecar files too, and one parsing as a reply would inflate the figure
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
        write(
            projects.join("-home-me-one/aaa/subagents/agent-x.jsonl"),
            reply("2026-08-02T10:00:00Z", 3, 4),
        );

        let window = (1_785_628_800, 1_785_715_200);
        assert_eq!(
            transcript_tokens(&projects, window),
            Some(42 + 107 + 7 + 3 * 900_000)
        );
        assert_eq!(transcript_tokens(&projects, (window.1, window.1)), Some(0));
        // a zero the account never spent would say more than a missing figure
        assert_eq!(transcript_tokens(&root.join("missing"), window), None);

        std::fs::remove_dir_all(&root).ok();
    }

    // a long session's early days fall out of claude's figure, so counting them would read above it
    #[test]
    fn only_the_tail_claude_reads_is_counted() {
        use std::io::BufRead as _;

        let path = std::env::temp_dir().join(format!("herdr-tail-{}.jsonl", std::process::id()));
        std::fs::write(&path, "aaaa\nbbbb\ncccc\n").expect("a transcript is writable");
        let lines = |tail| {
            transcript_tail(&path, tail)
                .expect("a transcript is readable")
                .lines()
                .map_while(Result::ok)
                .collect::<Vec<_>>()
        };

        // the cut lands inside `bbbb`, and the rest of that line goes with it
        assert_eq!(lines(7), ["cccc"]);
        // on a line's start it still drops that line, as claude does
        assert_eq!(lines(10), ["cccc"]);
        assert_eq!(lines(15), ["aaaa", "bbbb", "cccc"]);

        std::fs::remove_file(&path).ok();
    }

    // the credential blob is claude's, and only its access token is wanted
    #[test]
    fn only_the_access_token_is_taken_from_the_credential_blob() {
        let blob = r#"{"claudeAiOauth":{"accessToken":"tok","refreshToken":"secret"}}"#;
        assert_eq!(access_token_from(blob).as_deref(), Some("tok"));
        assert!(access_token_from("{}").is_none());
        assert!(access_token_from("not json").is_none());
    }
}
