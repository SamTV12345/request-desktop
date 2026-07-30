//! Booking the local worklog into Jira.
//!
//! # What this deliberately does not do
//!
//! Nothing here reads a ticket's summary, description or comments, and no ticket
//! content is ever handed to an agent. The MCP server — the only surface an agent
//! talks to — has no Jira tool at all, by omission and on purpose: an agent can
//! polish the *local* task names and notes the user wrote, and that is the whole
//! of its involvement. The issue key is typed by the user in the UI, the API calls
//! happen in this process, and the results are shown to the user. So work can be
//! booked without a ticket's contents leaving the company's Jira.
//!
//! # Why booking twice is the hard part
//!
//! `POST /issue/{key}/worklog` takes no client-supplied idempotency key. Jira will
//! happily accept the same hour twice, and there is no request this module could
//! send that would make it refuse. Replaying a day is therefore not made safe by
//! the API; it is made safe here, by three things in order:
//!
//! 1. **A local record per entry** ([`rp_store::Pushed`]) holding the worklog id
//!    Jira handed back, plus the times it was told. A replay finds the record and
//!    *updates* that worklog instead of adding another.
//! 2. **An overlap check against Jira itself**, for the entries with no record —
//!    work booked from Jira's own UI, from another machine, or by a run that died
//!    between the POST and writing the record. A worklog of *mine* on that issue
//!    covering the same time is treated as this entry already booked, and the id is
//!    adopted into the local record rather than a second worklog being created.
//! 3. **Deciding before writing.** [`plan`] is a pure function over the local
//!    entries, the local records and the remote worklogs; it returns what would be
//!    done. The UI shows that list, the user presses the button, and only then does
//!    anything get sent. A dry run and a real run compute the same plan.
//!
//! The residual risk is one request wide: if Jira creates a worklog and the process
//! dies before [`rp_store::Store::track_record_push`] commits, the record is
//! missing — and rule 2 is what catches it on the next run.
//!
//! # Cloud and Data Center
//!
//! Both are supported because both exist in the wild and the differences are small
//! but fatal: REST v3 vs v2, `Authorization: Basic email:token` vs
//! `Bearer <PAT>`, an Atlassian Document Format comment vs a plain string, and an
//! author identified by `accountId` vs by `name`. [`Flavor`] is the one switch;
//! every difference is expressed as a `match` on it, so a third flavour would be a
//! compile error in the places that need to change rather than a runtime surprise.
use crate::track::{Ymd, civil_from_days, days_from_civil};
use anyhow::{Context as _, Result, bail};
use rp_store::{Entry, Pushed, Store};
use std::collections::HashMap;

/// Where the credentials live. The base URL and the account are configuration; the
/// token is a secret and goes to the OS keychain, never to the database.
const KV_BASE: &str = "jira.base";
const KV_FLAVOR: &str = "jira.flavor";
const KV_ACCOUNT: &str = "jira.account";
/// The keychain service name. The account is the key within it, so switching Jira
/// accounts does not overwrite the previous token.
const KEYRING_SERVICE: &str = "request-pilot:jira";

/// Jira's minimum worklog granularity. Below this the API rejects the request, so
/// a shorter entry is reported as skipped rather than silently rounded up to a
/// minute the user did not work.
const MIN_SECONDS: i64 = 60;

/// Worklogs fetched per request. The issue worklog endpoint pages, and an issue
/// a team has worked on for a year has more than one page.
const PAGE: usize = 500;

/// Which Jira is on the other end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Flavor {
    /// `*.atlassian.net`: REST v3, Basic auth with an API token, ADF comments.
    #[default]
    Cloud,
    /// Jira Data Center or Server: REST v2, a Personal Access Token as a bearer,
    /// plain-text comments.
    Server,
}

impl Flavor {
    /// The API version in the path. The endpoints used here exist in both, with
    /// the same shapes apart from the comment field.
    fn api(self) -> &'static str {
        match self {
            Flavor::Cloud => "3",
            Flavor::Server => "2",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Flavor::Cloud => "cloud",
            Flavor::Server => "server",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "server" => Flavor::Server,
            _ => Flavor::Cloud,
        }
    }
}

/// Everything needed to talk to one Jira, minus the token.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    /// Site root without a trailing slash, e.g. `https://acme.atlassian.net`.
    pub base: String,
    pub flavor: Flavor,
    /// The Cloud account's e-mail, or the Data Center user name. Used for Basic
    /// auth on Cloud, and as the keychain entry's account on both.
    pub account: String,
}

impl Config {
    pub fn load(store: &Store) -> Result<Self> {
        Ok(Self {
            base: store.get(KV_BASE)?.unwrap_or_default(),
            flavor: store
                .get(KV_FLAVOR)?
                .map(|f| Flavor::parse(&f))
                .unwrap_or_default(),
            account: store.get(KV_ACCOUNT)?.unwrap_or_default(),
        })
    }

    pub fn save(&self, store: &Store) -> Result<()> {
        store.set(KV_BASE, self.base.trim().trim_end_matches('/'))?;
        store.set(KV_FLAVOR, self.flavor.label())?;
        store.set(KV_ACCOUNT, self.account.trim())?;
        Ok(())
    }

    /// Whether there is enough here to try a call. A Data Center PAT identifies its
    /// own user, so an account name is only required on Cloud, where it is half of
    /// the Basic credential.
    pub fn is_complete(&self) -> bool {
        !self.base.trim().is_empty()
            && (self.flavor == Flavor::Server || !self.account.trim().is_empty())
    }

    fn url(&self, path: &str) -> String {
        format!(
            "{}/rest/api/{}/{path}",
            self.base.trim().trim_end_matches('/'),
            self.flavor.api()
        )
    }

    fn entry(&self) -> Result<keyring::Entry> {
        let account = self.account.trim();
        let account = if account.is_empty() {
            "default"
        } else {
            account
        };
        keyring::Entry::new(KEYRING_SERVICE, account).context("cannot reach the OS keychain")
    }

