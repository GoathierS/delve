//! Saves, the roguelike way: one save, written when leaving and deleted when loaded, so
//! death stays final. Also the high scores.
//!
//! The gold of every finished run goes into a purse kept with the bought equipment, in
//! the [`Profile`].
//!
//! Files live in `$XDG_DATA_HOME/delve` (usually `~/.local/share/delve`) on Linux,
//! `%APPDATA%\delve` on Windows and `~/Library/Application Support/delve` on macOS.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::game::{Game, State};
use crate::gear::Profile;

/// Bumped whenever the saved data changes shape: older saves are then refused cleanly.
pub const SAVE_VERSION: u32 = 1;
const MAX_SCORES: usize = 10;

#[derive(Deserialize)]
struct SaveFile {
    version: u32,
    game: Game,
}

/// What [`SaveFile`] reads back, written without copying the game.
#[derive(Serialize)]
struct SaveFileRef<'a> {
    version: u32,
    game: &'a Game,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Score {
    pub score: i32,
    pub depth: i32,
    pub level: i32,
    pub won: bool,
    /// What ended the run: the monster's name, or `None` for a victory.
    pub killed_by: Option<String>,
    pub seed: u64,
}

#[derive(Debug)]
pub enum LoadError {
    Io(io::Error),
    /// Unreadable, or written by another version of delve. It has been removed.
    Incompatible(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "could not read the save: {e}"),
            LoadError::Incompatible(why) => write!(f, "the save could not be used and was removed ({why})"),
        }
    }
}

impl std::error::Error for LoadError {}

pub struct Storage {
    dir: PathBuf,
}

impl Storage {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Where each system keeps the data of its applications, in a `delve` folder.
    pub fn default_location() -> Self {
        let var = |name| std::env::var_os(name).map(PathBuf::from).filter(|p| p.is_absolute());
        let base = if cfg!(windows) {
            var("APPDATA")
        } else if cfg!(target_os = "macos") {
            var("HOME").map(|h| h.join("Library/Application Support"))
        } else {
            var("XDG_DATA_HOME").or_else(|| var("HOME").map(|h| h.join(".local/share")))
        };
        Self::new(base.unwrap_or_else(|| PathBuf::from(".")).join("delve"))
    }

    fn save_path(&self) -> PathBuf {
        self.dir.join("save.ron")
    }

    fn scores_path(&self) -> PathBuf {
        self.dir.join("scores.ron")
    }

    fn profile_path(&self) -> PathBuf {
        self.dir.join("profile.ron")
    }

    /// The purse and the equipment. A missing file reads as a fresh start.
    pub fn profile(&self) -> Profile {
        fs::read_to_string(self.profile_path()).ok().and_then(|t| ron::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save_profile(&self, profile: &Profile) -> io::Result<()> {
        let text = ron::ser::to_string_pretty(profile, Default::default()).map_err(io::Error::other)?;
        write_atomically(&self.profile_path(), text.as_bytes())
    }

    pub fn has_save(&self) -> bool {
        self.save_path().exists()
    }

    /// Writes the game, replacing any previous save. Only games still running are worth
    /// saving: call [`Storage::finish`] for the others.
    pub fn save(&self, game: &Game) -> io::Result<()> {
        let file = SaveFileRef { version: SAVE_VERSION, game };
        let text = ron::to_string(&file).map_err(io::Error::other)?;
        write_atomically(&self.save_path(), text.as_bytes())
    }

    /// Takes the saved game out: it is deleted as it is loaded, so that dying after a
    /// reload cannot be undone by loading again.
    pub fn load(&self) -> Result<Option<Game>, LoadError> {
        let path = self.save_path();
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(LoadError::Io(e)),
        };
        fs::remove_file(&path).map_err(LoadError::Io)?;
        #[derive(Deserialize)]
        struct Version {
            version: u32,
        }
        match ron::from_str::<Version>(&text) {
            Ok(v) if v.version == SAVE_VERSION => {}
            Ok(v) => return Err(LoadError::Incompatible(format!("version {} instead of {SAVE_VERSION}", v.version))),
            Err(e) => return Err(LoadError::Incompatible(e.to_string())),
        }
        let file: SaveFile = ron::from_str(&text).map_err(|e| LoadError::Incompatible(e.to_string()))?;
        debug_assert_eq!(file.version, SAVE_VERSION);
        let mut game = file.game;
        game.refresh_sight();
        Ok(Some(game))
    }

    /// Ends a run that is over: removes its save, puts its gold in the purse and enters it
    /// in the high scores. Returns the table and the rank of this run in it, if it made it.
    pub fn finish(&self, game: &Game) -> io::Result<(Vec<Score>, Option<usize>)> {
        debug_assert!(game.state != State::Playing);
        match fs::remove_file(self.save_path()) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
        let mut profile = self.profile();
        profile.purse += game.gold;
        self.save_profile(&profile)?;
        let entry = Score {
            score: game.score(),
            depth: game.depth,
            level: game.level,
            won: game.state == State::Won,
            killed_by: game.killed_by.clone(),
            seed: game.seed,
        };
        let mut scores = self.scores();
        // Ties go to the earlier run.
        let rank = scores.iter().position(|s| s.score < entry.score).unwrap_or(scores.len());
        scores.insert(rank, entry);
        scores.truncate(MAX_SCORES);
        let text = ron::ser::to_string_pretty(&scores, Default::default()).map_err(io::Error::other)?;
        write_atomically(&self.scores_path(), text.as_bytes())?;
        Ok((scores, (rank < MAX_SCORES).then_some(rank)))
    }

    /// The best runs so far, best first. A missing or damaged file reads as empty.
    pub fn scores(&self) -> Vec<Score> {
        fs::read_to_string(self.scores_path()).ok().and_then(|t| ron::from_str(&t).ok()).unwrap_or_default()
    }
}

/// Writes next to `path` then renames over it, so a crash never leaves half a file.
fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Action;

