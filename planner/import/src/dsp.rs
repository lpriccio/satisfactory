//! Dyson Sphere Program has no data export, so recipes come from the
//! hand-maintained TSV (Recipe, Item, Pair, Seconds, Item Count; negative
//! count = input) and go to data/dsp.json.

use crate::output::write_game_data;
use anyhow::{bail, Context, Result};
use planner_core::factory::now_iso;
use planner_core::model::*;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

const DEFAULT_TSV: &str = "/home/laura/playground/satisfactory/recipes_dsp.tsv";

pub fn run(root: &Path, input: Option<PathBuf>) -> Result<()> {
    let path = input.unwrap_or_else(|| PathBuf::from(DEFAULT_TSV));
    let path = std::path::absolute(&path).unwrap_or(path);
    let tsv = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let data = import(&tsv, &path.to_string_lossy())?;
    write_game_data(root, &data)?;
    let skipped = data.skipped.as_deref().unwrap_or_default();
    println!(
        "dsp: {} items, {} recipes, {} raw, {} skipped",
        data.items.len(),
        data.recipes.len(),
        data.default_imports.len(),
        skipped.len()
    );
    for s in skipped {
        println!("  skipped {}: {}", s.id, s.reason);
    }
    Ok(())
}

/// Like JavaScript's `Number(s)` for the cells this file holds: empty is 0,
/// junk is NaN.
fn js_number(s: &str) -> f64 {
    let s = s.trim();
    if s.is_empty() {
        0.0
    } else {
        s.parse().unwrap_or(f64::NAN)
    }
}

pub fn import(tsv: &str, source: &str) -> Result<GameData> {
    let mut lines = tsv.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l));
    let header: Vec<&str> = lines.next().unwrap_or_default().split('\t').map(str::trim).collect();
    let col = |name: &str| header.iter().position(|h| *h == name);
    let (Some(c_recipe), Some(c_item), Some(c_seconds), Some(c_count)) = (col("Recipe"), col("Item"), col("Seconds"), col("Item Count")) else {
        bail!("unexpected header: {}", header.join(", "));
    };

    let mut skipped: Vec<Skipped> = vec![];
    let mut skip = |id: String, reason: String| skipped.push(Skipped { id, reason });
    // Recipes in first-seen order, as the TSV lists them.
    let mut order: Vec<String> = vec![];
    let mut recipes: HashMap<String, Recipe> = HashMap::new();
    for (n, line) in lines.enumerate() {
        let cells: Vec<&str> = line.split('\t').map(str::trim).collect();
        if cells.iter().all(|c| c.is_empty()) {
            continue; // blank spacer rows
        }
        let row = format!("row {}", n + 2);
        let cell = |i: usize| cells.get(i).copied().unwrap_or("");
        let (name, item, secs, count) = (cell(c_recipe), cell(c_item), cell(c_seconds), cell(c_count));
        if name.is_empty() || item.is_empty() {
            skip(row, "missing recipe or item name".into());
            continue;
        }
        let time = js_number(secs);
        let amount = js_number(count);
        if !time.is_finite() || time <= 0.0 {
            skip(format!("{row} ({name})"), format!("bad Seconds \"{secs}\""));
            continue;
        }
        if !amount.is_finite() || amount == 0.0 {
            skip(format!("{row} ({name}/{item})"), format!("bad or zero Item Count \"{count}\""));
            continue;
        }
        let r = recipes.entry(name.to_string()).or_insert_with(|| {
            order.push(name.to_string());
            Recipe { id: name.into(), name: name.into(), time, buildings: vec!["producer".into()], ..Default::default() }
        });
        if r.time != time {
            skip(format!("{row} ({name})"), format!("Seconds {time} differs from first row's {}; using {}", r.time, r.time));
        }
        let stack = Stack { item: item.to_string(), amount: amount.abs() };
        if amount < 0.0 {
            r.inputs.push(stack);
        } else {
            r.outputs.push(stack);
        }
    }

    order.retain(|id| {
        let keep = !recipes[id].outputs.is_empty();
        if !keep {
            skip(id.clone(), "recipe has no outputs".into());
            recipes.remove(id);
        }
        keep
    });

    let mut produced: HashMap<String, Vec<String>> = HashMap::new();
    let mut items: BTreeMap<String, Item> = BTreeMap::new();
    for id in &order {
        let r = &recipes[id];
        for s in r.inputs.iter().chain(&r.outputs) {
            items.entry(s.item.clone()).or_insert_with(|| Item { id: s.item.clone(), name: s.item.clone(), ..Default::default() });
        }
        for (i, s) in r.outputs.iter().enumerate() {
            let list = produced.entry(s.item.clone()).or_default();
            if i == 0 {
                list.insert(0, r.id.clone());
            } else {
                list.push(r.id.clone());
            }
        }
    }
    let mut default_imports = vec![];
    for item in items.values_mut() {
        match produced.get(&item.id).filter(|m| !m.is_empty()) {
            None => {
                item.raw = true;
                default_imports.push(item.id.clone());
            }
            Some(makers) => {
                item.recipe = Some(if makers.contains(&item.id) { item.id.clone() } else { makers[0].clone() });
            }
        }
    }
    default_imports.sort();

    Ok(GameData {
        game: GameId::Dsp,
        name: "Dyson Sphere Program".into(),
        source: source.to_string(),
        generated_at: now_iso(),
        items,
        buildings: BTreeMap::from([(
            "producer".to_string(),
            Building { id: "producer".into(), name: "Producer".into(), power_mw: 0.0, speed: Some(1.0), ..Default::default() },
        )]),
        recipes: recipes.into_iter().collect(),
        features: GameFeatures { productivity: true, speed_bonus: true, ..Default::default() },
        default_imports,
        resource_weights: None,
        skipped: Some(skipped),
        cache: Default::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::satisfactory::tests::compare_with_committed;

    #[test]
    fn matches_committed_data() {
        let path = Path::new(DEFAULT_TSV);
        if !path.exists() {
            println!("skipping: {DEFAULT_TSV} not found");
            return;
        }
        let data = import(&std::fs::read_to_string(path).unwrap(), DEFAULT_TSV).unwrap();
        compare_with_committed("dsp", &data);
    }

    #[test]
    fn parses_rows_and_skips_bad_ones() {
        let tsv = "Recipe\tItem\tPair\tSeconds\tItem Count\n\t\t\t\t\nCB\tIron\tx\t1\t-2\nCB\tCB\tx\t1\t2\nBad\tX\tx\t0\t1\nNone\tY\tx\t2\t-1\n";
        let d = import(tsv, "t").unwrap();
        assert_eq!(d.recipes.len(), 1);
        assert_eq!(d.recipes["CB"].inputs, vec![Stack { item: "Iron".into(), amount: 2.0 }]);
        assert_eq!(d.items["CB"].recipe.as_deref(), Some("CB"));
        assert!(d.items["Iron"].raw);
        let reasons: Vec<&str> = d.skipped.as_ref().unwrap().iter().map(|s| s.reason.as_str()).collect();
        assert_eq!(reasons, vec!["bad Seconds \"0\"", "recipe has no outputs"]);
    }
}