    /// The stored token, or `None` when none has been saved for this account.
    pub fn token(&self) -> Result<Option<String>> {
        match self.entry()?.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e).context("cannot read the token from the OS keychain"),
        }
    }

    /// Store the token, or delete it when `token` is blank.
    pub fn set_token(&self, token: &str) -> Result<()> {
        let entry = self.entry()?;
        if token.trim().is_empty() {
            return match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(e) => Err(e).context("cannot remove the token from the OS keychain"),
            };
        }
        entry
            .set_password(token.trim())
            .context("cannot store the token in the OS keychain")
    }

    /// The `Authorization` header value.
    fn auth(&self, token: &str) -> String {
        match self.flavor {
            // Cloud has no bearer for API tokens: the token is the password half
            // of a Basic credential.
            Flavor::Cloud => format!(
                "Basic {}",
                base64(format!("{}:{token}", self.account.trim()))
            ),
            Flavor::Server => format!("Bearer {token}"),
        }
    }
}

/// Base64, standard alphabet with padding. Hand-rolled for the same reason the
/// version compare in `update.rs` is: `base64` is in the lock file as somebody
/// else's transitive dependency, and this needs one direction of one alphabet.
fn base64(input: String) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Timestamps. Jira speaks local wall clock with an explicit offset; the tracker
// speaks Unix seconds. Both directions are needed and both are pure.
// ---------------------------------------------------------------------------

/// Jira's `started` format: `2026-07-29T09:00:00.000+0200`.
///
/// The offset is not decoration. Jira interprets the wall clock *in* the offset it
/// is given, so a worklog sent with the wrong one lands on the wrong hour and, near
/// midnight, the wrong day. `offset` comes from
/// [`rp_store::Store::local_offset`] — the operating system's offset at that
/// instant, not a constant, because half the year it is a different number.
pub fn format_started(unix: i64, offset: i64) -> String {
    let local = unix + offset;
    let Ymd { y, m, d } = civil_from_days(local.div_euclid(86_400));
    let secs = local.rem_euclid(86_400);
    let sign = if offset < 0 { '-' } else { '+' };
    let off = offset.abs();
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.000{sign}{:02}{:02}",
        secs / 3_600,
        secs % 3_600 / 60,
        secs % 60,
        off / 3_600,
        off % 3_600 / 60,
    )
}

/// Read Jira's `started` back into Unix seconds.
///
/// Needed for the overlap check: a worklog booked in Jira's own UI is only
/// comparable with a local entry once both are the same kind of number. Written by
/// hand over the fixed prefix Jira always emits, tolerating a `Z`, a missing
/// offset, a `+HH:MM` and any number of fractional digits — which covers what Cloud
/// and Data Center have been seen to return.
pub fn parse_started(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' {
        return None;
    }
    let num = |from: usize, to: usize| s.get(from..to)?.parse::<i64>().ok();
    let (y, m, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (h, mi, sec) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || h > 23 || mi > 59 || sec > 60 {
        return None;
    }
    // Skip a fractional part, then read whatever offset follows.
    let mut rest = &s[19..];
    if let Some(frac) = rest.strip_prefix('.') {
        let digits = frac.bytes().take_while(u8::is_ascii_digit).count();
        rest = &frac[digits..];
    }
    let offset = match rest.as_bytes().first() {
        // No offset at all: read as UTC. Not a guess this module can improve on.
        None | Some(b'Z' | b'z') => 0,
        Some(sign @ (b'+' | b'-')) => {
            let sign = if *sign == b'-' { -1 } else { 1 };
            let digits: String = rest[1..].chars().filter(|c| c.is_ascii_digit()).collect();
            if digits.len() != 4 {
                return None;
            }
            let hh: i64 = digits[..2].parse().ok()?;
            let mm: i64 = digits[2..].parse().ok()?;
            if hh > 18 || mm > 59 {
                return None;
            }
            sign * (hh * 3_600 + mm * 60)
        }
        _ => return None,
    };
    let day = days_from_civil(Ymd {
        y: i32::try_from(y).ok()?,
        m: u32::try_from(m).ok()?,
        d: u32::try_from(d).ok()?,
    });
    Some(day * 86_400 + h * 3_600 + mi * 60 + sec - offset)
}

// ---------------------------------------------------------------------------
// The plan. Pure, so every dedup rule is a test rather than an experiment on a
// real Jira.
// ---------------------------------------------------------------------------

/// One of my worklogs as Jira reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worklog {
    pub id: String,
    pub started: i64,
    pub seconds: i64,
}

/// What will be done about one local entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Add a worklog. `readd` means Jira no longer has the worklog this entry was
    /// booked as — deleted in Jira's UI — so this is a fresh booking of old work
    /// and not a first one.
    Create { readd: bool },
    /// The entry was booked and has since been edited: correct the worklog Jira
    /// already has, which is what keeps a replay from adding a second one.
    Update { worklog_id: String },
    /// A worklog of mine already covers this time and no local record pointed at
    /// it. Nothing is sent; the id is written to the local record so the next run
    /// knows about it without asking Jira again.
    Adopt { worklog_id: String },
    /// Nothing to do, with the reason the user needs in order to fix it.
    Skip(Skip),
}

/// Why an entry is not being booked. Every variant is shown to the user: a silent
/// skip in a timesheet is time that quietly never got billed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Skip {
    /// The timer is still running. It has no end yet, so it has no duration to
    /// book; stop it and push again.
    Running,
    /// No issue key on the entry or its task. The one thing only the user can
    /// supply.
    Unmapped,
    /// Under Jira's one-minute minimum.
    TooShort,
    /// Booked already, unchanged since. The common case on a replay, and the whole
    /// point of the exercise.
    Unchanged { worklog_id: String },
}

/// One line of the plan: an entry, what it would book, and what will happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    pub entry_id: i64,
    /// Empty for [`Skip::Unmapped`], which is the case that has no issue.
    pub issue: String,
    pub task: String,
    pub started: i64,
    pub seconds: i64,
    /// Task and note, as the worklog comment. What an agent polished, if the user
    /// asked it to.
    pub comment: String,
    pub action: Action,
}

/// The comment a worklog gets: the task, and the note after it.
///
/// The local text and nothing else. Nothing is fetched from the ticket to build
/// this, and nothing derived from the ticket goes into it.
fn comment(entry: &Entry) -> String {
    match entry
        .note
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
    {
        Some(note) if note != entry.task => format!("{} — {note}", entry.task),
        _ => entry.task.clone(),
    }
}

