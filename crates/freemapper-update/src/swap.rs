//! Swapping the running executable for a new one, going back to the previous one, and removing stale
//! files (spec 3.5 step 7, decision D13).
//!
//! Every step is a rename inside the executable's folder: Windows allows renaming a running executable
//! but not deleting or overwriting it, and on Unix a running process keeps its inode. The replaced
//! executable stays as `freemapper.previous` (`freemapper.previous.exe`) for a rollback; an older
//! previous becomes `freemapper.stale-<unix-seconds>`, which a later start deletes once nothing runs from it.
//! The names follow the executable, so `freemapper-print` gets `freemapper-print.previous`.

use std::ffi::{OsStr, OsString};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime};

use crate::error::UpdateError;
use crate::version::Version;

/// How long `<exe> --version` may take.
const VERSION_TIMEOUT: Duration = Duration::from_secs(10);

/// More output than a version line is never read.
const VERSION_OUTPUT_CAP: u64 = 64 * 1024;

/// Runs an executable and reports the version it prints.
pub trait VersionRunner {
    /// The `x.y.z` that `<exe> --version` prints after the app's name.
    fn version_of(&self, exe: &Path) -> Result<String, UpdateError>;
}

/// Runs `<exe> --version` as a child process with stdin closed and a 10 s timeout, and expects
/// `<name> <x.y.z>` on the first line, `<name>` being the file name up to its first dot
/// (`freemapper.previous.exe` prints `freemapper 0.10.0`).
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessRunner;

impl VersionRunner for ProcessRunner {
    fn version_of(&self, exe: &Path) -> Result<String, UpdateError> {
        version_with_timeout(exe, VERSION_TIMEOUT)
    }
}

fn version_with_timeout(exe: &Path, timeout: Duration) -> Result<String, UpdateError> {
    let mut child = spawn_version(exe)?;
    let mut stdout = child.stdout.take().expect("stdout is piped");
    // A reader thread keeps the pipe drained while the loop below watches the clock.
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let read = (&mut stdout)
            .take(VERSION_OUTPUT_CAP)
            .read_to_end(&mut bytes);
        let _ = sender.send(read.map(|_| bytes));
    });
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "{} --version did not finish within {} s",
                    exe.display(),
                    timeout.as_secs_f32()
                ),
            )
            .into());
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    // A grandchild could keep the pipe open; do not wait for it forever.
    let bytes = receiver
        .recv_timeout(Duration::from_secs(1))
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                format!("{} --version left its output open", exe.display()),
            )
        })??;
    let text = String::from_utf8_lossy(&bytes);
    let name = app_name(exe);
    match parse_version_line(&name, &text) {
        Some(version) if status.success() => Ok(version),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} --version printed {:?} ({status}), expected \"{name} x.y.z\"",
                exe.display(),
                text.lines().next().unwrap_or("")
            ),
        )
        .into()),
    }
}

/// Starts `<exe> --version`. Linux refuses to run a file that another thread still has open for
/// writing (it was just written, and a fork elsewhere may hold the handle a moment), so that is retried.
fn spawn_version(exe: &Path) -> io::Result<std::process::Child> {
    let mut tries = 0;
    loop {
        let spawned = Command::new(exe)
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        match spawned {
            Err(e) if e.kind() == io::ErrorKind::ExecutableFileBusy && tries < 50 => {
                tries += 1;
                std::thread::sleep(Duration::from_millis(20));
            }
            other => return other,
        }
    }
}

/// `<name> <x.y.z>` on the first line, nothing else on it; returns the `x.y.z`.
fn parse_version_line(name: &str, stdout: &str) -> Option<String> {
    let line = stdout.lines().next()?;
    let mut words = line.split_whitespace();
    let (first, version) = (words.next()?, words.next()?);
    (first == name && words.next().is_none()).then(|| version.to_string())
}

/// Moves a file. The real one is `std::fs::rename`; tests inject failures.
pub(crate) trait Renamer {
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
}

/// `std::fs::rename`.
struct FsRenamer;

impl Renamer for FsRenamer {
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }
}

/// The path of the running executable with links resolved (`current_exe`, then `canonicalize`), so a
/// link to it stays and its target is replaced. On Windows the result carries the `\\?\` prefix.
pub fn exe_path() -> io::Result<PathBuf> {
    resolve_exe(&std::env::current_exe()?)
}

pub(crate) fn resolve_exe(path: &Path) -> io::Result<PathBuf> {
    std::fs::canonicalize(path)
}

