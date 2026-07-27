//! Update from GitHub Releases: check on startup, swap in place, restart.
//!
//! The user should see "Update available", click it, answer one question, and end
//! up on the new version with the app already running. No installer, no download
//! folder, no manual restart.
//!
//! # How replacing a running binary works on Windows
//!
//! You cannot *delete* or *overwrite* the image of a running process, but you can
//! **rename** it — the file stays open, its directory entry moves. So:
//!
//! 1. download beside the current exe, as `request-pilot.new.exe`
//! 2. rename `request-pilot.exe` → `request-pilot.old.exe`  (the running one)
//! 3. rename `request-pilot.new.exe` → `request-pilot.exe`
//! 4. start `request-pilot.exe`, then exit
//! 5. next startup, delete the `.old.exe` left behind
//!
//! Both renames must be on one volume, which is why the download goes next to the
//! exe and not to `%TEMP%`. If step 3 fails the second step is undone, so a failed
//! update leaves a working app rather than none.
//!
//! # What is checked, and what is not
//!
//! HTTPS to `api.github.com` is the trust anchor: it establishes that the bytes
//! came from this repository's releases. On top of that the download must *be* a
//! Windows GUI executable — [`looks_like_gui_exe`] reads the PE header — which
//! catches a truncated download, a rate-limit page saved as an `.exe`, and the
//! console-subsystem build that this repo shipped once.
//!
//! It is **not** a signature check. Nothing in the release is signed today, so
//! there is no key to verify against and pretending otherwise would be worse than
//! saying so. If releases ever get signed, this is the function to extend.
use anyhow::{Context as _, Result, bail};
use std::path::{Path, PathBuf};

/// The repository the releases come from.
pub const REPO: &str = "SamTV12345/request-desktop";

/// The asset to download. The release publishes `rp-mcp.exe` beside it, which is a
/// different program and must not be picked by accident.
pub const ASSET: &str = "request-pilot.exe";

/// GitHub requires a User-Agent and answers 403 without one.
const UA: &str = concat!("request-pilot/", env!("CARGO_PKG_VERSION"));

/// This build's version, from the workspace manifest.
pub fn current() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// A release newer than this build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Available {
    /// The tag as GitHub gave it, `v` and all — shown to the user unchanged so it
    /// matches what they see on the releases page.
    pub tag: String,
    /// Download URL of [`ASSET`].
    pub url: String,
}

/// `MAJOR.MINOR.PATCH` and whether a pre-release suffix followed.
///
/// Hand-rolled rather than pulling in `semver`, which is in the lock file only as
/// somebody else's transitive dependency. What is needed is a numeric compare of
/// three fields, and the one trap — `1.10.0` sorting below `1.9.0` because the
/// strings were compared instead of the numbers — is what the tests pin.
fn parts(tag: &str) -> Option<([u64; 3], bool)> {
    let tag = tag.trim().trim_start_matches(['v', 'V']);
    // Everything from the first `-` or `+` is a pre-release or build suffix.
    let (core, rest) = match tag.find(['-', '+']) {
        Some(at) => (&tag[..at], &tag[at..]),
        None => (tag, ""),
    };
    let mut out = [0u64; 3];
    let mut seen = 0;
    for (i, field) in core.split('.').enumerate() {
        if i >= 3 {
            return None;
        }
        out[i] = field.parse().ok()?;
        seen += 1;
    }
    if seen == 0 {
        return None;
    }
    Some((out, rest.starts_with('-')))
}

/// Whether `tag` is a version to move *to* from `current`.
///
/// Anything unparseable on either side is "no": an update prompt that cannot say
/// what it would install is worse than no prompt. A pre-release never wins against
/// the same release proper, per semver — though in practice GitHub's
/// `releases/latest` already excludes pre-releases, which is why that endpoint is
/// the one used.
pub fn is_newer(current: &str, tag: &str) -> bool {
    let (Some((have, have_pre)), Some((got, got_pre))) = (parts(current), parts(tag)) else {
        return false;
    };
    match got.cmp(&have) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        // Same numbers: only a move off a pre-release onto the real thing.
        std::cmp::Ordering::Equal => have_pre && !got_pre,
    }
}

