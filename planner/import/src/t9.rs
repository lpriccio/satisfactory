//! Imports the build tabs of Laura's T9 Satisfactory workbook as manual-mode
//! factories (PRD milestone 4).
//!
//!   planner-import t9 [workbook.ods|xlsx]
//!
//! Reads cached cell values only; the workbook is never written. Writes
//! factories/satisfactory/t9-<tab>.json (re-imports overwrite) and
//! data/t9-recipe-map.json (generated mapping; its `manual` section is kept).
//!
//! Two tab templates exist in T9:
//!   A. "Shard:" in E1/F1, "Scale:" in G1/H1, headers on row 3:
//!      F base qty, G Recipe, H Qty (= F × scale), J Sharded (= H / shard),
//!      K Size (= size × CEILING(J)). Machines = J, each at clock = shard.
//!   B. "Scale:" in G1/H1, headers on row 2: G Recipe, H Qty, J "Size (2.5x)"
//!      (= size × H, fractional). Some tabs add a "SHARDED" column E holding
//!      the real machine count; then clock = H / E.
//! H is always production-equivalent machines at 100% clock, so production
//! matches the workbook whichever way the line is expressed. Neither template
//! records somersloops per row (the sloops sheet tracks them separately), so
//! none are imported.

use anyhow::{Context, Result};
use calamine::{Data, Range, Reader, open_workbook_auto};
use planner_core::factory::now_iso;
use planner_core::model::*;
use planner_core::store::Store;
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

const WORKBOOK_NAME: &str = "20260425 T9 fixedrows.ods";
const WORKBOOK_FALLBACK: &str = "/home/laura/playground/satisfactory/20260425 T9 fixedrows.ods";
const MAP_FILE: &str = "t9-recipe-map.json";

/// Sheets that are not build tabs.
pub const NON_BUILD: [&str; 9] = [
    "planner",
    "sloops",
    "floor calc",
    "trucks",
    "trains",
    "drones",
    "recipes",
    "scratch",
    "wtg",
];

/// Workbook item names that differ from the game's display names.
pub(crate) const ITEM_ALIASES: &[(&str, &str)] = &[
    ("Screw", "IronScrew"),
    ("RIP", "IronPlateReinforced"),
    ("MF", "ModularFrame"),
    ("EIB", "SteelPlateReinforced"),
    ("HOR", "HeavyOilResidue"),
    ("HSC", "HighSpeedConnector"),
    ("Canister", "FluidCanister"),
    ("Alclad Sheet", "AluminumPlate"),
    ("Nitrogen", "NitrogenGas"),
    ("Packaged Nitrogen", "PackagedNitrogenGas"),
    ("Rifle Cartridge", "CartridgeStandard"),
    ("Automated Miner", "BP_ItemDescriptorPortableMiner"),
    ("SAM Fluctator", "SAMFluctuator"),
    ("Diamond", "Diamond"),
    ("Uranium Ore", "OreUranium"),
    ("Packaged Turbo Fuel", "TurboFuel"),
    ("MW", POWER),
];

/// The workbook next to the planner folder, else its usual location.
pub fn default_workbook(root: &Path) -> PathBuf {
    let beside = root.join("..").join(WORKBOOK_NAME);
    if beside.exists() {
        beside
    } else {
        PathBuf::from(WORKBOOK_FALLBACK)
    }
}

// --- numbers ------------------------------------------------------------------

/// `Number(n.toFixed(4))` as JavaScript prints it, so notes read the same as
/// the files the TypeScript importer wrote.
fn round(n: f64) -> String {
    let r: f64 = format!("{n:.4}").parse().unwrap_or(n);
    if r == 0.0 { "0".into() } else { format!("{r}") }
}

/// Signed per-minute flow by item, in first-seen order.
type Vector = Vec<(String, f64)>;

fn vec_add(v: &mut Vector, key: &str, x: f64) {
    match v.iter_mut().find(|(k, _)| k == key) {
        Some((_, y)) => *y += x,
        None => v.push((key.to_string(), x)),
    }
}

fn vec_set(v: &mut Vector, key: &str, x: f64) {
    match v.iter_mut().find(|(k, _)| k == key) {
        Some((_, y)) => *y = x,
        None => v.push((key.to_string(), x)),
    }
}

fn vec_get(v: &Vector, key: &str) -> f64 {
    v.iter().find(|(k, _)| k == key).map_or(0.0, |(_, x)| *x)
}

