//! Converts the old Streamlit saved chains (saved_chains/<game>/*.json) into
//! target-mode factories under factories/<game>/<id>.json, keeping each old
//! chain id.
//!
//!   planner-import chains [saved_chains/]
//!
//! Satisfactory names in the old app came from a hand-made TSV with its own
//! abbreviations (MF, RIP, HSC) and shortened alternate names, so recipes are
//! matched by name first and otherwise by their ingredient/product signature
//! against recipes.tsv; each signature match also teaches the item aliases.

use anyhow::{Context, Result};
use planner_core::factory::{iso_from_unix, now_iso};
use planner_core::model::*;
use planner_core::solve::default_recipe;
use planner_core::store::Store;
use serde::de::{Deserializer, MapAccess, Visitor};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

const CHAINS_FALLBACK: &str = "/home/laura/playground/satisfactory/saved_chains";
const TSV_FALLBACK: &str = "/home/laura/playground/satisfactory/recipes.tsv";

/// A JSON object read in file order (the old app's maps carry meaning in their order
/// only for messages, but keeping it makes notes match what the TypeScript tool wrote).
#[derive(Debug, Clone)]
struct Ordered<V>(Vec<(String, V)>);

impl<V> Default for Ordered<V> {
    fn default() -> Self {
        Ordered(vec![])
    }
}

impl<'de, V: Deserialize<'de>> Deserialize<'de> for Ordered<V> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Vis<V>(PhantomData<V>);
        impl<'de, V: Deserialize<'de>> Visitor<'de> for Vis<V> {
            type Value = Ordered<V>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("an object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
                let mut v = vec![];
                while let Some(entry) = a.next_entry()? {
                    v.push(entry);
                }
                Ok(Ordered(v))
            }
            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(Ordered(vec![]))
            }
        }
        d.deserialize_any(Vis(PhantomData))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct OldChain {
    id: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
    target_item: String,
    target_rate: f64,
    #[serde(default)]
    recipe_selections: Ordered<String>,
    #[serde(default)]
    speed_multipliers: Ordered<f64>,
    #[serde(default)]
    productivity_multipliers: Ordered<f64>,
    #[serde(default)]
    imported_items: Vec<String>,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    updated_at: Option<String>,
}

#[derive(Debug, Clone)]
struct LegacyRecipe {
    name: String,
    time: f64,
    inputs: Vec<(String, f64)>,
    outputs: Vec<(String, f64)>,
}

// --- legacy TSV ---------------------------------------------------------------

/// JavaScript's `Number(s)` for the plain decimals the TSV holds.
fn js_number(s: &str) -> f64 {
    if s.is_empty() { 0.0 } else { s.parse().unwrap_or(f64::NAN) }
}

/// Parse the old Satisfactory recipes.tsv the way the old loader did.
fn parse_legacy_tsv(tsv: &str) -> Vec<LegacyRecipe> {
    let rows: Vec<Vec<&str>> = tsv.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l).split('\t').map(str::trim).collect()).collect();
    let Some(header) = rows.first() else { return vec![] };
    let index = |name: &str| header.iter().position(|h| *h == name);
    let at = |row: &[&str], name: &str| -> String { index(name).and_then(|i| row.get(i)).map_or(String::new(), |s| s.to_string()) };
    let mut out: Vec<LegacyRecipe> = vec![];
    for row in &rows[1..] {
        let name = at(row, "Recipe");
        let building = at(row, "Building");
        if name.is_empty() || name == "xxx" || building.is_empty() || building.starts_with('#') {
            continue;
        }
        let time = js_number(&at(row, "Runtime"));
        let amount = js_number(&at(row, "Amount"));
        let item = at(row, "Item");
        // `time > 0` is false for NaN too, like the JavaScript check.
        let positive_time = time > 0.0;
        if !positive_time || item.is_empty() || !amount.is_finite() || amount == 0.0 || item == "MW" {
            continue;
        }
        let i = match out.iter().position(|r| r.name == name) {
            Some(i) => i,
            None => {
                out.push(LegacyRecipe { name: name.clone(), time, inputs: vec![], outputs: vec![] });
                out.len() - 1
            }
        };
        let r = &mut out[i];
        if amount < 0.0 { &mut r.inputs } else { &mut r.outputs }.push((item, amount.abs()));
    }
    out.retain(|r| !r.outputs.is_empty());
    out
}

