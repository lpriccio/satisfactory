mod common;

use common::*;
use planner_core::edits::*;
use planner_core::factory::new_world;
use planner_core::solve::solve_factory;
use planner_core::world::{Member, combine_plans, world_to_factory};
use planner_core::*;

fn solve(data: &GameData, f: &Factory) -> Plan {
    solve_factory(data, f, &GameSettings::default())
}

#[test]
fn simple_chain_and_power() {
    let d = mini();
    let mut f = factory(&d);
    f.targets = vec![target("IronPlate", 20.0)];
    let p = solve(&d, &f);
    assert_eq!(p.status, PlanStatus::Ok);
    assert_close!(machines(&p, "IronPlate"), 1.0);
    assert_close!(machines(&p, "IngotIron"), 1.0);
    assert_close!(imported(&p, "OreIron"), 30.0);
    assert_close!(p.power_mw, 8.0);
    assert_close!(p.footprint, 134.0);
    assert_close!(p.foundations.unwrap(), 134.0 * 4.0 / 64.0);
}

#[test]
fn byproducts_net_globally() {
    // Hand-checked: scrap's water and alumina's silica offset demand elsewhere.
    let d = mini();
    let mut f = factory(&d);
    f.targets = vec![target("AluminumPlate", 10.0)];
    let p = solve(&d, &f);
    assert_eq!(p.status, PlanStatus::Ok);
    assert_close!(machines(&p, "AluminumSheet"), 1.0 / 3.0);
    assert_close!(machines(&p, "IngotAluminum"), 1.0 / 6.0);
    assert_close!(machines(&p, "AluminumScrap"), 1.0 / 24.0);
    assert_close!(machines(&p, "AluminaSolution"), 1.0 / 12.0);
    assert_close!(machines(&p, "Silica"), 8.333_333_333_333_334 / 37.5);
    assert_close!(imported(&p, "OreBauxite"), 10.0);
    assert_close!(imported(&p, "Coal"), 5.0);
    assert_close!(imported(&p, "RawQuartz"), 5.0);
    assert_close!(imported(&p, "OreCopper"), 10.0 / 3.0);
    assert_close!(imported(&p, "Water"), 10.0);
    assert_close!(p.item("Silica").surplus, 0.0);
}

#[test]
fn loops_are_solved_not_cut() {
    let d = mini();
    let mut f = factory(&d);
    f.targets = vec![target("Plastic", 60.0)];
    f.recipe_choice = choice(&[
        ("Plastic", "Alternate_RecycledPlastic"),
        ("Rubber", "Alternate_RecycledRubber"),
    ]);
    f.imports.items = strings(&["LiquidFuel"]);
    let p = solve(&d, &f);
    assert_close!(machines(&p, "Alternate_RecycledPlastic"), 4.0 / 3.0);
    assert_close!(machines(&p, "Alternate_RecycledRubber"), 2.0 / 3.0);
    assert_close!(imported(&p, "LiquidFuel"), 120.0);
    assert_close!(imported(&p, "Plastic"), 0.0);
}

#[test]
fn clock_exponent_and_sloops() {
    let d = mini();
    let mut f = factory(&d);
    f.targets = vec![target("IronIngot", 75.0)];
    f.line_settings.insert(
        "IngotIron".into(),
        LineSettings {
            clock: Some(2.5),
            ..Default::default()
        },
    );
    let p = solve(&d, &f);
    assert_close!(machines(&p, "IngotIron"), 1.0);
    assert_close!(p.power_mw, 4.0 * 2.5f64.powf(1.321928));

    let mut g = factory(&d);
    g.targets = vec![target("IronIngot", 60.0)];
    g.line_settings.insert(
        "IngotIron".into(),
        LineSettings {
            sloops: Some(1),
            ..Default::default()
        },
    );
    let p = solve(&d, &g);
    assert_close!(machines(&p, "IngotIron"), 1.0);
    assert_close!(imported(&p, "OreIron"), 30.0);
    assert_close!(p.power_mw, 16.0);
}

#[test]
fn edge_import_only_covers_its_consumer() {
    let d = mini();
    let mut f = factory(&d);
    f.targets = vec![target("AluminumPlate", 30.0), target("CopperIngot", 15.0)];
    f.imports.edges = strings(&["CopperIngot>r:AluminumSheet"]);
    let p = solve(&d, &f);
    assert_close!(p.item("CopperIngot").imported, 10.0);
    assert_close!(p.item("CopperIngot").produced, 15.0);
    assert_close!(imported(&p, "OreCopper"), 15.0);
    let e = p
        .edges
        .iter()
        .find(|e| e.item == "CopperIngot" && e.to == "r:AluminumSheet")
        .unwrap();
    assert_eq!(e.from, "import");
    assert_close!(e.rate, 10.0);
}

