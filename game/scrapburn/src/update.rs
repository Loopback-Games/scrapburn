//! In-game updates from GitHub Releases.
//!
//! A packaged install (one with a [`MARKER`] file beside the executable) asks GitHub
//! for the latest release once per launch. When the player accepts, the archive for
//! this platform is downloaded over HTTPS, checked against the release's
//! `SHA256SUMS`, unpacked beside the install, and swapped in by renaming: the files
//! being replaced move into `.update/old` and are deleted on the next launch. Renaming
//! works on a running executable on both Linux and Windows, so the game can install
//! the update itself and then start the new copy.

mod zip;

use std::fmt;
use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use sha2::{Digest, Sha256};

pub use zip::ZipError;

/// Written into every packaged install by `tools/package.py`. Builds run from a
/// source checkout have no marker and never look for updates.
pub const MARKER: &str = "scrapburn.release";
/// Set to any value to switch the updater off.
pub const DISABLE_ENV: &str = "SCRAPBURN_NO_UPDATE";
const SUMS: &str = "SHA256SUMS";
/// Staging directory inside the install.
const STAGING: &str = ".update";
const MAX_ARCHIVE: u64 = 2 << 30;
const MAX_UNPACKED: u64 = 4 << 30;
const MAX_METADATA: u64 = 1 << 20;
/// The whole update check, start to finish. Offline, it gives up quickly and the
/// title screen simply never offers an update; play is never blocked on it.
const CHECK_TIMEOUT: Duration = Duration::from_secs(8);
/// Looking up and connecting to GitHub, for the check and the download alike.
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(3);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
/// A stalled download gives up after this long without data.
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// A release version, `vMAJOR.MINOR.PATCH`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u64, pub u64, pub u64);

impl Version {
    /// The version this binary was built as.
    pub fn current() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).expect("Cargo versions are MAJOR.MINOR.PATCH")
    }

    /// Parses `1.2.3` or `v1.2.3`. Pre-release and build suffixes are rejected so a
    /// test tag is never offered as an update.
    pub fn parse(s: &str) -> Option<Self> {
        let mut parts = s.strip_prefix('v').unwrap_or(s).split('.');
        let mut next = || {
            parts
                .next()
                .filter(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))?
                .parse()
                .ok()
        };
        let v = Self(next()?, next()?, next()?);
        parts.next().is_none().then_some(v)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{}.{}.{}", self.0, self.1, self.2)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("can't reach GitHub: {0}")]
    Http(String),
    #[error("the release is missing {0}")]
    Incomplete(&'static str),
    #[error("the download failed its checksum")]
    Checksum,
    #[error(transparent)]
    Archive(#[from] ZipError),
    #[error("can't write to the install folder: {0}")]
    Io(#[from] io::Error),
    #[error("unexpected reply from GitHub")]
    BadReply,
}

impl From<ureq::Error> for UpdateError {
    fn from(e: ureq::Error) -> Self {
        Self::Http(e.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Asset {
    url: String,
    size: u64,
    /// GitHub's own `sha256:` digest of the upload, when the API reports one.
    digest: Option<String>,
}

/// A newer release with an archive for this platform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    tag: String,
    archive_name: String,
    archive: Asset,
    sums: Asset,
}

/// News from a background check or install.
#[derive(Debug)]
pub enum Event {
    /// A newer release is available.
    Available(Release),
    /// Already on the latest release (or none is published yet).
    Current,
    /// Download progress, 0 to 100.
    Progress(u8),
    /// Installed. Start this executable and exit.
    Installed(PathBuf),
    Failed(UpdateError),
}

/// `owner/repo` on GitHub, from the crate's `repository` field.
pub fn repository() -> &'static str {
    env!("CARGO_PKG_REPOSITORY")
        .trim_start_matches("https://github.com/")
        .trim_end_matches('/')
}

/// The install directory, if this is a packaged install that may update itself.
/// These checks are all local, so a launch that will not update never touches the
/// network.
pub fn install_dir() -> Option<PathBuf> {
    if std::env::var_os(DISABLE_ENV).is_some() || archive_name("v0.0.0").is_none() {
        return None;
    }
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    let dir = exe.parent()?;
    dir.join(MARKER).is_file().then(|| dir.to_path_buf())
}

/// The archive `tools/package.py` names for `tag` on this platform.
pub fn archive_name(tag: &str) -> Option<String> {
    let os = match std::env::consts::OS {
        "linux" => "linux",
        "windows" => "windows",
        "macos" => "darwin",
        _ => return None,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        _ => return None,
    };
    Some(format!("scrapburn-{tag}-{os}-{arch}.zip"))
}

/// Checks for a newer release on a background thread.
pub fn spawn_check(dir: PathBuf) -> Receiver<Event> {
    spawn(move |_| {
        // The previous update's leftovers, now that the old executable has exited.
        let _ = fs::remove_dir_all(dir.join(STAGING));
        Ok(match check(Version::current())? {
            Some(release) => Event::Available(release),
            None => Event::Current,
        })
    })
}

/// Downloads and installs `release` into `dir` on a background thread.
pub fn spawn_install(release: Release, dir: PathBuf) -> Receiver<Event> {
    spawn(move |tx| {
        let progress = |p| {
            let _ = tx.send(Event::Progress(p));
        };
        install(&release, &dir, &progress).map(Event::Installed)
    })
}

fn spawn(
    work: impl FnOnce(&Sender<Event>) -> Result<Event, UpdateError> + Send + 'static,
) -> Receiver<Event> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let event = work(&tx).unwrap_or_else(Event::Failed);
        let _ = tx.send(event);
    });
    rx
}