/// The executable's file name split into a base and the `.exe` extension if it has one:
/// `freemapper.exe` gives `freemapper` and `.exe`, `freemapper` gives `freemapper` and nothing.
pub(crate) fn split_name(exe: &Path) -> (OsString, OsString) {
    let name = exe.file_name().unwrap_or_default();
    match (exe.file_stem(), exe.extension()) {
        (Some(stem), Some(ext)) if ext.eq_ignore_ascii_case("exe") => {
            let mut suffix = OsString::from(".");
            suffix.push(ext);
            (stem.to_owned(), suffix)
        }
        _ => (name.to_owned(), OsString::new()),
    }
}

/// `<base><middle><suffix>` next to `exe`.
pub(crate) fn sibling(exe: &Path, middle: &str) -> PathBuf {
    let (mut name, suffix) = split_name(exe);
    name.push(middle);
    name.push(suffix);
    exe.with_file_name(name)
}

/// Where the replaced executable is kept: `freemapper.previous` (`freemapper.previous.exe`) next to `exe`.
pub fn previous_path(exe: &Path) -> PathBuf {
    sibling(exe, ".previous")
}

/// The stale names start with this, e.g. `freemapper.stale-`.
fn stale_prefix(exe: &Path) -> OsString {
    let (mut prefix, _) = split_name(exe);
    prefix.push(".stale-");
    prefix
}

/// `freemapper.stale-<secs>[.exe]`, with `-<n>` after the seconds for `n > 0`.
fn stale_path(exe: &Path, secs: u64, n: u32) -> PathBuf {
    if n == 0 {
        sibling(exe, &format!(".stale-{secs}"))
    } else {
        sibling(exe, &format!(".stale-{secs}-{n}"))
    }
}

/// The first stale name for `secs` that is not taken (two updates in one second get `-1`, `-2`, …).
fn fresh_stale_path(exe: &Path, secs: u64) -> PathBuf {
    (0..)
        .map(|n| stale_path(exe, secs, n))
        .find(|path| {
            matches!(std::fs::symlink_metadata(path), Err(e) if e.kind() == io::ErrorKind::NotFound)
        })
        .expect("some stale name is free")
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The app name an executable prints with `--version`: its file name up to the first dot.
fn app_name(exe: &Path) -> String {
    let name = exe.file_name().unwrap_or_default().to_string_lossy();
    name.split('.').next().unwrap_or_default().to_string()
}

/// The folder of `exe`, `.` for a bare file name.
fn folder_of(exe: &Path) -> &Path {
    match exe.parent() {
        Some(folder) if !folder.as_os_str().is_empty() => folder,
        _ => Path::new("."),
    }
}

/// Creates and removes `<base>.update-probe` in the executable's folder. A folder the user cannot
/// write (`/usr/local/bin`, `C:\Program Files`) gives `NotWritable` before anything is renamed.
pub(crate) fn ensure_writable(exe: &Path) -> Result<(), UpdateError> {
    let folder = folder_of(exe);
    let (mut probe, _) = split_name(exe);
    probe.push(".update-probe");
    let probe = exe.with_file_name(probe);
    match freemapper_fs::create_new_file(&probe) {
        Ok(file) => {
            drop(file);
            let _ = std::fs::remove_file(&probe);
            Ok(())
        }
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem
            ) =>
        {
            Err(UpdateError::NotWritable(folder.to_path_buf()))
        }
        Err(e) => Err(e.into()),
    }
}

/// A lock file older than this was left by a crashed install and is taken over (ruling R12).
const STALE_LOCK_AGE: Duration = Duration::from_secs(15 * 60);

/// Holds `<base>.update-lock` in the executable's folder: one install or rollback at a time, so a
/// second one cannot replace the first one's verified file with its own half-written one. The file
/// is removed when this is dropped.
pub(crate) struct InstallLock(PathBuf);

impl Drop for InstallLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Takes the install lock. An existing lock younger than 15 minutes gives [`UpdateError::Busy`]; an
/// older one is removed and created once more. A folder the user cannot write gives `NotWritable`.
pub(crate) fn lock_install(exe: &Path) -> Result<InstallLock, UpdateError> {
    let (mut name, _) = split_name(exe);
    name.push(".update-lock");
    let lock = exe.with_file_name(name);
    let create = || {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock)
    };
    let not_writable = |e: io::Error| match e.kind() {
        io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem => {
            UpdateError::NotWritable(folder_of(exe).to_path_buf())
        }
        _ => e.into(),
    };
    match create() {
        Ok(_) => return Ok(InstallLock(lock)),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(not_writable(e)),
    }
    let stale = std::fs::symlink_metadata(&lock)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age > STALE_LOCK_AGE);
    if !stale {
        return Err(UpdateError::Busy(lock));
    }
    let _ = std::fs::remove_file(&lock);
    match create() {
        Ok(_) => Ok(InstallLock(lock)),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Err(UpdateError::Busy(lock)),
        Err(e) => Err(not_writable(e)),
    }
}

