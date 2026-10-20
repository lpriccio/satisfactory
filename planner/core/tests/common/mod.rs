#![allow(dead_code)]

use planner_core::factory::new_factory;
use planner_core::store::read_json;
use planner_core::*;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * (1.0 + b.abs())
}

#[macro_export]
macro_rules! assert_close {
    ($a:expr, $b:expr) => {{
        let (a, b): (f64, f64) = ($a, $b);
        assert!(
            common::close(a, b),
            "{} = {a}, expected {b}",
            stringify!($a)
        );
    }};
}

fn recipe(
    id: &str,
    time: f64,
    inputs: &[(&str, f64)],
    outputs: &[(&str, f64)],
    building: &str,
) -> Recipe {
    let stacks = |s: &[(&str, f64)]| {
        s.iter()
            .map(|(i, a)| Stack {
                item: i.to_string(),
                amount: *a,
            })
            .collect()
    };
    Recipe {
        id: id.into(),
        name: id.into(),
        time,
        inputs: stacks(inputs),
        outputs: stacks(outputs),
        buildings: vec![building.into()],
        ..Default::default()
    }
}

fn building(id: &str, power: f64, slots: u32, footprint: f64) -> Building {
    Building {
        id: id.into(),
        name: id.into(),
        power_mw: power,
        power_exponent: (slots > 0).then_some(1.321928),
        sloop_slots: (slots > 0).then_some(slots),
        footprint: Some(footprint),
        ..Default::default()
    }
}

/// A small Satisfactory-shaped data set with numbers from the game.
pub fn mini() -> GameData {
    let mut alt_plastic = recipe(
        "Alternate_RecycledPlastic",
        12.0,
        &[("Rubber", 6.0), ("LiquidFuel", 12.0)],
        &[("Plastic", 12.0)],
        "OilRefinery",
    );
    alt_plastic.alternate = true;
    let mut alt_rubber = recipe(
        "Alternate_RecycledRubber",
        12.0,
        &[("Plastic", 6.0), ("LiquidFuel", 12.0)],
        &[("Rubber", 12.0)],
        "OilRefinery",
    );
    alt_rubber.alternate = true;
    let mut coal_gen = recipe(
        "Power_GeneratorCoal_Coal",
        60.0,
        &[("Coal", 15.0), ("Water", 45.0)],
        &[],
        "GeneratorCoal",
    );
    coal_gen.generates_mw = Some(75.0);
    let recipes = vec![
        recipe(
            "IngotIron",
            2.0,
            &[("OreIron", 1.0)],
            &[("IronIngot", 1.0)],
            "SmelterMk1",
        ),
        recipe(
            "IronPlate",
            6.0,
            &[("IronIngot", 3.0)],
            &[("IronPlate", 2.0)],
            "ConstructorMk1",
        ),
        recipe(
            "IngotCopper",
            2.0,
            &[("OreCopper", 1.0)],
            &[("CopperIngot", 1.0)],
            "SmelterMk1",
        ),
        recipe(
            "AluminumSheet",
            6.0,
            &[("AluminumIngot", 3.0), ("CopperIngot", 1.0)],
            &[("AluminumPlate", 3.0)],
            "AssemblerMk1",
        ),
        recipe(
            "IngotAluminum",
            4.0,
            &[("AluminumScrap", 6.0), ("Silica", 5.0)],
            &[("AluminumIngot", 4.0)],
            "FoundryMk1",
        ),
        recipe(
            "AluminumScrap",
            1.0,
            &[("AluminaSolution", 4.0), ("Coal", 2.0)],
            &[("AluminumScrap", 6.0), ("Water", 2.0)],
            "OilRefinery",
        ),
        recipe(
            "AluminaSolution",
            6.0,
            &[("OreBauxite", 12.0), ("Water", 18.0)],
            &[("AluminaSolution", 12.0), ("Silica", 5.0)],
            "OilRefinery",
        ),
        recipe(
            "Silica",
            8.0,
            &[("RawQuartz", 3.0)],
            &[("Silica", 5.0)],
            "ConstructorMk1",
        ),
        recipe(
            "Plastic",
            6.0,
            &[("LiquidOil", 3.0)],
            &[("Plastic", 2.0), ("HeavyOilResidue", 1.0)],
            "OilRefinery",
        ),
        recipe(
            "Rubber",
            6.0,
            &[("LiquidOil", 3.0)],
            &[("Rubber", 2.0), ("HeavyOilResidue", 2.0)],
            "OilRefinery",
        ),
        alt_plastic,
        alt_rubber,
        coal_gen,
    ];
    let raw = [
        "OreIron",
        "OreCopper",
        "Coal",
        "Water",
        "OreBauxite",
        "RawQuartz",
        "LiquidOil",
    ];
    let items = [
        "OreIron",
        "IronIngot",
        "IronPlate",
        "OreCopper",
        "CopperIngot",
        "AluminumIngot",
        "AluminumPlate",
        "AluminumScrap",
        "AluminaSolution",
        "Silica",
        "Coal",
        "Water",
        "OreBauxite",
        "RawQuartz",
        "Plastic",
        "Rubber",
        "LiquidFuel",
        "LiquidOil",
        "HeavyOilResidue",
    ];
    let buildings = [
        building("SmelterMk1", 4.0, 1, 54.0),
        building("ConstructorMk1", 4.0, 1, 80.0),
        building("AssemblerMk1", 15.0, 2, 150.0),
        building("FoundryMk1", 16.0, 2, 90.0),
        building("OilRefinery", 30.0, 2, 200.0),
        building("GeneratorCoal", 0.0, 0, 260.0),
    ];
    GameData {
        game: GameId::Satisfactory,
        name: "Satisfactory (test)".into(),
        source: "fixture".into(),
        generated_at: "2026-10-19".into(),
        items: items
            .iter()
            .map(|id| {
                (
                    id.to_string(),
                    Item {
                        id: id.to_string(),
                        name: id.to_string(),
                        raw: raw.contains(id),
                        ..Default::default()
                    },
                )
            })
            .collect(),
        buildings: buildings.into_iter().map(|b| (b.id.clone(), b)).collect(),
        recipes: recipes.into_iter().map(|r| (r.id.clone(), r)).collect(),
        features: GameFeatures {
            clock: true,
            sloops: true,
            power: true,
            footprint: true,
            area_unit: Some("m²".into()),
            ..Default::default()
        },
        default_imports: raw.iter().map(|s| s.to_string()).collect(),
        resource_weights: None,
        skipped: None,
        cache: Default::default(),
    }
}

pub fn factory(data: &GameData) -> Factory {
    new_factory(data.game, &GameSettings::default(), "test", Mode::Target)
}

pub fn target(item: &str, rate: f64) -> Target {
    Target {
        item: item.into(),
        rate,
    }
}

pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

pub fn real(game: &str) -> GameData {
    read_json(&root().join("data").join(format!("{game}.json"))).expect("game data")
}

pub fn machines(plan: &Plan, recipe: &str) -> f64 {
    plan.lines
        .iter()
        .filter(|l| l.recipe == recipe)
        .map(|l| l.machines)
        .sum()
}

pub fn imported(plan: &Plan, item: &str) -> f64 {
    plan.item(item).imported
}

pub fn choice(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

pub fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}
