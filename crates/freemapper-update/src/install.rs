//! Installing an update (spec 3.5 steps 4 to 7): download the `.gz` next to the executable, check its
//! size and SHA-256, unpack it, check the executable's SHA-256, run it, and only then swap it in.
//!
//! Both temp files sit in the executable's folder, so the final rename stays on one file system, and
//! carry this install's process id and start time: `freemapper.update-download-<pid>-<nanos>.gz` and
//! `freemapper.update-new-<pid>-<nanos>` (`….exe` on Windows, so it can run). They are created with
//! `freemapper_fs::create_new_file`, which never follows a link, and removed on every way out; after a
//! success only the swapped-in executable and its previous remain. The install lock
//! (`freemapper.update-lock`) holds a token of this install: a slow install can lose it to a takeover
//! after 15 minutes, so the token is checked again right before the swap, and an install that lost it
//! ends with [`UpdateError::Busy`]. The names are its own, so it never touches the other install's
//! files. Temp files older than 15 minutes, left by killed runs, are swept while the lock is held.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};

use crate::check::Outcome;
use crate::error::UpdateError;
use crate::manifest::Asset;
use crate::source::{App, UpdateSource};
use crate::swap::{VersionRunner, ensure_writable, lock_install, swap_in, temp_names};
use crate::version::Version;

/// The unpacked executable may not be larger than this (512 MiB); a larger one is refused.
pub const MAX_EXE_BYTES: u64 = 512 * 1024 * 1024;

/// Installs `available` in the place of `exe` and returns the installed version (spec 3.5 steps 4 to 7).
///
/// The download must match the manifest's size and SHA-256, the unpacked program its `exe_sha256`,
/// and `runner.version_of` on the unpacked program must report the manifest's version; only then is
/// it swapped in with [`swap_in`]. Real callers pass `ProcessRunner::new(app.name)`; `app.name` also
/// names the program in errors. Any failure before the swap leaves the folder as it was.
/// `Outcome::Current` gives [`UpdateError::NothingToInstall`]; another install or rollback that
/// holds the lock, or took it over during a slow download, gives [`UpdateError::Busy`].
pub fn install(
    app: &App,
    source: &dyn UpdateSource,
    runner: &dyn VersionRunner,
    exe: &Path,
    available: &Outcome,
) -> Result<Version, UpdateError> {
    install_capped(app, source, runner, exe, available, MAX_EXE_BYTES)
}

fn install_capped(
    app: &App,
    source: &dyn UpdateSource,
    runner: &dyn VersionRunner,
    exe: &Path,
    available: &Outcome,
    max_exe_bytes: u64,
) -> Result<Version, UpdateError> {
    let Outcome::Available { version, asset, .. } = available else {
        return Err(UpdateError::NothingToInstall);
    };
    // One install at a time (ruling R12). Declared first, so it is released last, after the temp
    // files are gone.
    let lock = lock_install(exe)?;
    // A folder the user cannot write fails here, before the download.
    ensure_writable(exe)?;
    let (new, download) = temp_names(exe);
    // Declared before any file handle, so it runs after they are closed (Windows cannot remove an open file).
    let _cleanup = RemoveOnDrop([download.clone(), new.clone()]);
    fetch(source, asset, &download)?;
    unpack(&download, &new, &asset.exe_sha256, max_exe_bytes)?;
    let got = runner.version_of(&new).map_err(|e| match e {
        UpdateError::Io(e) => UpdateError::Io(io::Error::new(
            e.kind(),
            format!(
                "the downloaded {} {version} ({}) did not run: {e}",
                app.name,
                new.display()
            ),
        )),
        other => other,
    })?;
    match got.trim().parse::<Version>() {
        Ok(reported) if reported == *version => {}
        _ => {
            return Err(UpdateError::VersionMismatch {
                expected: *version,
                got,
            });
        }
    }
    // A download slower than 15 minutes may have cost the lock; the new holder owns the folder now.
    lock.ensure_ours()?;
    swap_in(exe, &new)?;
    Ok(*version)
}

