//! Known builds against the real game data in data/, hand-checked.
mod common;

use common::*;
use planner_core::edits::{import_locked_reason, regenerate, reset_plan, toggle_item_import};
use planner_core::solve::{can_produce, solve_factory};
use planner_core::store::Store;
use planner_core::*;
use std::time::Instant;

fn solve(data: &GameData, f: &Factory) -> Plan {
    solve_factory(data, f, &GameSettings::default())
}

#[test]
fn alclad_nets_scrap_water_and_alumina_silica() {
    let d = real("satisfactory");
    let mut f = factory(&d);
    f.targets = vec![target("AluminumPlate", 10.0)];
    let p = solve(&d, &f);
    assert_eq!(p.status, PlanStatus::Ok);
    assert_close!(machines(&p, "AluminumSheet"), 1.0 / 3.0);
    assert_close!(imported(&p, "OreBauxite"), 10.0);
    assert_close!(imported(&p, "Coal"), 5.0);
    assert_close!(imported(&p, "RawQuartz"), 5.0);
    assert_close!(imported(&p, "OreCopper"), 10.0 / 3.0);
    assert_close!(imported(&p, "Water"), 10.0);
}

#[test]
fn recycled_plastic_rubber_loop() {
    let d = real("satisfactory");
    let mut f = factory(&d);
    f.targets = vec![target("Plastic", 60.0)];
    f.recipe_choice = choice(&[("Plastic", "Alternate_Plastic_1"), ("Rubber", "Alternate_RecycledRubber")]);
    f.imports.items = strings(&["LiquidFuel"]);
    let p = solve(&d, &f);
    assert_close!(machines(&p, "Alternate_Plastic_1"), 4.0 / 3.0);
    assert_close!(machines(&p, "Alternate_RecycledRubber"), 2.0 / 3.0);
    // 6 fuel per 12 s cycle in game 1.0: 30/min per machine × 2 machines.
    assert_close!(imported(&p, "LiquidFuel"), 60.0);
}

#[test]
fn iron_plates() {
    let d = real("satisfactory");
    let mut f = factory(&d);
    f.targets = vec![target("IronPlate", 20.0)];
    let p = solve(&d, &f);
    assert_close!(machines(&p, "IronPlate"), 1.0);
    assert_close!(machines(&p, "IngotIron"), 1.0);
    assert_close!(p.power_mw, 8.0);
}

#[test]
fn nuclear_plant_burns_rods_and_makes_waste() {
    let d = real("satisfactory");
    let mut f = factory(&d);
    f.targets = vec![target(POWER, 2500.0)];
    f.imports.exclude = strings(&[POWER]);
    f.imports.items = strings(&["NuclearFuelRod"]);
    let generator = d.recipes.keys().find(|id| id.starts_with("Power_GeneratorNuclear_NuclearFuelRod")).unwrap().clone();
    f.recipe_choice = choice(&[(POWER, &generator)]);
    let p = solve(&d, &f);
    assert_eq!(p.status, PlanStatus::Ok);
    assert_close!(imported(&p, "NuclearFuelRod"), 0.2);
    assert_close!(p.item("NuclearWaste").surplus, 10.0);
    assert_close!(p.generated_mw, 2500.0);
}

#[test]
fn pasta_matches_the_workbook() {
    let d = real("satisfactory");
    let f = Store::new(root()).load_factory(GameId::Satisfactory, "t9-pasta").unwrap();
    let p = solve(&d, &f);
    assert_close!(p.power_mw, 7440.0);
    assert_close!(p.machines, 95.0);
    assert_close!(p.foundations.unwrap(), 1335.0);
}

/// The case from real use: Plastic AI Limiter chosen, crude oil import off.
fn stuck(d: &GameData) -> Factory {
    let mut f = factory(d);
    f.targets = vec![target("CircuitBoardHighSpeed", 24.0)];
    f.recipe_choice = choice(&[("CircuitBoardHighSpeed", "Alternate_AILimiter_Plastic")]);
    f.imports.exclude = strings(&["LiquidOil"]);
    f
}

#[test]
fn shortfall_blames_crude_oil_not_the_target() {
    let d = real("satisfactory");
    let p = solve(&d, &stuck(&d));
    assert_eq!(p.status, PlanStatus::Unbalanced);
    assert!(p.item("LiquidOil").unbalanced > 0.0);
    assert_close!(p.item("CircuitBoardHighSpeed").unbalanced, 0.0);
    assert!(p.message.unwrap().contains("Crude Oil"));
}

#[test]
fn regenerate_returns_to_standard_recipes() {
    let d = real("satisfactory");
    let f = stuck(&d);
    let g = regenerate(&f, &solve(&d, &f));
    assert!(g.recipe_choice.is_empty());
    let p = solve(&d, &g);
    assert_eq!(p.status, PlanStatus::Ok);
    assert!(p.lines.iter().any(|l| l.recipe == "AILimiter"));
    assert_eq!(regenerate(&g, &p), g);
    assert_eq!(solve(&d, &reset_plan(&f)).status, PlanStatus::Ok);
}

#[test]
fn crude_oil_import_cannot_be_turned_off() {
    let d = real("satisfactory");
    assert!(!can_produce(&d, "LiquidOil"));
    // Scrap's water byproduct needs more water upstream, so water is import-only too.
    assert!(!can_produce(&d, "Water"));
    assert!(can_produce(&d, "OreIron"));
    assert!(can_produce(&d, POWER));
    let f = factory(&d);
    assert!(import_locked_reason(&d, &f, "LiquidOil").is_some());
    assert_eq!(toggle_item_import(&d, &f, "LiquidOil"), f);
    assert!(import_locked_reason(&d, &f, "OreIron").is_none());
    assert_ne!(toggle_item_import(&d, &f, "OreIron"), f);
}

#[test]
fn optimizer_stays_fast_with_everything_unlocked() {
    let d = real("satisfactory");
    let unlocked = d.recipes.values().filter(|r| r.alternate).map(|r| r.id.clone()).collect();
    let mut f = factory(&d);
    f.targets = vec![target("SpaceElevatorPart_9", 1.0)];
    f.optimize = true;
    let t0 = Instant::now();
    let p = solve_factory(&d, &f, &GameSettings { unlocked, ..Default::default() });
    let ms = t0.elapsed().as_millis();
    assert_eq!(p.status, PlanStatus::Ok);
    assert!(p.lines.len() > 5);
    assert!(ms < 2000, "took {ms} ms");
}

#[test]
fn factorio_circuits_with_em_plant_productivity() {
    let d = real("factorio");
    let mut f = factory(&d);
    f.targets = vec![target("electronic-circuit", 10.0)];
    let p = solve(&d, &f);
    assert_eq!(p.status, PlanStatus::Ok);
    // Electromagnetic plant: +50% productivity, so 10 circuits need 6.67 plates.
    assert_close!(p.item("iron-plate").consumed, 10.0 / 1.5);
}

#[test]
fn every_saved_factory_solves() {
    let store = Store::new(root());
    for game in GameId::ALL {
        let d = store.load_game_data(game).unwrap();
        for s in store.list_factories(game) {
            let f = store.load_factory(game, &s.id).unwrap();
            let p = solve(&d, &f);
            assert_ne!(p.status, PlanStatus::Error, "{} {}: {:?}", game.key(), f.name, p.message);
        }
    }
}