#[test]
fn shortfall_is_reported_where_it_is() {
    let d = mini();
    let mut f = factory(&d);
    f.targets = vec![target("IronIngot", 30.0)];
    f.imports.exclude = strings(&["OreIron"]);
    let p = solve(&d, &f);
    assert_eq!(p.status, PlanStatus::Unbalanced);
    assert_close!(p.item("OreIron").unbalanced, 30.0);
    assert_close!(p.item("IronIngot").unbalanced, 0.0);
    assert!(p.message.unwrap().contains("OreIron"));
}

#[test]
fn generators_when_grid_power_is_off() {
    let d = mini();
    let mut f = factory(&d);
    f.targets = vec![target("IronIngot", 30.0)];
    f.imports.exclude = strings(&[POWER]);
    let p = solve(&d, &f);
    assert_eq!(p.status, PlanStatus::Ok);
    assert_close!(machines(&p, "Power_GeneratorCoal_Coal"), 4.0 / 75.0);
    assert_close!(p.power_mw, 0.0);
    assert_close!(imported(&p, "Coal"), 15.0 * 4.0 / 75.0);
}

#[test]
fn manual_lines_import_the_shortfall() {
    let d = mini();
    let mut f = factory(&d);
    f.mode = Mode::Manual;
    f.lines = vec![
        Line {
            id: "a".into(),
            recipe: "IronPlate".into(),
            machines: Some(2.0),
            ..Default::default()
        },
        Line {
            id: "b".into(),
            recipe: "IngotIron".into(),
            machines: Some(1.0),
            ..Default::default()
        },
    ];
    let p = solve(&d, &f);
    assert_close!(p.item("IronPlate").surplus, 40.0);
    assert_close!(p.item("IronIngot").imported, 30.0);
    assert_close!(p.item("OreIron").imported, 30.0);
}

#[test]
fn manual_free_lines_are_sized_before_importing() {
    let d = mini();
    let mut f = factory(&d);
    f.mode = Mode::Manual;
    f.targets = vec![target("IronPlate", 40.0)];
    f.lines = vec![
        Line {
            id: "a".into(),
            recipe: "IronPlate".into(),
            machines: Some(2.0),
            ..Default::default()
        },
        Line {
            id: "b".into(),
            recipe: "IngotIron".into(),
            machines: None,
            ..Default::default()
        },
    ];
    let p = solve(&d, &f);
    assert_close!(machines(&p, "IngotIron"), 2.0);
    assert_close!(p.item("IronIngot").imported, 0.0);
}

#[test]
fn locked_line_plus_auto_line_for_extra_demand() {
    let d = mini();
    let mut f = factory(&d);
    f.targets = vec![target("AluminumPlate", 60.0)];
    f.lines = vec![Line {
        id: "extra".into(),
        recipe: "AluminumSheet".into(),
        machines: Some(1.0),
        ..Default::default()
    }];
    let p = solve(&d, &f);
    assert_eq!(p.status, PlanStatus::Ok);
    assert_close!(machines(&p, "AluminumSheet"), 2.0);
}

// --- edits -----------------------------------------------------------------

fn two_consumers(d: &GameData) -> Factory {
    let mut f = factory(d);
    f.targets = vec![target("AluminumPlate", 60.0)];
    f.lines = vec![Line {
        id: "extra".into(),
        recipe: "AluminumSheet".into(),
        machines: Some(1.0),
        ..Default::default()
    }];
    f
}

#[test]
fn click_toggles_one_occurrence() {
    let d = mini();
    let f = two_consumers(&d);
    let g = toggle_import(&d, &f, &solve(&d, &f), "CopperIngot", "extra", false);
    assert!(occurrence_imported(&d, &g, "CopperIngot", "extra"));
    assert!(!occurrence_imported(
        &d,
        &g,
        "CopperIngot",
        "r:AluminumSheet"
    ));
    assert_close!(solve(&d, &g).item("CopperIngot").imported, 10.0);
}