/// Removes this install's temp files when dropped, ignoring errors (after a success the new file is
/// already gone). It does so even after the lock was lost: the names carry this install's process id
/// and start time, so they are never another install's files.
struct RemoveOnDrop([PathBuf; 2]);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Downloads the asset into `download` and checks its size and SHA-256.
fn fetch(source: &dyn UpdateSource, asset: &Asset, download: &Path) -> Result<(), UpdateError> {
    let mut file = freemapper_fs::create_new_file(download)?;
    source.download_to(&asset.url, asset.size, &mut file)?;
    // The source should refuse a wrong size already; checked here anyway.
    let size = file.metadata()?.len();
    if size != asset.size {
        return Err(UpdateError::Download(format!(
            "expected {} bytes, got {size}",
            asset.size
        )));
    }
    drop(file);
    let mut hasher = Sha256::new();
    let mut reader = File::open(download)?;
    let mut buffer = vec![0; 64 * 1024];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => hasher.update(&buffer[..n]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
    if hex(&hasher.finalize()) != asset.sha256 {
        return Err(UpdateError::Hash { what: "download" });
    }
    Ok(())
}

/// Unpacks `download` into `new`, refusing more than `max_bytes`, checks the SHA-256 of the result and
/// makes it executable (Unix). The file is closed on return, so it can run.
fn unpack(
    download: &Path,
    new: &Path,
    exe_sha256: &str,
    max_bytes: u64,
) -> Result<(), UpdateError> {
    let mut decoder = GzDecoder::new(File::open(download)?);
    let mut output = freemapper_fs::create_new_file(new)?;
    let mut hasher = Sha256::new();
    let mut written: u64 = 0;
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let n = match decoder.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => {
                return Err(UpdateError::Download(format!(
                    "cannot unpack the download: {e}"
                )));
            }
        };
        written += n as u64;
        if written > max_bytes {
            return Err(UpdateError::Download(format!(
                "the unpacked program is larger than {max_bytes} bytes"
            )));
        }
        hasher.update(&buffer[..n]);
        output.write_all(&buffer[..n])?;
    }
    if hex(&hasher.finalize()) != exe_sha256 {
        return Err(UpdateError::Hash { what: "program" });
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // On the open handle, so no path is followed.
        output.set_permissions(std::fs::Permissions::from_mode(0o755))?;
    }
    output.sync_all()?;
    Ok(())
}

