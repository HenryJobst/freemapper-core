//! The update mode: `off`, `notify` or `install`, from an environment value or `settings.toml`.

use std::io;
use std::path::Path;

use crate::atomic::write_atomic;

const FILE: &str = "settings.toml";

/// What the app does when it finds a newer version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Does not check at all.
    Off,
    /// Checks and tells the user (the default).
    Notify,
    /// Checks and installs.
    Install,
}

impl Mode {
    fn parse(text: &str) -> Option<Mode> {
        match text.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Mode::Off),
            "notify" => Some(Mode::Notify),
            "install" => Some(Mode::Install),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Mode::Off => "off",
            Mode::Notify => "notify",
            Mode::Install => "install",
        }
    }
}

/// The mode: a valid `env` value wins, then `[update] mode` in `<config_dir>/settings.toml`, else
/// [`Mode::Notify`]. Unknown values and an unreadable file also give `Notify`.
pub fn load_mode(config_dir: &Path, env: Option<&str>) -> Mode {
    if let Some(mode) = env.and_then(Mode::parse) {
        return mode;
    }
    std::fs::read_to_string(config_dir.join(FILE))
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .and_then(|table| {
            table
                .get("update")?
                .as_table()?
                .get("mode")?
                .as_str()
                .and_then(Mode::parse)
        })
        .unwrap_or(Mode::Notify)
}

/// Sets `[update] mode`, keeping the other tables (comments are lost), and creates the folder.
/// A file that cannot be parsed is an error and stays as it is.
pub fn save_mode(config_dir: &Path, mode: Mode) -> io::Result<()> {
    let path = config_dir.join(FILE);
    let mut table = match std::fs::read_to_string(&path) {
        Ok(text) => text
            .parse::<toml::Table>()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => toml::Table::new(),
        Err(e) => return Err(e),
    };
    let update = table
        .entry("update")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    if !update.is_table() {
        *update = toml::Value::Table(toml::Table::new());
    }
    if let Some(update) = update.as_table_mut() {
        update.insert("mode".into(), toml::Value::String(mode.as_str().into()));
    }
    let text = toml::to_string(&table).map_err(io::Error::other)?;
    std::fs::create_dir_all(config_dir)?;
    write_atomic(&path, text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_env_overrides_file() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load_mode(dir.path(), None), Mode::Notify);
        save_mode(dir.path(), Mode::Install).unwrap();
        assert_eq!(load_mode(dir.path(), None), Mode::Install);
        assert_eq!(load_mode(dir.path(), Some("OFF")), Mode::Off);
        assert_eq!(load_mode(dir.path(), Some("Notify")), Mode::Notify);
        assert_eq!(load_mode(dir.path(), Some("bogus")), Mode::Install);
    }

    #[test]
    fn unknown_or_unreadable_file_is_notify() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FILE), "[update]\nmode = \"sometimes\"\n").unwrap();
        assert_eq!(load_mode(dir.path(), None), Mode::Notify);
        std::fs::write(dir.path().join(FILE), "= = broken").unwrap();
        assert_eq!(load_mode(dir.path(), None), Mode::Notify);
        assert!(save_mode(dir.path(), Mode::Off).is_err());
        assert_eq!(
            std::fs::read_to_string(dir.path().join(FILE)).unwrap(),
            "= = broken"
        );
    }

    #[test]
    fn save_mode_keeps_other_tables() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("freemapper");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join(FILE),
            "[ui]\nlang = \"de\"\n\n[update]\nmode = \"off\"\nother = 3\n",
        )
        .unwrap();
        save_mode(&folder, Mode::Install).unwrap();
        let table: toml::Table = std::fs::read_to_string(folder.join(FILE))
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(table["ui"]["lang"].as_str(), Some("de"));
        assert_eq!(table["update"]["mode"].as_str(), Some("install"));
        assert_eq!(table["update"]["other"].as_integer(), Some(3));
        let fresh = dir.path().join("new/freemapper");
        save_mode(&fresh, Mode::Off).unwrap();
        assert_eq!(load_mode(&fresh, None), Mode::Off);
    }
}