/// After `failed` (moving something into `to`), moves `from` back to `to` and hands `failed` back. If
/// that fails too, the error names both paths, so a person can rename the file by hand.
fn restore(
    renamer: &dyn Renamer,
    failed: io::Error,
    from: &Path,
    to: &Path,
) -> Result<io::Error, UpdateError> {
    match renamer.rename(from, to) {
        Ok(()) => Ok(failed),
        Err(again) => Err(io::Error::new(
            failed.kind(),
            format!(
                "could not replace {to}: {failed}; moving {from} back to {to} failed too: {again}; \
                 rename {from} to {to} by hand",
                to = to.display(),
                from = from.display(),
            ),
        )
        .into()),
    }
}

/// Puts `new_file` in the place of `exe` and keeps the old one as [`previous_path`] (spec 3.5 step 7).
///
/// `new_file` must be a plain file (not a link) in `exe`'s folder, so every step is a rename on one
/// file system and nothing outside the folder takes the program's place. An existing
/// previous is first renamed to a stale name (that works while an old version still runs from it). If
/// the new file cannot take `exe`'s place, the old executable is moved back. A folder that cannot be
/// written gives [`UpdateError::NotWritable`] and nothing changes.
pub fn swap_in(exe: &Path, new_file: &Path) -> Result<(), UpdateError> {
    swap_in_with(exe, new_file, &FsRenamer)
}

pub(crate) fn swap_in_with(
    exe: &Path,
    new_file: &Path,
    renamer: &dyn Renamer,
) -> Result<(), UpdateError> {
    if new_file.parent() != exe.parent() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "the new executable {} is not in the folder of {}",
                new_file.display(),
                exe.display()
            ),
        )
        .into());
    }
    if !std::fs::symlink_metadata(new_file)?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "the new executable {} is not a plain file",
                new_file.display()
            ),
        )
        .into());
    }
    ensure_writable(exe)?;
    let previous = previous_path(exe);
    let stale = match std::fs::symlink_metadata(&previous) {
        Ok(_) => {
            let stale = fresh_stale_path(exe, now_secs());
            renamer.rename(&previous, &stale)?;
            Some(stale)
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    // On a failure below the old previous gets its name back, so a rollback still has it.
    let unstale = || {
        if let Some(stale) = &stale {
            let _ = renamer.rename(stale, &previous);
        }
    };
    if let Err(failed) = renamer.rename(exe, &previous) {
        unstale();
        return Err(failed.into());
    }
    if let Err(failed) = renamer.rename(new_file, exe) {
        let failed = restore(renamer, failed, &previous, exe)?;
        unstale();
        return Err(failed.into());
    }
    Ok(())
}

/// Swaps [`previous_path`] and `exe` back (`update --rollback`) after checking that the previous one
/// runs and prints a version, which it returns. The current executable becomes the new previous.
///
/// No previous gives an `Io` error of kind `NotFound`; a previous that prints no version gives
/// `InvalidData`, and nothing changes. While an install or another rollback runs (its
/// `<base>.update-lock` exists and is younger than 15 minutes) it gives [`UpdateError::Busy`].
pub fn rollback(exe: &Path, runner: &dyn VersionRunner) -> Result<Version, UpdateError> {
    rollback_with(exe, runner, &FsRenamer)
}

pub(crate) fn rollback_with(
    exe: &Path,
    runner: &dyn VersionRunner,
    renamer: &dyn Renamer,
) -> Result<Version, UpdateError> {
    let _lock = lock_install(exe)?;
    let previous = previous_path(exe);
    match std::fs::symlink_metadata(&previous) {
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "there is no previous version to go back to: {} is missing",
                    previous.display()
                ),
            )
            .into());
        }
        Err(e) => return Err(e.into()),
    }
    let text = runner.version_of(&previous)?;
    let version: Version = text.trim().parse().map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} reports {text:?}, not a version: {e}",
                previous.display()
            ),
        )
    })?;
    ensure_writable(exe)?;
    // The current executable waits under a stale name, so if the last step fails a later start removes it.
    let parked = fresh_stale_path(exe, now_secs());
    renamer.rename(exe, &parked)?;
    if let Err(failed) = renamer.rename(&previous, exe) {
        return Err(restore(renamer, failed, &parked, exe)?.into());
    }
    // The rollback has happened; failing to keep the newer one as previous only loses the way forward.
    let _ = renamer.rename(&parked, &previous);
    Ok(version)
}