/// Decide what to do with every entry, given what was already pushed and what Jira
/// currently has.
///
/// `remote` maps an issue key to *my* worklogs on it; an issue missing from the map
/// means "not asked", which is treated the same as "none of mine there" — the local
/// record still protects those. `now` closes running entries for the duration
/// calculation only; they are skipped regardless.
///
/// The order of the rules is the contract:
///
/// 1. running, unmapped and too-short entries can never be booked at all
/// 2. a local record decides between unchanged, update and re-add
/// 3. only then does an overlapping remote worklog make this an adoption
/// 4. anything left is new work
pub fn plan(
    entries: &[Entry],
    pushed: &[Pushed],
    remote: &HashMap<String, Vec<Worklog>>,
    now: i64,
) -> Vec<Planned> {
    let by_entry: HashMap<i64, &Pushed> = pushed.iter().map(|p| (p.entry_id, p)).collect();
    // A remote worklog already claimed by *another* entry's record must not be
    // adopted by this one, or two entries would take turns overwriting one
    // worklog.
    let claimed: HashMap<(&str, &str), i64> = pushed
        .iter()
        .map(|p| ((p.issue_key.as_str(), p.worklog_id.as_str()), p.entry_id))
        .collect();

    entries
        .iter()
        .map(|entry| {
            let seconds = entry.seconds(now);
            let mut line = Planned {
                entry_id: entry.id,
                issue: entry.issue_key.clone().unwrap_or_default(),
                task: entry.task.clone(),
                started: entry.started_at,
                seconds,
                comment: comment(entry),
                action: Action::Create { readd: false },
            };
            line.action = decide(entry, &line, seconds, &by_entry, &claimed, remote);
            line
        })
        .collect()
}

fn decide(
    entry: &Entry,
    line: &Planned,
    seconds: i64,
    by_entry: &HashMap<i64, &Pushed>,
    claimed: &HashMap<(&str, &str), i64>,
    remote: &HashMap<String, Vec<Worklog>>,
) -> Action {
    if entry.is_running() {
        return Action::Skip(Skip::Running);
    }
    if line.issue.is_empty() {
        return Action::Skip(Skip::Unmapped);
    }
    if seconds < MIN_SECONDS {
        return Action::Skip(Skip::TooShort);
    }
    let mine = remote.get(&line.issue).map(Vec::as_slice).unwrap_or(&[]);

    if let Some(record) = by_entry.get(&entry.id) {
        let still_there = mine.iter().any(|w| w.id == record.worklog_id);
        // Only trust the record's own claim when it is about the issue this entry
        // now books to: re-pointing an entry at another ticket is a new booking
        // there, and the old worklog is somebody else's problem — Jira's UI is
        // where it gets removed.
        if record.issue_key == line.issue && (still_there || !remote.contains_key(&line.issue)) {
            return if record.started_at == line.started && record.seconds == seconds {
                Action::Skip(Skip::Unchanged {
                    worklog_id: record.worklog_id.clone(),
                })
            } else {
                Action::Update {
                    worklog_id: record.worklog_id.clone(),
                }
            };
        }
        if record.issue_key == line.issue {
            // The record points at a worklog Jira no longer has.
            return Action::Create { readd: true };
        }
    }

    // No usable record: has this time already been booked on this issue by hand,
    // on another machine, or by a run that died after the POST?
    let overlap = mine.iter().find(|w| {
        w.started < line.started + seconds
            && line.started < w.started + w.seconds.max(1)
            && !claimed.contains_key(&(line.issue.as_str(), w.id.as_str()))
    });
    match overlap {
        Some(w) => Action::Adopt {
            worklog_id: w.id.clone(),
        },
        None => Action::Create { readd: false },
    }
}

// ---------------------------------------------------------------------------
// The requests. Everything above this line is testable without a network.
// ---------------------------------------------------------------------------

/// Who the token belongs to, in the form the worklog author is reported in.
///
/// Fetched rather than assumed: the overlap check must compare *my* worklogs, and
/// on Cloud the field to compare is an `accountId` the user does not know by heart,
/// while on Data Center it is the user name. Getting this wrong in the permissive
/// direction would skip a colleague's worklog as if it were mine — under-booking
/// the day silently.
async fn whoami(cfg: &Config, token: &str) -> Result<String> {
    let me: serde_json::Value = send(cfg, token, reqwest::Method::GET, "myself", None)
        .await
        .context("cannot identify the token's owner")?;
    let field = match cfg.flavor {
        Flavor::Cloud => "accountId",
        Flavor::Server => "name",
    };
    me.get(field)
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .with_context(|| format!("/myself answered without a {field}"))
}

/// A one-call check that the base URL, the flavour and the token agree, for the
/// button in settings. Returns the display name, so the user sees *who* they are
/// connected as rather than a green tick.
pub async fn check(cfg: &Config, token: &str) -> Result<String> {
    let me: serde_json::Value = send(cfg, token, reqwest::Method::GET, "myself", None).await?;
    Ok(me
        .get("displayName")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown user")
        .to_string())
}

/// My worklogs on `issue` that start inside `[from, to)`.
///
/// Filtered here rather than by the endpoint: `startedAfter` exists on Cloud and on
/// newer Data Center only, so paging the issue and filtering locally is the one
/// path that behaves the same on both. The window is generous on purpose — an entry
/// crossing midnight is compared against a worklog that started the day before.
async fn my_worklogs(
    cfg: &Config,
    token: &str,
    me: &str,
    issue: &str,
    from: i64,
    to: i64,
) -> Result<Vec<Worklog>> {
    let mut out = Vec::new();
    let mut start = 0usize;
    loop {
        let path = format!("issue/{issue}/worklog?startAt={start}&maxResults={PAGE}");
        let page: serde_json::Value = send(cfg, token, reqwest::Method::GET, &path, None)
            .await
            .with_context(|| format!("cannot read the worklogs of {issue}"))?;
        let logs = page
            .get("worklogs")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let seen = logs.len();
        for log in logs {
            let author = log.get("author").and_then(|a| {
                a.get(match cfg.flavor {
                    Flavor::Cloud => "accountId",
                    Flavor::Server => "name",
                })
                .and_then(|v| v.as_str())
            });
            if author != Some(me) {
                continue;
            }
            let (Some(id), Some(started), Some(seconds)) = (
                log.get("id").and_then(|v| v.as_str()),
                log.get("started")
                    .and_then(|v| v.as_str())
                    .and_then(parse_started),
                log.get("timeSpentSeconds").and_then(|v| v.as_i64()),
            ) else {
                continue;
            };
            if started < to && started + seconds.max(1) > from {
                out.push(Worklog {
                    id: id.to_string(),
                    started,
                    seconds,
                });
            }
        }
        // `total` is what says whether another page exists; a page that returned
        // nothing also ends the loop, so a server that lies about `total` cannot
        // spin here.
        let total = page.get("total").and_then(|v| v.as_i64()).unwrap_or(0);
        start += seen.max(1);
        if seen == 0 || start as i64 >= total {
            return Ok(out);
        }
    }
}

