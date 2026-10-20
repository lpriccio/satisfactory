//! Builds data/factorio.json from Factorio's `--dump-data` output.
//!
//!   factorio --dump-data    # writes ~/.factorio/script-output/data-raw-dump.json
//!   planner-import factorio [path to data-raw-dump.json]
//!
//! Display names come from the game's English locale files and from enabled
//! mods' zips.

use anyhow::{Context, Result};
use planner_core::model::{
    Building, Form, GameData, GameFeatures, GameId, Item, Recipe, Skipped, Stack,
};
use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io::Read;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

const GAME_DATA: &str = "/mnt/pomni/SteamLibrary/steamapps/common/Factorio/data";
const BUILTIN_MODS: [&str; 5] = ["core", "base", "space-age", "quality", "elevated-rails"];
const SKIP_CATEGORIES: [&str; 2] = ["recycling", "parameters"];
const BUILDING_TYPES: [&str; 3] = ["assembling-machine", "furnace", "rocket-silo"];
const MAX_SKIPPED: usize = 500;

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

fn default_dump() -> PathBuf {
    home().join(".factorio/script-output/data-raw-dump.json")
}

fn mods_dir() -> PathBuf {
    home().join(".factorio/mods")
}

pub fn run(root: &Path, input: Option<PathBuf>) -> Result<()> {
    let dump = input.unwrap_or_else(default_dump);
    let data = build(&dump)?;
    println!(
        "factorio: {} recipes, {} items, {} buildings, {} raw, {} skipped",
        data.recipes.len(),
        data.items.len(),
        data.buildings.len(),
        data.default_imports.len(),
        data.skipped.as_ref().map_or(0, Vec::len)
    );
    crate::output::write_game_data(root, &data)
}

// ---------------------------------------------------------------------------
// The dump, with the order of groups and prototypes kept: default recipe
// choices depend on which recipe the dump lists first.

struct Ordered<T>(Vec<(String, T)>);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Ordered<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct V<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for V<T> {
            type Value = Ordered<T>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a map")
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut out = Vec::new();
                while let Some((k, v)) = map.next_entry::<String, T>()? {
                    out.push((k, v));
                }
                Ok(Ordered(out))
            }
            // Lua empty tables may come out as [].
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                while seq.next_element::<serde::de::IgnoredAny>()?.is_some() {}
                Ok(Ordered(Vec::new()))
            }
        }
        d.deserialize_any(V(PhantomData))
    }
}

type Raw = Ordered<Ordered<Value>>;

fn group<'a>(raw: &'a Raw, name: &str) -> &'a [(String, Value)] {
    raw.0
        .iter()
        .find(|(k, _)| k == name)
        .map_or(&[], |(_, g)| g.0.as_slice())
}

// --- JS-style value helpers -------------------------------------------------

fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

fn num(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64)
}

fn string(v: Option<&Value>) -> Option<&str> {
    v.and_then(Value::as_str)
}

/// Arrays as-is; Lua tables dumped as objects by their values.
fn list(v: Option<&Value>) -> Vec<&Value> {
    match v {
        Some(Value::Array(a)) => a.iter().collect(),
        Some(Value::Object(o)) => o.values().collect(),
        _ => vec![],
    }
}

fn strings(v: Option<&Value>) -> Vec<String> {
    list(v)
        .into_iter()
        .filter_map(|x| x.as_str().map(String::from))
        .collect()
}

/// Math.round(x * 10^6) / 10^6.
fn round6(x: f64) -> f64 {
    ((x * 1e6) + 0.5).floor() / 1e6
}

/// JavaScript's String.prototype.trim (also strips a BOM).
fn js_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

fn parse_energy(s: Option<&str>, unit: char) -> Option<f64> {
    let t = js_trim(s?);
    let t = t.strip_suffix(unit)?;
    let (t, scale) = match t.chars().last()? {
        'k' => (&t[..t.len() - 1], 1e-3),
        'M' => (&t[..t.len() - 1], 1.0),
        'G' => (&t[..t.len() - 1], 1e3),
        'T' => (&t[..t.len() - 1], 1e6),
        _ => (t, 1e-6),
    };
    let digits = t.trim_end_matches(|c: char| c.is_whitespace());
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    Some(digits.parse::<f64>().ok()? * scale)
}

