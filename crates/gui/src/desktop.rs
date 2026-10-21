//! On Linux, delve puts itself in the applications menu when it starts, however it was
//! installed: by the release installer, from an archive, from itch.io or by `install.sh`.
//! The entry follows the game if it moves; `install.sh --remove` takes it away.

use std::fs;
use std::path::{Path, PathBuf};

const ICON: &[u8] = include_bytes!("../assets/delve.png");

/// Writes the menu entry and its icon, unless they are there already and point at this
/// very program. Failing is harmless: the game runs all the same.
pub fn register() {
    let Ok(exe) = std::env::current_exe() else { return };
    // A build under development, run from cargo's target folder, stays out of the menu.
    if exe.components().any(|c| c.as_os_str() == "target") {
        return;
    }
    let Some(data) = data_dir() else { return };
    let entry = data.join("applications/delve.desktop");
    let text = desktop_entry(&exe);
    if fs::read_to_string(&entry).is_ok_and(|old| exec_line(&old) == exec_line(&text)) {
        return;
    }
    let icon = data.join("icons/hicolor/256x256/apps/delve.png");
    let _ = write(&icon, ICON);
    let _ = write(&entry, text.as_bytes());
}

fn data_dir() -> Option<PathBuf> {
    let var = |name| std::env::var_os(name).map(PathBuf::from).filter(|p| p.is_absolute());
    var("XDG_DATA_HOME").or_else(|| var("HOME").map(|h| h.join(".local/share")))
}

fn write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, bytes)
}

fn exec_line(entry: &str) -> Option<&str> {
    entry.lines().find(|l| l.starts_with("Exec="))
}

fn desktop_entry(exe: &Path) -> String {
    // The desktop entry format wants paths with spaces quoted.
    let exe = exe.to_string_lossy();
    let exec = if exe.contains(' ') { format!("\"{exe}\"") } else { exe.into_owned() };
    format!(
        "[Desktop Entry]
Type=Application
Name=delve
GenericName=Roguelike
Comment=Go down the dungeon and seize the Amulet
Exec={exec}
Icon=delve
Terminal=false
Categories=Game;RolePlaying;
Keywords=roguelike;dungeon;rpg;
StartupWMClass=delve
"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_entry_runs_this_program_and_quotes_spaces() {
        let entry = desktop_entry(Path::new("/home/me/Games/my games/delve"));
        assert_eq!(exec_line(&entry), Some("Exec=\"/home/me/Games/my games/delve\""));
        let entry = desktop_entry(Path::new("/home/me/.local/bin/delve"));
        assert_eq!(exec_line(&entry), Some("Exec=/home/me/.local/bin/delve"));
        assert!(entry.contains("Icon=delve") && entry.contains("StartupWMClass=delve"));
    }
}