/// The worklog body. `comment` is the only field whose shape differs between the
/// two flavours.
fn worklog_body(cfg: &Config, line: &Planned, offset: i64) -> serde_json::Value {
    let comment = match cfg.flavor {
        Flavor::Cloud => serde_json::json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "paragraph",
                "content": [{ "type": "text", "text": line.comment }],
            }],
        }),
        Flavor::Server => serde_json::Value::String(line.comment.clone()),
    };
    serde_json::json!({
        "started": format_started(line.started, offset),
        "timeSpentSeconds": line.seconds,
        "comment": comment,
    })
}

async fn send(
    cfg: &Config,
    token: &str,
    method: reqwest::Method,
    path: &str,
    body: Option<serde_json::Value>,
) -> Result<serde_json::Value> {
    let url = cfg.url(path);
    let mut req = reqwest::Client::new()
        .request(method, &url)
        .header("Authorization", cfg.auth(token))
        .header("Accept", "application/json");
    if let Some(body) = body {
        req = req.json(&body);
    }
    let res = req
        .send()
        .await
        .with_context(|| format!("cannot reach {}", cfg.base))?;
    let status = res.status();
    let text = res.text().await.unwrap_or_default();
    if !status.is_success() {
        // Jira's error bodies are JSON with `errorMessages`; anything else (a proxy's
        // HTML, an empty 401) is shown truncated, because a page of markup in a
        // banner tells the user nothing.
        let detail = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| {
                let msgs = v.get("errorMessages")?.as_array()?;
                Some(
                    msgs.iter()
                        .filter_map(|m| m.as_str())
                        .collect::<Vec<_>>()
                        .join("; "),
                )
            })
            .filter(|d| !d.is_empty())
            .unwrap_or_else(|| text.chars().take(200).collect());
        let hint = match status.as_u16() {
            401 => " — check the account and token in settings",
            403 => " — the token is valid but not allowed to do this",
            404 => " — no such issue, or no permission to see it",
            _ => "",
        };
        bail!("Jira answered {status}{hint}: {detail}");
    }
    // A successful PUT can answer 204 with no body; that is not a parse failure.
    if text.trim().is_empty() {
        return Ok(serde_json::Value::Null);
    }
    serde_json::from_str(&text)
        .with_context(|| format!("{url} answered something that is not JSON"))
}

/// What one push run did, for the panel.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub created: usize,
    pub updated: usize,
    pub adopted: usize,
    pub skipped: usize,
    /// One line per entry, in the order they were planned.
    pub lines: Vec<String>,
    /// The first failure, if any. The run stops there: a half-pushed day the user
    /// is told about beats carrying on past an auth error and reporting twenty of
    /// them.
    pub failed: Option<String>,
}

/// Plan a day and, unless `dry`, book it.
///
/// The store is borrowed mutably across the awaits, so this runs where the store
/// lives — one background task at a time, which is also what keeps two concurrent
/// runs from planning against the same un-recorded state.
///
/// Every successful request is recorded *immediately*, before the next one is sent.
/// A run that dies halfway therefore leaves a database that knows exactly what Jira
/// has, and the next run's plan is correct rather than duplicating.
pub async fn push_day(store: &mut Store, date: &str, now: i64, dry: bool) -> Result<Outcome> {
    let cfg = Config::load(store)?;
    if !cfg.is_complete() {
        bail!("Jira is not configured — set the site URL and account in settings");
    }
    let token = cfg
        .token()?
        .context("no Jira token stored — add one in settings")?;
    push(store, &cfg, &token, date, now, dry).await
}

