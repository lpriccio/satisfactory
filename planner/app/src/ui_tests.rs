//! Drive the real app headlessly with egui_kittest: click by label, type,
//! and check what the plan and the files say afterwards.

use crate::app::{PlannerApp, Route};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use planner_core::store::Store;
use planner_core::{GameId, PlanStatus};
use std::path::{Path, PathBuf};

/// A scratch planner folder: the real game data plus a few saved factories.
fn scratch(name: &str) -> PathBuf {
    let src = crate::root();
    let dir = std::env::temp_dir().join(format!("planner-ui-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("data")).unwrap();
    std::fs::create_dir_all(dir.join("factories/satisfactory")).unwrap();
    for g in ["satisfactory", "factorio", "dsp"] {
        std::fs::copy(
            src.join(format!("data/{g}.json")),
            dir.join(format!("data/{g}.json")),
        )
        .unwrap();
    }
    std::fs::copy(
        src.join("factories/satisfactory/t9-pasta.json"),
        dir.join("factories/satisfactory/t9-pasta.json"),
    )
    .unwrap();
    dir
}

/// A logistics file: one drone hub whose plastic Kidney draws on, and a to-do.
fn with_logistics(root: &Path) {
    use planner_core::logistics::*;
    let mut l = new_logistics(GameId::Satisfactory);
    let mut hub = new_station(Transport::Drone, "Coal Hole");
    hub.ports[0].item = "Plastic".into();
    hub.ports[0].rate = 600.0;
    let mut kidney = new_station(Transport::Drone, "Kidney");
    kidney.ports[0] = Port {
        item: "Plastic".into(),
        rate: 273.0,
        from: Some(hub.ports[0].id.clone()),
        ..new_port(Dir::In)
    };
    l.stations = vec![hub, kidney];
    l.todos = vec![new_todo("Drain the swamp")];
    Store::new(root).save_logistics(&l).unwrap();
}

fn harness(root: &Path) -> Harness<'static, PlannerApp> {
    let store = Store::new(root);
    Harness::builder()
        .with_size([1400.0, 1000.0])
        .with_max_steps(40)
        .build_eframe(move |cc| PlannerApp::new(cc, store))
}

#[test]
fn opens_a_t9_tab_with_the_workbook_numbers() {
    let root = scratch("pasta");
    let mut h = harness(&root);
    h.run();
    h.get_by_label("pasta").click();
    h.run();
    assert_eq!(h.state().route, Route::Factory);
    let plan = &h.state().editor.as_ref().unwrap().plan;
    assert!((plan.foundations.unwrap() - 1335.0).abs() < 1e-6);
    h.get_by_label("1,335");
    h.get_by_label("7,440 MW");
}

#[test]
fn new_factory_from_the_target_picker() {
    let root = scratch("new");
    let mut h = harness(&root);
    h.run();
    h.get_by_label("New factory").click();
    h.run();
    h.get_by_label("What do you want to make?").click();
    h.run();
    // The picker's search field (the factory name is the other text input).
    h.get_all_by_role(egui::accesskit::Role::TextInput)
        .last()
        .unwrap()
        .type_text("Alclad Aluminum Sheet");
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    let ed = h.state().editor.as_ref().unwrap();
    assert_eq!(ed.factory.targets[0].item, "AluminumPlate");
    assert!((ed.plan.item("OreBauxite").imported - 10.0).abs() < 1e-6);
    // Autosave lands within the debounce delay.
    std::thread::sleep(std::time::Duration::from_millis(450));
    h.run();
    let id = ed_id(&h);
    let saved = Store::new(&root)
        .load_factory(GameId::Satisfactory, &id)
        .unwrap();
    assert_eq!(saved.targets[0].item, "AluminumPlate");
}

fn ed_id(h: &Harness<'static, PlannerApp>) -> String {
    h.state().editor.as_ref().unwrap().factory.id.clone()
}

/// The case from real use: Plastic AI Limiter chosen, crude oil import off.
fn stuck(h: &mut Harness<'static, PlannerApp>) {
    h.get_by_label("New factory").click();
    h.run();
    let ed = h.state_mut().editor.as_mut().unwrap();
    ed.apply(
        Box::new(|f, _| {
            let mut g = f.clone();
            g.targets = vec![planner_core::Target {
                item: "CircuitBoardHighSpeed".into(),
                rate: 24.0,
            }];
            g.recipe_choice.insert(
                "CircuitBoardHighSpeed".into(),
                "Alternate_AILimiter_Plastic".into(),
            );
            g.imports.exclude = vec!["LiquidOil".into()];
            g
        }),
        None,
    );
    h.run();
    assert_eq!(
        h.state().editor.as_ref().unwrap().plan.status,
        PlanStatus::Unbalanced
    );
}

#[test]
fn regenerate_fixes_a_stuck_plan() {
    let root = scratch("regen");
    let mut h = harness(&root);
    h.run();
    stuck(&mut h);
    h.get_by_label_contains("Can't balance Crude Oil");
    // Both the banner and the flow panel offer it; either works.
    h.get_all_by_label("Regenerate with standard recipes")
        .next()
        .unwrap()
        .click();
    h.run();
    let ed = h.state().editor.as_ref().unwrap();
    assert_eq!(ed.plan.status, PlanStatus::Ok);
    assert!(ed.factory.recipe_choice.is_empty());
}

#[test]
fn reset_starts_over_and_undo_brings_it_back() {
    let root = scratch("reset");
    let mut h = harness(&root);
    h.run();
    stuck(&mut h);
    h.get_all_by_label("Reset").next().unwrap().click();
    h.run();
    {
        let ed = h.state().editor.as_ref().unwrap();
        assert_eq!(ed.plan.status, PlanStatus::Ok);
        assert!(ed.factory.imports.exclude.is_empty());
        assert_eq!(ed.factory.targets.len(), 1);
    }
    h.get_by_label("Undo").click();
    h.run();
    assert_eq!(
        h.state().editor.as_ref().unwrap().plan.status,
        PlanStatus::Unbalanced
    );
}

#[test]
fn views_switch_and_render() {
    let root = scratch("views");
    let mut h = harness(&root);
    h.run();
    h.get_by_label("pasta").click();
    h.run();
    for (tab, expect) in [
        ("Lines", "Add a recipe line…"),
        ("Balance", "Hide items with no flow"),
        ("Buildings", "Total"),
        ("Flow", "Imports"),
    ] {
        h.get_by_label(tab).click();
        h.run();
        assert!(
            h.query_by_label(expect).is_some()
                || h.query_all_by_label_contains(expect).next().is_some(),
            "{tab} view missing {expect:?}"
        );
    }
}

#[test]
fn delete_asks_first() {
    let root = scratch("delete");
    let mut h = harness(&root);
    h.run();
    h.get_by_label("Delete").click();
    h.run();
    h.get_by_label_contains("This removes its file");
    h.get_by_label("Cancel").click();
    h.run();
    assert!(root.join("factories/satisfactory/t9-pasta.json").exists());
    h.get_by_label("Delete").click();
    h.run();
    h.get_all_by_label("Delete").last().unwrap().click();
    h.run();
    assert!(!root.join("factories/satisfactory/t9-pasta.json").exists());
}

#[test]
fn other_games_load() {
    let root = scratch("games");
    let mut h = harness(&root);
    h.run();
    h.get_by_label("Factorio").click();
    h.run();
    h.get_by_label_contains("366 recipes");
    h.get_by_label("Dyson Sphere Program").click();
    h.run();
    h.get_by_label_contains("78 recipes");
}

#[test]
fn logistics_shows_what_is_free() {
    let root = scratch("logistics");
    with_logistics(&root);
    let mut h = harness(&root);
    h.run();
    h.get_by_label("Logistics").click();
    h.run();
    h.get_by_label("Drones").click();
    h.run();
    assert_eq!(h.state().route, Route::Logistics);
    // 600 out, 273 used by Kidney: 327 free.
    h.get_by_label("327");
    h.get_by_label_contains("Kidney 273");
    h.get_by_label("Item totals").click();
    h.run();
    h.get_by_label("+327");
}

#[test]
fn todo_add_and_tick_saves() {
    let root = scratch("todo");
    with_logistics(&root);
    let mut h = harness(&root);
    h.run();
    h.get_by_label("To-do").click();
    h.run();
    h.get_by_label("New task").click();
    h.run();
    h.get_by_label("New task").type_text("Find the somersloops");
    h.run();
    h.get_by_label("Add").click();
    h.run();
    h.get_by_label("Drain the swamp").click();
    h.run();
    std::thread::sleep(std::time::Duration::from_millis(450));
    h.run();
    let l = Store::new(&root)
        .load_logistics(GameId::Satisfactory)
        .unwrap();
    assert_eq!(l.todos.len(), 2);
    assert!(l.todos[0].done);
    assert_eq!(l.todos[1].text, "Find the somersloops");
    // Undo brings the tick back off.
    h.get_by_label("Undo").click();
    h.run();
    let lg = h.state().logistics.as_ref().unwrap();
    assert!(!lg.doc.todos[0].done);
}

#[test]
fn logistics_is_satisfactory_only() {
    let root = scratch("only");
    let mut h = harness(&root);
    h.run();
    assert!(h.query_by_label("Logistics").is_some());
    h.get_by_label("Factorio").click();
    h.run();
    assert!(h.query_by_label("Logistics").is_none());
    assert!(h.query_by_label("To-do").is_none());
}

#[test]
fn every_symbol_we_use_has_a_glyph() {
    let mut h = Harness::new_ui(|ui| {
        ui.label("x");
    });
    crate::fonts::install(&h.ctx);
    h.run();
    let missing: String = "·×÷–—“”…−²"
        .chars()
        .filter(|&c| {
            !h.ctx
                .fonts_mut(|f| f.has_glyph(&egui::FontId::proportional(14.0), c))
        })
        .collect();
    assert!(missing.is_empty(), "missing glyphs: {missing}");
}