/// Deletes the stale files of `exe`'s app (`freemapper.stale-*`) in its folder, as far as it can.
///
/// Called at every start: a stale file that an old version still runs from (Windows) stays until a
/// later start. Errors are ignored; links, folders and other apps' files are left alone.
pub fn remove_stale(exe: &Path) {
    let prefix = stale_prefix(exe);
    let Ok(entries) = std::fs::read_dir(folder_of(exe)) else {
        return;
    };
    for entry in entries.flatten() {
        let is_file = entry.file_type().is_ok_and(|t| t.is_file());
        if is_file && starts_with(&entry.file_name(), &prefix) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn starts_with(name: &OsStr, prefix: &OsStr) -> bool {
    name.as_encoded_bytes()
        .starts_with(prefix.as_encoded_bytes())
}
#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::fs;
    use std::process::{Child, Command, Stdio};
    use std::time::Instant;

    use super::*;

    const EXE: &str = if cfg!(windows) {
        "freemapper.exe"
    } else {
        "freemapper"
    };
    const SUFFIX: &str = if cfg!(windows) { ".exe" } else { "" };

    /// An executable `old` and a new file `new` in `dir`, as the caller in A6 leaves them.
    fn setup(dir: &Path, old: &str, new: &str) -> (PathBuf, PathBuf) {
        let exe = dir.join(EXE);
        fs::write(&exe, old).unwrap();
        let new_file = dir.join("freemapper.update-new");
        fs::write(&new_file, new).unwrap();
        (exe, new_file)
    }

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn stale_files(dir: &Path) -> Vec<PathBuf> {
        names(dir)
            .into_iter()
            .filter(|n| n.starts_with("freemapper.stale-"))
            .map(|n| dir.join(n))
            .collect()
    }

    /// A renamer that fails on the listed calls (counted from 1) and renames otherwise.
    struct FailingRenamer {
        calls: Cell<usize>,
        fail_on: &'static [usize],
    }

    impl FailingRenamer {
        fn new(fail_on: &'static [usize]) -> Self {
            FailingRenamer {
                calls: Cell::new(0),
                fail_on,
            }
        }
    }

    impl Renamer for FailingRenamer {
        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            let n = self.calls.get() + 1;
            self.calls.set(n);
            if self.fail_on.contains(&n) {
                Err(io::Error::other(format!("injected failure {n}")))
            } else {
                fs::rename(from, to)
            }
        }
    }

    /// Answers with a fixed text and remembers what it was asked about.
    struct FakeRunner {
        answer: Result<&'static str, ()>,
        asked: RefCell<Vec<PathBuf>>,
    }

    impl VersionRunner for FakeRunner {
        fn version_of(&self, exe: &Path) -> Result<String, UpdateError> {
            self.asked.borrow_mut().push(exe.to_path_buf());
            self.answer
                .map(str::to_string)
                .map_err(|()| UpdateError::Io(io::Error::other("runner failed")))
        }
    }

    fn fake(answer: Result<&'static str, ()>) -> FakeRunner {
        FakeRunner {
            answer,
            asked: RefCell::new(Vec::new()),
        }
    }

    /// Spawns, retrying while Linux still reports the freshly written file as busy.
    fn spawn(command: &mut Command) -> Child {
        let mut tries = 0;
        loop {
            match command.spawn() {
                Err(e) if e.kind() == io::ErrorKind::ExecutableFileBusy && tries < 50 => {
                    tries += 1;
                    std::thread::sleep(Duration::from_millis(20));
                }
                other => return other.unwrap(),
            }
        }
    }

    /// Starts a copy of a system program from `path`, standing in for an old version that still runs
    /// (an MCP server). Ruling R6: on Windows a running image, not just an open handle.
    fn run_old_version(path: &Path) -> Child {
        #[cfg(windows)]
        {
            let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
            fs::copy(Path::new(&root).join("System32").join("cmd.exe"), path).unwrap();
            spawn(
                Command::new(path)
                    .args(["/c", "ping", "-n", "6", "127.0.0.1"])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null()),
            )
        }
        // A copy of this test binary running `old_version_sleeps`. Not a copied /bin/sleep: macOS kills a
        // copy of a system binary at once (exit 137), while an ad-hoc signed copy runs, like the real app.
        #[cfg(not(windows))]
        {
            fs::copy(std::env::current_exe().unwrap(), path).unwrap();
            spawn(
                Command::new(path)
                    .args([
                        "--exact",
                        "swap::tests::old_version_sleeps",
                        "--ignored",
                        "--test-threads=1",
                    ])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null()),
            )
        }
    }

    #[test]
    #[ignore = "the running old version that previous_in_use_becomes_stale starts; it only sleeps"]
    fn old_version_sleeps() {
        std::thread::sleep(Duration::from_secs(5));
    }

    #[test]
    fn names_follow_the_executable() {
        let dir = Path::new("dir");
        assert_eq!(
            previous_path(&dir.join("freemapper")),
            dir.join("freemapper.previous")
        );
        assert_eq!(
            previous_path(&dir.join("freemapper.exe")),
            dir.join("freemapper.previous.exe")
        );
        assert_eq!(
            previous_path(&dir.join("freemapper-print")),
            dir.join("freemapper-print.previous")
        );
        assert_eq!(
            stale_path(&dir.join("freemapper"), 1_700_000_000, 0),
            dir.join("freemapper.stale-1700000000")
        );
        assert_eq!(
            stale_path(&dir.join("freemapper.exe"), 1_700_000_000, 2),
            dir.join("freemapper.stale-1700000000-2.exe")
        );
        assert_eq!(app_name(&dir.join("freemapper.previous.exe")), "freemapper");
        assert_eq!(
            app_name(&dir.join("freemapper-print.update-new")),
            "freemapper-print"
        );
        assert_eq!(app_name(&dir.join("freemapper")), "freemapper");
    }

    #[test]
    fn stale_name_taken_in_the_same_second_gets_a_number() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join(EXE);
        assert_eq!(fresh_stale_path(&exe, 100), stale_path(&exe, 100, 0));
        fs::write(stale_path(&exe, 100, 0), "").unwrap();
        fs::write(stale_path(&exe, 100, 1), "").unwrap();
        assert_eq!(fresh_stale_path(&exe, 100), stale_path(&exe, 100, 2));
    }

    #[test]
    fn version_line_is_name_and_version() {
        let line = |s: &str| parse_version_line("freemapper", s);
        assert_eq!(line("freemapper 0.10.0\n"), Some("0.10.0".to_string()));
        assert_eq!(
            line("freemapper 0.10.0\r\nmore\n"),
            Some("0.10.0".to_string())
        );
        assert_eq!(line("freemapper-print 0.10.0\n"), None);
        assert_eq!(line("freemapper\n"), None);
        assert_eq!(line("freemapper 0.10.0 extra\n"), None);
        assert_eq!(line(""), None);
        assert_eq!(
            parse_version_line("freemapper-print", "freemapper-print 1.2.3"),
            Some("1.2.3".to_string())
        );
    }

    #[test]
    fn exe_path_is_absolute_and_exists() {
        let path = exe_path().unwrap();
        assert!(path.is_absolute());
        assert!(path.is_file());
    }

    #[test]
    fn swap_keeps_old_as_previous() {
        let dir = tempfile::tempdir().unwrap();
        let (exe, new_file) = setup(dir.path(), "old", "new");
        swap_in(&exe, &new_file).unwrap();
        assert_eq!(read(&exe), "new");
        assert_eq!(read(&previous_path(&exe)), "old");
        let previous = format!("freemapper.previous{SUFFIX}");
        assert_eq!(names(dir.path()), vec![EXE.to_string(), previous]);
    }

    #[test]
    fn second_swap_turns_old_previous_into_stale() {
        let dir = tempfile::tempdir().unwrap();
        let (exe, new_file) = setup(dir.path(), "v1", "v2");
        swap_in(&exe, &new_file).unwrap();
        fs::write(&new_file, "v3").unwrap();
        swap_in(&exe, &new_file).unwrap();
        assert_eq!(read(&exe), "v3");
        assert_eq!(read(&previous_path(&exe)), "v2");
        let stale = stale_files(dir.path());
        assert_eq!(stale.len(), 1, "{stale:?}");
        assert_eq!(read(&stale[0]), "v1");
        let name = stale[0].file_name().unwrap().to_str().unwrap();
        let digits = name
            .strip_prefix("freemapper.stale-")
            .and_then(|rest| rest.strip_suffix(SUFFIX))
            .unwrap();
        assert!(digits.bytes().all(|b| b.is_ascii_digit()), "{name}");
    }

    #[test]
    fn failed_second_rename_restores_the_old_exe() {
        let dir = tempfile::tempdir().unwrap();
        let (exe, new_file) = setup(dir.path(), "old", "new");
        fs::write(previous_path(&exe), "older").unwrap();
        // Calls: 1 previous -> stale, 2 exe -> previous, 3 new -> exe (fails), then the restores.
        let error = swap_in_with(&exe, &new_file, &FailingRenamer::new(&[3])).unwrap_err();
        assert!(error.to_string().contains("injected failure 3"), "{error}");
        assert_eq!(read(&exe), "old");
        assert_eq!(read(&previous_path(&exe)), "older");
        assert_eq!(read(&new_file), "new");
        assert!(stale_files(dir.path()).is_empty());
    }

    #[test]
    fn failed_first_rename_keeps_the_old_previous() {
        let dir = tempfile::tempdir().unwrap();
        let (exe, new_file) = setup(dir.path(), "old", "new");
        fs::write(previous_path(&exe), "older").unwrap();
        // Calls: 1 previous -> stale, 2 exe -> previous (fails), 3 stale -> previous.
        let error = swap_in_with(&exe, &new_file, &FailingRenamer::new(&[2])).unwrap_err();
        assert!(error.to_string().contains("injected failure 2"), "{error}");
        assert_eq!(read(&exe), "old");
        assert_eq!(read(&previous_path(&exe)), "older");
        assert!(stale_files(dir.path()).is_empty());
    }

    #[test]
    fn failed_restore_names_both_paths() {
        let dir = tempfile::tempdir().unwrap();
        let (exe, new_file) = setup(dir.path(), "old", "new");
        let error = swap_in_with(&exe, &new_file, &FailingRenamer::new(&[2, 3])).unwrap_err();
        // No previous existed: 1 exe -> previous, 2 new -> exe (fails), 3 restore (fails).
        let text = error.to_string();
        assert!(text.contains(&exe.display().to_string()), "{text}");
        assert!(
            text.contains(&previous_path(&exe).display().to_string()),
            "{text}"
        );
        assert_eq!(read(&previous_path(&exe)), "old");
    }

    #[test]
    fn new_file_from_another_folder_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let (exe, _) = setup(dir.path(), "old", "new");
        let elsewhere = other.path().join("freemapper.update-new");
        fs::write(&elsewhere, "new").unwrap();
        assert!(swap_in(&exe, &elsewhere).is_err());
        assert_eq!(read(&exe), "old");
        assert!(!previous_path(&exe).exists());
    }

    /// The new file must be a plain file: a link would put whatever it points to in the program's place.
    #[cfg(unix)]
    #[test]
    fn new_file_that_is_a_link_or_folder_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let (exe, new_file) = setup(dir.path(), "old", "new");
        fs::remove_file(&new_file).unwrap();
        let target = other.path().join("program");
        fs::write(&target, "elsewhere").unwrap();
        std::os::unix::fs::symlink(&target, &new_file).unwrap();
        let error = swap_in(&exe, &new_file).unwrap_err();
        assert!(error.to_string().contains("not a plain file"), "{error}");
        fs::remove_file(&new_file).unwrap();
        fs::create_dir(&new_file).unwrap();
        let error = swap_in(&exe, &new_file).unwrap_err();
        assert!(error.to_string().contains("not a plain file"), "{error}");
        assert_eq!(read(&exe), "old");
        assert!(!previous_path(&exe).exists());
    }

    /// Windows has no folder mode that a test can set without a platform crate; the probe path is the same.
    #[cfg(unix)]
    #[test]
    fn swap_in_read_only_folder_changes_nothing() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("app");
        fs::create_dir(&app).unwrap();
        let (exe, new_file) = setup(&app, "old", "new");
        fs::write(previous_path(&exe), "older").unwrap();
        let before = names(&app);
        fs::set_permissions(&app, fs::Permissions::from_mode(0o555)).unwrap();
        let privileged = fs::write(app.join("privilege-check"), "").is_ok();
        let result = (!privileged).then(|| swap_in(&exe, &new_file));
        fs::set_permissions(&app, fs::Permissions::from_mode(0o755)).unwrap();
        let Some(result) = result else {
            eprintln!("skipped: this user can write into a 0o555 folder (root)");
            return;
        };
        match result {
            Err(UpdateError::NotWritable(folder)) => assert_eq!(folder, app),
            other => panic!("expected NotWritable, got {other:?}"),
        }
        assert_eq!(names(&app), before);
        assert_eq!(read(&exe), "old");
        assert_eq!(read(&previous_path(&exe)), "older");
        assert_eq!(read(&new_file), "new");
    }

    #[cfg(unix)]
    #[test]
    fn swap_through_symlink_replaces_the_target() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("app");
        let bin = dir.path().join("bin");
        fs::create_dir(&app).unwrap();
        fs::create_dir(&bin).unwrap();
        fs::write(app.join("freemapper"), "old").unwrap();
        let link = bin.join("freemapper");
        std::os::unix::fs::symlink(app.join("freemapper"), &link).unwrap();

        let exe = resolve_exe(&link).unwrap();
        assert_eq!(exe, fs::canonicalize(app.join("freemapper")).unwrap());
        let new_file = exe.with_file_name("freemapper.update-new");
        fs::write(&new_file, "new").unwrap();
        swap_in(&exe, &new_file).unwrap();

        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(read(&link), "new");
        assert_eq!(read(&app.join("freemapper.previous")), "old");
        assert_eq!(names(&bin), vec!["freemapper".to_string()]);
    }

    #[test]
    fn previous_in_use_becomes_stale() {
        let dir = tempfile::tempdir().unwrap();
        let (exe, new_file) = setup(dir.path(), "old", "new");
        let mut child = run_old_version(&previous_path(&exe));

        let swapped = swap_in(&exe, &new_file);
        let still_running = child.try_wait().unwrap().is_none();
        let stale = stale_files(dir.path());
        // Removing while it runs must not fail; on Windows the file stays until the process ends.
        remove_stale(&exe);
        let _ = child.kill();
        let _ = child.wait();

        swapped.unwrap();
        assert!(
            still_running,
            "the old version ended before the swap was checked"
        );
        assert_eq!(read(&exe), "new");
        assert_eq!(read(&previous_path(&exe)), "old");
        assert_eq!(stale.len(), 1, "{stale:?}");
        assert!(stale[0].to_string_lossy().ends_with(SUFFIX));

        // A later start removes it once it is free; Windows may need a moment to release the image.
        let start = Instant::now();
        while !stale_files(dir.path()).is_empty() && start.elapsed() < Duration::from_secs(10) {
            remove_stale(&exe);
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(stale_files(dir.path()).is_empty());
        assert!(exe.exists() && previous_path(&exe).exists());
    }

    #[test]
    fn rollback_swaps_back_after_version_check() {
        let dir = tempfile::tempdir().unwrap();
        let (exe, new_file) = setup(dir.path(), "old", "new");
        swap_in(&exe, &new_file).unwrap();
        let runner = fake(Ok("0.9.0"));
        let version = rollback(&exe, &runner).unwrap();
        assert_eq!(version, "0.9.0".parse().unwrap());
        assert_eq!(*runner.asked.borrow(), vec![previous_path(&exe)]);
        assert_eq!(read(&exe), "old");
        assert_eq!(read(&previous_path(&exe)), "new");
        assert!(stale_files(dir.path()).is_empty());
    }

    /// A rollback waits for nobody: while an install (or another rollback) holds the lock it is refused,
    /// and it releases the lock it took.
    #[test]
    fn rollback_is_refused_while_locked() {
        let dir = tempfile::tempdir().unwrap();
        let (exe, new_file) = setup(dir.path(), "old", "new");
        swap_in(&exe, &new_file).unwrap();
        let lock = dir.path().join("freemapper.update-lock");
        fs::write(&lock, "").unwrap();
        let before = names(dir.path());
        let runner = fake(Ok("0.9.0"));
        match rollback(&exe, &runner) {
            Err(UpdateError::Busy(path)) => assert_eq!(path, lock),
            other => panic!("expected Busy, got {other:?}"),
        }
        assert!(runner.asked.borrow().is_empty());
        assert_eq!(names(dir.path()), before);
        assert_eq!(read(&exe), "new");
        fs::remove_file(&lock).unwrap();
        rollback(&exe, &runner).unwrap();
        assert_eq!(read(&exe), "old");
        assert!(!lock.exists());
    }

    #[test]
    fn rollback_refuses_a_previous_that_reports_no_version() {
        for answer in [Ok("not a version"), Err(())] {
            let dir = tempfile::tempdir().unwrap();
            let (exe, new_file) = setup(dir.path(), "old", "new");
            swap_in(&exe, &new_file).unwrap();
            let error = rollback(&exe, &fake(answer)).unwrap_err();
            if answer.is_ok() {
                match &error {
                    UpdateError::Io(e) => assert_eq!(e.kind(), io::ErrorKind::InvalidData),
                    other => panic!("expected InvalidData, got {other:?}"),
                }
            }
            assert_eq!(read(&exe), "new");
            assert_eq!(read(&previous_path(&exe)), "old");
        }
    }

    #[test]
    fn failed_rollback_rename_restores_the_current_exe() {
        let dir = tempfile::tempdir().unwrap();
        let (exe, new_file) = setup(dir.path(), "old", "new");
        swap_in(&exe, &new_file).unwrap();
        // Calls: 1 exe -> parked, 2 previous -> exe (fails), 3 parked -> exe.
        let error =
            rollback_with(&exe, &fake(Ok("0.9.0")), &FailingRenamer::new(&[2])).unwrap_err();
        assert!(error.to_string().contains("injected failure 2"), "{error}");
        assert_eq!(read(&exe), "new");
        assert_eq!(read(&previous_path(&exe)), "old");
        assert!(stale_files(dir.path()).is_empty());
    }

    #[test]
    fn rollback_without_previous_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let (exe, _) = setup(dir.path(), "new", "unused");
        let runner = fake(Ok("0.9.0"));
        match rollback(&exe, &runner) {
            Err(UpdateError::Io(e)) => {
                assert_eq!(e.kind(), io::ErrorKind::NotFound);
                let text = e.to_string();
                assert!(
                    text.contains(&previous_path(&exe).display().to_string()),
                    "{text}"
                );
            }
            other => panic!("expected NotFound, got {other:?}"),
        }
        assert!(runner.asked.borrow().is_empty());
        assert_eq!(read(&exe), "new");
    }

    #[test]
    fn remove_stale_ignores_locked_files() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let exe = dir.path().join(EXE);
        fs::write(&exe, "exe").unwrap();
        fs::write(previous_path(&exe), "previous").unwrap();
        fs::write(stale_path(&exe, 1, 0), "stale").unwrap();
        fs::write(stale_path(&exe, 2, 1), "stale").unwrap();
        fs::write(dir.path().join("freemapper-print.stale-3"), "other app").unwrap();
        fs::write(dir.path().join("notes.stale-4"), "other").unwrap();
        fs::create_dir(dir.path().join("freemapper.stale-5")).unwrap();
        fs::write(dir.path().join("freemapper.stale-5").join("inside"), "").unwrap();
        let target = outside.path().join("target");
        fs::write(&target, "target").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, dir.path().join("freemapper.stale-6")).unwrap();

        // Windows: a stale file held open without sharing cannot be deleted; that must not stop the rest.
        #[cfg(windows)]
        let locked = {
            use std::os::windows::fs::OpenOptionsExt;
            let path = stale_path(&exe, 7, 0);
            fs::write(&path, "locked").unwrap();
            fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(&path)
                .unwrap()
        };

        remove_stale(&exe);

        let mut expected = vec![
            EXE.to_string(),
            "freemapper-print.stale-3".to_string(),
            format!("freemapper.previous{SUFFIX}"),
            "freemapper.stale-5".to_string(),
            "notes.stale-4".to_string(),
        ];
        if cfg!(unix) {
            expected.push("freemapper.stale-6".to_string());
        }
        if cfg!(windows) {
            expected.push("freemapper.stale-7.exe".to_string());
        }
        expected.sort();
        assert_eq!(names(dir.path()), expected);
        assert_eq!(read(&target), "target");
        #[cfg(windows)]
        drop(locked);

        remove_stale(&dir.path().join("missing").join(EXE));
    }

    #[cfg(unix)]
    fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[cfg(unix)]
    #[test]
    fn process_runner_reads_name_and_version() {
        let dir = tempfile::tempdir().unwrap();
        let good = script(dir.path(), "freemapper.previous", "echo 'freemapper 0.9.0'");
        assert_eq!(ProcessRunner.version_of(&good).unwrap(), "0.9.0");
        let other = script(dir.path(), "freemapper.other", "echo 'something 0.9.0'");
        assert!(ProcessRunner.version_of(&other).is_err());
        let failing = script(
            dir.path(),
            "freemapper.failing",
            "echo 'freemapper 0.9.0'; exit 3",
        );
        assert!(ProcessRunner.version_of(&failing).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn process_runner_stops_a_hanging_program() {
        let dir = tempfile::tempdir().unwrap();
        let hanging = script(dir.path(), "freemapper.hanging", "exec sleep 5");
        let start = Instant::now();
        match version_with_timeout(&hanging, Duration::from_millis(300)) {
            Err(UpdateError::Io(e)) => assert_eq!(e.kind(), io::ErrorKind::TimedOut),
            other => panic!("expected TimedOut, got {other:?}"),
        }
        assert!(start.elapsed() < Duration::from_secs(3));
    }
}
