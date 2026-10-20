//! Converts Satisfactory's CommunityResources/Docs/en-US.json into
//! data/satisfactory.json (GameData).

use crate::output::write_game_data;
use anyhow::{Context, Result};
use planner_core::factory::now_iso;
use planner_core::model::*;
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

const DEFAULT_DOCS: &str =
    "/mnt/pomni/SteamLibrary/steamapps/common/Satisfactory/CommunityResources/Docs/en-US.json";

/// Floor area per machine in m² (width × length). The game's docs export has
/// no building dimensions, so this table is maintained by hand.
/// "TSV" = the Size column of the legacy recipes.tsv (preferred when present);
/// "wiki" = published building dimensions.
const FOOTPRINTS: &[(&str, f64)] = &[
    ("SmelterMk1", 54.0),                 // TSV (6 × 9)
    ("ConstructorMk1", 80.0),             // TSV (8 × 10)
    ("AssemblerMk1", 150.0),              // TSV (10 × 15)
    ("FoundryMk1", 90.0),                 // TSV (10 × 9)
    ("ManufacturerMk1", 360.0),           // TSV (18 × 20)
    ("OilRefinery", 200.0),               // TSV (10 × 20)
    ("Packager", 64.0),                   // TSV (8 × 8)
    ("Blender", 288.0),                   // TSV (18 × 16)
    ("HadronCollider", 912.0),            // TSV (24 × 38), Particle Accelerator
    ("Converter", 256.0),                 // TSV (16 × 16)
    ("QuantumEncoder", 1200.0),           // TSV (wiki gives 22 × 48 = 1056)
    ("GeneratorCoal", 260.0),             // TSV (10 × 26)
    ("GeneratorFuel", 400.0),             // TSV (20 × 20)
    ("GeneratorNuclear", 1548.0),         // TSV (36 × 43)
    ("GeneratorBiomass_Automated", 64.0), // wiki, approximate (8 × 8)
];

/// Approximate Satisfactory 1.0 world maximum extraction rates (items or m³
/// per minute) used for scarcity weights. Water is effectively unlimited.
const WORLD_MAX: &[(&str, f64)] = &[
    ("OreIron", 92100.0),
    ("OreCopper", 36900.0),
    ("Stone", 69300.0),
    ("Coal", 42300.0),
    ("OreGold", 15000.0),
    ("RawQuartz", 13500.0),
    ("Sulfur", 10800.0),
    ("OreBauxite", 12300.0),
    ("OreUranium", 2100.0),
    ("SAM", 10200.0),
    ("LiquidOil", 12600.0),
    ("NitrogenGas", 12000.0),
];

const ITEM_CLASSES: &[&str] = &[
    "FGResourceDescriptor",
    "FGItemDescriptor",
    "FGEquipmentDescriptor",
    "FGItemDescriptorBiomass",
    "FGConsumableDescriptor",
    "FGAmmoTypeProjectile",
    "FGAmmoTypeSpreadshot",
    "FGAmmoTypeInstantHit",
    "FGItemDescriptorNuclearFuel",
    "FGPowerShardDescriptor",
    "FGItemDescriptorPowerBoosterFuel",
];
const MANUFACTURER_CLASSES: &[&str] = &[
    "FGBuildableManufacturer",
    "FGBuildableManufacturerVariablePower",
];
const GENERATOR_CLASSES: &[&str] = &["FGBuildableGeneratorFuel", "FGBuildableGeneratorNuclear"];

pub fn run(root: &Path, input: Option<PathBuf>) -> Result<()> {
    let path = input.unwrap_or_else(|| PathBuf::from(DEFAULT_DOCS));
    let docs = read_docs(&path)?;
    let data = import(&docs, &path.to_string_lossy())?;
    write_game_data(root, &data)?;
    let gens = data
        .recipes
        .values()
        .filter(|r| r.generates_mw.is_some())
        .count();
    let skipped = data.skipped.as_deref().unwrap_or_default();
    println!(
        "items {}, recipes {}, generator recipes {gens}, buildings {}, skipped {}",
        data.items.len(),
        data.recipes.len() - gens,
        data.buildings.len(),
        skipped.len()
    );
    for s in skipped {
        println!("  skipped {}: {}", s.id, s.reason);
    }
    Ok(())
}