/// An HTTPS-only client. `total` caps a whole request, body included.
fn agent(total: Option<Duration>) -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .unversioned_rustls_crypto_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .build();
    ureq::config::Config::builder()
        .https_only(true)
        .tls_config(tls)
        .user_agent(concat!("scrapburn/", env!("CARGO_PKG_VERSION")))
        .timeout_global(total)
        .timeout_resolve(Some(RESOLVE_TIMEOUT))
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_per_read(Some(READ_TIMEOUT))
        .build()
        .new_agent()
}

/// The latest published release, if it is newer than `current` and has an archive
/// for this platform.
pub fn check(current: Version) -> Result<Option<Release>, UpdateError> {
    let url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        repository()
    );
    let response = agent(Some(CHECK_TIMEOUT))
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .call();
    let mut response = match response {
        // No release has been published yet.
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        other => other?,
    };
    let reader = response
        .body_mut()
        .with_config()
        .limit(MAX_METADATA)
        .reader();
    let json: serde_json::Value =
        serde_json::from_reader(reader).map_err(|_| UpdateError::BadReply)?;
    let Some(name) = json["tag_name"].as_str().and_then(archive_name) else {
        return Ok(None);
    };
    parse_release(&json, current, &name)
}

fn parse_release(
    json: &serde_json::Value,
    current: Version,
    archive_name: &str,
) -> Result<Option<Release>, UpdateError> {
    if json["draft"].as_bool() != Some(false) || json["prerelease"].as_bool() != Some(false) {
        return Ok(None);
    }
    let tag = json["tag_name"].as_str().ok_or(UpdateError::BadReply)?;
    let Some(version) = Version::parse(tag).filter(|v| *v > current) else {
        return Ok(None);
    };
    let assets = json["assets"].as_array().ok_or(UpdateError::BadReply)?;
    let find = |wanted: &str| {
        assets
            .iter()
            .find(|a| a["name"].as_str() == Some(wanted))
            .and_then(|a| {
                let url = a["browser_download_url"].as_str()?;
                let release_path =
                    format!("https://github.com/{}/releases/download/", repository());
                url.starts_with(&release_path).then(|| Asset {
                    url: url.to_string(),
                    size: a["size"].as_u64().unwrap_or(0),
                    digest: a["digest"]
                        .as_str()
                        .and_then(|d| d.strip_prefix("sha256:"))
                        .map(str::to_ascii_lowercase),
                })
            })
    };
    let Some(archive) = find(archive_name) else {
        // Nothing published for this platform.
        return Ok(None);
    };
    let sums = find(SUMS).ok_or(UpdateError::Incomplete(SUMS))?;
    Ok(Some(Release {
        version,
        tag: tag.to_string(),
        archive_name: archive_name.to_string(),
        archive,
        sums,
    }))
}