fn vec_has(v: &Vector, key: &str) -> bool {
    v.iter().any(|(k, _)| k == key)
}

/// Keys of `a` then the keys of `b` not already seen.
fn union_keys<'a>(a: &'a Vector, b: &'a Vector) -> Vec<&'a str> {
    let mut keys: Vec<&str> = a.iter().map(|(k, _)| k.as_str()).collect();
    for (k, _) in b {
        if !keys.contains(&k.as_str()) {
            keys.push(k);
        }
    }
    keys
}

// --- workbook -----------------------------------------------------------------

static EMPTY: Data = Data::Empty;

/// One sheet, addressed by absolute zero-based (row, column) from A1.
pub(crate) struct Grid {
    pub(crate) range: Range<Data>,
}

impl Grid {
    pub(crate) fn at(&self, r: u32, c: u32) -> &Data {
        self.range.get_value((r, c)).unwrap_or(&EMPTY)
    }
    pub(crate) fn num(&self, r: u32, c: u32) -> Option<f64> {
        match self.at(r, c) {
            Data::Float(f) if f.is_finite() => Some(*f),
            Data::Int(i) => Some(*i as f64),
            _ => None,
        }
    }
    pub(crate) fn str(&self, r: u32, c: u32) -> &str {
        match self.at(r, c) {
            Data::String(s) => s.trim(),
            _ => "",
        }
    }
    pub(crate) fn rows(&self) -> u32 {
        self.range.end().map_or(0, |(r, _)| r + 1)
    }
}

pub(crate) const fn col(letter: u8) -> u32 {
    (letter - b'A') as u32
}

// --- item and recipe mapping -----------------------------------------------

pub(crate) struct Resolver {
    by_name: HashMap<String, String>,
}

impl Resolver {
    pub(crate) fn new(data: &GameData) -> Resolver {
        let by_name = data
            .items
            .values()
            .map(|i| (i.name.to_lowercase(), i.id.clone()))
            .collect();
        Resolver { by_name }
    }
    pub(crate) fn resolve(&self, name: &str) -> Option<String> {
        ITEM_ALIASES
            .iter()
            .find(|(alias, _)| *alias == name)
            .map(|(_, id)| id.to_string())
            .or_else(|| self.by_name.get(&name.to_lowercase()).cloned())
    }
}

struct WorkbookRecipe {
    name: String,
    runtime: f64,
    /// item name -> signed amount per cycle, in sheet order
    amounts: Vector,
}

fn read_workbook_recipes(g: &Grid) -> Vec<WorkbookRecipe> {
    let mut out: Vec<WorkbookRecipe> = vec![];
    // A recipe's rows are contiguous. Some recipes appear twice (Silicon HSC);
    // a later block repeats the first, so only the first is read.
    let mut current = String::new();
    let mut done: HashSet<String> = HashSet::new();
    for r in 1..g.rows() {
        let name = g.str(r, 1);
        let item = g.str(r, 2);
        let (Some(amount), Some(runtime)) = (g.num(r, 3), g.num(r, 4)) else {
            continue;
        };
        if name.is_empty() || item.is_empty() || runtime == 0.0 {
            continue;
        }
        if name != current {
            if !current.is_empty() {
                done.insert(current.clone());
            }
            current = name.to_string();
            if done.contains(name) {
                continue;
            }
        } else if done.contains(name) {
            continue;
        }
        let i = match out.iter().position(|w| w.name == name) {
            Some(i) => i,
            None => {
                out.push(WorkbookRecipe {
                    name: name.to_string(),
                    runtime,
                    amounts: vec![],
                });
                out.len() - 1
            }
        };
        vec_add(&mut out[i].amounts, item, amount);
    }
    out
}

/// Per-minute signed flow of one machine at 100%.
fn game_vector(r: &Recipe) -> Vector {
    let mut v = vec![];
    let k = 60.0 / r.time;
    for s in &r.inputs {
        vec_add(&mut v, &s.item, -s.amount * k);
    }
    for s in &r.outputs {
        vec_add(&mut v, &s.item, s.amount * k);
    }
    if let Some(mw) = r.generates_mw {
        vec_set(&mut v, POWER, mw);
    }
    v
}