/// Lower-case hex, as in the manifest.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(64), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;

    use flate2::Compression;
    use flate2::write::GzEncoder;
    use sha2::{Digest, Sha256};

    use super::*;
    use crate::manifest::Asset;
    use crate::swap::previous_path;

    const EXE: &str = if cfg!(windows) {
        "freemapper.exe"
    } else {
        "freemapper"
    };
    const SUFFIX: &str = if cfg!(windows) { ".exe" } else { "" };
    const LOCK: &str = "freemapper.update-lock";
    const URL: &str = "https://github.com/HenryJobst/freemapper/releases/download/v0.12.0/x.gz";

    fn sha256_hex(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    fn gzip(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    /// The `.gz` of `program` and the manifest asset that describes it truthfully.
    fn release(program: &[u8]) -> (Vec<u8>, Asset) {
        let gz = gzip(program);
        let asset = Asset {
            url: URL.to_string(),
            size: gz.len() as u64,
            sha256: sha256_hex(&gz),
            exe_sha256: sha256_hex(program),
        };
        (gz, asset)
    }

    fn available(version: &str, asset: Asset) -> Outcome {
        Outcome::Available {
            version: version.parse().unwrap(),
            released: "2026-11-20".into(),
            notes: "https://github.com/HenryJobst/freemapper/releases/tag/v0.12.0".into(),
            asset,
        }
    }

    fn app() -> App {
        App {
            name: "freemapper",
            repo_url: "https://github.com/HenryJobst/freemapper",
            keys: &[],
            current: "0.11.0".parse().unwrap(),
        }
    }

    /// Writes its body into the file, whatever the size it was asked for (to test the size checks).
    struct FakeSource {
        body: Result<Vec<u8>, ()>,
        calls: RefCell<usize>,
    }

    impl FakeSource {
        fn serving(body: Vec<u8>) -> Self {
            FakeSource {
                body: Ok(body),
                calls: RefCell::new(0),
            }
        }
    }

    impl UpdateSource for FakeSource {
        fn get(&self, _url: &str, _max_bytes: u64) -> Result<Vec<u8>, UpdateError> {
            unreachable!("the install fetches nothing into memory")
        }

        fn download_to(
            &self,
            url: &str,
            _size: u64,
            file: &mut fs::File,
        ) -> Result<(), UpdateError> {
            *self.calls.borrow_mut() += 1;
            assert_eq!(url, URL);
            match &self.body {
                Ok(body) => Ok(file.write_all(body)?),
                Err(()) => Err(UpdateError::Download("connection reset".into())),
            }
        }
    }

    /// Answers `version_of` with a fixed text and notes the path it was asked about and what the
    /// executable held at that moment (to show the run comes before the swap).
    struct FakeRunner {
        answer: Result<&'static str, ()>,
        /// Per call: the path, the executable's bytes and whether the install lock was held.
        asked: RefCell<Vec<(PathBuf, Vec<u8>, bool)>>,
        exe: PathBuf,
    }

    impl VersionRunner for FakeRunner {
        fn version_of(&self, path: &Path) -> Result<String, UpdateError> {
            let exe_now = fs::read(&self.exe).unwrap();
            let locked = self.exe.with_file_name(LOCK).exists();
            self.asked
                .borrow_mut()
                .push((path.to_path_buf(), exe_now, locked));
            self.answer
                .map(str::to_string)
                .map_err(|()| UpdateError::Io(std::io::Error::other("does not run")))
        }
    }

    fn runner(exe: &Path, answer: Result<&'static str, ()>) -> FakeRunner {
        FakeRunner {
            answer,
            asked: RefCell::new(Vec::new()),
            exe: exe.to_path_buf(),
        }
    }

    /// Plays install B while A asks for the version: takes A's aged lock over, as B would after A's
    /// download outlasted 15 minutes, and leaves B's own half-written temp files. Answers like a good
    /// program, so A goes on to the swap.
    struct TakeoverRunner {
        exe: PathBuf,
        second: RefCell<Option<crate::swap::InstallLock>>,
    }

    impl VersionRunner for TakeoverRunner {
        fn version_of(&self, _: &Path) -> Result<String, UpdateError> {
            *self.second.borrow_mut() = Some(crate::swap::take_over_lock(&self.exe));
            for name in [
                "freemapper.update-download-9-9.gz".to_string(),
                format!("freemapper.update-new-9-9{SUFFIX}"),
            ] {
                fs::write(self.exe.with_file_name(name), b"B's half-written file").unwrap();
            }
            Ok("0.12.0".into())
        }
    }

    /// An executable holding `old program` in a fresh folder.
    fn setup() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join(EXE);
        fs::write(&exe, b"old program").unwrap();
        (dir, exe)
    }

    /// Every entry of `dir` with its bytes, sorted by name.
    fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
        let mut entries: Vec<(String, Vec<u8>)> = fs::read_dir(dir)
            .unwrap()
            .map(|e| {
                let e = e.unwrap();
                (
                    e.file_name().to_string_lossy().into_owned(),
                    fs::read(e.path()).unwrap(),
                )
            })
            .collect();
        entries.sort();
        entries
    }

    fn names(dir: &Path) -> Vec<String> {
        snapshot(dir).into_iter().map(|(name, _)| name).collect()
    }

    /// Runs an install that must fail and asserts the folder is exactly as before.
    fn fails_and_changes_nothing(
        source: &FakeSource,
        runner: &FakeRunner,
        exe: &Path,
        outcome: &Outcome,
    ) -> UpdateError {
        let dir = exe.parent().unwrap();
        let before = snapshot(dir);
        let error = install(&app(), source, runner, exe, outcome).unwrap_err();
        assert_eq!(snapshot(dir), before, "{error}");
        error
    }

    #[test]
    fn install_replaces_and_keeps_previous() {
        let (dir, exe) = setup();
        let (gz, asset) = release(b"new program 0.12.0");
        let source = FakeSource::serving(gz);
        let runner = runner(&exe, Ok("0.12.0"));
        let installed =
            install(&app(), &source, &runner, &exe, &available("0.12.0", asset)).unwrap();
        assert_eq!(installed, "0.12.0".parse().unwrap());
        assert_eq!(fs::read(&exe).unwrap(), b"new program 0.12.0");
        assert_eq!(fs::read(previous_path(&exe)).unwrap(), b"old program");
        let previous = format!("freemapper.previous{SUFFIX}");
        assert_eq!(names(dir.path()), vec![EXE.to_string(), previous]);
        // The new program ran from its temp name while the old one was still in place.
        let asked = runner.asked.borrow();
        assert_eq!(asked.len(), 1);
        let (ran, exe_then, locked) = &asked[0];
        assert!(locked, "the install lock is held during the run");
        assert_eq!(ran.parent(), Some(dir.path()));
        let ran = ran.file_name().unwrap().to_str().unwrap();
        assert!(
            ran.starts_with(&format!("freemapper.update-new-{}-", std::process::id()))
                && ran.ends_with(SUFFIX),
            "{ran}"
        );
        assert_eq!(exe_then, b"old program");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&exe).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o755);
        }
    }

    #[test]
    fn current_is_refused() {
        let (dir, exe) = setup();
        let source = FakeSource::serving(Vec::new());
        let runner = runner(&exe, Ok("0.12.0"));
        let error = fails_and_changes_nothing(&source, &runner, &exe, &Outcome::Current);
        assert!(matches!(error, UpdateError::NothingToInstall), "{error}");
        assert_eq!(*source.calls.borrow(), 0);
        assert_eq!(names(dir.path()), vec![EXE.to_string()]);
    }

    #[test]
    fn wrong_gz_hash_changes_nothing() {
        let (_dir, exe) = setup();
        let (gz, mut asset) = release(b"new program");
        asset.sha256 = sha256_hex(b"something else");
        let runner = runner(&exe, Ok("0.12.0"));
        let error = fails_and_changes_nothing(
            &FakeSource::serving(gz),
            &runner,
            &exe,
            &available("0.12.0", asset),
        );
        assert!(
            matches!(error, UpdateError::Hash { what: "download" }),
            "{error}"
        );
        assert!(runner.asked.borrow().is_empty());
    }

    /// The review focus: one flipped byte in the download, with an old previous and a stale file
    /// around. Every file keeps its name and bytes, and no temp file stays.
    #[test]
    fn tampered_download_changes_nothing() {
        let (_dir, exe) = setup();
        fs::write(previous_path(&exe), b"older program").unwrap();
        fs::write(
            exe.with_file_name(format!("freemapper.stale-1{SUFFIX}")),
            b"stale",
        )
        .unwrap();
        let (mut gz, asset) = release(b"new program");
        let last = gz.len() - 1;
        gz[last] ^= 0x01;
        let runner = runner(&exe, Ok("0.12.0"));
        let error = fails_and_changes_nothing(
            &FakeSource::serving(gz),
            &runner,
            &exe,
            &available("0.12.0", asset),
        );
        assert!(
            matches!(error, UpdateError::Hash { what: "download" }),
            "{error}"
        );
        assert_eq!(fs::read(&exe).unwrap(), b"old program");
        assert!(runner.asked.borrow().is_empty());
    }

    #[test]
    fn wrong_exe_hash_changes_nothing() {
        let (_dir, exe) = setup();
        let (gz, mut asset) = release(b"new program");
        asset.exe_sha256 = sha256_hex(b"another program");
        let runner = runner(&exe, Ok("0.12.0"));
        let error = fails_and_changes_nothing(
            &FakeSource::serving(gz),
            &runner,
            &exe,
            &available("0.12.0", asset),
        );
        assert!(
            matches!(error, UpdateError::Hash { what: "program" }),
            "{error}"
        );
        assert!(runner.asked.borrow().is_empty());
    }

    #[test]
    fn size_larger_than_manifest_is_refused() {
        let (_dir, exe) = setup();
        let (gz, asset) = release(b"new program");
        let size = asset.size;
        let mut longer = gz.clone();
        longer.extend_from_slice(b"tail");
        let runner = runner(&exe, Ok("0.12.0"));
        let error = fails_and_changes_nothing(
            &FakeSource::serving(longer),
            &runner,
            &exe,
            &available("0.12.0", asset.clone()),
        );
        let UpdateError::Download(message) = &error else {
            panic!("expected Download, got {error:?}");
        };
        assert!(message.contains(&size.to_string()), "{message}");
        assert!(message.contains(&(size + 4).to_string()), "{message}");
        // Shorter is refused the same way.
        let error = fails_and_changes_nothing(
            &FakeSource::serving(gz[..gz.len() - 1].to_vec()),
            &runner,
            &exe,
            &available("0.12.0", asset),
        );
        assert!(matches!(error, UpdateError::Download(_)), "{error}");
        assert!(runner.asked.borrow().is_empty());
    }

    #[test]
    fn unpacked_program_over_the_cap_is_refused() {
        let (dir, exe) = setup();
        let program = vec![0u8; 1000];
        let (gz, asset) = release(&program);
        let runner = runner(&exe, Ok("0.12.0"));
        let before = snapshot(dir.path());
        let outcome = available("0.12.0", asset);
        let source = FakeSource::serving(gz.clone());
        let error = install_capped(&app(), &source, &runner, &exe, &outcome, 999).unwrap_err();
        assert!(matches!(error, UpdateError::Download(_)), "{error}");
        assert_eq!(snapshot(dir.path()), before);
        // Exactly at the cap is fine.
        install_capped(&app(), &source, &runner, &exe, &outcome, 1000).unwrap();
        assert_eq!(fs::read(&exe).unwrap(), program);
    }

    #[test]
    fn version_mismatch_changes_nothing() {
        let (_dir, exe) = setup();
        let (gz, asset) = release(b"new program");
        let outcome = available("0.12.0", asset);
        for answer in ["0.11.5", "0.12.0-rc1", "not a version"] {
            let runner = runner(&exe, Ok(answer));
            let error = fails_and_changes_nothing(
                &FakeSource::serving(gz.clone()),
                &runner,
                &exe,
                &outcome,
            );
            match error {
                UpdateError::VersionMismatch { expected, got } => {
                    assert_eq!(expected, "0.12.0".parse().unwrap());
                    assert_eq!(got, answer);
                }
                other => panic!("expected VersionMismatch, got {other:?}"),
            }
            assert_eq!(runner.asked.borrow().len(), 1);
        }
    }

    #[test]
    fn temp_files_are_gone_after_failure() {
        let (dir, exe) = setup();
        let (gz, asset) = release(b"new program");
        let outcome = available("0.12.0", asset.clone());
        // The source fails mid-way.
        let failing = FakeSource {
            body: Err(()),
            calls: RefCell::new(0),
        };
        let error =
            fails_and_changes_nothing(&failing, &runner(&exe, Ok("0.12.0")), &exe, &outcome);
        assert!(matches!(error, UpdateError::Download(_)), "{error}");
        // The new program does not run.
        let error = fails_and_changes_nothing(
            &FakeSource::serving(gz),
            &runner(&exe, Err(())),
            &exe,
            &outcome,
        );
        let text = error.to_string();
        assert!(matches!(error, UpdateError::Io(_)), "{error}");
        assert!(text.contains("the downloaded freemapper 0.12.0"), "{text}");
        assert!(text.contains("did not run"), "{text}");
        assert!(text.contains("freemapper.update-new-"), "{text}");
        // A download that matches the manifest but is no gzip.
        let junk = b"not gzip at all".to_vec();
        let junk_asset = Asset {
            size: junk.len() as u64,
            sha256: sha256_hex(&junk),
            ..asset
        };
        let error = fails_and_changes_nothing(
            &FakeSource::serving(junk),
            &runner(&exe, Ok("0.12.0")),
            &exe,
            &available("0.12.0", junk_asset),
        );
        assert!(matches!(error, UpdateError::Download(_)), "{error}");
        assert_eq!(names(dir.path()), vec![EXE.to_string()]);
    }

    /// Temp files of killed installs older than 15 minutes are swept while the lock is held; younger
    /// ones may belong to a slow install that lost its lock and stay, as do links and other apps' files.
    #[test]
    fn leftovers_older_than_15_minutes_are_swept() {
        let (dir, exe) = setup();
        let outside = tempfile::tempdir().unwrap();
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(16 * 60);
        let leftover = |name: &str, aged: bool| {
            let file = fs::File::create(dir.path().join(name)).unwrap();
            if aged {
                file.set_modified(old).unwrap();
            }
        };
        leftover(&format!("freemapper.update-new-1-2{SUFFIX}"), true);
        leftover("freemapper.update-download-1-2.gz", true);
        leftover(&format!("freemapper.update-new-3-4{SUFFIX}"), false);
        leftover("freemapper.update-download-3-4.gz", false);
        leftover("freemapper-print.update-new-1-2", true);
        let target = outside.path().join("target");
        fs::write(&target, b"target").unwrap();
        fs::File::options()
            .write(true)
            .open(&target)
            .unwrap()
            .set_modified(old)
            .unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, dir.path().join("freemapper.update-new-5-6")).unwrap();
        let (gz, asset) = release(b"new program");
        let runner = runner(&exe, Ok("0.12.0"));
        install(
            &app(),
            &FakeSource::serving(gz),
            &runner,
            &exe,
            &available("0.12.0", asset),
        )
        .unwrap();
        assert_eq!(fs::read(&exe).unwrap(), b"new program");
        let mut expected = vec![
            EXE.to_string(),
            "freemapper-print.update-new-1-2".to_string(),
            format!("freemapper.previous{SUFFIX}"),
            "freemapper.update-download-3-4.gz".to_string(),
            format!("freemapper.update-new-3-4{SUFFIX}"),
        ];
        if cfg!(unix) {
            expected.push("freemapper.update-new-5-6".to_string());
        }
        expected.sort();
        let mut got: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        got.sort();
        assert_eq!(got, expected);
        assert_eq!(fs::read(&target).unwrap(), b"target");
    }

    /// Review finding 1: install A is slow, its lock ages past 15 minutes, and install B takes it
    /// over. A must end with `Busy` before swapping and leave B's lock and files alone.
    #[test]
    fn install_that_lost_its_lock_is_busy_before_the_swap() {
        let (dir, exe) = setup();
        fs::write(previous_path(&exe), b"older program").unwrap();
        let (gz, asset) = release(b"new program");
        let source = FakeSource::serving(gz);
        let runner = TakeoverRunner {
            exe: exe.clone(),
            second: RefCell::new(None),
        };
        let error =
            install(&app(), &source, &runner, &exe, &available("0.12.0", asset)).unwrap_err();
        match &error {
            UpdateError::Busy(path) => assert_eq!(path, &dir.path().join(LOCK)),
            other => panic!("expected Busy, got {other:?}"),
        }
        assert_eq!(fs::read(&exe).unwrap(), b"old program");
        assert_eq!(fs::read(previous_path(&exe)).unwrap(), b"older program");
        // B's lock and its temp files are still there; A's own temp files are gone.
        let b_files = [
            "freemapper.update-download-9-9.gz".to_string(),
            format!("freemapper.update-new-9-9{SUFFIX}"),
        ];
        let mut expected = vec![
            EXE.to_string(),
            format!("freemapper.previous{SUFFIX}"),
            LOCK.to_string(),
        ];
        expected.extend(b_files.iter().cloned());
        expected.sort();
        assert_eq!(names(dir.path()), expected);
        for name in &b_files {
            assert_eq!(
                fs::read(dir.path().join(name)).unwrap(),
                b"B's half-written file"
            );
        }
        // Dropping B's guard removes the lock: A left B's token in it.
        drop(runner.second.borrow_mut().take());
        assert!(!dir.path().join(LOCK).exists());
    }

    /// The program may be renamed: the runner expects the app's name, not the file's.
    #[cfg(unix)]
    #[test]
    fn renamed_executable_updates() {
        use crate::swap::ProcessRunner;
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("fm");
        fs::write(&exe, b"old program").unwrap();
        let good = b"#!/bin/sh\necho \"freemapper 0.12.0\"\n";
        let (gz, asset) = release(good);
        let installed = install(
            &app(),
            &FakeSource::serving(gz),
            &ProcessRunner::new("freemapper"),
            &exe,
            &available("0.12.0", asset),
        )
        .unwrap();
        assert_eq!(installed, "0.12.0".parse().unwrap());
        assert_eq!(fs::read(&exe).unwrap(), good);
        assert_eq!(
            fs::read(dir.path().join("fm.previous")).unwrap(),
            b"old program"
        );
    }

    /// Finding 1 of the review: a second install while one runs would replace the first one's verified
    /// file with its own half-written one. The lock refuses it before anything is touched.
    #[test]
    fn concurrent_install_is_refused_while_locked() {
        let (dir, exe) = setup();
        let lock = dir.path().join(LOCK);
        fs::write(&lock, b"").unwrap();
        let (gz, asset) = release(b"new program");
        let source = FakeSource::serving(gz);
        let runner = runner(&exe, Ok("0.12.0"));
        let error = fails_and_changes_nothing(&source, &runner, &exe, &available("0.12.0", asset));
        match &error {
            UpdateError::Busy(path) => assert_eq!(path, &lock),
            other => panic!("expected Busy, got {other:?}"),
        }
        assert!(
            error.to_string().contains("another update is running"),
            "{error}"
        );
        assert_eq!(*source.calls.borrow(), 0);
        assert!(runner.asked.borrow().is_empty());
    }

    /// A lock older than 15 minutes is left by a crash and is taken over.
    #[test]
    fn stale_lock_is_taken_over() {
        let (dir, exe) = setup();
        let lock = dir.path().join(LOCK);
        let file = fs::File::create(&lock).unwrap();
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(16 * 60);
        file.set_modified(old).unwrap();
        drop(file);
        let (gz, asset) = release(b"new program");
        let runner = runner(&exe, Ok("0.12.0"));
        install(
            &app(),
            &FakeSource::serving(gz),
            &runner,
            &exe,
            &available("0.12.0", asset),
        )
        .unwrap();
        assert_eq!(fs::read(&exe).unwrap(), b"new program");
        let previous = format!("freemapper.previous{SUFFIX}");
        assert_eq!(names(dir.path()), vec![EXE.to_string(), previous]);
    }

    #[test]
    fn lock_is_released_after_success_and_after_failure() {
        let (dir, exe) = setup();
        let (gz, asset) = release(b"new program");
        let mut wrong = asset.clone();
        wrong.exe_sha256 = sha256_hex(b"another program");
        let error = install(
            &app(),
            &FakeSource::serving(gz.clone()),
            &runner(&exe, Ok("0.12.0")),
            &exe,
            &available("0.12.0", wrong),
        )
        .unwrap_err();
        assert!(
            matches!(error, UpdateError::Hash { what: "program" }),
            "{error}"
        );
        assert_eq!(names(dir.path()), vec![EXE.to_string()]);
        let runner = runner(&exe, Ok("0.12.0"));
        install(
            &app(),
            &FakeSource::serving(gz),
            &runner,
            &exe,
            &available("0.12.0", asset),
        )
        .unwrap();
        assert!(
            runner.asked.borrow()[0].2,
            "the lock is held during the run"
        );
        assert!(!dir.path().join(LOCK).exists());
        // And a second install can take it again.
        let (gz, asset) = release(b"newer program");
        install(
            &app(),
            &FakeSource::serving(gz),
            &runner,
            &exe,
            &available("0.12.0", asset),
        )
        .unwrap();
        assert_eq!(fs::read(&exe).unwrap(), b"newer program");
        assert!(!dir.path().join(LOCK).exists());
    }

    /// Finding 5 of the review: a folder the user cannot write stops the install before the download.
    #[cfg(unix)]
    #[test]
    fn read_only_folder_is_not_writable_and_downloads_nothing() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, _) = setup();
        let folder = dir.path().join("app");
        fs::create_dir(&folder).unwrap();
        let exe = folder.join(EXE);
        fs::write(&exe, b"old program").unwrap();
        let before = snapshot(&folder);
        let (gz, asset) = release(b"new program");
        let source = FakeSource::serving(gz);
        let runner = runner(&exe, Ok("0.12.0"));
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o555)).unwrap();
        let privileged = fs::write(folder.join("privilege-check"), "").is_ok();
        let result = (!privileged)
            .then(|| install(&app(), &source, &runner, &exe, &available("0.12.0", asset)));
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o755)).unwrap();
        let Some(result) = result else {
            eprintln!("skipped: this user can write into a 0o555 folder (root)");
            return;
        };
        match result {
            Err(UpdateError::NotWritable(path)) => assert_eq!(path, folder),
            other => panic!("expected NotWritable, got {other:?}"),
        }
        assert_eq!(*source.calls.borrow(), 0);
        assert_eq!(snapshot(&folder), before);
    }

    /// End to end with the real runner: the new program is a shell script that prints its version.
    #[cfg(unix)]
    #[test]
    fn install_runs_the_new_program_before_the_swap() {
        use crate::swap::ProcessRunner;
        let (dir, exe) = setup();
        let good = b"#!/bin/sh\necho \"freemapper 0.12.0\"\n";
        let (gz, asset) = release(good);
        let installed = install(
            &app(),
            &FakeSource::serving(gz),
            &ProcessRunner::new("freemapper"),
            &exe,
            &available("0.12.0", asset),
        )
        .unwrap();
        assert_eq!(installed, "0.12.0".parse().unwrap());
        assert_eq!(fs::read(&exe).unwrap(), good);
        assert_eq!(
            ProcessRunner::new("freemapper").version_of(&exe).unwrap(),
            "0.12.0"
        );
        // A script that reports another version never takes the place of the working one.
        let wrong = b"#!/bin/sh\necho \"freemapper 0.12.1\"\n";
        let (gz, asset) = release(wrong);
        let before = snapshot(dir.path());
        let error = install(
            &app(),
            &FakeSource::serving(gz),
            &ProcessRunner::new("freemapper"),
            &exe,
            &available("0.13.0", asset),
        )
        .unwrap_err();
        assert!(
            matches!(&error, UpdateError::VersionMismatch { got, .. } if got == "0.12.1"),
            "{error}"
        );
        assert_eq!(snapshot(dir.path()), before);
    }
}
