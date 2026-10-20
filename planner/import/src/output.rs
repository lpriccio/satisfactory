//! Writing generated game data: keys sorted, 1-space indent, so regenerating
//! gives small diffs.

use anyhow::Result;
use planner_core::GameData;
use serde::Serialize;
use std::path::Path;

pub fn write_game_data(root: &Path, data: &GameData) -> Result<()> {
    // Round-trip through Value: its maps are sorted, which sorts every key.
    let value = serde_json::to_value(data)?;
    let mut buf = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, serde_json::ser::PrettyFormatter::with_indent(b" "));
    value.serialize(&mut ser)?;
    buf.push(b'\n');
    let path = root.join("data").join(format!("{}.json", data.game.key()));
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, buf)?;
    println!("wrote {}", path.display());
    Ok(())
}