fn workbook_vector(w: &WorkbookRecipe, res: &Resolver) -> Option<Vector> {
    let mut v = vec![];
    for (name, amount) in &w.amounts {
        let id = res.resolve(name)?;
        vec_add(&mut v, &id, amount * 60.0 / w.runtime);
    }
    Some(v)
}

fn same_vector(a: &Vector, b: &Vector, ignore: Option<&str>) -> bool {
    union_keys(a, b)
        .into_iter()
        .filter(|k| Some(*k) != ignore)
        .all(|k| {
            let (x, y) = (vec_get(a, k), vec_get(b, k));
            (x - y).abs() <= 1e-6 * 1f64.max(x.abs()).max(y.abs())
        })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Generated {
    pub id: String,
    /// name | ingredients | name-rates-differ | manual
    pub how: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Unmatched {
    pub name: String,
    pub reason: String,
}

/// data/t9-recipe-map.json. `generated` keeps the workbook's recipe order.
#[derive(Debug, Clone, Serialize)]
pub struct RecipeMap {
    /// Hand overrides: workbook recipe name -> game recipe id. Kept on regeneration.
    pub manual: BTreeMap<String, String>,
    #[serde(serialize_with = "ordered")]
    pub generated: Vec<(String, Generated)>,
    pub unmatched: Vec<Unmatched>,
}

fn ordered<S: Serializer>(entries: &[(String, Generated)], s: S) -> Result<S::Ok, S::Error> {
    let mut m = s.serialize_map(Some(entries.len()))?;
    for (k, v) in entries {
        m.serialize_entry(k, v)?;
    }
    m.end()
}

impl RecipeMap {
    pub fn get(&self, name: &str) -> Option<&Generated> {
        self.generated
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, g)| g)
    }
}

fn strip_alternate(name: &str) -> String {
    let lower = name.to_lowercase();
    match lower.strip_prefix("alternate:") {
        Some(rest) => rest.trim_start().to_string(),
        None => lower,
    }
}

fn build_recipe_map(
    data: &GameData,
    wb: &[WorkbookRecipe],
    manual: BTreeMap<String, String>,
) -> RecipeMap {
    let res = Resolver::new(data);
    let games: Vec<(&Recipe, Vector)> =
        data.recipes.values().map(|r| (r, game_vector(r))).collect();
    let mut generated = vec![];
    let mut unmatched = vec![];

    for w in wb {
        if let Some(id) = manual.get(&w.name) {
            if data.recipes.contains_key(id) {
                generated.push((
                    w.name.clone(),
                    Generated {
                        id: id.clone(),
                        how: "manual",
                        note: None,
                    },
                ));
            } else {
                unmatched.push(Unmatched {
                    name: w.name.clone(),
                    reason: format!("manual id {id} not in game data"),
                });
            }
            continue;
        }
        let Some(vec) = workbook_vector(w, &res) else {
            let missing: Vec<&str> = w
                .amounts
                .iter()
                .filter(|(n, _)| res.resolve(n).is_none())
                .map(|(n, _)| n.as_str())
                .collect();
            unmatched.push(Unmatched {
                name: w.name.clone(),
                reason: format!("unknown items: {}", missing.join(", ")),
            });
            continue;
        };
        let wanted = w.name.to_lowercase();
        let named: Vec<usize> = (0..games.len())
            .filter(|&i| strip_alternate(&games[i].0.name) == wanted)
            .collect();
        let same: Vec<usize> = (0..games.len())
            .filter(|&i| same_vector(&games[i].1, &vec, None))
            .collect();
        let by_name_and_rates = same.iter().find(|i| named.contains(i));
        // The workbook leaves cooling water out of its generator rows.
        let generator: Vec<usize> = if vec_has(&vec, POWER) && same.is_empty() {
            (0..games.len())
                .filter(|&i| {
                    games[i].0.generates_mw.is_some()
                        && same_vector(&games[i].1, &vec, Some("Water"))
                })
                .collect()
        } else {
            vec![]
        };

        let entry = if let Some(&i) = by_name_and_rates {
            Generated {
                id: games[i].0.id.clone(),
                how: "name",
                note: None,
            }
        } else if generator.len() == 1 {
            Generated {
                id: games[generator[0]].0.id.clone(),
                how: "ingredients",
                note: Some("generator; workbook omits cooling water".into()),
            }
        } else if !same.is_empty() {
            // Prefer a standard recipe when several share the same flows.
            let pick = same
                .iter()
                .copied()
                .find(|&i| !games[i].0.alternate)
                .unwrap_or(same[0]);
            let others: Vec<&str> = same
                .iter()
                .filter(|&&i| i != pick)
                .map(|&i| games[i].0.id.as_str())
                .collect();
            Generated {
                id: games[pick].0.id.clone(),
                how: "ingredients",
                note: (same.len() > 1).then(|| format!("also matches {}", others.join(", "))),
            }
        } else if named.len() == 1 {
            let (r, v) = &games[named[0]];
            Generated {
                id: r.id.clone(),
                how: "name-rates-differ",
                note: Some(diff_note(&vec, v, data)),
            }
        } else {
            let reason = if named.is_empty() {
                "no game recipe with these flows or name"
            } else {
                "several game recipes share the name; rates differ"
            };
            unmatched.push(Unmatched {
                name: w.name.clone(),
                reason: reason.into(),
            });
            continue;
        };
        generated.push((w.name.clone(), entry));
    }
    RecipeMap {
        manual,
        generated,
        unmatched,
    }
}