fn parse_energy_mw(s: Option<&str>) -> f64 {
    parse_energy(s, 'W').unwrap_or(0.0)
}

fn parse_energy_mj(s: Option<&str>) -> Option<f64> {
    parse_energy(s, 'J')
}

fn humanize(id: &str) -> String {
    let s = id.replace('-', " ");
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => s,
    }
}

// ---------------------------------------------------------------------------
// Locale

#[derive(Default)]
struct Locale(HashMap<String, HashMap<String, String>>);

impl Locale {
    fn parse_cfg(&mut self, text: &str) {
        let mut section = String::new();
        for line in text.split('\n') {
            let t = js_trim(line.strip_suffix('\r').unwrap_or(line));
            if t.is_empty() || t.starts_with(';') || t.starts_with('#') {
                continue;
            }
            if t.len() >= 2 && t.starts_with('[') && t.ends_with(']') {
                section = t[1..t.len() - 1].to_string();
                continue;
            }
            let Some(eq) = t.find('=') else { continue };
            if eq == 0 {
                continue;
            }
            self.0
                .entry(section.clone())
                .or_default()
                .insert(t[..eq].to_string(), t[eq + 1..].to_string());
        }
    }

    fn get(&self, section: &str, id: &str) -> Option<&String> {
        self.0.get(section)?.get(id)
    }

    /// Replace __ITEM__x__ style references with the referenced name.
    fn resolve_refs(&self, s: &str, depth: u32) -> String {
        if depth > 3 {
            return s.to_string();
        }
        const KINDS: [(&str, &[&str]); 4] = [
            ("ITEM", &["item-name", "entity-name", "equipment-name"]),
            ("FLUID", &["fluid-name"]),
            ("ENTITY", &["entity-name", "item-name"]),
            ("RECIPE", &["recipe-name"]),
        ];
        let b = s.as_bytes();
        let mut out = String::new();
        let mut i = 0;
        let mut copied = 0;
        while i < b.len() {
            let mut matched = None;
            if b[i..].starts_with(b"__") {
                for (kind, sections) in KINDS {
                    let head = format!("__{kind}__");
                    if !b[i..].starts_with(head.as_bytes()) {
                        continue;
                    }
                    let start = i + head.len();
                    if let Some(end) = ref_id_end(b, start) {
                        matched = Some((start, end, sections));
                        break;
                    }
                }
            }
            if let Some((start, end, sections)) = matched {
                let id = &s[start..end];
                let whole_end = end + 2;
                out.push_str(&s[copied..i]);
                match sections.iter().find_map(|sec| self.get(sec, id)) {
                    Some(v) => out.push_str(&self.resolve_refs(v, depth + 1)),
                    None => out.push_str(&s[i..whole_end]),
                }
                i = whole_end;
                copied = i;
            } else {
                i += 1;
            }
        }
        out.push_str(&s[copied..]);
        out
    }

    fn lookup(&self, sections: &[&str], id: &str) -> Option<String> {
        for sec in sections {
            let Some(v) = self.get(sec, id) else { continue };
            let resolved = self.resolve_refs(v, 0);
            // Parameterized strings ("__1__ dock") need runtime arguments we don't have.
            return if has_placeholder(&resolved) {
                None
            } else {
                Some(resolved)
            };
        }
        None
    }
}

/// End (exclusive) of the lazy id in /([^_]+(?:_[^_]+)*?)__/ starting at `start`.
fn ref_id_end(b: &[u8], start: usize) -> Option<usize> {
    let mut j = start;
    loop {
        let seg = j;
        while j < b.len() && b[j] != b'_' {
            j += 1;
        }
        if j == seg {
            return None;
        }
        if b[j..].starts_with(b"__") {
            return Some(j);
        }
        if j + 1 < b.len() && b[j] == b'_' && b[j + 1] != b'_' {
            j += 1;
            continue;
        }
        return None;
    }
}

