//! Writing generated game data: keys sorted, 1-space indent, so regenerating
//! gives small diffs.

use anyhow::Result;
use planner_core::GameData;
use serde::Serialize;
use serde_json::Value;
use std::path::Path;

/// Write whole floats as integers (`15`, not `15.0`) to match the existing files.
fn integers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n
                .as_f64()
                .filter(|f| n.is_f64() && f.fract() == 0.0 && f.abs() < 9e15)
            {
                *v = Value::from(f as i64);
            }
        }
        Value::Array(a) => a.iter_mut().for_each(integers),
        Value::Object(o) => o.values_mut().for_each(integers),
        _ => {}
    }
}

pub fn write_game_data(root: &Path, data: &GameData) -> Result<()> {
    // Round-trip through Value: its maps are sorted, which sorts every key.
    let mut value = serde_json::to_value(data)?;
    integers(&mut value);
    let mut buf = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(
        &mut buf,
        serde_json::ser::PrettyFormatter::with_indent(b" "),
    );
    value.serialize(&mut ser)?;
    buf.push(b'\n');
    let path = root.join("data").join(format!("{}.json", data.game.key()));
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, buf)?;
    println!("wrote {}", path.display());
    Ok(())
}