// --- name and signature matching ----------------------------------------------

fn norm(s: &str) -> String {
    s.trim().to_lowercase()
}

/// Drop a leading "Alternate:" (any case) and the spaces after it.
fn strip_alt(s: &str) -> &str {
    match s.get(..10) {
        Some(head) if head.eq_ignore_ascii_case("alternate:") => s[10..].trim_start(),
        _ => s,
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 0.01 * a.abs().max(b.abs())
}

/// Legacy item name -> game item id, in the order the pairs were found.
type Pairs = Vec<(String, String)>;

/// All ways to pair legacy stacks with game stacks (same amounts, consistent with known aliases).
fn pairings(legacy: &[(String, f64)], game: &[Stack], items: &HashMap<String, String>) -> Vec<Pairs> {
    fn pick(
        i: usize,
        legacy: &[(String, f64)],
        game: &[Stack],
        items: &HashMap<String, String>,
        used: &mut Vec<bool>,
        acc: &mut Pairs,
        out: &mut Vec<Pairs>,
    ) {
        if i == legacy.len() {
            out.push(acc.clone());
            return;
        }
        let (item, amount) = &legacy[i];
        let known = items.get(item);
        for (j, g) in game.iter().enumerate() {
            if used[j] || !close(*amount, g.amount) || known.is_some_and(|k| *k != g.item) {
                continue;
            }
            used[j] = true;
            acc.push((item.clone(), g.item.clone()));
            pick(i + 1, legacy, game, items, used, acc, out);
            acc.pop();
            used[j] = false;
        }
    }
    if legacy.len() != game.len() {
        return vec![];
    }
    let mut out = vec![];
    pick(0, legacy, game, items, &mut vec![false; game.len()], &mut vec![], &mut out);
    out
}

fn signature_matches(l: &LegacyRecipe, g: &Recipe, items: &HashMap<String, String>) -> Vec<Pairs> {
    if g.generates_mw.is_some() || !close(l.time, g.time) {
        return vec![];
    }
    let ins = pairings(&l.inputs, &g.inputs, items);
    if ins.is_empty() {
        return vec![];
    }
    let outs = pairings(&l.outputs, &g.outputs, items);
    let mut merged = vec![];
    for a in &ins {
        for b in &outs {
            let mut m = a.clone();
            for (k, v) in b {
                match m.iter_mut().find(|(x, _)| x == k) {
                    Some(e) => e.1 = v.clone(),
                    None => m.push((k.clone(), v.clone())),
                }
            }
            merged.push(m);
        }
    }
    merged
}

#[derive(Debug, Clone, Default)]
pub struct NameMaps {
    items: HashMap<String, String>,
    recipes: HashMap<String, String>,
}

/// Build legacy-name -> game-id maps for items and recipes.
fn build_name_maps(data: &GameData, legacy: &[LegacyRecipe]) -> NameMaps {
    let item_by_name: HashMap<String, String> = data.items.values().map(|i| (norm(&i.name), i.id.clone())).collect();
    let mut recipe_by_name: HashMap<String, String> = HashMap::new();
    for r in data.recipes.values() {
        recipe_by_name.insert(norm(&r.name), r.id.clone());
        recipe_by_name.entry(norm(strip_alt(&r.name))).or_insert_with(|| r.id.clone());
    }
    for r in data.recipes.values() {
        recipe_by_name.entry(norm(&r.id)).or_insert_with(|| r.id.clone());
    }

    let mut items: HashMap<String, String> = HashMap::new();
    for l in legacy {
        for (name, _) in l.inputs.iter().chain(&l.outputs) {
            if items.contains_key(name) {
                continue;
            }
            let id = item_by_name.get(&norm(name)).cloned().or_else(|| data.items.contains_key(name).then(|| name.clone()));
            if let Some(id) = id {
                items.insert(name.clone(), id);
            }
        }
    }

    let mut recipes: HashMap<String, String> = HashMap::new();
    let mut pending: Vec<&LegacyRecipe> = legacy.iter().collect();
    let mut done: HashSet<String> = HashSet::new();
    let accept = |l: &LegacyRecipe, gid: &str, pairs: &Pairs, items: &mut HashMap<String, String>, recipes: &mut HashMap<String, String>, done: &mut HashSet<String>| {
        recipes.insert(l.name.clone(), gid.to_string());
        for (a, b) in pairs {
            items.entry(a.clone()).or_insert_with(|| b.clone());
        }
        done.insert(l.name.clone());
    };

    // Pass 1: names, confirmed by signature when one is possible.
    for l in &pending {
        let Some(gid) = recipe_by_name.get(&norm(&l.name)) else { continue };
        let pairs = signature_matches(l, &data.recipes[gid], &items);
        if let Some(p) = pairs.first() {
            accept(l, gid, p, &mut items, &mut recipes, &mut done);
        }
    }
    pending.retain(|l| !done.contains(&l.name));

    // Pass 2: signatures, repeated while new aliases unlock more matches.
    let mut progress = true;
    while progress {
        progress = false;
        for l in &pending {
            if done.contains(&l.name) {
                continue;
            }
            let found: Vec<(&Recipe, Vec<Pairs>)> = data
                .recipes
                .values()
                .map(|g| (g, signature_matches(l, g, &items)))
                .filter(|(_, p)| !p.is_empty())
                .collect();
            if found.is_empty() {
                continue;
            }
            // A candidate whose name matches wins; otherwise accept only an unambiguous match.
            let wanted = norm(&l.name);
            let named = found.iter().find(|(g, _)| norm(strip_alt(&g.name)) == wanted || norm(&g.name) == wanted);
            let choice = named.or_else(|| (found.len() == 1 && found[0].1.len() == 1).then(|| &found[0]));
            let Some((g, pairs)) = choice else { continue };
            accept(l, &g.id, &pairs[0], &mut items, &mut recipes, &mut done);
            progress = true;
        }
        pending.retain(|l| !done.contains(&l.name));
    }

    // Unconfirmed name matches still beat nothing (e.g. a recipe the game rebalanced).
    for l in &pending {
        if let Some(gid) = recipe_by_name.get(&norm(&l.name)) {
            recipes.insert(l.name.clone(), gid.clone());
        }
    }
    NameMaps { items, recipes }
}

// --- chain -> factory ----------------------------------------------------------

/// An old timestamp as ISO UTC. The old app wrote naive local times; GNU `date`
/// reads those as local, the way JavaScript's `new Date()` did.
fn iso(s: Option<&str>, fallback: &str) -> String {
    let Some(s) = s.filter(|s| !s.is_empty()) else { return fallback.to_string() };
    // Epoch milliseconds; `-u` would also make date read the input as UTC.
    std::process::Command::new("date")
        .args(["-d", s, "+%s%3N"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|t| t.trim().parse::<i64>().ok())
        .map(|ms| iso_from_unix(ms.div_euclid(1000), ms.rem_euclid(1000) as u32))
        .unwrap_or_else(|| fallback.to_string())
}

/// `Number(x.toFixed(6))`.
fn round6(x: f64) -> f64 {
    format!("{x:.6}").parse().unwrap_or(x)
}

pub fn migrate_chain(chain: &OldChain, game: GameId, data: &GameData, maps: &NameMaps, now: &str) -> (Factory, Vec<String>) {
    let mut problems = vec![];
    let item_by_name: HashMap<String, String> = data.items.values().map(|i| (norm(&i.name), i.id.clone())).collect();
    let map_item = |name: &str| -> Option<String> {
        maps.items
            .get(name)
            .cloned()
            .or_else(|| item_by_name.get(&norm(name)).cloned())
            .or_else(|| data.items.contains_key(name).then(|| name.to_string()))
    };
    let map_recipe = |name: &str| -> Option<String> {
        if let Some(id) = maps.recipes.get(name) {
            return Some(id.clone());
        }
        if data.recipes.contains_key(name) {
            return Some(name.to_string());
        }
        let wanted = norm(name);
        data.recipes.values().find(|r| norm(&r.name) == wanted || norm(strip_alt(&r.name)) == wanted).map(|r| r.id.clone())
    };

    let target = map_item(&chain.target_item);
    if target.is_none() {
        problems.push(format!("target item \"{}\"", chain.target_item));
    }

    let mut recipe_choice = BTreeMap::new();
    for (item_name, recipe_name) in &chain.recipe_selections.0 {
        let (Some(item), Some(recipe)) = (map_item(item_name), map_recipe(recipe_name)) else {
            problems.push(format!("recipe choice \"{item_name}\" → \"{recipe_name}\""));
            continue;
        };
        // The old app let you pick a recipe that only makes the item as a byproduct;
        // keep it only when the recipe really outputs the item.
        if !data.recipes[&recipe].outputs.iter().any(|s| s.item == item) {
            problems.push(format!("recipe choice \"{item_name}\" → \"{recipe_name}\" (that recipe doesn't make it)"));
            continue;
        }
        if Some(&recipe) != default_recipe(data, &item).as_ref() {
            recipe_choice.insert(item, recipe);
        }
    }

    let mut line_settings: BTreeMap<String, LineSettings> = BTreeMap::new();
    let mut setting = |recipe_name: &str, problems: &mut Vec<String>, change: &dyn Fn(&mut LineSettings)| match map_recipe(recipe_name) {
        Some(recipe) => change(line_settings.entry(recipe).or_default()),
        None => problems.push(format!("multiplier for recipe \"{recipe_name}\"")),
    };
    for (name, m) in &chain.speed_multipliers.0 {
        if *m == 1.0 {
            continue;
        }
        if data.features.clock {
            setting(name, &mut problems, &|s| s.clock = Some(*m));
        } else {
            setting(name, &mut problems, &|s| s.speed_bonus = Some(m - 1.0));
        }
    }
    for (name, m) in &chain.productivity_multipliers.0 {
        if *m == 1.0 {
            continue;
        }
        if !data.features.productivity {
            problems.push(format!("productivity for \"{name}\" (not a feature of this game)"));
            continue;
        }
        setting(name, &mut problems, &|s| s.prod_bonus = Some(round6(m - 1.0)));
    }

    let mut imports: Vec<String> = vec![];
    for name in &chain.imported_items {
        match map_item(name) {
            None => problems.push(format!("imported item \"{name}\"")),
            Some(item) => {
                if !data.default_imports.contains(&item) && !imports.contains(&item) {
                    imports.push(item);
                }
            }
        }
    }
    imports.sort();

    let mut notes = vec![];
    if let Some(d) = chain.description.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        notes.push(d.to_string());
    }
    if !problems.is_empty() {
        notes.push(format!("Migration: couldn't map {}.", problems.join("; ")));
    }
    let created_at = iso(chain.created_at.as_deref(), now);
    let factory = Factory {
        schema: 1,
        id: chain.id.clone(),
        game,
        name: chain.name.clone(),
        notes: (!notes.is_empty()).then(|| notes.join("\n\n")),
        mode: Mode::Target,
        objective: Objective::MinRaw,
        targets: target.map(|item| vec![Target { item, rate: chain.target_rate }]).unwrap_or_default(),
        recipe_choice,
        optimize: false,
        line_settings,
        lines: vec![],
        imports: FactoryImports { items: imports, edges: vec![], exclude: vec![] },
        spacing: 4.0,
        updated_at: iso(chain.updated_at.as_deref(), &created_at),
        created_at,
    };
    (factory, problems)
}

// --- running it -----------------------------------------------------------------

fn first_existing(candidates: &[PathBuf]) -> PathBuf {
    candidates.iter().find(|p| p.exists()).unwrap_or(&candidates[candidates.len() - 1]).clone()
}

pub fn default_chains_dir(root: &Path) -> PathBuf {
    first_existing(&[root.join("..").join("saved_chains"), PathBuf::from(CHAINS_FALLBACK)])
}

pub fn default_tsv(root: &Path) -> PathBuf {
    first_existing(&[root.join("..").join("recipes.tsv"), PathBuf::from(TSV_FALLBACK)])
}

/// Migrate every saved chain under `chains`, writing factories into `root`.
/// Returns (game, factory, problems) per chain.
pub fn migrate_all(root: &Path, chains: &Path, tsv: &Path) -> Result<Vec<(GameId, Factory, Vec<String>)>> {
    let store = Store::new(root);
    let now = now_iso();
    let mut out = vec![];
    for (game, legacy) in [(GameId::Satisfactory, Some(tsv)), (GameId::Dsp, None)] {
        let dir = chains.join(game.key());
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut files: Vec<PathBuf> =
            entries.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
        if files.is_empty() {
            continue;
        }
        files.sort();
        let data = store.load_game_data(game)?;
        let maps = match legacy {
            Some(tsv) => {
                let text = std::fs::read_to_string(tsv).with_context(|| format!("reading {}", tsv.display()))?;
                build_name_maps(&data, &parse_legacy_tsv(&text))
            }
            None => NameMaps::default(),
        };
        for file in files {
            let text = std::fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?;
            let chain: OldChain = serde_json::from_str(&text).with_context(|| format!("parsing {}", file.display()))?;
            let (factory, problems) = migrate_chain(&chain, game, &data, &maps, &now);
            store.save_factory(&factory)?;
            out.push((game, factory, problems));
        }
    }
    Ok(out)
}

pub fn run(root: &Path, input: Option<PathBuf>) -> Result<()> {
    let chains = input.unwrap_or_else(|| default_chains_dir(root));
    let done = migrate_all(root, &chains, &default_tsv(root))?;
    for (game, f, problems) in &done {
        let extra = if problems.is_empty() { String::new() } else { format!("  — couldn't map: {}", problems.join("; ")) };
        println!("{}/{}  {}{extra}", game.key(), f.id, f.name);
    }
    println!("{} chains migrated.", done.len());
    Ok(())
}

// --- tests --------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::t9::tests::{differences, planner_dir, temp_root};
    use planner_core::solve::solve_factory;

    #[test]
    fn chains_reproduce_committed_factories() {
        let chains = default_chains_dir(&planner_dir());
        if !chains.exists() {
            eprintln!("skipping: no saved chains at {}", chains.display());
            return;
        }
        let root = temp_root("chains", &["satisfactory.json", "dsp.json"]);
        let done = migrate_all(&root, &chains, &default_tsv(&planner_dir())).unwrap();
        assert_eq!(done.len(), 5);
        let committed = Store::new(planner_dir());
        let mut diffs = vec![];
        for (game, got, _) in &done {
            match committed.load_factory(*game, &got.id) {
                Ok(want) => diffs.extend(differences(got, &want)),
                Err(e) => diffs.push(format!("{}: no committed file ({e})", got.id)),
            }
        }
        assert!(diffs.is_empty(), "{diffs:#?}");

        // Every migrated chain solves, and Alclad 1 still asks for 10 Alclad sheets.
        for (game, f, _) in &done {
            let data = committed.load_game_data(*game).unwrap();
            assert_ne!(solve_factory(&data, f, &GameSettings::default()).status, PlanStatus::Error, "{}", f.name);
        }
        let alclad = done.iter().find(|(_, f, _)| f.name == "Alclad 1").unwrap();
        assert_eq!(alclad.1.targets, vec![Target { item: "AluminumPlate".into(), rate: 10.0 }]);
    }

    #[test]
    fn chains_naive_times_read_as_local() {
        // Whatever the zone, the result is ISO UTC with milliseconds.
        let t = iso(Some("2026-01-11T15:52:31.392883"), "x");
        assert_eq!(t.len(), 24, "{t}");
        assert!(t.ends_with('Z'));
        assert_eq!(iso(Some("2026-01-11T15:52:31Z"), "x"), "2026-01-11T15:52:31.000Z");
        assert_eq!(iso(None, "fallback"), "fallback");
        assert_eq!(iso(Some("not a date"), "fallback"), "fallback");
    }

    #[test]
    fn chains_strip_alternate_prefix() {
        assert_eq!(strip_alt("Alternate: Pure Iron Ingot"), "Pure Iron Ingot");
        assert_eq!(strip_alt("ALTERNATE:Cast Screw"), "Cast Screw");
        assert_eq!(strip_alt("Iron Plate"), "Iron Plate");
    }
}