/// [`push_day`] with the credentials handed in rather than resolved.
///
/// Split out for the tests: [`Config::token`] reads the *operating system's*
/// keychain, and a test that has to write a credential into the machine's real
/// credential store to run is a test nobody runs twice. Everything the run actually
/// does is here.
async fn push(
    store: &mut Store,
    cfg: &Config,
    token: &str,
    date: &str,
    now: i64,
    dry: bool,
) -> Result<Outcome> {
    let offset = store.local_offset(now)?;
    let entries = store.track_entries_on(date)?;
    if entries.is_empty() {
        return Ok(Outcome::default());
    }
    let bounds = store.local_midnights(date, 2)?;
    let (from, to) = (bounds[0], bounds[1]);
    let pushed = store.track_pushed_between(from, to)?;
    let me = whoami(cfg, token).await?;

    // One request per distinct issue, not per entry: a day is usually three
    // tickets and twenty blocks of work.
    let mut remote = HashMap::new();
    for issue in entries
        .iter()
        .filter_map(|e| e.issue_key.clone())
        .collect::<std::collections::BTreeSet<_>>()
    {
        // A day's entries are compared against a window a day wider either side, so
        // an entry crossing midnight still sees the worklog it may already be.
        let logs = my_worklogs(cfg, token, &me, &issue, from - 86_400, to + 86_400).await?;
        remote.insert(issue, logs);
    }

    let plan = plan(&entries, &pushed, &remote, now);
    let mut out = Outcome::default();
    for line in &plan {
        let time = format!("{} {}", hm(line.seconds), short_time(line.started, offset));
        match &line.action {
            Action::Skip(why) => {
                out.skipped += 1;
                out.lines.push(match why {
                    Skip::Running => format!("{} — still running, not booked", line.task),
                    Skip::Unmapped => format!("{} {time} — no ticket linked", line.task),
                    Skip::TooShort => format!("{} {time} — under a minute, not booked", line.task),
                    Skip::Unchanged { .. } => {
                        format!("{} {time} {} — already in Jira", line.issue, line.task)
                    }
                });
            }
            Action::Adopt { worklog_id } => {
                out.adopted += 1;
                out.lines.push(format!(
                    "{} {time} {} — already booked in Jira, linked",
                    line.issue, line.task
                ));
                if !dry {
                    store.track_record_push(
                        line.entry_id,
                        &line.issue,
                        worklog_id,
                        line.started,
                        line.seconds,
                        now,
                    )?;
                }
            }
            Action::Create { readd } => {
                out.created += 1;
                out.lines.push(format!(
                    "{} {time} {} — {}",
                    line.issue,
                    line.task,
                    if *readd { "re-added" } else { "booked" }
                ));
                if dry {
                    continue;
                }
                let path = format!(
                    "issue/{}/worklog?notifyUsers=false&adjustEstimate=leave",
                    line.issue
                );
                let body = worklog_body(cfg, line, offset);
                let answer = match send(cfg, token, reqwest::Method::POST, &path, Some(body)).await
                {
                    Ok(answer) => answer,
                    Err(e) => {
                        out.failed = Some(format!("{e:#}"));
                        break;
                    }
                };
                let id = answer
                    .get("id")
                    .and_then(|v| v.as_str())
                    .context("Jira accepted the worklog but did not return its id")?;
                store.track_record_push(
                    line.entry_id,
                    &line.issue,
                    id,
                    line.started,
                    line.seconds,
                    now,
                )?;
            }
            Action::Update { worklog_id } => {
                out.updated += 1;
                out.lines
                    .push(format!("{} {time} {} — corrected", line.issue, line.task));
                if dry {
                    continue;
                }
                let path = format!(
                    "issue/{}/worklog/{worklog_id}?notifyUsers=false&adjustEstimate=leave",
                    line.issue
                );
                let body = worklog_body(cfg, line, offset);
                if let Err(e) = send(cfg, token, reqwest::Method::PUT, &path, Some(body)).await {
                    out.failed = Some(format!("{e:#}"));
                    break;
                }
                store.track_record_push(
                    line.entry_id,
                    &line.issue,
                    worklog_id,
                    line.started,
                    line.seconds,
                    now,
                )?;
            }
        }
    }
    Ok(out)
}

/// `2h 15m`, matching how the tracker writes durations elsewhere.
fn hm(seconds: i64) -> String {
    let (h, m) = (seconds / 3_600, seconds % 3_600 / 60);
    if h > 0 {
        format!("{h}h {m:02}m")
    } else {
        format!("{m}m")
    }
}