/// The docs file is UTF-16LE with a byte-order mark.
pub fn read_docs(path: &Path) -> Result<Value> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    let text = String::from_utf16(&units).context("docs file is not UTF-16LE")?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    serde_json::from_str(text).context("parsing docs JSON")
}

pub fn item_id(class_name: &str) -> String {
    let s = class_name.strip_suffix("_C").unwrap_or(class_name);
    s.strip_prefix("Desc_").unwrap_or(s).to_string()
}

pub fn recipe_id(class_name: &str) -> String {
    let s = class_name.strip_suffix("_C").unwrap_or(class_name);
    s.strip_prefix("Recipe_").unwrap_or(s).to_string()
}

pub fn building_id(class_name: &str) -> String {
    let s = class_name.strip_suffix("_C").unwrap_or(class_name);
    s.strip_prefix("Build_").unwrap_or(s).to_string()
}

/// Like JavaScript's `parseFloat(String(v ?? '0')) || 0`.
fn num(v: Option<&Value>) -> f64 {
    let s = match v {
        None | Some(Value::Null) => return 0.0,
        Some(Value::Number(n)) => return n.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => s.trim_start(),
        Some(_) => return 0.0,
    };
    // Longest leading prefix that parses as a number.
    let mut end = 0;
    for (i, ch) in s.char_indices() {
        if ch.is_ascii_digit() || matches!(ch, '.' | '-' | '+' | 'e' | 'E') {
            end = i + ch.len_utf8();
        } else {
            break;
        }
    }
    (0..=end)
        .rev()
        .filter(|&e| s.is_char_boundary(e))
        .find_map(|e| s[..e].parse::<f64>().ok())
        .filter(|n| n.is_finite())
        .unwrap_or(0.0)
}

fn text<'a>(c: &'a Value, key: &str) -> &'a str {
    c.get(key).and_then(Value::as_str).unwrap_or("")
}

/// `((ItemClass="…/Desc_X.Desc_X_C'",Amount=4),…)` → [("Desc_X_C", 4), …]
fn parse_stacks(s: &str) -> Vec<(String, f64)> {
    let mut out = vec![];
    let mut rest = s;
    while let Some(pos) = rest.find("ItemClass=\"") {
        rest = &rest[pos + "ItemClass=\"".len()..];
        let Some(close) = rest.find('"') else { break };
        let path = rest[..close].trim_end_matches('\'');
        let class = path.rsplit('.').next().unwrap_or(path);
        let after = rest[close + 1..].trim_start();
        let Some(after) = after.strip_prefix(',') else {
            continue;
        };
        let Some(after) = after.trim_start().strip_prefix("Amount=") else {
            continue;
        };
        let digits: String = after
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        let valid_class =
            !class.is_empty() && class.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if let (true, Ok(amount)) = (valid_class, digits.parse::<f64>()) {
            out.push((class.to_string(), amount));
        }
        rest = &rest[close + 1..];
    }
    out
}

/// `("/Game/…/Build_X.Build_X_C","…")` → ["Build_X_C", …]
fn parse_class_list(s: &str) -> Vec<String> {
    let s = s.strip_prefix('(').unwrap_or(s);
    let s = s.strip_suffix(')').unwrap_or(s);
    s.split(',')
        .map(|p| p.trim().replace('"', ""))
        .filter(|p| !p.is_empty())
        .map(|p| p.rsplit('.').next().unwrap_or(&p).to_string())
        .collect()
}