fn diff_note(wb: &Vector, game: &Vector, data: &GameData) -> String {
    let mut parts = vec![];
    for k in union_keys(wb, game) {
        let (a, b) = (vec_get(wb, k), vec_get(game, k));
        if (a - b).abs() > 1e-6 {
            let name = data.items.get(k).map_or(k, |i| i.name.as_str());
            parts.push(format!(
                "{name}: workbook {}/min, game {}/min",
                round(a),
                round(b)
            ));
        }
    }
    parts.join("; ")
}

fn read_manual(path: &Path) -> BTreeMap<String, String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return BTreeMap::new();
    };
    value
        .get("manual")
        .and_then(|m| m.as_object())
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

// --- build tabs ---------------------------------------------------------------

#[derive(Debug, Clone)]
struct TabRow {
    /// 1-based sheet row.
    row: u32,
    recipe: String,
    /// Production-equivalent machines at 100% (workbook H, already scaled).
    qty: f64,
    /// Real machine count when the tab records one (template A Sharded, B SHARDED).
    sharded: Option<f64>,
}

#[derive(Debug, Clone)]
struct SideNote {
    section: String,
    label: String,
    item: String,
    rate: Option<f64>,
}

#[derive(Debug, Clone)]
struct ParsedTab {
    name: String,
    template: char,
    title: Option<String>,
    shard: f64,
    scale: f64,
    rows: Vec<TabRow>,
    /// Left-hand block: miners, imports, exports, scratch notes.
    side: Vec<SideNote>,
    total_size: Option<f64>,
    foundations_text: Option<String>,
}

fn non_empty(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_string())
}

fn parse_tab(g: &Grid, name: &str) -> Result<ParsedTab, String> {
    let header_row = (0..6)
        .rev()
        .find(|&r| g.str(r, col(b'G')) == "Recipe")
        .ok_or("no \"Recipe\" header in column G")?;

    let template = if g.str(0, col(b'E')) == "Shard:" {
        'A'
    } else {
        'B'
    };
    let shard = if template == 'A' {
        g.num(0, col(b'F')).unwrap_or(1.0)
    } else {
        1.0
    };
    let scale = if g.str(0, col(b'G')) == "Scale:" {
        g.num(0, col(b'H')).unwrap_or(1.0)
    } else {
        1.0
    };
    let sharded_col_b = g.str(header_row, col(b'E')).to_uppercase() == "SHARDED";

    let mut rows = vec![];
    let mut side = vec![];
    let mut section = non_empty(g.str(header_row, col(b'A'))).unwrap_or_else(|| "Notes".into());

    for r in header_row + 1..g.rows() {
        let recipe = g.str(r, col(b'G'));
        if let Some(qty) = g
            .num(r, col(b'H'))
            .filter(|q| !recipe.is_empty() && *q > 1e-9)
        {
            let sharded = if template == 'A' {
                g.num(r, col(b'J'))
            } else if sharded_col_b {
                g.num(r, col(b'E')).filter(|e| *e > 0.0)
            } else {
                None
            };
            rows.push(TabRow {
                row: r + 1,
                recipe: recipe.to_string(),
                qty,
                sharded,
            });
        }

        let a = g.str(r, col(b'A'));
        let b = g.str(r, col(b'B'));
        let c = g.num(r, col(b'C'));
        if !a.is_empty() && b.is_empty() && c.is_none() {
            section = a.to_string();
        } else if !b.is_empty() && (c.is_some() || !a.is_empty()) {
            side.push(SideNote {
                section: section.clone(),
                label: a.to_string(),
                item: b.to_string(),
                rate: c,
            });
        }
    }

    let (size_col, text_col) = if template == 'A' {
        (col(b'K'), col(b'L'))
    } else {
        (col(b'J'), col(b'K'))
    };
    Ok(ParsedTab {
        name: name.to_string(),
        template,
        title: if template == 'A' {
            non_empty(g.str(0, col(b'A')))
        } else {
            None
        },
        shard,
        scale,
        rows,
        side,
        total_size: g.num(0, size_col),
        foundations_text: non_empty(g.str(0, text_col)),
    })
}

pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for ch in name.to_lowercase().chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            out.push(ch);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

fn tab_to_factory(
    tab: &ParsedTab,
    map: &RecipeMap,
    data: &GameData,
    now: &str,
    created: Option<String>,
) -> (Factory, Vec<String>) {
    let id = format!("t9-{}", slug(&tab.name));
    let mut lines = vec![];
    let mut unmapped = vec![];
    for row in &tab.rows {
        let Some(hit) = map
            .get(&row.recipe)
            .filter(|h| data.recipes.contains_key(&h.id))
        else {
            unmapped.push(format!(
                "{} ×{} (row {})",
                row.recipe,
                round(row.qty),
                row.row
            ));
            continue;
        };
        // Express the line as real machines × clock; production stays qty × 100%.
        let (mut machines, mut clock) = (row.qty, None);
        if let Some(s) = row
            .sharded
            .filter(|s| *s > 0.0 && (s - row.qty).abs() > 1e-9)
        {
            machines = s;
            clock = Some(row.qty / s);
        }
        let note = (hit.how == "name-rates-differ").then(|| {
            format!(
                "Workbook rates differ from the game: {}",
                hit.note.as_deref().unwrap_or("")
            )
        });
        lines.push(Line {
            id: format!("{id}-r{}", row.row),
            recipe: hit.id.clone(),
            settings: LineSettings {
                clock,
                ..Default::default()
            },
            machines: Some(machines),
            note,
        });
    }

    let mut notes = vec![];
    if let Some(t) = &tab.title {
        notes.push(t.clone());
    }
    let shard = if tab.template == 'A' {
        format!(", shard {}", round(tab.shard))
    } else {
        String::new()
    };
    notes.push(format!(
        "Imported from T9 workbook tab \"{}\" (template {}, scale {}{shard}).",
        tab.name,
        tab.template,
        round(tab.scale)
    ));
    if let Some(size) = tab.total_size {
        let text = tab
            .foundations_text
            .as_ref()
            .map_or(String::new(), |t| format!(" {t}"));
        notes.push(format!("Workbook size total: {} m²{text}.", round(size)));
    }
    let mut by_section: Vec<(String, Vec<String>)> = vec![];
    for s in &tab.side {
        let label = if s.label.is_empty() {
            String::new()
        } else {
            format!("{} ", s.label)
        };
        let rate = s
            .rate
            .map_or(String::new(), |r| format!(" {}/min", round(r)));
        let text = format!("{label}{}{rate}", s.item);
        match by_section.iter_mut().find(|(k, _)| *k == s.section) {
            Some((_, list)) => list.push(text),
            None => by_section.push((s.section.clone(), vec![text])),
        }
    }
    for (section, list) in &by_section {
        notes.push(format!("{section}: {}", list.join("; ")));
    }
    if !unmapped.is_empty() {
        notes.push(format!(
            "Unmapped workbook recipes (not imported): {}",
            unmapped.join("; ")
        ));
    }

    let factory = Factory {
        schema: 1,
        id,
        game: GameId::Satisfactory,
        name: tab.name.clone(),
        notes: Some(notes.join("\n")),
        mode: Mode::Manual,
        objective: Objective::MinRaw,
        targets: vec![],
        recipe_choice: BTreeMap::new(),
        optimize: false,
        line_settings: BTreeMap::new(),
        lines,
        imports: FactoryImports::default(),
        spacing: 4.0,
        created_at: created.unwrap_or_else(|| now.to_string()),
        updated_at: now.to_string(),
    };
    (factory, unmapped)
}

// --- running it -----------------------------------------------------------------