/// `09:30`, in the local zone the worklog is booked in.
fn short_time(unix: i64, offset: i64) -> String {
    let secs = (unix + offset).rem_euclid(86_400);
    format!("{:02}:{:02}", secs / 3_600, secs % 3_600 / 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: i64, issue: Option<&str>, started: i64, seconds: i64) -> Entry {
        Entry {
            id,
            task: format!("task {id}"),
            note: None,
            started_at: started,
            stopped_at: Some(started + seconds),
            source: "app".into(),
            issue_key: issue.map(str::to_string),
            issue_own: false,
        }
    }

    fn record(entry_id: i64, issue: &str, worklog_id: &str, started: i64, seconds: i64) -> Pushed {
        Pushed {
            entry_id,
            issue_key: issue.into(),
            worklog_id: worklog_id.into(),
            started_at: started,
            seconds,
            pushed_at: 0,
        }
    }

    fn remote(issue: &str, logs: &[(&str, i64, i64)]) -> HashMap<String, Vec<Worklog>> {
        HashMap::from([(
            issue.to_string(),
            logs.iter()
                .map(|(id, started, seconds)| Worklog {
                    id: (*id).into(),
                    started: *started,
                    seconds: *seconds,
                })
                .collect(),
        )])
    }

    fn actions(plan: &[Planned]) -> Vec<Action> {
        plan.iter().map(|p| p.action.clone()).collect()
    }

    #[test]
    fn base64_matches_the_known_answers_including_both_paddings() {
        for (input, want) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
            ("a@b.com:tok", "YUBiLmNvbTp0b2s="),
        ] {
            assert_eq!(base64(input.to_string()), want, "{input:?}");
        }
    }

    #[test]
    fn a_started_timestamp_round_trips_through_both_directions() {
        // 2026-07-29 09:30:00 in +02:00 is 07:30 UTC.
        let unix = 1_785_310_200;
        assert_eq!(
            format_started(unix, 2 * 3_600),
            "2026-07-29T09:30:00.000+0200"
        );
        assert_eq!(parse_started("2026-07-29T09:30:00.000+0200"), Some(unix));
        // The same instant, written the ways Jira and its proxies write it.
        for s in [
            "2026-07-29T07:30:00.000+0000",
            "2026-07-29T07:30:00.000Z",
            "2026-07-29T07:30:00",
            "2026-07-29T07:30:00.123456+00:00",
            "2026-07-29T04:30:00.000-0300",
        ] {
            assert_eq!(parse_started(s), Some(unix), "{s}");
        }
        // A negative offset must not be written as +.
        assert_eq!(
            format_started(unix, -3 * 3_600 - 30 * 60),
            "2026-07-29T04:00:00.000-0330"
        );
        for bad in [
            "",
            "nope",
            "2026-07-29",
            "2026-07-29T09:30",
            "2026-13-29T09:30:00.000+0200",
        ] {
            assert_eq!(parse_started(bad), None, "{bad}");
        }
    }

    #[test]
    fn an_unmapped_running_or_too_short_entry_is_never_booked() {
        let mut running = entry(1, Some("P-1"), 1_000, 0);
        running.stopped_at = None;
        let entries = [
            running,
            entry(2, None, 2_000, 3_600),
            entry(3, Some("P-1"), 6_000, 59),
        ];
        assert_eq!(
            actions(&plan(&entries, &[], &HashMap::new(), 99_999)),
            [
                Action::Skip(Skip::Running),
                Action::Skip(Skip::Unmapped),
                Action::Skip(Skip::TooShort),
            ]
        );
    }

    #[test]
    fn replaying_an_untouched_day_sends_nothing() {
        let entries = [entry(1, Some("P-1"), 1_000, 3_600)];
        let pushed = [record(1, "P-1", "w1", 1_000, 3_600)];
        let plan = plan(
            &entries,
            &pushed,
            &remote("P-1", &[("w1", 1_000, 3_600)]),
            0,
        );
        assert_eq!(
            actions(&plan),
            [Action::Skip(Skip::Unchanged {
                worklog_id: "w1".into()
            })]
        );
        assert!(
            !matches!(
                plan[0].action,
                Action::Create { .. } | Action::Update { .. }
            ),
            "a replay must send no request"
        );
    }

    #[test]
    fn working_longer_after_closing_the_day_corrects_the_worklog_instead_of_adding_one() {
        // The case the whole feature exists for: booked an hour, worked another
        // half, replayed the day.
        let entries = [entry(1, Some("P-1"), 1_000, 5_400)];
        let pushed = [record(1, "P-1", "w1", 1_000, 3_600)];
        assert_eq!(
            actions(&plan(
                &entries,
                &pushed,
                &remote("P-1", &[("w1", 1_000, 3_600)]),
                0
            )),
            [Action::Update {
                worklog_id: "w1".into()
            }]
        );
    }

    #[test]
    fn a_worklog_deleted_in_jira_is_re_added_not_silently_skipped() {
        let entries = [entry(1, Some("P-1"), 1_000, 3_600)];
        let pushed = [record(1, "P-1", "w1", 1_000, 3_600)];
        assert_eq!(
            actions(&plan(&entries, &pushed, &remote("P-1", &[]), 0)),
            [Action::Create { readd: true }]
        );
    }

    #[test]
    fn time_already_booked_in_jira_is_adopted_rather_than_booked_twice() {
        // No local record — booked from Jira's web UI, from another machine, or by
        // a run that died between the POST and the commit.
        let entries = [entry(1, Some("P-1"), 1_000, 3_600)];
        assert_eq!(
            actions(&plan(
                &entries,
                &[],
                &remote("P-1", &[("w9", 2_000, 1_800)]),
                0
            )),
            [Action::Adopt {
                worklog_id: "w9".into()
            }],
            "an overlap is the same work, whatever its exact bounds"
        );
        // Adjacent, not overlapping: 1_000..4_600 and 4_600..8_200 are two blocks.
        assert_eq!(
            actions(&plan(
                &entries,
                &[],
                &remote("P-1", &[("w9", 4_600, 3_600)]),
                0
            )),
            [Action::Create { readd: false }]
        );
        // A worklog another entry has already claimed is not up for adoption, or the
        // two entries would take turns overwriting it.
        let entries = [
            entry(1, Some("P-1"), 1_000, 3_600),
            entry(2, Some("P-1"), 1_200, 3_600),
        ];
        let pushed = [record(1, "P-1", "w9", 1_000, 3_600)];
        assert_eq!(
            actions(&plan(
                &entries,
                &pushed,
                &remote("P-1", &[("w9", 1_000, 3_600)]),
                0
            )),
            [
                Action::Skip(Skip::Unchanged {
                    worklog_id: "w9".into()
                }),
                Action::Create { readd: false },
            ]
        );
    }

    #[test]
    fn moving_an_entry_to_another_ticket_books_it_there() {
        // The old worklog stays where it is: removing somebody's booked time is
        // Jira's UI's job, not a side effect of retyping a key here.
        let entries = [entry(1, Some("NEW-1"), 1_000, 3_600)];
        let pushed = [record(1, "OLD-1", "w1", 1_000, 3_600)];
        let mut remote = remote("OLD-1", &[("w1", 1_000, 3_600)]);
        remote.insert("NEW-1".into(), Vec::new());
        assert_eq!(
            actions(&plan(&entries, &pushed, &remote, 0)),
            [Action::Create { readd: false }]
        );
    }

    #[test]
    fn an_unreadable_issue_falls_back_to_the_local_record_rather_than_double_booking() {
        // `remote` has no entry for the issue: the GET was never made or failed.
        // Trusting the record is the safe direction — the unsafe one adds a second
        // worklog.
        let entries = [entry(1, Some("P-1"), 1_000, 3_600)];
        let pushed = [record(1, "P-1", "w1", 1_000, 3_600)];
        assert_eq!(
            actions(&plan(&entries, &pushed, &HashMap::new(), 0)),
            [Action::Skip(Skip::Unchanged {
                worklog_id: "w1".into()
            })]
        );
    }

    #[test]
    fn the_comment_is_the_local_text_and_nothing_else() {
        let mut e = entry(1, Some("P-1"), 0, 3_600);
        assert_eq!(comment(&e), "task 1");
        e.note = Some("  ".into());
        assert_eq!(comment(&e), "task 1");
        e.note = Some("fixed the retry loop".into());
        assert_eq!(comment(&e), "task 1 — fixed the retry loop");
        // A note that merely repeats the task is not repeated.
        e.note = Some("task 1".into());
        assert_eq!(comment(&e), "task 1");
    }

    #[test]
    fn the_two_flavours_differ_only_where_they_must() {
        let cloud = Config {
            base: "https://acme.atlassian.net/".into(),
            flavor: Flavor::Cloud,
            account: "a@b.com".into(),
        };
        let server = Config {
            base: "https://jira.acme.local".into(),
            flavor: Flavor::Server,
            account: "sam".into(),
        };
        assert_eq!(
            cloud.url("issue/P-1/worklog"),
            "https://acme.atlassian.net/rest/api/3/issue/P-1/worklog",
            "the trailing slash must not double up"
        );
        assert_eq!(
            server.url("myself"),
            "https://jira.acme.local/rest/api/2/myself"
        );
        assert_eq!(cloud.auth("tok"), "Basic YUBiLmNvbTp0b2s=");
        assert_eq!(server.auth("tok"), "Bearer tok");
        // A Data Center PAT identifies its own user; Cloud needs the e-mail.
        assert!(server.is_complete());
        assert!(
            !Config {
                account: String::new(),
                ..cloud.clone()
            }
            .is_complete()
        );
        assert!(!Config::default().is_complete());

        let line = &plan(&[entry(1, Some("P-1"), 0, 3_600)], &[], &HashMap::new(), 0)[0];
        assert_eq!(
            worklog_body(&server, line, 0)["comment"],
            serde_json::json!("task 1"),
            "Data Center takes plain text"
        );
        assert_eq!(
            worklog_body(&cloud, line, 0)["comment"]["content"][0]["content"][0]["text"],
            serde_json::json!("task 1"),
            "Cloud takes ADF"
        );
        assert_eq!(worklog_body(&cloud, line, 0)["timeSpentSeconds"], 3_600);
    }

    #[test]
    fn durations_and_clock_faces_read_the_way_the_tracker_writes_them() {
        assert_eq!(hm(0), "0m");
        assert_eq!(hm(59), "0m");
        assert_eq!(hm(3_600), "1h 00m");
        assert_eq!(hm(8_100), "2h 15m");
        assert_eq!(short_time(1_785_310_200, 2 * 3_600), "09:30");
    }

    // -----------------------------------------------------------------------
    // Against a stub Jira. What `plan` cannot tell us: whether the requests this
    // module actually sends are the ones Jira wants — the header, the path, the body
    // and the timestamp — and whether a replay over the wire really sends nothing.
    //
    // A stub and not a container on purpose. `atlassian/jira-software` is a real
    // option and worth running once by hand before trusting this against a company
    // Jira, but it needs a licence key, a few gigabytes and minutes per start, which
    // is not something that can sit in `cargo test`. Four endpoints are enough to be
    // wrong about, and four endpoints fit in the stub below.
    // -----------------------------------------------------------------------

    use std::io::{BufRead, BufReader, Read, Write};
    use std::sync::{Arc, Mutex};

    /// One request the stub saw: `(method, path, Authorization, body)`.
    type Seen = (String, String, String, String);

    /// A throwaway HTTP server that answers the four endpoints this module calls.
    ///
    /// Hand-rolled over `TcpListener` rather than a test-server dependency: it speaks
    /// exactly one dialect of HTTP/1.1 — the one `reqwest` sends — and a crate to do
    /// that would be a bigger thing to keep working than this is.
    struct Stub {
        base: String,
        seen: Arc<Mutex<Vec<Seen>>>,
        /// Worklogs the stub reports on `PROJ-1`, as `(id, started, seconds)`.
        worklogs: Arc<Mutex<Vec<(String, String, i64)>>>,
    }

    impl Stub {
        fn start() -> Self {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let seen: Arc<Mutex<Vec<Seen>>> = Arc::new(Mutex::new(Vec::new()));
            let worklogs = Arc::new(Mutex::new(Vec::new()));
            let (s, w) = (seen.clone(), worklogs.clone());
            // Detached: the test drops the Stub and the thread dies with the process.
            // A join handle would only be something to remember to wait on.
            std::thread::spawn(move || {
                for stream in listener.incoming().flatten() {
                    let mut reader = BufReader::new(&stream);
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_err() {
                        continue;
                    }
                    let mut parts = line.split_whitespace();
                    let method = parts.next().unwrap_or_default().to_string();
                    let path = parts.next().unwrap_or_default().to_string();
                    // Headers, for the length and the credential.
                    let (mut len, mut auth) = (0usize, String::new());
                    loop {
                        let mut header = String::new();
                        if reader.read_line(&mut header).is_err() || header.trim().is_empty() {
                            break;
                        }
                        let lower = header.to_ascii_lowercase();
                        if let Some(v) = lower.strip_prefix("content-length:") {
                            len = v.trim().parse().unwrap_or(0);
                        } else if lower.starts_with("authorization:") {
                            auth = header
                                .split_once(':')
                                .map(|(_, v)| v.trim().to_string())
                                .unwrap_or_default();
                        }
                    }
                    let mut body = vec![0u8; len];
                    if len > 0 && reader.read_exact(&mut body).is_err() {
                        continue;
                    }
                    let body = String::from_utf8_lossy(&body).to_string();
                    s.lock()
                        .unwrap()
                        .push((method.clone(), path.clone(), auth, body));

                    let reply = if path.ends_with("/myself") {
                        json_ok(r#"{"name":"sam","accountId":"acc-1","displayName":"Sam"}"#)
                    } else if method == "GET" {
                        let logs: Vec<String> = w
                            .lock()
                            .unwrap()
                            .iter()
                            .map(|(id, started, seconds)| {
                                format!(
                                    r#"{{"id":"{id}","started":"{started}",
                                        "timeSpentSeconds":{seconds},
                                        "author":{{"name":"sam","accountId":"acc-1"}}}}"#
                                )
                            })
                            .collect();
                        json_ok(&format!(
                            r#"{{"total":{},"worklogs":[{}]}}"#,
                            logs.len(),
                            logs.join(",")
                        ))
                    } else if method == "POST" {
                        // What Jira answers a create with, and the id this module has
                        // to remember or the next run double-books.
                        json_ok(r#"{"id":"10001"}"#)
                    } else {
                        // A PUT answering 204 with no body: the case that must not be
                        // read as a parse failure.
                        "HTTP/1.1 204 No Content\r\ncontent-length: 0\r\n\r\n".to_string()
                    };
                    let mut stream = &stream;
                    let _ = stream.write_all(reply.as_bytes());
                    let _ = stream.flush();
                }
            });
            Self {
                base,
                seen,
                worklogs,
            }
        }

        fn requests(&self) -> Vec<Seen> {
            self.seen.lock().unwrap().clone()
        }

        fn writes(&self) -> Vec<Seen> {
            self.requests()
                .into_iter()
                .filter(|(m, ..)| m == "POST" || m == "PUT")
                .collect()
        }
    }

    fn json_ok(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
            body.len()
        )
    }

    /// A day with one hour of work on `PROJ-1`, and the store to push it from.
    fn one_hour_day(base: &str) -> (Store, String, i64, Config) {
        let mut store = Store::open_in_memory().unwrap();
        // The day is built from the store's own local midnight, so this is correct
        // in whatever timezone the test runs in — the same rule the app follows.
        let now = 1_785_310_200;
        let date = store.local_date(now).unwrap();
        let midnight = store.local_midnights(&date, 1).unwrap()[0];
        let start = midnight + 9 * 3_600;
        store
            .track_log(
                "coding",
                Some("the retry loop"),
                start,
                start + 3_600,
                "app",
            )
            .unwrap();
        store
            .track_set_task_issue("coding", Some("PROJ-1"))
            .unwrap();
        let cfg = Config {
            base: base.to_string(),
            flavor: Flavor::Server,
            account: "sam".into(),
        };
        (store, date, start, cfg)
    }

    fn run(store: &mut Store, cfg: &Config, date: &str, dry: bool) -> Outcome {
        // The runtime is built here rather than shared: a test owns its executor.
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(push(store, cfg, "tok-123", date, 1_785_310_200, dry))
            .expect("the push must not fail against a healthy stub")
    }

    #[test]
    fn a_dry_run_reads_jira_but_writes_nothing_anywhere() {
        let stub = Stub::start();
        let (mut store, date, _, cfg) = one_hour_day(&stub.base);

        let out = run(&mut store, &cfg, &date, true);
        assert_eq!((out.created, out.skipped), (1, 0));
        assert!(stub.writes().is_empty(), "{:?}", stub.requests());
        assert!(
            store.track_pushed_between(0, i64::MAX).unwrap().is_empty(),
            "a preview must not leave a record claiming Jira has it"
        );
    }

    #[test]
    fn a_real_run_sends_what_data_center_wants_and_a_replay_sends_nothing() {
        let stub = Stub::start();
        let (mut store, date, start, cfg) = one_hour_day(&stub.base);

        let out = run(&mut store, &cfg, &date, false);
        assert_eq!((out.created, out.updated, out.skipped), (1, 0, 0));
        assert!(out.failed.is_none(), "{:?}", out.failed);

        let writes = stub.writes();
        assert_eq!(writes.len(), 1, "{writes:?}");
        let (method, path, auth, body) = &writes[0];
        assert_eq!(method, "POST");
        assert_eq!(
            path, "/rest/api/2/issue/PROJ-1/worklog?notifyUsers=false&adjustEstimate=leave",
            "v2 for Data Center, and neither a notification nor an estimate change"
        );
        assert_eq!(auth, "Bearer tok-123", "a PAT is a bearer, not Basic");
        let sent: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(sent["timeSpentSeconds"], 3_600);
        assert_eq!(
            sent["comment"], "coding — the retry loop",
            "Data Center takes the comment as plain text, and it is the local text"
        );
        // The timestamp Jira will interpret must be the wall clock the tracker shows,
        // with the offset that was in force that day.
        let offset = store.local_offset(start).unwrap();
        assert_eq!(sent["started"], format_started(start, offset));
        assert_eq!(
            parse_started(sent["started"].as_str().unwrap()),
            Some(start)
        );

        // The id came back and was recorded, which is what makes the replay safe.
        let recorded = store.track_pushed_between(0, i64::MAX).unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].worklog_id, "10001");

        // Now Jira has it, and a second run must not book it again.
        stub.worklogs
            .lock()
            .unwrap()
            .push(("10001".into(), format_started(start, offset), 3_600));
        let before = stub.writes().len();
        let out = run(&mut store, &cfg, &date, false);
        assert_eq!((out.created, out.updated, out.skipped), (0, 0, 1));
        assert_eq!(
            stub.writes().len(),
            before,
            "the replay must send no write at all"
        );
    }

    #[test]
    fn working_longer_after_the_push_corrects_the_worklog_over_the_wire() {
        let stub = Stub::start();
        let (mut store, date, start, cfg) = one_hour_day(&stub.base);
        run(&mut store, &cfg, &date, false);
        let offset = store.local_offset(start).unwrap();
        stub.worklogs
            .lock()
            .unwrap()
            .push(("10001".into(), format_started(start, offset), 3_600));

        // Another half hour on the same entry, then a replay.
        let entry = store.track_entries_on(&date).unwrap()[0].clone();
        store
            .track_update_entry(
                entry.id,
                &entry.task,
                entry.note.as_deref(),
                start,
                start + 5_400,
                "app",
            )
            .unwrap();

        let out = run(&mut store, &cfg, &date, false);
        assert_eq!((out.created, out.updated), (0, 1));
        let put = stub
            .writes()
            .into_iter()
            .find(|(m, ..)| m == "PUT")
            .expect("the correction must be a PUT, not a second POST");
        assert!(put.1.contains("/worklog/10001"), "{}", put.1);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&put.3).unwrap()["timeSpentSeconds"],
            5_400
        );
    }

    #[test]
    fn time_already_in_jira_is_adopted_over_the_wire_without_booking_it_again() {
        let stub = Stub::start();
        let (mut store, date, start, cfg) = one_hour_day(&stub.base);
        // Booked by hand in Jira's own UI: overlapping, no local record.
        let offset = store.local_offset(start).unwrap();
        stub.worklogs.lock().unwrap().push((
            "99".into(),
            format_started(start + 600, offset),
            1_800,
        ));

        let out = run(&mut store, &cfg, &date, false);
        assert_eq!((out.created, out.adopted), (0, 1));
        assert!(stub.writes().is_empty(), "{:?}", stub.requests());
        assert_eq!(
            store.track_pushed_between(0, i64::MAX).unwrap()[0].worklog_id,
            "99",
            "the existing worklog is adopted, so the next run knows about it too"
        );
    }

    #[test]
    fn cloud_sends_basic_auth_v3_and_an_adf_comment() {
        let stub = Stub::start();
        let (mut store, date, _, cfg) = one_hour_day(&stub.base);
        let cfg = Config {
            flavor: Flavor::Cloud,
            account: "sam@acme.com".into(),
            ..cfg
        };

        run(&mut store, &cfg, &date, false);
        let (_, path, auth, body) = stub.writes().into_iter().next().unwrap();
        assert!(path.starts_with("/rest/api/3/"), "{path}");
        assert_eq!(auth, "Basic c2FtQGFjbWUuY29tOnRvay0xMjM=");
        let sent: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            sent["comment"]["content"][0]["content"][0]["text"],
            "coding — the retry loop"
        );
    }
}