/// /__\w+__/ in JavaScript (ASCII word characters).
fn has_placeholder(s: &str) -> bool {
    let b = s.as_bytes();
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    for i in 0..b.len().saturating_sub(1) {
        if b[i] != b'_' || b[i + 1] != b'_' {
            continue;
        }
        let mut run_end = i + 2;
        while run_end < b.len() && word(b[run_end]) {
            run_end += 1;
        }
        for j in i + 3..run_end.saturating_sub(1) {
            if b[j] == b'_' && b[j + 1] == b'_' {
                return true;
            }
        }
    }
    false
}

fn enabled_mods() -> Vec<String> {
    let Ok(text) = fs::read_to_string(mods_dir().join("mod-list.json")) else {
        return vec![];
    };
    let Ok(list) = serde_json::from_str::<Value>(&text) else {
        return vec![];
    };
    list["mods"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m["enabled"].as_bool() == Some(true))
        .filter_map(|m| m["name"].as_str())
        .filter(|n| !BUILTIN_MODS.contains(n))
        .map(String::from)
        .collect()
}

fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let pa: Vec<f64> = a
        .split('.')
        .map(|x| x.parse().unwrap_or(f64::NAN))
        .collect();
    let pb: Vec<f64> = b
        .split('.')
        .map(|x| x.parse().unwrap_or(f64::NAN))
        .collect();
    for i in 0..pa.len().max(pb.len()) {
        let d = pa.get(i).copied().unwrap_or(0.0) - pb.get(i).copied().unwrap_or(0.0);
        if d != 0.0 && !d.is_nan() {
            return d.partial_cmp(&0.0).unwrap();
        }
    }
    std::cmp::Ordering::Equal
}

/// `<mod>_<version>.zip` -> version.
fn zip_version<'a>(file: &'a str, module: &str) -> Option<&'a str> {
    let v = file
        .strip_prefix(module)?
        .strip_prefix('_')?
        .strip_suffix(".zip")?;
    let ok = !v.is_empty()
        && v.split('.')
            .all(|p| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit()));
    ok.then_some(v)
}

fn load_locale() -> Locale {
    let mut locale = Locale::default();
    for module in BUILTIN_MODS {
        let dir = Path::new(GAME_DATA).join(module).join("locale/en");
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "cfg"))
            .collect();
        files.sort();
        for f in files {
            if let Ok(text) = fs::read_to_string(&f) {
                locale.parse_cfg(&text);
            }
        }
    }
    let Ok(entries) = fs::read_dir(mods_dir()) else {
        return locale;
    };
    let zips: Vec<String> = entries
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|f| f.ends_with(".zip"))
        .collect();
    for module in enabled_mods() {
        let mut candidates: Vec<(&String, &str)> = zips
            .iter()
            .filter_map(|f| zip_version(f, &module).map(|v| (f, v)))
            .collect();
        candidates.sort_by(|a, b| compare_versions(b.1, a.1));
        let Some((file, _)) = candidates.first() else {
            continue;
        };
        // Equivalent of `unzip -p <zip> '*/locale/en/*.cfg'`: every matching
        // file, concatenated in archive order.
        let Ok(reader) = fs::File::open(mods_dir().join(file)) else {
            continue;
        };
        let Ok(mut archive) = zip::ZipArchive::new(reader) else {
            continue;
        };
        let mut text = String::new();
        for i in 0..archive.len() {
            let Ok(mut entry) = archive.by_index(i) else {
                continue;
            };
            let name = entry.name().to_string();
            let matches = name.find("/locale/en/").is_some_and(|p| p >= 1)
                && name.ends_with(".cfg")
                && entry.is_file();
            if matches {
                let mut buf = Vec::new();
                if entry.read_to_end(&mut buf).is_ok() {
                    text.push_str(&String::from_utf8_lossy(&buf));
                }
            }
        }
        locale.parse_cfg(&text);
    }
    locale
}

// ---------------------------------------------------------------------------
// Build

struct StackList {
    stacks: Vec<Stack>,
    ignored: BTreeMap<String, f64>,
    bad: Option<String>,
}