pub struct Report {
    pub map: RecipeMap,
    /// Tab name, factory id, line count, unmapped count.
    pub imported: Vec<(String, String, usize, usize)>,
    pub skipped: Vec<String>,
}

/// Import every build tab of `workbook` into `root`'s factories and recipe map.
pub fn import(root: &Path, workbook: &Path) -> Result<Report> {
    let store = Store::new(root);
    let data = store.load_game_data(GameId::Satisfactory)?;
    let mut wb =
        open_workbook_auto(workbook).with_context(|| format!("opening {}", workbook.display()))?;
    let recipes = Grid {
        range: wb
            .worksheet_range("recipes")
            .context("the workbook has no `recipes` sheet")?,
    };

    let map_path = root.join("data").join(MAP_FILE);
    let map = build_recipe_map(
        &data,
        &read_workbook_recipes(&recipes),
        read_manual(&map_path),
    );
    planner_core::store::write_json(&map_path, &map)?;

    let now = now_iso();
    let mut imported = vec![];
    let mut skipped = vec![];
    for name in wb.sheet_names() {
        if NON_BUILD.contains(&name.as_str()) {
            continue;
        }
        let grid = Grid {
            range: wb
                .worksheet_range(&name)
                .with_context(|| format!("reading sheet {name}"))?,
        };
        let tab = match parse_tab(&grid, &name) {
            Ok(t) => t,
            Err(e) => {
                skipped.push(format!("{name}: {e}"));
                continue;
            }
        };
        if tab.rows.is_empty() {
            skipped.push(format!("{name}: no recipe rows"));
            continue;
        }
        let id = format!("t9-{}", slug(&tab.name));
        let created = store
            .load_factory(GameId::Satisfactory, &id)
            .ok()
            .map(|f| f.created_at);
        let (factory, unmapped) = tab_to_factory(&tab, &map, &data, &now, created);
        store.save_factory(&factory)?;
        imported.push((name, factory.id, factory.lines.len(), unmapped.len()));
    }
    Ok(Report {
        map,
        imported,
        skipped,
    })
}

pub fn run(root: &Path, input: Option<PathBuf>) -> Result<()> {
    let workbook = input.unwrap_or_else(|| default_workbook(root));
    let report = import(root, &workbook)?;

    let mut counts: Vec<(&str, usize)> = vec![];
    for (_, g) in &report.map.generated {
        match counts.iter_mut().find(|(k, _)| *k == g.how) {
            Some((_, n)) => *n += 1,
            None => counts.push((g.how, 1)),
        }
    }
    let counts: Vec<String> = counts.iter().map(|(k, n)| format!("{k} {n}")).collect();
    println!("Workbook: {}", workbook.display());
    println!(
        "Recipe map: {}, unmatched {}",
        counts.join(", "),
        report.map.unmatched.len()
    );
    for u in &report.map.unmatched {
        println!("  unmatched: {} — {}", u.name, u.reason);
    }
    println!("Imported {} tabs:", report.imported.len());
    for (name, id, lines, unmapped) in &report.imported {
        let extra = if *unmapped > 0 {
            format!(", {unmapped} unmapped")
        } else {
            String::new()
        };
        println!("  {name} → {id} ({lines} lines{extra})");
    }
    if !report.skipped.is_empty() {
        println!("Skipped {}:", report.skipped.len());
        for s in &report.skipped {
            println!("  {s}");
        }
    }
    Ok(())
}