fn native_name(n: &str) -> String {
    let last = n.rsplit('.').next().unwrap_or(n);
    last.strip_suffix('\'').unwrap_or(last).to_string()
}

/// Approximates JavaScript's `localeCompare` (ICU root collation) for ASCII
/// ids: punctuation before digits before letters, letters compared without
/// case first, then lowercase before uppercase.
fn locale_cmp(a: &str, b: &str) -> Ordering {
    fn primary(c: char) -> (u8, char) {
        if c.is_ascii_alphabetic() {
            (2, c.to_ascii_lowercase())
        } else if c.is_ascii_digit() {
            (1, c)
        } else {
            (0, c)
        }
    }
    let pa: Vec<_> = a.chars().map(primary).collect();
    let pb: Vec<_> = b.chars().map(primary).collect();
    pa.cmp(&pb).then_with(|| {
        // Tertiary: lowercase sorts before uppercase.
        let ta: Vec<bool> = a.chars().map(|c| c.is_ascii_uppercase()).collect();
        let tb: Vec<bool> = b.chars().map(|c| c.is_ascii_uppercase()).collect();
        ta.cmp(&tb)
    })
}

pub fn import(docs: &Value, source: &str) -> Result<GameData> {
    let groups = docs.as_array().context("docs root is not an array")?;
    let mut by_class: Vec<(String, Vec<&Value>)> = vec![];
    for g in groups {
        let k = native_name(text(g, "NativeClass"));
        let classes = g
            .get("Classes")
            .and_then(Value::as_array)
            .map(|v| v.iter().collect::<Vec<_>>())
            .unwrap_or_default();
        match by_class.iter_mut().find(|(n, _)| *n == k) {
            Some((_, list)) => list.extend(classes),
            None => by_class.push((k, classes)),
        }
    }
    let of = |k: &str| -> Vec<&Value> {
        by_class
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    let mut skipped: Vec<Skipped> = vec![];
    let skip = |skipped: &mut Vec<Skipped>, id: &str, reason: String| {
        skipped.push(Skipped {
            id: id.to_string(),
            reason,
        })
    };
    let footprint = |id: &str| FOOTPRINTS.iter().find(|(b, _)| *b == id).map(|(_, f)| *f);
    let positive = |n: f64| (n != 0.0).then_some(n);

    // Items
    let mut items: BTreeMap<String, Item> = BTreeMap::new();
    let mut fluid: HashSet<String> = HashSet::new();
    for (cls, list) in &by_class {
        if !ITEM_CLASSES.contains(&cls.as_str()) {
            continue;
        }
        for c in list {
            let id = item_id(text(c, "ClassName"));
            let form = match text(c, "mForm") {
                "RF_LIQUID" => Form::Liquid,
                "RF_GAS" => Form::Gas,
                _ => Form::Solid,
            };
            if form != Form::Solid {
                fluid.insert(id.clone());
            }
            let energy =
                num(c.get("mEnergyValue")) * if form == Form::Solid { 1.0 } else { 1000.0 };
            items.insert(
                id.clone(),
                Item {
                    id,
                    name: text(c, "mDisplayName").to_string(),
                    form: Some(form),
                    raw: cls == "FGResourceDescriptor",
                    recipe: None,
                    energy_mj: (energy > 0.0).then_some(energy),
                },
            );
        }
    }
    let amount = |id: &str, a: f64| if fluid.contains(id) { a / 1000.0 } else { a };

    // Buildings
    let mut buildings: BTreeMap<String, Building> = BTreeMap::new();
    let mut manufacturers: HashSet<String> = HashSet::new();
    for cls in MANUFACTURER_CLASSES {
        for c in of(cls) {
            let class_name = text(c, "ClassName");
            let id = building_id(class_name);
            manufacturers.insert(class_name.to_string());
            let fp = footprint(&id);
            if fp.is_none() {
                skip(&mut skipped, &id, "no footprint in hand table".into());
            }
            buildings.insert(
                id.clone(),
                Building {
                    id,
                    name: text(c, "mDisplayName").to_string(),
                    power_mw: num(c.get("mPowerConsumption")),
                    power_exponent: positive(num(c.get("mPowerConsumptionExponent"))),
                    sloop_slots: positive(num(c.get("mProductionShardSlotSize"))).map(|n| n as u32),
                    footprint: fp,
                    ..Default::default()
                },
            );
        }
    }

    // Recipes
    let mut recipes: BTreeMap<String, Recipe> = BTreeMap::new();
    for c in of("FGRecipe") {
        let class_name = text(c, "ClassName");
        let id = recipe_id(class_name);
        let produced_in: Vec<String> = parse_class_list(text(c, "mProducedIn"))
            .into_iter()
            .filter(|p| manufacturers.contains(p))
            .collect();
        if produced_in.is_empty() {
            continue; // workbench, build gun, equipment workshop
        }
        let mut to_stacks = |s: &str| -> Option<Vec<Stack>> {
            let mut out = vec![];
            for (cls, a) in parse_stacks(s) {
                let iid = item_id(&cls);
                if !items.contains_key(&iid) {
                    skip(&mut skipped, &id, format!("unknown item {cls}"));
                    return None;
                }
                out.push(Stack {
                    amount: amount(&iid, a),
                    item: iid,
                });
            }
            Some(out)
        };
        let inputs = to_stacks(text(c, "mIngredients"));
        let outputs = to_stacks(text(c, "mProduct"));
        let (Some(inputs), Some(outputs)) = (inputs, outputs) else {
            continue;
        };
        if outputs.is_empty() {
            skip(&mut skipped, &id, "no products".into());
            continue;
        }
        let variable = produced_in
            .iter()
            .any(|p| buildings[&building_id(p)].power_mw == 0.0);
        let power = variable.then(|| {
            let min = num(c.get("mVariablePowerConsumptionConstant"));
            PowerRange {
                min,
                max: min + num(c.get("mVariablePowerConsumptionFactor")),
            }
        });
        recipes.insert(
            id.clone(),
            Recipe {
                id,
                name: text(c, "mDisplayName").to_string(),
                time: num(c.get("mManufactoringDuration")),
                inputs,
                outputs,
                buildings: produced_in.iter().map(|p| building_id(p)).collect(),
                alternate: class_name.contains("Alternate"),
                power,
                ..Default::default()
            },
        );
    }

    // Generators: one synthesized recipe per fuel.
    for cls in GENERATOR_CLASSES {
        for g in of(cls) {
            let gid = building_id(text(g, "ClassName"));
            let gname = text(g, "mDisplayName");
            let mw = num(g.get("mPowerProduction"));
            let fp = footprint(&gid);
            buildings.insert(
                gid.clone(),
                Building {
                    id: gid.clone(),
                    name: gname.to_string(),
                    power_mw: 0.0,
                    power_exponent: positive(num(g.get("mPowerConsumptionExponent"))),
                    footprint: fp,
                    ..Default::default()
                },
            );
            if fp.is_none() {
                skip(&mut skipped, &gid, "no footprint in hand table".into());
            }
            let ratio = num(g.get("mSupplementalToPowerRatio"));
            for f in g
                .get("mFuel")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let fuel = item_id(text(f, "mFuelClass"));
                let rid = format!("Power_{gid}_{fuel}");
                let Some(energy) = items
                    .get(&fuel)
                    .and_then(|i| i.energy_mj)
                    .filter(|e| *e != 0.0)
                else {
                    skip(
                        &mut skipped,
                        &rid,
                        format!("fuel {fuel} has no energy value"),
                    );
                    continue;
                };
                let per_min = mw * 60.0 / energy;
                let mut inputs = vec![Stack {
                    item: fuel.clone(),
                    amount: per_min,
                }];
                let supplemental = text(f, "mSupplementalResourceClass");
                if !supplemental.is_empty() && ratio > 0.0 {
                    inputs.push(Stack {
                        item: item_id(supplemental),
                        amount: mw * ratio * 60.0 / 1000.0,
                    });
                }
                let mut outputs = vec![];
                let byproduct = text(f, "mByproduct");
                if !byproduct.is_empty() {
                    outputs.push(Stack {
                        item: item_id(byproduct),
                        amount: per_min * num(f.get("mByproductAmount")),
                    });
                }
                recipes.insert(
                    rid.clone(),
                    Recipe {
                        id: rid,
                        name: format!("{} in {gname}", items[&fuel].name),
                        time: 60.0,
                        inputs,
                        outputs,
                        buildings: vec![gid.clone()],
                        generates_mw: Some(mw),
                        ..Default::default()
                    },
                );
            }
        }
    }

    // Default recipe per produced item.
    let mut producers: HashMap<String, Vec<&Recipe>> = HashMap::new();
    for r in recipes.values().filter(|r| r.generates_mw.is_none()) {
        for o in &r.outputs {
            producers.entry(o.item.clone()).or_default().push(r);
        }
    }
    let mut defaults: Vec<(String, String)> = vec![];
    for (iid, rs) in &producers {
        let item = &items[iid];
        if item.raw {
            continue;
        }
        let not_unpack: Vec<&Recipe> = rs
            .iter()
            .copied()
            .filter(|r| !r.name.starts_with("Unpackage"))
            .collect();
        let mut pool = if not_unpack.is_empty() {
            rs.clone()
        } else {
            not_unpack
        };
        pool.sort_by(|a, b| locale_cmp(&a.id, &b.id));
        let is_primary = |r: &&Recipe| r.outputs[0].item == *iid;
        let primary: Vec<&Recipe> = pool
            .iter()
            .copied()
            .filter(|r| !r.alternate)
            .filter(is_primary)
            .collect();
        // A recipe that makes the item as its main product (even an alternate)
        // beats one that only yields it as a byproduct: Compacted Coal should
        // not default to building Rocket Fuel.
        let pick = primary
            .iter()
            .find(|r| r.name == item.name)
            .or(primary.first())
            .or(pool.iter().find(|r| r.alternate && is_primary(r)))
            .or(pool.iter().find(|r| !r.alternate))
            .or(pool.first())
            .expect("an item with producers");
        defaults.push((iid.clone(), pick.id.clone()));
    }
    for (iid, rid) in defaults {
        items.get_mut(&iid).unwrap().recipe = Some(rid);
    }

    let mut default_imports: Vec<String> = items
        .values()
        .filter(|i| i.raw)
        .map(|i| i.id.clone())
        .collect();
    default_imports.sort();
    let iron = WORLD_MAX.iter().find(|(k, _)| *k == "OreIron").unwrap().1;
    let resource_weights = default_imports
        .iter()
        .map(|id| {
            (
                id.clone(),
                WORLD_MAX
                    .iter()
                    .find(|(k, _)| k == id)
                    .map_or(0.0, |(_, max)| iron / max),
            )
        })
        .collect();

    Ok(GameData {
        game: GameId::Satisfactory,
        name: "Satisfactory".into(),
        source: source.to_string(),
        generated_at: now_iso(),
        items,
        buildings,
        recipes,
        features: GameFeatures {
            clock: true,
            sloops: true,
            power: true,
            productivity: false,
            speed_bonus: false,
            footprint: true,
            area_unit: Some("m²".into()),
        },
        default_imports,
        resource_weights: Some(resource_weights),
        skipped: Some(skipped),
        cache: Default::default(),
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Compare JSON values: numbers by value (≤ 1e-9 relative), objects
    /// ignoring `generatedAt`. Returns the first difference found.
    pub fn diff(a: &Value, b: &Value, path: &str) -> Option<String> {
        match (a, b) {
            (Value::Number(x), Value::Number(y)) => {
                let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
                ((x - y).abs() > 1e-9 * x.abs().max(y.abs()).max(1e-300))
                    .then(|| format!("{path}: {x} vs {y}"))
            }
            (Value::Array(x), Value::Array(y)) => {
                if x.len() != y.len() {
                    return Some(format!("{path}: length {} vs {}", x.len(), y.len()));
                }
                x.iter()
                    .zip(y)
                    .enumerate()
                    .find_map(|(i, (p, q))| diff(p, q, &format!("{path}[{i}]")))
            }
            (Value::Object(x), Value::Object(y)) => {
                let keys: std::collections::BTreeSet<&String> = x
                    .keys()
                    .chain(y.keys())
                    .filter(|k| *k != "generatedAt")
                    .collect();
                keys.into_iter().find_map(|k| match (x.get(k), y.get(k)) {
                    (Some(p), Some(q)) => diff(p, q, &format!("{path}.{k}")),
                    (p, q) => Some(format!("{path}.{k}: {} vs {}", p.is_some(), q.is_some())),
                })
            }
            _ => (a != b).then(|| format!("{path}: {a} vs {b}")),
        }
    }

    /// Regenerate into a temp root and compare with the committed data file.
    pub fn compare_with_committed(game: &str, data: &GameData) {
        let tmp =
            std::env::temp_dir().join(format!("planner-import-test-{game}-{}", std::process::id()));
        write_game_data(&tmp, data).unwrap();
        let ours: Value = serde_json::from_str(
            &std::fs::read_to_string(tmp.join("data").join(format!("{game}.json"))).unwrap(),
        )
        .unwrap();
        let committed_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../data")
            .join(format!("{game}.json"));
        let committed: Value =
            serde_json::from_str(&std::fs::read_to_string(committed_path).unwrap()).unwrap();
        std::fs::remove_dir_all(&tmp).ok();
        if let Some(d) = diff(&ours, &committed, "") {
            panic!("{game}: regenerated data differs from committed: {d}");
        }
    }

    #[test]
    fn matches_committed_data() {
        let path = Path::new(DEFAULT_DOCS);
        if !path.exists() {
            println!("skipping: {DEFAULT_DOCS} not found");
            return;
        }
        let data = import(&read_docs(path).unwrap(), DEFAULT_DOCS).unwrap();
        compare_with_committed("satisfactory", &data);
    }

    #[test]
    fn parses_stacks_and_class_lists() {
        let s = r#"((ItemClass="/Script/Engine.BlueprintGeneratedClass'/Game/X/Desc_OreIron.Desc_OreIron_C'",Amount=3),(ItemClass="/Game/Y/Desc_Water.Desc_Water_C",Amount=1500))"#;
        assert_eq!(
            parse_stacks(s),
            vec![
                ("Desc_OreIron_C".to_string(), 3.0),
                ("Desc_Water_C".to_string(), 1500.0)
            ]
        );
        assert_eq!(
            parse_class_list(
                r#"("/Game/A/Build_SmelterMk1.Build_SmelterMk1_C","/Game/B/BP_BuildGun.BP_BuildGun_C")"#
            ),
            vec!["Build_SmelterMk1_C", "BP_BuildGun_C"]
        );
        assert_eq!(item_id("Desc_OreIron_C"), "OreIron");
        assert_eq!(
            recipe_id("Recipe_Alternate_PureIronIngot_C"),
            "Alternate_PureIronIngot"
        );
        assert_eq!(num(Some(&Value::String("4.000000".into()))), 4.0);
        assert_eq!(num(Some(&Value::String(String::new()))), 0.0);
    }

    #[test]
    fn locale_order() {
        assert_eq!(locale_cmp("Alternate_X", "AlternateX"), Ordering::Less);
        assert_eq!(locale_cmp("abc", "ABD"), Ordering::Less);
        assert_eq!(locale_cmp("a", "A"), Ordering::Less);
    }
}