    fn storage(name: &str) -> Storage {
        let dir = std::env::temp_dir().join(format!("delve-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        Storage::new(dir)
    }

    #[test]
    fn a_save_resumes_the_exact_game_once() {
        let s = storage("resume");
        let mut game = Game::new(21);
        for _ in 0..5 {
            game.act(Action::Wait);
        }
        s.save(&game).unwrap();
        assert!(s.has_save());

        let mut loaded = s.load().unwrap().expect("a save");
        assert!(!s.has_save(), "loading must consume the save");
        assert!(s.load().unwrap().is_none());

        assert_eq!(loaded.player, game.player);
        assert_eq!(loaded.turn, game.turn);
        assert_eq!(loaded.monsters.len(), game.monsters.len());
        // Same random state: both games go on identically.
        for _ in 0..30 {
            game.act(Action::Wait);
            loaded.act(Action::Wait);
        }
        assert_eq!(loaded.stats, game.stats);
        assert_eq!(
            loaded.monsters.iter().map(|m| m.pos).collect::<Vec<_>>(),
            game.monsters.iter().map(|m| m.pos).collect::<Vec<_>>()
        );
    }

    #[test]
    fn other_versions_are_refused_and_removed() {
        let s = storage("version");
        s.save(&Game::new(1)).unwrap();
        let text = fs::read_to_string(s.save_path()).unwrap();
        fs::write(s.save_path(), text.replacen(&format!("version:{SAVE_VERSION}"), "version:999", 1)).unwrap();
        assert!(matches!(s.load(), Err(LoadError::Incompatible(_))));
        assert!(!s.has_save());

        fs::write(s.save_path(), "not a save").unwrap();
        assert!(matches!(s.load(), Err(LoadError::Incompatible(_))));
        assert!(!s.has_save());
    }

    #[test]
    fn finishing_removes_the_save_and_ranks_the_run() {
        let s = storage("scores");
        let mut game = Game::new(2);
        s.save(&game).unwrap();
        game.state = State::Dead;
        game.killed_by = Some("rat".into());
        let (scores, rank) = s.finish(&game).unwrap();
        assert!(!s.has_save());
        assert_eq!(rank, Some(0));
        assert_eq!(scores[0].killed_by.as_deref(), Some("rat"));
        assert_eq!(s.profile().purse, 0);

        game.gold = 500;
        let (scores, rank) = s.finish(&game).unwrap();
        assert_eq!(rank, Some(0));
        assert_eq!(scores.len(), 2);
        assert!(scores[0].score > scores[1].score);
        assert_eq!(s.scores(), scores);
        assert_eq!(s.profile().purse, 500, "the gold of a finished run goes to the purse");
    }

    #[test]
    fn the_profile_keeps_purchases() {
        let s = storage("profile");
        assert_eq!(s.profile(), Profile::new());
        let mut p = s.profile();
        p.purse = 100;
        p.buy(crate::gear::by_name("potion belt").unwrap()).unwrap();
        s.save_profile(&p).unwrap();
        assert_eq!(s.profile(), p);
        assert_eq!(s.profile().loadout().len(), 1);
    }

    #[test]
    fn saves_from_before_equipment_still_load() {
        let s = storage("old");
        s.save(&Game::new(3)).unwrap();
        let text = fs::read_to_string(s.save_path()).unwrap();
        assert!(text.contains("gear:[],"));
        fs::write(s.save_path(), text.replacen("gear:[],", "", 1)).unwrap();
        assert!(s.load().unwrap().unwrap().gear.is_empty());
    }
}