/// Downloads, verifies and swaps in `release`. Returns the new executable.
pub fn install(
    release: &Release,
    dir: &Path,
    progress: &dyn Fn(u8),
) -> Result<PathBuf, UpdateError> {
    let staging = dir.join(STAGING);
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;
    // No overall cap: a large download on a slow link is fine while data flows.
    let agent = agent(None);
    let mut sums = String::new();
    agent
        .get(&release.sums.url)
        .call()?
        .body_mut()
        .with_config()
        .limit(MAX_METADATA)
        .reader()
        .read_to_string(&mut sums)?;
    let expected = expected_sum(&sums, &release.archive_name)
        .ok_or(UpdateError::Incomplete("a checksum for this platform"))?;
    if release
        .archive
        .digest
        .as_deref()
        .is_some_and(|d| d != expected)
    {
        return Err(UpdateError::Checksum);
    }
    let archive = staging.join(&release.archive_name);
    let mut response = agent.get(&release.archive.url).call()?;
    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_ARCHIVE)
        .reader();
    let actual = save_hashed(body, &archive, release.archive.size, progress)?;
    if actual != expected {
        return Err(UpdateError::Checksum);
    }
    unpack_and_swap(&archive, &release.tag, dir)
}

/// Unpacks a verified archive and swaps its contents into `dir`.
fn unpack_and_swap(archive: &Path, tag: &str, dir: &Path) -> Result<PathBuf, UpdateError> {
    let staging = dir.join(STAGING);
    let unpacked = staging.join("new");
    fs::create_dir_all(&staging)?;
    zip::extract(archive, &unpacked, MAX_UNPACKED)?;
    let root_name = archive
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_suffix(".zip"))
        .ok_or(UpdateError::Incomplete("the archive name"))?;
    let root = unpacked.join(root_name);
    let exe_name = format!("scrapburn{}", std::env::consts::EXE_SUFFIX);
    let marker = fs::read_to_string(root.join(MARKER)).unwrap_or_default();
    if !root.join(&exe_name).is_file() || marker.trim() != tag {
        return Err(UpdateError::Incomplete("the game"));
    }
    swap(&root, dir, &staging.join("old"))?;
    Ok(dir.join(exe_name))
}

/// Streams `body` to `path`, returning its SHA-256 as lowercase hex.
fn save_hashed(
    mut body: impl Read,
    path: &Path,
    expected_len: u64,
    progress: &dyn Fn(u8),
) -> Result<String, UpdateError> {
    let mut out = BufWriter::new(File::create(path)?);
    let mut hash = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = 0u64;
    let mut shown = u8::MAX;
    loop {
        let n = body.read(&mut buf)?;
        if n == 0 {
            break;
        }
        done += n as u64;
        hash.update(&buf[..n]);
        out.write_all(&buf[..n])?;
        if let Some(p) = done.saturating_mul(100).checked_div(expected_len) {
            let p = p.min(100) as u8;
            if p != shown {
                shown = p;
                progress(p);
            }
        }
    }
    out.flush()?;
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// The checksum `sha256sum` printed for `name`.
fn expected_sum(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, file) = line.split_once(char::is_whitespace)?;
        let file = file.trim_start().trim_start_matches('*');
        (file == name && hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| hash.to_ascii_lowercase())
    })
}