// --- tests --------------------------------------------------------------------

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use planner_core::solve::solve_factory;
    use std::sync::OnceLock;

    pub(crate) fn planner_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
    }

    /// Frozen copies of the original import output. The live files under
    /// factories/ are yours to edit in the app, so tests never compare to them.
    pub(crate) fn expected_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/expected")
    }

    /// A scratch planner root holding copies of the given data files.
    pub(crate) fn temp_root(tag: &str, data_files: &[&str]) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("planner-import-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("data")).unwrap();
        for f in data_files {
            std::fs::copy(
                planner_dir().join("data").join(f),
                root.join("data").join(f),
            )
            .unwrap();
        }
        root
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-6 * 1f64.max(a.abs()).max(b.abs())
    }

    fn squash(s: &Option<String>) -> String {
        s.as_deref()
            .unwrap_or("")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Differences that matter between a regenerated factory and a committed one.
    pub(crate) fn differences(got: &Factory, want: &Factory) -> Vec<String> {
        let mut d = vec![];
        let mut check = |what: &str, ok: bool| {
            if !ok {
                d.push(format!("{}: {what}", want.id));
            }
        };
        check("id", got.id == want.id);
        check("name", got.name == want.name);
        check("game", got.game == want.game);
        check("mode", got.mode == want.mode);
        check("objective", got.objective == want.objective);
        check(
            "targets",
            got.targets.len() == want.targets.len()
                && got
                    .targets
                    .iter()
                    .zip(&want.targets)
                    .all(|(a, b)| a.item == b.item && close(a.rate, b.rate)),
        );
        check("recipe choices", got.recipe_choice == want.recipe_choice);
        check("line settings", got.line_settings == want.line_settings);
        check("imports", got.imports == want.imports);
        check("spacing", close(got.spacing, want.spacing));
        check("notes", squash(&got.notes) == squash(&want.notes));
        check("line count", got.lines.len() == want.lines.len());
        for (a, b) in got.lines.iter().zip(&want.lines) {
            let opt = |x: Option<f64>, y: Option<f64>| match (x, y) {
                (Some(x), Some(y)) => close(x, y),
                (None, None) => true,
                _ => false,
            };
            check(
                &format!("line {}", b.id),
                a.id == b.id
                    && a.recipe == b.recipe
                    && opt(a.machines, b.machines)
                    && opt(a.settings.clock, b.settings.clock)
                    && a.settings.sloops == b.settings.sloops
                    && squash(&a.note) == squash(&b.note),
            );
        }
        d
    }

    /// Import once into a scratch root; tests share it.
    fn imported() -> Option<&'static (PathBuf, Report)> {
        static ONCE: OnceLock<Option<(PathBuf, Report)>> = OnceLock::new();
        ONCE.get_or_init(|| {
            let wb = default_workbook(&planner_dir());
            if !wb.exists() {
                eprintln!("skipping: workbook not found at {}", wb.display());
                return None;
            }
            let root = temp_root("t9", &["satisfactory.json", MAP_FILE]);
            let report = import(&root, &wb).expect("import");
            Some((root, report))
        })
        .as_ref()
    }

    #[test]
    fn t9_reproduces_committed_factories() {
        let Some((root, report)) = imported() else {
            return;
        };
        let committed = Store::new(expected_dir());
        let fresh = Store::new(root);
        assert_eq!(report.imported.len(), 49, "skipped: {:?}", report.skipped);
        let mut diffs = vec![];
        for s in committed
            .list_factories(GameId::Satisfactory)
            .iter()
            .filter(|s| s.id.starts_with("t9-"))
        {
            let want = committed.load_factory(GameId::Satisfactory, &s.id).unwrap();
            match fresh.load_factory(GameId::Satisfactory, &s.id) {
                Ok(got) => diffs.extend(differences(&got, &want)),
                Err(_) => diffs.push(format!("{}: not produced", s.id)),
            }
        }
        assert!(diffs.is_empty(), "{diffs:#?}");
    }

    #[test]
    fn t9_recipe_map_matches_committed() {
        let Some((_, report)) = imported() else {
            return;
        };
        let committed: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(planner_dir().join("data").join(MAP_FILE)).unwrap(),
        )
        .unwrap();
        assert_eq!(serde_json::to_value(&report.map).unwrap(), committed);
    }

    #[test]
    fn t9_pasta_matches_the_workbook() {
        let Some((root, _)) = imported() else { return };
        let store = Store::new(root);
        let data = store.load_game_data(GameId::Satisfactory).unwrap();
        let pasta = store
            .load_factory(GameId::Satisfactory, "t9-pasta")
            .unwrap();
        let plan = solve_factory(&data, &pasta, &GameSettings::default());
        assert!(close(plan.power_mw, 7440.0), "power {}", plan.power_mw);
        assert!(close(plan.machines, 95.0), "machines {}", plan.machines);
        assert!(
            close(plan.foundations.unwrap(), 1335.0),
            "foundations {:?}",
            plan.foundations
        );
    }

    #[test]
    fn t9_slugs_and_numbers() {
        assert_eq!(slug("Builders Beach"), "builders-beach");
        assert_eq!(slug("  floor calc! "), "floor-calc");
        assert_eq!(round(21360.0), "21360");
        assert_eq!(round(2.5), "2.5");
        assert_eq!(round(-0.00001), "0");
        assert_eq!(round(1.0 / 3.0), "0.3333");
    }
}