/// Decide from a `releases/latest` payload. `Ok(None)` means "nothing to do".
///
/// The whole decision minus the network, so it can be tested against real payload
/// shapes: a newer release, the same version, a release whose assets do not include
/// this app, and a draft with no assets at all.
pub fn decide(current: &str, json: &str) -> Result<Option<Available>> {
    let v: serde_json::Value = serde_json::from_str(json).context("release JSON")?;
    let tag = v["tag_name"]
        .as_str()
        .context("the release has no tag_name")?
        .to_string();
    if !is_newer(current, &tag) {
        return Ok(None);
    }
    let assets = v["assets"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let url = assets
        .iter()
        .find(|a| a["name"].as_str() == Some(ASSET))
        .and_then(|a| a["browser_download_url"].as_str());
    match url {
        Some(url) => Ok(Some(Available {
            tag,
            url: url.to_string(),
        })),
        // A tag with no matching asset is a release for another platform, or one
        // still uploading. Not an error, and not an offer either.
        None => Ok(None),
    }
}

/// Ask GitHub, against `api` as the API root so a test can point it elsewhere.
///
/// **404 is not an error.** `releases/latest` answers 404 when a repository has no
/// published release yet — which is what this one answers today — and also when
/// every release is a pre-release, since that endpoint excludes them. Treating it
/// as a failure meant a log line on every single startup for a repository that is
/// simply not there yet. Found by asking the real endpoint; the mocked test could
/// not have shown it.
pub async fn check_at(api: &str, current: &str) -> Result<Option<Available>> {
    let url = format!("{api}/repos/{REPO}/releases/latest");
    let res = reqwest::Client::builder()
        .user_agent(UA)
        .build()?
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?;
    if res.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let body = res.error_for_status()?.text().await?;
    decide(current, &body)
}

pub async fn check() -> Result<Option<Available>> {
    check_at("https://api.github.com", current()).await
}

pub async fn download(url: &str) -> Result<Vec<u8>> {
    let bytes = reqwest::Client::builder()
        .user_agent(UA)
        .build()?
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    Ok(bytes.to_vec())
}

/// Whether `bytes` is a Windows **GUI** executable.
///
/// The PE optional header's Subsystem field: 2 is GUI, 3 is console. Reading it is
/// the same check `release.yml` runs on the artefact, here to make sure what
/// arrived over the wire is the thing that was published — an HTML error page, a
/// half-finished download, or the console build all fail it.
pub fn looks_like_gui_exe(bytes: &[u8]) -> bool {
    // MZ, then the PE offset at 0x3C, then `PE\0\0`, then 20 bytes of COFF header,
    // then 68 into the optional header.
    if bytes.len() < 0x40 || &bytes[..2] != b"MZ" {
        return false;
    }
    let at = |i: usize| -> Option<u32> {
        let b = bytes.get(i..i + 4)?;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    let Some(pe) = at(0x3C).map(|v| v as usize) else {
        return false;
    };
    if bytes.get(pe..pe + 4) != Some(b"PE\0\0") {
        return false;
    }
    let sub = pe + 4 + 20 + 68;
    match bytes.get(sub..sub + 2) {
        Some(b) => u16::from_le_bytes([b[0], b[1]]) == 2,
        None => false,
    }
}

/// Where the incoming build is written: beside the running one, so the renames
/// stay on one volume.
pub fn incoming(exe: &Path) -> PathBuf {
    with_suffix(exe, "new")
}

/// Where the running build is moved aside to.
pub fn displaced(exe: &Path) -> PathBuf {
    with_suffix(exe, "old")
}

fn with_suffix(exe: &Path, what: &str) -> PathBuf {
    let stem = exe
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "request-pilot".into());
    let ext = exe.extension().map(|e| e.to_string_lossy().to_string());
    let name = match ext {
        Some(ext) => format!("{stem}.{what}.{ext}"),
        None => format!("{stem}.{what}"),
    };
    exe.with_file_name(name)
}

/// Put `bytes` in place of `exe`, leaving the old build at [`displaced`].
///
/// Returns the path to run. The order matters and so does the rollback: if the
/// second rename fails — a virus scanner holding the new file, a permission
/// problem — the first is undone, so the outcome is the old version still working
/// rather than no version at all.
pub fn swap(exe: &Path, bytes: &[u8]) -> Result<PathBuf> {
    if !looks_like_gui_exe(bytes) {
        bail!(
            "the download is not a Windows GUI executable ({} bytes) — not installing it",
            bytes.len()
        );
    }
    let new = incoming(exe);
    let old = displaced(exe);
    // A leftover from an interrupted attempt would otherwise be renamed into place.
    let _ = std::fs::remove_file(&new);
    let _ = std::fs::remove_file(&old);
    std::fs::write(&new, bytes)
        .with_context(|| format!("cannot write {} — is the folder writable?", new.display()))?;
    std::fs::rename(exe, &old)
        .with_context(|| format!("cannot move the running build aside to {}", old.display()))?;
    if let Err(e) = std::fs::rename(&new, exe) {
        // Put the working build back before reporting.
        let _ = std::fs::rename(&old, exe);
        let _ = std::fs::remove_file(&new);
        return Err(anyhow::Error::new(e)
            .context(format!("cannot move the new build into {}", exe.display())));
    }
    Ok(exe.to_path_buf())
}

/// Delete the build left behind by the last update, if the file is free now.
///
/// Called at startup: the process that was holding it is the one that spawned this
/// one and has exited. A failure is ignored on purpose — a stray `.old.exe` is
/// untidy, not broken, and it will be swept on any later run.
pub fn sweep(exe: &Path) {
    let old = displaced(exe);
    if old.exists() && std::fs::remove_file(&old).is_ok() {
        crate::log::note!("removed the previous build at {}", old.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A payload in the shape GitHub actually returns, trimmed to the fields read.
    fn payload(tag: &str, assets: &[&str]) -> String {
        let assets: Vec<String> = assets
            .iter()
            .map(|n| {
                format!(r#"{{"name":"{n}","browser_download_url":"https://example.invalid/{n}"}}"#)
            })
            .collect();
        format!(
            r#"{{"tag_name":"{tag}","name":"Toolbench {tag}","assets":[{}]}}"#,
            assets.join(",")
        )
    }

    #[test]
    fn a_newer_tag_with_the_right_asset_is_offered() {
        let got = decide("0.1.0", &payload("v0.2.0", &["rp-mcp.exe", ASSET]))
            .expect("valid payload")
            .expect("an offer");
        assert_eq!(got.tag, "v0.2.0", "the tag is shown as GitHub wrote it");
        assert!(got.url.ends_with(ASSET), "and not rp-mcp.exe: {}", got.url);
    }

    #[test]
    fn the_same_version_or_an_older_one_is_not_offered() {
        for tag in ["v0.1.0", "0.1.0", "v0.0.9"] {
            assert_eq!(
                decide("0.1.0", &payload(tag, &[ASSET])).unwrap(),
                None,
                "{tag}"
            );
        }
    }

    /// The trap in hand-rolling this: `"1.10.0" < "1.9.0"` as strings.
    #[test]
    fn versions_compare_as_numbers_and_not_as_strings() {
        assert!(is_newer("1.9.0", "1.10.0"));
        assert!(!is_newer("1.10.0", "1.9.0"));
        assert!(is_newer("0.9.9", "0.10.0"));
        assert!(is_newer("1.2.3", "2.0.0"));
        assert!(!is_newer("2.0.0", "1.99.99"));
    }

    #[test]
    fn a_v_prefix_on_either_side_makes_no_difference() {
        assert!(is_newer("v0.1.0", "v0.2.0"));
        assert!(is_newer("0.1.0", "v0.2.0"));
        assert!(!is_newer("v0.2.0", "0.2.0"));
    }

    /// A pre-release loses to the release proper and never the other way round.
    #[test]
    fn a_prerelease_is_older_than_the_release_it_precedes() {
        assert!(is_newer("0.2.0-rc1", "0.2.0"));
        assert!(!is_newer("0.2.0", "0.2.0-rc1"));
        assert!(is_newer("0.1.0", "0.2.0-rc1"), "still a newer number");
    }

    /// Junk on either side means no offer, rather than a prompt that cannot say
    /// what it would install.
    #[test]
    fn nothing_unparseable_is_ever_offered() {
        for tag in ["", "latest", "v", "nightly", "1.2.3.4", "vX.Y.Z"] {
            assert!(!is_newer("0.1.0", tag), "{tag:?}");
        }
        assert!(!is_newer("not-a-version", "9.9.9"));
    }

    /// A release for another platform, and one still uploading its assets: newer,
    /// but nothing here to install.
    #[test]
    fn a_newer_release_without_this_asset_is_not_an_offer() {
        assert_eq!(
            decide("0.1.0", &payload("v9.0.0", &["rp-mcp.exe"])).unwrap(),
            None
        );
        assert_eq!(
            decide("0.1.0", r#"{"tag_name":"v9.0.0"}"#).unwrap(),
            None,
            "no assets array at all"
        );
    }

    #[test]
    fn a_payload_that_is_not_a_release_is_an_error_and_not_a_silent_no() {
        assert!(decide("0.1.0", "not json").is_err());
        assert!(decide("0.1.0", r#"{"message":"Not Found"}"#).is_err());
    }

    /// The integrity check, against this repository's own binaries — the strongest
    /// available form of "the thing we publish passes the check we apply".
    #[test]
    fn the_real_gui_binary_passes_and_the_console_one_does_not() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let gui = root.join("target/release/request-pilot.exe");
        let console = root.join("target/release/rp-mcp.exe");
        if !gui.exists() || !console.exists() {
            return; // No release build here; the shape tests below still run.
        }
        assert!(
            looks_like_gui_exe(&std::fs::read(&gui).unwrap()),
            "the shipped app must pass its own check"
        );
        assert!(
            !looks_like_gui_exe(&std::fs::read(&console).unwrap()),
            "rp-mcp is a console program and must be refused"
        );
    }

    #[test]
    fn html_and_truncation_and_emptiness_are_all_refused() {
        assert!(!looks_like_gui_exe(b""));
        assert!(!looks_like_gui_exe(
            b"<html><body>rate limited</body></html>"
        ));
        assert!(!looks_like_gui_exe(b"MZ"), "a header and nothing else");
        // MZ plus a plausible length but no PE signature where it points.
        let mut junk = vec![0u8; 512];
        junk[0] = b'M';
        junk[1] = b'Z';
        junk[0x3C] = 0x80;
        assert!(!looks_like_gui_exe(&junk));
    }

    /// A buffer that passes [`looks_like_gui_exe`], carrying `marker` so one build
    /// can be told from another. Synthetic rather than a copy of the real binary,
    /// so these tests do not need a release build to have happened.
    fn fake_gui_exe(marker: &str) -> Vec<u8> {
        let pe = 0x80usize;
        let mut b = vec![0u8; pe + 4 + 20 + 68 + 2 + 64];
        b[0] = b'M';
        b[1] = b'Z';
        b[0x3C..0x40].copy_from_slice(&(pe as u32).to_le_bytes());
        b[pe..pe + 4].copy_from_slice(b"PE\0\0");
        let sub = pe + 4 + 20 + 68;
        b[sub..sub + 2].copy_from_slice(&2u16.to_le_bytes()); // GUI
        let tail = b.len() - marker.len();
        b[tail..].copy_from_slice(marker.as_bytes());
        b
    }

    /// A directory to play the installation folder, removed at the end.
    struct Dir(PathBuf);

    impl Dir {
        fn new(name: &str) -> Self {
            let p = std::env::temp_dir().join(format!("rp-update-{name}"));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).expect("temp dir");
            Dir(p)
        }
        fn exe(&self) -> PathBuf {
            self.0.join("request-pilot.exe")
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// The mechanism itself: after the swap the exe *is* the new build, and the old
    /// one is still on disk under its own name — which is what makes the next
    /// startup's sweep, and a manual rollback, possible.
    #[test]
    fn swapping_puts_the_new_build_in_place_and_keeps_the_old_one() {
        let dir = Dir::new("swap");
        let exe = dir.exe();
        std::fs::write(&exe, fake_gui_exe("OLD")).unwrap();

        let ran = swap(&exe, &fake_gui_exe("NEW")).expect("swap");
        assert_eq!(ran, exe, "the path to start is the original one");
        assert!(
            std::fs::read(&exe).unwrap().ends_with(b"NEW"),
            "the exe is the new build"
        );
        assert!(
            std::fs::read(displaced(&exe)).unwrap().ends_with(b"OLD"),
            "and the old one is kept"
        );
        assert!(!incoming(&exe).exists(), "no leftover .new.exe");

        sweep(&exe);
        assert!(!displaced(&exe).exists(), "the sweep clears it");
        assert!(exe.exists(), "and does not touch the app");
    }

    /// The check is not advisory: a bad download must leave the working app exactly
    /// where it was, still runnable.
    #[test]
    fn a_download_that_is_not_an_executable_leaves_the_app_untouched() {
        let dir = Dir::new("refuse");
        let exe = dir.exe();
        std::fs::write(&exe, fake_gui_exe("OLD")).unwrap();

        let err = swap(&exe, b"<html>404</html>").expect_err("must refuse");
        assert!(format!("{err:#}").contains("not a Windows GUI executable"));
        assert!(
            std::fs::read(&exe).unwrap().ends_with(b"OLD"),
            "still the working build"
        );
        assert!(!displaced(&exe).exists(), "and nothing was moved aside");
    }

    /// An interrupted earlier attempt leaves files behind; they must not be renamed
    /// into place on the next try.
    #[test]
    fn leftovers_from_a_failed_attempt_are_replaced_rather_than_installed() {
        let dir = Dir::new("leftover");
        let exe = dir.exe();
        std::fs::write(&exe, fake_gui_exe("OLD")).unwrap();
        std::fs::write(incoming(&exe), fake_gui_exe("STALE")).unwrap();
        std::fs::write(displaced(&exe), fake_gui_exe("ANCIENT")).unwrap();

        swap(&exe, &fake_gui_exe("NEW")).expect("swap");
        assert!(std::fs::read(&exe).unwrap().ends_with(b"NEW"));
        assert!(
            std::fs::read(displaced(&exe)).unwrap().ends_with(b"OLD"),
            "the displaced build is the one that was running, not the ancient one"
        );
    }

    /// A one-shot HTTP server: for each expected request, match a fragment of the
    /// request line and answer with that status and body.
    ///
    /// Enough to exercise the real reqwest path — status, headers, Content-Length —
    /// without reaching GitHub, so the tests are offline and deterministic.
    ///
    /// The request **must** be read before answering. Closing a socket with unread
    /// bytes still in its receive buffer sends an RST rather than a FIN, and the
    /// client then reports a send failure instead of reading the response — which is
    /// exactly how the 404 test failed when it skipped the read.
    fn serve(exchanges: Vec<(&'static str, u16, Vec<u8>)>) -> String {
        use std::io::{Read as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for _ in 0..exchanges.len() {
                let Ok((mut sock, _)) = listener.accept() else {
                    return;
                };
                let mut buf = [0u8; 2048];
                let n = sock.read(&mut buf).unwrap_or(0);
                let head = String::from_utf8_lossy(&buf[..n]).to_string();
                let (status, body) = exchanges
                    .iter()
                    .find(|(m, _, _)| head.contains(m))
                    .map(|(_, s, b)| (*s, b.clone()))
                    .unwrap_or((500, Vec::new()));
                let _ = write!(
                    sock,
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = sock.write_all(&body);
                let _ = sock.flush();
            }
        });
        format!("http://127.0.0.1:{port}")
    }

    /// The whole path with the network mocked: ask, be told a newer version exists,
    /// download it, and install it — ending with the exe on disk being the new
    /// build. Everything except spawning the process and exiting.
    #[test]
    fn the_whole_update_runs_against_a_local_server() {
        let dir = Dir::new("e2e");
        let exe = dir.exe();
        std::fs::write(&exe, fake_gui_exe("OLD")).unwrap();
        let new = fake_gui_exe("NEW");

        // The asset URL has to point back at the same server, so it is built after
        // the port is known — which is why the JSON is assembled here.
        let base = serve(vec![("/download", 200, new.clone())]);
        let json = format!(
            r#"{{"tag_name":"v9.9.9","assets":[{{"name":"{ASSET}","browser_download_url":"{base}/download"}}]}}"#
        );
        let api = serve(vec![("releases/latest", 200, json.into_bytes())]);

        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let found = rt
            .block_on(check_at(&api, "0.1.0"))
            .expect("the check reaches the server")
            .expect("a newer release");
        assert_eq!(found.tag, "v9.9.9");

        let bytes = rt.block_on(download(&found.url)).expect("download");
        assert_eq!(bytes, new, "the bytes arrive intact");

        swap(&exe, &bytes).expect("install");
        assert!(std::fs::read(&exe).unwrap().ends_with(b"NEW"));
    }

    /// What this repository answers today: 404, because it has no release yet. It
    /// must be silence, not an error logged on every launch.
    #[test]
    fn a_repository_with_no_releases_yet_is_not_a_failure() {
        let api = serve(vec![(
            "releases/latest",
            404,
            br#"{"message":"Not Found","status":"404"}"#.to_vec(),
        )]);
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        assert_eq!(
            rt.block_on(check_at(&api, "0.1.0"))
                .expect("404 is not an error"),
            None,
            "no release yet means nothing to offer"
        );
    }

    /// The other half of the mocked check: the server says the current version is
    /// the latest, so nothing is offered and the user sees no chip.
    #[test]
    fn a_server_reporting_the_current_version_offers_nothing() {
        let json = payload("v0.1.0", &[ASSET]);
        let api = serve(vec![("releases/latest", 200, json.into_bytes())]);
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        assert_eq!(rt.block_on(check_at(&api, "0.1.0")).unwrap(), None);
    }

    #[test]
    fn the_side_paths_sit_beside_the_exe_and_keep_its_extension() {
        let exe = Path::new(r"C:\Apps\request-pilot.exe");
        assert_eq!(incoming(exe), Path::new(r"C:\Apps\request-pilot.new.exe"));
        assert_eq!(displaced(exe), Path::new(r"C:\Apps\request-pilot.old.exe"));
        // Same directory, or the renames would cross volumes.
        assert_eq!(incoming(exe).parent(), exe.parent());
        assert_eq!(displaced(exe).parent(), exe.parent());
        // No extension, as on Linux.
        let bare = Path::new("/usr/local/bin/request-pilot");
        assert_eq!(
            incoming(bare),
            Path::new("/usr/local/bin/request-pilot.new")
        );
    }
}