fn stack_list(entries: Option<&Value>, is_result: bool) -> StackList {
    let mut amounts: Vec<(String, f64)> = vec![];
    let mut ignored: BTreeMap<String, f64> = BTreeMap::new();
    for e in list(entries) {
        let kind = string(e.get("type"));
        if kind != Some("item") && kind != Some("fluid") {
            continue;
        }
        let name = string(e.get("name")).unwrap_or_default().to_string();
        let mut amount = if let Some(a) = num(e.get("amount")) {
            a
        } else if let (Some(lo), Some(hi)) = (num(e.get("amount_min")), num(e.get("amount_max"))) {
            (lo + hi) / 2.0
        } else {
            return StackList {
                stacks: vec![],
                ignored,
                bad: Some(format!("no amount for {name}")),
            };
        };
        if is_result {
            let p = num(e.get("probability")).unwrap_or(1.0);
            amount = amount * p + num(e.get("extra_count_fraction")).unwrap_or(0.0);
            if truthy(e.get("ignored_by_productivity")) {
                *ignored.entry(name.clone()).or_insert(0.0) +=
                    num(e.get("ignored_by_productivity")).unwrap_or(0.0) * p;
            }
        }
        match amounts.iter_mut().find(|(n, _)| *n == name) {
            Some((_, a)) => *a += amount,
            None => amounts.push((name, amount)),
        }
    }
    let stacks = amounts
        .into_iter()
        .filter(|(_, a)| *a > 0.0)
        .map(|(item, a)| Stack {
            item,
            amount: round6(a),
        })
        .collect();
    StackList {
        stacks,
        ignored,
        bad: None,
    }
}

struct Skips(Vec<Skipped>);

impl Skips {
    fn add(&mut self, id: &str, reason: impl Into<String>) {
        if self.0.len() < MAX_SKIPPED {
            self.0.push(Skipped {
                id: id.to_string(),
                reason: reason.into(),
            });
        }
    }
}

/// Order machines fastest first; ties by name.
fn by_speed(
    buildings: &BTreeMap<String, Building>,
) -> impl Fn(&String, &String) -> std::cmp::Ordering + '_ {
    move |a, b| {
        let sa = buildings[a].speed.unwrap_or(1.0);
        let sb = buildings[b].speed.unwrap_or(1.0);
        sb.partial_cmp(&sa)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.cmp(b))
    }
}

