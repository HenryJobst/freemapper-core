//! Small files written whole: a temporary file next to the target, then a rename.

use std::io::{self, Write};
use std::path::Path;

/// Writes `bytes` to `path` without following a link at the temporary name or at `path`
/// (`freemapper_fs::create_new_file`, then `freemapper_fs::rename_cache_file`). The folder must exist.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut part = path.as_os_str().to_owned();
    part.push(".part");
    let part = std::path::PathBuf::from(part);
    let result = freemapper_fs::create_new_file(&part)
        .and_then(|mut file| file.write_all(bytes).and_then(|()| file.sync_all()));
    let result =
        result.and_then(|()| freemapper_fs::rename_cache_file(&part, path).map_err(|(_, e)| e));
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}