#[test]
fn shift_click_sets_all_to_the_clicked_value() {
    let d = mini();
    let f = two_consumers(&d);
    let one = toggle_import(&d, &f, &solve(&d, &f), "CopperIngot", "extra", false);
    let all = toggle_import(
        &d,
        &one,
        &solve(&d, &one),
        "CopperIngot",
        "r:AluminumSheet",
        true,
    );
    assert!(all.imports.items.contains(&"CopperIngot".to_string()));
    assert!(all.imports.edges.is_empty());
    let none = toggle_import(&d, &all, &solve(&d, &all), "CopperIngot", "extra", true);
    assert!(!item_imported(&d, &none, "CopperIngot"));
}

#[test]
fn click_on_whole_item_import_produces_for_that_consumer() {
    let d = mini();
    let mut f = two_consumers(&d);
    f.imports.items = strings(&["CopperIngot"]);
    let g = toggle_import(&d, &f, &solve(&d, &f), "CopperIngot", "extra", false);
    assert!(!occurrence_imported(&d, &g, "CopperIngot", "extra"));
    assert!(occurrence_imported(
        &d,
        &g,
        "CopperIngot",
        "r:AluminumSheet"
    ));
    let p = solve(&d, &g);
    assert_close!(p.item("CopperIngot").imported, 10.0);
    assert_close!(p.item("CopperIngot").produced, 10.0);
}

#[test]
fn imported_consumption_counted_once() {
    let d = mini();
    let mut f = two_consumers(&d);
    f.imports.items = strings(&["CopperIngot"]);
    let p = solve(&d, &f);
    assert_close!(p.item("CopperIngot").consumed, 20.0);
    assert_close!(p.item("CopperIngot").imported, 20.0);
}

#[test]
fn lock_and_unlock_round_trip() {
    let d = mini();
    let f = two_consumers(&d);
    let edged = toggle_import(
        &d,
        &f,
        &solve(&d, &f),
        "CopperIngot",
        "r:AluminumSheet",
        false,
    );
    let locked = lock_line(&edged, &solve(&d, &edged), "r:IngotAluminum");
    let id = locked
        .lines
        .iter()
        .find(|l| l.recipe == "IngotAluminum")
        .unwrap()
        .id
        .clone();
    assert!(solve(&d, &locked).line(&id).unwrap().locked);
    let back = unlock_line(&locked, &id);
    assert!(!back.lines.iter().any(|l| l.recipe == "IngotAluminum"));
    assert!(solve(&d, &back).line("r:IngotAluminum").is_some());
}

#[test]
fn switching_to_manual_freezes_the_plan() {
    let d = mini();
    let f = two_consumers(&d);
    let p = solve(&d, &f);
    let m = set_mode(&f, &p, Mode::Manual);
    let p2 = solve(&d, &m);
    assert_eq!(p2.lines.len(), p.lines.len());
    assert_close!(
        p2.item("AluminumPlate").produced,
        p.item("AluminumPlate").produced
    );
}

#[test]
fn reset_keeps_targets_and_clears_the_rest() {
    let d = mini();
    let mut f = two_consumers(&d);
    f.recipe_choice = choice(&[("Plastic", "Alternate_RecycledPlastic")]);
    f.imports.exclude = strings(&["OreIron"]);
    f.line_settings.insert(
        "IngotIron".into(),
        LineSettings {
            clock: Some(2.0),
            ..Default::default()
        },
    );
    let g = reset_plan(&f);
    assert_eq!(g.targets, f.targets);
    assert!(g.recipe_choice.is_empty() && g.line_settings.is_empty() && g.lines.is_empty());
    assert_eq!(g.imports, FactoryImports::default());
}

// --- worlds ----------------------------------------------------------------

#[test]
fn worlds_scale_sum_and_flatten() {
    let d = mini();
    let mut a = factory(&d);
    a.name = "plates".into();
    a.targets = vec![target("IronPlate", 20.0)];
    let mut b = factory(&d);
    b.targets = vec![target("CopperIngot", 30.0)];
    let (pa, pb) = (solve(&d, &a), solve(&d, &b));
    let members = [
        Member {
            factory: &a,
            plan: &pa,
            multiplier: 2.0,
        },
        Member {
            factory: &b,
            plan: &pb,
            multiplier: 1.0,
        },
    ];
    let t = combine_plans(&members);
    assert_close!(t.power_mw, 20.0);
    assert_close!(t.items["OreIron"].imported, 60.0);
    let flat = world_to_factory(&new_world(GameId::Satisfactory, "w"), &members);
    assert_eq!(flat.lines.len(), 3);
    let pf = solve(&d, &flat);
    assert_close!(pf.power_mw, t.power_mw);
    assert_close!(pf.item("IronPlate").surplus, 40.0);
}