pub fn build(dump_path: &Path) -> Result<GameData> {
    let file =
        fs::File::open(dump_path).with_context(|| format!("opening {}", dump_path.display()))?;
    let raw: Raw = serde_json::from_reader(std::io::BufReader::new(file))
        .with_context(|| format!("parsing {}", dump_path.display()))?;
    let locale = load_locale();
    let mut skipped = Skips(vec![]);

    // Buildings
    let mut buildings: BTreeMap<String, Building> = BTreeMap::new();
    for kind in BUILDING_TYPES {
        for (name, p) in group(&raw, kind) {
            let flags = strings(p.get("flags"));
            if !flags.iter().any(|f| f == "player-creation") || truthy(p.get("hidden")) {
                skipped.add(name, format!("{kind} not player-creatable or hidden"));
                continue;
            }
            let footprint = p.get("collision_box").and_then(|b| {
                let x1 = b.get(0)?.get(0)?.as_f64()?;
                let y1 = b.get(0)?.get(1)?.as_f64()?;
                let x2 = b.get(1)?.get(0)?.as_f64()?;
                let y2 = b.get(1)?.get(1)?.as_f64()?;
                Some((x2 - x1).ceil() * (y2 - y1).ceil())
            });
            let prod = num(p
                .get("effect_receiver")
                .and_then(|e| e.get("base_effect"))
                .and_then(|e| e.get("productivity")));
            buildings.insert(
                name.clone(),
                Building {
                    id: name.clone(),
                    name: locale
                        .lookup(&["entity-name"], name)
                        .unwrap_or_else(|| humanize(name)),
                    power_mw: round6(parse_energy_mw(string(p.get("energy_usage")))),
                    speed: Some(num(p.get("crafting_speed")).unwrap_or(1.0)),
                    categories: Some(strings(p.get("crafting_categories"))),
                    base_productivity: prod.filter(|x| *x != 0.0),
                    module_slots: num(p.get("module_slots")).map(|x| x as u32),
                    footprint: footprint.filter(|x| *x != 0.0 && !x.is_nan()),
                    ..Default::default()
                },
            );
        }
    }

    let mut by_category: HashMap<String, Vec<String>> = HashMap::new();
    for b in buildings.values() {
        for c in b.categories.iter().flatten() {
            by_category.entry(c.clone()).or_default().push(b.id.clone());
        }
    }
    for list in by_category.values_mut() {
        list.sort_by(by_speed(&buildings));
    }

    // Recipes, kept in dump order for default-recipe choices.
    let mut item_protos: HashMap<&str, &Value> = HashMap::new();
    for (_, g) in &raw.0 {
        for (name, p) in &g.0 {
            if p.is_object() && p.get("stack_size").is_some_and(Value::is_number) {
                item_protos.entry(name.as_str()).or_insert(p);
            }
        }
    }
    let fluid_protos: HashMap<&str, &Value> = group(&raw, "fluid")
        .iter()
        .map(|(k, v)| (k.as_str(), v))
        .collect();
    let item_name = |id: &str| -> String {
        if fluid_protos.contains_key(id) {
            return locale
                .lookup(&["fluid-name"], id)
                .unwrap_or_else(|| humanize(id));
        }
        let p = item_protos.get(id);
        locale
            .lookup(&["item-name"], id)
            .or_else(|| {
                let place = p
                    .and_then(|p| p.get("place_result"))
                    .filter(|v| truthy(Some(v)))?;
                locale.lookup(&["entity-name"], place.as_str()?)
            })
            .or_else(|| {
                let place = p
                    .and_then(|p| p.get("place_as_equipment_result"))
                    .filter(|v| truthy(Some(v)))?;
                locale.lookup(&["equipment-name"], place.as_str()?)
            })
            .or_else(|| locale.lookup(&["entity-name", "equipment-name"], id))
            .unwrap_or_else(|| humanize(id))
    };

    let mut recipes: BTreeMap<String, Recipe> = BTreeMap::new();
    let mut order: Vec<String> = vec![];
    let mut main_product: HashMap<String, Option<String>> = HashMap::new();
    for (name, r) in group(&raw, "recipe") {
        let category = string(r.get("category")).unwrap_or("crafting").to_string();
        if SKIP_CATEGORIES.contains(&category.as_str()) {
            continue; // recycling alone is thousands of recipes; not recorded
        }
        if truthy(r.get("hidden")) {
            skipped.add(name, "hidden");
            continue;
        }
        if truthy(r.get("parameter")) {
            skipped.add(name, "parameter recipe");
            continue;
        }
        let res = stack_list(r.get("results"), true);
        if let Some(bad) = res.bad {
            skipped.add(name, bad);
            continue;
        }
        if res.stacks.is_empty() {
            skipped.add(name, "no results");
            continue;
        }
        let ing = stack_list(r.get("ingredients"), false);
        if let Some(bad) = ing.bad {
            skipped.add(name, bad);
            continue;
        }
        let mut categories = vec![category.clone()];
        categories.extend(strings(r.get("additional_categories")));
        let mut able: Vec<String> = vec![];
        for c in &categories {
            for b in by_category.get(c).into_iter().flatten() {
                if !able.contains(b) {
                    able.push(b.clone());
                }
            }
        }
        if able.is_empty() {
            skipped.add(
                name,
                format!("no machine for category {}", categories.join(", ")),
            );
            continue;
        }
        able.sort_by(by_speed(&buildings));

        let main = string(r.get("main_product"))
            .filter(|s| !s.is_empty())
            .map(String::from)
            .or_else(|| (res.stacks.len() == 1).then(|| res.stacks[0].item.clone()));
        main_product.insert(name.clone(), main.clone());

        let display = locale
            .lookup(&["recipe-name"], name)
            .unwrap_or_else(|| main.as_deref().map_or_else(|| humanize(name), item_name));
        recipes.insert(
            name.clone(),
            Recipe {
                id: name.clone(),
                name: display,
                time: num(r.get("energy_required")).unwrap_or(0.5),
                inputs: ing.stacks,
                outputs: res.stacks,
                buildings: able,
                category: Some(category),
                allow_productivity: truthy(r.get("allow_productivity")),
                ignored_by_productivity: (!res.ignored.is_empty()).then_some(res.ignored),
                ..Default::default()
            },
        );
        order.push(name.clone());
    }

    // Raw resources
    let mut raw_ids: BTreeSet<String> = BTreeSet::new();
    let mut add_minable = |m: Option<&Value>| {
        let Some(m) = m else { return };
        if let Some(r) = string(m.get("result")).filter(|s| !s.is_empty()) {
            raw_ids.insert(r.to_string());
        }
        for r in list(m.get("results")) {
            if let Some(n) = string(r.get("name")).filter(|s| !s.is_empty()) {
                raw_ids.insert(n.to_string());
            }
        }
        if let Some(fl) = string(m.get("fluid")).filter(|s| !s.is_empty()) {
            raw_ids.insert(fl.to_string());
        }
    };
    for g in ["resource", "plant", "fish"] {
        for (_, p) in group(&raw, g) {
            add_minable(p.get("minable"));
        }
    }
    for (_, p) in group(&raw, "tile") {
        if let Some(fl) = string(p.get("fluid")).filter(|s| !s.is_empty()) {
            raw_ids.insert(fl.to_string());
        }
    }
    for (name, _) in group(&raw, "asteroid-chunk") {
        if !name.starts_with("parameter-") && name != "asteroid-chunk-unknown" {
            raw_ids.insert(name.clone());
        }
    }

    let mut produced: HashSet<String> = HashSet::new();
    let mut used: HashSet<String> = HashSet::new();
    for r in recipes.values() {
        produced.extend(r.outputs.iter().map(|s| s.item.clone()));
        used.extend(r.inputs.iter().map(|s| s.item.clone()));
    }
    for id in &used {
        if !produced.contains(id) {
            raw_ids.insert(id.clone());
        }
    }

    // Items
    let mut keep: BTreeSet<String> = produced.union(&used).cloned().collect();
    keep.extend(
        raw_ids
            .iter()
            .filter(|id| {
                item_protos.contains_key(id.as_str()) || fluid_protos.contains_key(id.as_str())
            })
            .cloned(),
    );
    let mut items: BTreeMap<String, Item> = BTreeMap::new();
    for id in &keep {
        let fluid = fluid_protos.get(id.as_str()).copied();
        let Some(proto) = fluid.or_else(|| item_protos.get(id.as_str()).copied()) else {
            skipped.add(id, "referenced but no item or fluid prototype");
            continue;
        };
        let mj =
            parse_energy_mj(string(proto.get("fuel_value"))).filter(|x| *x != 0.0 && !x.is_nan());
        items.insert(
            id.clone(),
            Item {
                id: id.clone(),
                name: item_name(id),
                form: fluid.map(|_| Form::Liquid),
                raw: raw_ids.contains(id),
                recipe: None,
                energy_mj: mj.map(round6),
            },
        );
    }

    // Default recipe per item
    let is_barrel = |id: &str| id.contains("barrel") || id.starts_with("empty-");
    let mut producers: Vec<(String, Vec<String>)> = vec![];
    for rid in &order {
        for s in &recipes[rid].outputs {
            match producers.iter_mut().find(|(i, _)| *i == s.item) {
                Some((_, l)) => l.push(rid.clone()),
                None => producers.push((s.item.clone(), vec![rid.clone()])),
            }
        }
    }
    for (id, list) in &producers {
        let Some(item) = items.get_mut(id) else {
            continue;
        };
        let pick = list
            .iter()
            .find(|r| *r == id)
            .or_else(|| {
                list.iter().find(|r| {
                    !is_barrel(r)
                        && main_product.get(*r).and_then(|m| m.as_deref()) == Some(id.as_str())
                })
            })
            .or_else(|| list.iter().find(|r| !is_barrel(r)))
            .or(list.first());
        item.recipe = pick.cloned();
    }

    let default_imports: Vec<String> = items
        .values()
        .filter(|i| i.raw)
        .map(|i| i.id.clone())
        .collect();
    let version = if group(&raw, "utility-constants").is_empty() {
        ""
    } else {
        "Factorio 2.0 "
    };

    Ok(GameData {
        game: GameId::Factorio,
        name: "Factorio".into(),
        source: format!(
            "{} ({version}{})",
            dump_path.display(),
            enabled_mods().join(", ")
        ),
        generated_at: planner_core::factory::now_iso(),
        items,
        buildings,
        recipes,
        features: GameFeatures {
            clock: false,
            sloops: false,
            power: true,
            productivity: true,
            speed_bonus: true,
            footprint: true,
            area_unit: Some("tiles".into()),
        },
        default_imports,
        resource_weights: None,
        skipped: Some(skipped.0),
        cache: Default::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers_match_javascript() {
        assert_eq!(parse_energy_mw(Some("150kW")), 0.15);
        assert_eq!(parse_energy_mw(Some("1.5MW")), 1.5);
        assert_eq!(parse_energy_mw(Some("nonsense")), 0.0);
        assert_eq!(parse_energy_mj(Some("4MJ")), Some(4.0));
        assert_eq!(humanize("electronic-circuit"), "Electronic circuit");
        assert!(has_placeholder("__1__ dock"));
        assert!(!has_placeholder("Iron plate"));
        assert_eq!(
            zip_version("Flare Stack_4.1.0.zip", "Flare Stack"),
            Some("4.1.0")
        );
        assert_eq!(zip_version("Flare Stack_4.1.0.zip", "Flare"), None);
        let mut l = Locale::default();
        l.parse_cfg(
            "[item-name]\niron-plate=Iron plate\n[recipe-name]\nfoo=Melt __ITEM__iron-plate__\n",
        );
        assert_eq!(
            l.lookup(&["recipe-name"], "foo").as_deref(),
            Some("Melt Iron plate")
        );
    }

    /// Compare JSON loosely: numbers within 1e-9 relative, `skipped` as a set,
    /// `generatedAt` ignored.
    fn diff(path: &str, a: &Value, b: &Value, out: &mut Vec<String>) {
        if out.len() > 40 {
            return;
        }
        match (a, b) {
            (Value::Number(x), Value::Number(y)) => {
                let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
                if (x - y).abs() > 1e-9 * x.abs().max(y.abs()).max(1e-300) && x != y {
                    out.push(format!("{path}: {x} vs {y}"));
                }
            }
            (Value::Object(x), Value::Object(y)) => {
                for k in x.keys().chain(y.keys()).collect::<BTreeSet<_>>() {
                    if path.is_empty() && k == "generatedAt" {
                        continue;
                    }
                    let p = format!("{path}/{k}");
                    match (x.get(k), y.get(k)) {
                        (Some(u), Some(v)) => diff(&p, u, v, out),
                        (u, v) => out.push(format!("{p}: {u:?} vs {v:?}")),
                    }
                }
            }
            (Value::Array(x), Value::Array(y)) if path == "/skipped" => {
                let key = |v: &Value| v.to_string();
                let sx: BTreeSet<String> = x.iter().map(key).collect();
                let sy: BTreeSet<String> = y.iter().map(key).collect();
                for d in sx.symmetric_difference(&sy) {
                    out.push(format!("/skipped: {d}"));
                }
            }
            (Value::Array(x), Value::Array(y)) => {
                if x.len() != y.len() {
                    out.push(format!("{path}: length {} vs {}", x.len(), y.len()));
                    return;
                }
                for (i, (u, v)) in x.iter().zip(y).enumerate() {
                    diff(&format!("{path}/{i}"), u, v, out);
                }
            }
            _ if a != b => out.push(format!("{path}: {a} vs {b}")),
            _ => {}
        }
    }

    #[test]
    fn matches_committed_data() {
        let dump = default_dump();
        if !dump.exists() {
            println!("skipping: no Factorio dump at {}", dump.display());
            return;
        }
        let built = serde_json::to_value(build(&dump).unwrap()).unwrap();
        let committed_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../data/factorio.json");
        let committed: Value =
            serde_json::from_str(&fs::read_to_string(committed_path).unwrap()).unwrap();
        let mut out = vec![];
        diff("", &built, &committed, &mut out);
        assert!(
            out.is_empty(),
            "{} differences:\n{}",
            out.len(),
            out.join("\n")
        );
    }
}