/// Moves every top-level entry of `new` into `dir`, first moving any entry it
/// replaces into `old`. On failure, everything already moved is put back.
fn swap(new: &Path, dir: &Path, old: &Path) -> io::Result<()> {
    fs::create_dir_all(old)?;
    let mut names: Vec<_> = fs::read_dir(new)?
        .map(|e| e.map(|e| e.file_name()))
        .collect::<io::Result<_>>()?;
    names.sort();
    // (from, to) renames done so far, undone in reverse on failure.
    let mut done: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut step = |from: PathBuf, to: PathBuf| -> io::Result<()> {
        fs::rename(&from, &to)?;
        done.push((from, to));
        Ok(())
    };
    let result = names.iter().try_for_each(|name| {
        let target = dir.join(name);
        if target.symlink_metadata().is_ok() {
            step(target.clone(), old.join(name))?;
        }
        step(new.join(name), target)
    });
    if result.is_err() {
        for (from, to) in done.iter().rev() {
            let _ = fs::rename(to, from);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::zip::tests::{Member, build, scratch};
    use super::*;
    use serde_json::json;

    #[test]
    fn versions_parse_and_order() {
        assert_eq!(Version::parse("v1.2.3"), Some(Version(1, 2, 3)));
        assert_eq!(Version::parse("0.10.0"), Some(Version(0, 10, 0)));
        for bad in [
            "",
            "v1.2",
            "1.2.3.4",
            "v1.2.3-rc1",
            "1.x.3",
            "v1..3",
            "+1.2.3",
        ] {
            assert_eq!(Version::parse(bad), None, "{bad}");
        }
        assert!(Version(0, 10, 0) > Version(0, 9, 9));
        assert_eq!(Version(1, 2, 3).to_string(), "v1.2.3");
        assert!(Version::parse(&Version::current().to_string()).is_some());
    }

    #[test]
    fn repository_comes_from_the_cargo_manifest() {
        assert_eq!(repository(), "Loopback-Games/scrapburn");
    }

    fn release_json(tag: &str, assets: &[&str]) -> serde_json::Value {
        let base = format!(
            "https://github.com/{}/releases/download/{tag}",
            repository()
        );
        json!({
            "tag_name": tag, "draft": false, "prerelease": false,
            "assets": assets.iter().map(|n| json!({
                "name": n, "size": 10, "browser_download_url": format!("{base}/{n}"),
                "digest": "sha256:ABCD",
            })).collect::<Vec<_>>(),
        })
    }

    #[test]
    fn offers_only_newer_complete_releases_for_this_platform() {
        let name = "scrapburn-v0.2.0-linux-x86_64.zip";
        let now = Version(0, 1, 0);
        let json = release_json("v0.2.0", &[name, SUMS]);
        let release = parse_release(&json, now, name).unwrap().unwrap();
        assert_eq!(release.version, Version(0, 2, 0));
        assert_eq!(release.archive.digest.as_deref(), Some("abcd"));
        assert_eq!(parse_release(&json, Version(0, 2, 0), name).unwrap(), None);
        let other = release_json("v0.2.0", &["scrapburn-v0.2.0-windows-x86_64.zip", SUMS]);
        assert_eq!(parse_release(&other, now, name).unwrap(), None);
        let unsummed = release_json("v0.2.0", &[name]);
        assert!(matches!(
            parse_release(&unsummed, now, name),
            Err(UpdateError::Incomplete(_))
        ));
        let mut pre = json.clone();
        pre["prerelease"] = json!(true);
        assert_eq!(parse_release(&pre, now, name).unwrap(), None);
        let mut elsewhere = json;
        elsewhere["assets"][0]["browser_download_url"] = json!("https://example.com/x.zip");
        assert_eq!(parse_release(&elsewhere, now, name).unwrap(), None);
    }

    #[test]
    fn finds_the_checksum_line() {
        let a = "a".repeat(64);
        let b = "B".repeat(64);
        let sums = format!("{a}  other.zip\n{b} *game.zip\nshort  game.zip\n");
        assert_eq!(expected_sum(&sums, "game.zip"), Some("b".repeat(64)));
        assert_eq!(expected_sum(&sums, "missing.zip"), None);
    }

    #[test]
    fn hashes_while_saving_and_reports_progress() {
        let dir = scratch("update-hash");
        let seen = std::cell::RefCell::new(Vec::new());
        let sum = save_hashed(&b"abc"[..], &dir.join("f"), 3, &|p| {
            seen.borrow_mut().push(p)
        })
        .unwrap();
        assert_eq!(
            sum,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(seen.into_inner(), vec![100]);
        assert_eq!(fs::read(dir.join("f")).unwrap(), b"abc");
    }

    #[test]
    fn installs_by_swapping_and_keeps_unrelated_files() {
        let dir = scratch("update-swap");
        let exe = format!("scrapburn{}", std::env::consts::EXE_SUFFIX);
        fs::write(dir.join(&exe), b"old game").unwrap();
        fs::write(dir.join(MARKER), b"v0.1.0").unwrap();
        fs::create_dir_all(dir.join("assets")).unwrap();
        fs::write(dir.join("assets/gone.png"), b"old").unwrap();
        fs::write(dir.join("notes.txt"), b"mine").unwrap();
        let root = "scrapburn-v0.2.0-test";
        let exe_path = format!("{root}/{exe}");
        let marker_path = format!("{root}/{MARKER}");
        let asset_path = format!("{root}/assets/new.png");
        let archive = dir.join(format!("{root}.zip"));
        fs::write(
            &archive,
            build(&[
                Member(&exe_path, b"new game", 0o100_755, true),
                Member(&marker_path, b"v0.2.0\n", 0o100_644, false),
                Member(&asset_path, b"new", 0o100_644, false),
            ]),
        )
        .unwrap();
        let installed = unpack_and_swap(&archive, "v0.2.0", &dir).unwrap();
        assert_eq!(installed, dir.join(&exe));
        assert_eq!(fs::read(dir.join(&exe)).unwrap(), b"new game");
        assert!(dir.join("assets/new.png").is_file());
        assert!(
            !dir.join("assets/gone.png").exists(),
            "the old assets folder is replaced"
        );
        assert_eq!(fs::read(dir.join("notes.txt")).unwrap(), b"mine");
        assert_eq!(
            fs::read(dir.join(STAGING).join("old").join(&exe)).unwrap(),
            b"old game"
        );
    }

    #[test]
    fn refuses_an_archive_for_a_different_tag() {
        let dir = scratch("update-tag");
        let exe = format!("scrapburn{}", std::env::consts::EXE_SUFFIX);
        fs::write(dir.join(&exe), b"old game").unwrap();
        let root = "scrapburn-v0.3.0-test";
        let archive = dir.join(format!("{root}.zip"));
        let exe_path = format!("{root}/{exe}");
        let marker_path = format!("{root}/{MARKER}");
        fs::write(
            &archive,
            build(&[
                Member(&exe_path, b"new", 0o100_755, false),
                Member(&marker_path, b"v0.3.0", 0o100_644, false),
            ]),
        )
        .unwrap();
        assert!(matches!(
            unpack_and_swap(&archive, "v0.2.0", &dir),
            Err(UpdateError::Incomplete(_))
        ));
        assert_eq!(fs::read(dir.join(&exe)).unwrap(), b"old game");
    }

    #[test]
    fn a_failed_swap_puts_everything_back() {
        let dir = scratch("update-rollback");
        let new = dir.join("new");
        fs::create_dir_all(new.join("b")).unwrap();
        fs::write(new.join("a"), b"new a").unwrap();
        fs::write(new.join("b/x"), b"new b").unwrap();
        fs::write(dir.join("a"), b"old a").unwrap();
        fs::create_dir_all(dir.join("b")).unwrap();
        fs::write(dir.join("b/y"), b"old b").unwrap();
        // `a` swaps cleanly; moving `b` aside then fails because a non-empty `old/b`
        // is already there (renaming onto it fails on Linux and Windows alike).
        fs::create_dir_all(dir.join("old/b")).unwrap();
        fs::write(dir.join("old/b/occupied"), b"").unwrap();
        assert!(swap(&new, &dir, &dir.join("old")).is_err());
        assert_eq!(fs::read(dir.join("a")).unwrap(), b"old a");
        assert_eq!(fs::read(dir.join("b/y")).unwrap(), b"old b");
        assert_eq!(fs::read(new.join("a")).unwrap(), b"new a");
        assert!(!dir.join("old/a").exists());
    }
}
