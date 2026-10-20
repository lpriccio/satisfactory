//! Plain JSON files on disk, so saves diff and commit cleanly:
//!
//!   factories/<game>/<id>.json           one factory per file
//!   factories/<game>/worlds/<id>.json    one world per file
//!   factories/<game>/settings.json       game settings
//!   data/<game>.json                     generated game data

use crate::model::{Factory, GameData, GameId, GameSettings, World};
use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Summary {
    pub id: String,
    pub name: String,
    pub detail: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct Store {
    root: PathBuf,
}

/// Ids become file names, so keep them to a safe alphabet.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 80 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

impl Store {
    /// `root` holds `data/` and `factories/` (the planner folder).
    pub fn new(root: impl Into<PathBuf>) -> Store {
        Store { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn data_path(&self, game: GameId) -> PathBuf {
        self.root.join("data").join(format!("{}.json", game.key()))
    }

    fn game_dir(&self, game: GameId) -> PathBuf {
        self.root.join("factories").join(game.key())
    }

    fn factory_path(&self, game: GameId, id: &str) -> Result<PathBuf> {
        anyhow::ensure!(valid_id(id), "bad id {id:?}");
        Ok(self.game_dir(game).join(format!("{id}.json")))
    }

    fn world_path(&self, game: GameId, id: &str) -> Result<PathBuf> {
        anyhow::ensure!(valid_id(id), "bad id {id:?}");
        Ok(self.game_dir(game).join("worlds").join(format!("{id}.json")))
    }

    pub fn load_game_data(&self, game: GameId) -> Result<GameData> {
        let path = self.data_path(game);
        read_json(&path).with_context(|| format!("No game data at {} — run `planner-import {}`", path.display(), game.key()))
    }

    pub fn list_factories(&self, game: GameId) -> Vec<Summary> {
        let mut out: Vec<Summary> = list_json::<Factory>(&self.game_dir(game))
            .into_iter()
            .map(|f| Summary {
                detail: if f.targets.is_empty() {
                    format!("{} line{}", f.lines.len(), if f.lines.len() == 1 { "" } else { "s" })
                } else {
                    String::new()
                },
                id: f.id,
                name: f.name,
                updated_at: f.updated_at,
            })
            .collect();
        out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        out
    }

    pub fn load_factory(&self, game: GameId, id: &str) -> Result<Factory> {
        read_json(&self.factory_path(game, id)?)
    }

    pub fn save_factory(&self, f: &Factory) -> Result<()> {
        write_json(&self.factory_path(f.game, &f.id)?, f)
    }

    pub fn delete_factory(&self, game: GameId, id: &str) -> Result<()> {
        remove_file(&self.factory_path(game, id)?)
    }

    pub fn list_worlds(&self, game: GameId) -> Vec<World> {
        let mut out = list_json::<World>(&self.game_dir(game).join("worlds"));
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    pub fn load_world(&self, game: GameId, id: &str) -> Result<World> {
        read_json(&self.world_path(game, id)?)
    }

    pub fn save_world(&self, w: &World) -> Result<()> {
        write_json(&self.world_path(w.game, &w.id)?, w)
    }

    pub fn delete_world(&self, game: GameId, id: &str) -> Result<()> {
        remove_file(&self.world_path(game, id)?)
    }

    pub fn load_settings(&self, game: GameId) -> GameSettings {
        read_json(&self.game_dir(game).join("settings.json")).unwrap_or_default()
    }

    pub fn save_settings(&self, game: GameId, s: &GameSettings) -> Result<()> {
        write_json(&self.game_dir(game).join("settings.json"), s)
    }
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

/// Pretty JSON (2-space), written to a temp file and renamed into place.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn remove_file(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

/// Every parseable document in a folder; a hand-broken file doesn't hide the rest.
fn list_json<T: DeserializeOwned>(dir: &Path) -> Vec<T> {
    let Ok(entries) = fs::read_dir(dir) else { return vec![] };
    entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json") && p.file_name().is_some_and(|n| n != "settings.json"))
        .filter_map(|p| read_json(&p).ok())
        .collect()
}
