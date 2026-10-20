//! Factory page: header, targets, summary tiles, and the four views.

use crate::app::{Action, Session, View};
use crate::editor::Editor;
use crate::fmt;
use crate::widgets::{Opt, Palette, picker, tile};
use eframe::egui::{self, DragValue, RichText, Ui};
use planner_core::edits::*;
use planner_core::linemath::{MAX_CLOCK, MIN_CLOCK, whole_machines};
use planner_core::solve::{default_recipe, item_name, producers, recipe_name};
use planner_core::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub struct FactoryUi {
    pub collapsed: BTreeSet<String>,
    pub balance_sort: (usize, bool),
    pub balance_hide_zero: Option<bool>,
}

pub fn edit(
    actions: &mut Vec<Action>,
    merge: Option<String>,
    f: impl FnOnce(&Factory, &Plan) -> Factory + 'static,
) {
    actions.push(Action::Edit(Box::new(f), merge));
}

/// Items for target pickers: every item, plus power when the game has it.
pub fn item_options(data: &GameData) -> Vec<Opt> {
    let mut out: Vec<Opt> = data
        .items
        .values()
        .map(|i| Opt::new(&i.id, &i.name, if i.raw { "raw" } else { "" }))
        .collect();
    out.sort_by(|a, b| a.label.cmp(&b.label));
    if data.features.power {
        out.insert(0, Opt::new(POWER, "Power (MW)", "generators"));
    }
    out
}

/// Recipes able to make an item, with default / alternate / byproduct hints.
pub fn recipe_options_for(data: &GameData, item: &str) -> Vec<Opt> {
    let def = default_recipe(data, item);
    producers(data)
        .get(item)
        .map(|ids| {
            ids.iter()
                .map(|id| {
                    let r = &data.recipes[id];
                    let mut hint = vec![];
                    if Some(id) == def.as_ref() {
                        hint.push("default".to_string());
                    } else if r.alternate {
                        hint.push("alternate".into());
                    }
                    if r.outputs.first().is_some_and(|o| o.item != item) {
                        hint.push(format!(
                            "byproduct of {}",
                            item_name(data, &r.outputs[0].item)
                        ));
                    }
                    Opt::new(id, &r.name, hint.join(" · "))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn all_recipe_options(data: &GameData) -> Vec<Opt> {
    let mut out: Vec<Opt> = data
        .recipes
        .values()
        .map(|r| {
            let outs: Vec<String> = r.outputs.iter().map(|o| item_name(data, &o.item)).collect();
            let hint = if r.alternate {
                format!("alternate · {}", outs.join(", "))
            } else {
                outs.join(", ")
            };
            Opt::new(&r.id, &r.name, hint)
        })
        .collect();
    out.sort_by(|a, b| a.label.cmp(&b.label));
    out
}

pub fn line_label(data: &GameData, plan: &Plan, id: &str) -> String {
    match id {
        "import" | "target" | "surplus" => id.to_string(),
        _ => plan
            .line(id)
            .map_or_else(|| id.to_string(), |l| recipe_name(data, &l.recipe)),
    }
}

/// The item a line is "for": its recipe's first output.
pub fn main_item(data: &GameData, line: &PlanLine) -> String {
    data.recipes
        .get(&line.recipe)
        .and_then(|r| r.outputs.first())
        .map(|s| s.item.clone())
        .unwrap_or_default()
}

/// Lines from the targets down: a line's depth is one more than the deepest
/// line it feeds. Loops are cut after one pass per line.
pub fn order_lines(plan: &Plan) -> Vec<&PlanLine> {
    let mut depth: BTreeMap<&str, usize> = BTreeMap::new();
    let mut feeds: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for e in &plan.edges {
        if e.to == "target" || e.to == "surplus" {
            depth.insert(e.from.as_str(), 0);
        } else if e.from != "import" {
            feeds
                .entry(e.to.as_str())
                .or_default()
                .insert(e.from.as_str());
        }
    }
    for _ in 0..plan.lines.len() {
        let mut changed = false;
        for (consumer, suppliers) in &feeds {
            let Some(&d) = depth.get(consumer) else {
                continue;
            };
            for s in suppliers {
                if depth.get(s).is_none_or(|&x| x < d + 1) && d < plan.lines.len() {
                    depth.insert(s, d + 1);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    let max = depth.values().max().map_or(0, |m| m + 1);
    let mut lines: Vec<&PlanLine> = plan.lines.iter().collect();
    lines.sort_by_key(|l| depth.get(l.id.as_str()).copied().unwrap_or(max));
    lines
}

/// Per-line knobs shared by the Flow and Lines views.
pub fn line_controls(
    ui: &mut Ui,
    ed: &Editor,
    line: &PlanLine,
    actions: &mut Vec<Action>,
    compact: bool,
) {
    let data = &ed.data;
    let features = &data.features;
    let recipe = &data.recipes[&line.recipe];
    let building = data.buildings.get(&line.building);
    let slots = building.and_then(|b| b.sloop_slots).unwrap_or(0);
    let s = &line.settings;
    let (id, rid) = (line.id.clone(), line.recipe.clone());

    if !line.auto && line.locked {
        let mut m = line.machines;
        let r = ui
            .add(
                DragValue::new(&mut m)
                    .speed(0.05)
                    .range(0.0..=1e6)
                    .max_decimals(4),
            )
            .accessible_name("Machines")
            .on_hover_text("Machines (locked)");
        if r.changed() {
            let id = id.clone();
            edit(actions, Some(format!("{id}-machines")), move |f, _| {
                update_line(f, &id, |l| l.machines = Some(m))
            });
        }
    } else {
        ui.label(RichText::new(fmt::num(line.machines)).strong().monospace())
            .on_hover_text(format!("{:.4} machines", line.machines));
    }
    let whole = whole_machines(line.machines);
    let bname = building.map_or(line.building.as_str(), |b| b.name.as_str());
    ui.label(
        RichText::new(if whole != line.machines {
            format!("× {bname} ({whole})")
        } else {
            format!("× {bname}")
        })
        .weak(),
    );

    if recipe.buildings.len() > 1 {
        let current = building.map_or(String::new(), |b| b.name.clone());
        let combo = egui::ComboBox::from_id_salt(("building", &id))
            .selected_text(current)
            .show_ui(ui, |ui| {
                for b in &recipe.buildings {
                    let name = data
                        .buildings
                        .get(b)
                        .map_or(b.as_str(), |x| x.name.as_str());
                    if ui.selectable_label(*b == line.building, name).clicked() {
                        let (id, rid, b) = (id.clone(), rid.clone(), b.clone());
                        edit(actions, None, move |f, _| {
                            set_line_settings(f, &id, &rid, |s| s.building = Some(b))
                        });
                    }
                }
            });
        combo.response.accessible_name("Building");
    }
    if features.clock {
        let mut pct = s.clock.unwrap_or(1.0) * 100.0;
        let r = ui
            .add(
                DragValue::new(&mut pct)
                    .range(MIN_CLOCK * 100.0..=MAX_CLOCK * 100.0)
                    .suffix("%")
                    .speed(1.0)
                    .max_decimals(4),
            )
            .accessible_name("Clock speed")
            .on_hover_text("Clock speed 1–250%. Power grows by clock^1.32.");
        if r.changed() {
            let (id, rid) = (id.clone(), rid.clone());
            let clock = (pct - 100.0).abs() > 1e-9;
            edit(actions, Some(format!("{id}-clock")), move |f, _| {
                set_line_settings(f, &id, &rid, |s| s.clock = clock.then_some(pct / 100.0))
            });
        }
    }
    if features.sloops && slots > 0 {
        let n = s.sloops.unwrap_or(0);
        egui::ComboBox::from_id_salt(("sloops", &id))
            .selected_text(format!("{n} sloop{}", if n == 1 { "" } else { "s" }))
            .show_ui(ui, |ui| {
                for k in 0..=slots {
                    if ui
                        .selectable_label(
                            k == n,
                            format!("{k} sloop{}", if k == 1 { "" } else { "s" }),
                        )
                        .clicked()
                    {
                        let (id, rid) = (id.clone(), rid.clone());
                        edit(actions, None, move |f, _| {
                            set_line_settings(f, &id, &rid, |s| s.sloops = (k > 0).then_some(k))
                        });
                    }
                }
            })
            .response
            .accessible_name("Somersloops")
            .on_hover_text(format!(
                "Somersloops: output ×(1 + n/{slots}), power ×(1 + n/{slots})²"
            ));
    }
    if features.speed_bonus {
        let mut v = s.speed_bonus.unwrap_or(0.0) * 100.0;
        ui.label(RichText::new("speed +").weak());
        if ui
            .add(DragValue::new(&mut v).suffix("%").speed(1.0))
            .accessible_name("Speed bonus")
            .on_hover_text("Speed bonus: modules, beacons, proliferator")
            .changed()
        {
            let (id, rid) = (id.clone(), rid.clone());
            edit(actions, Some(format!("{id}-speed")), move |f, _| {
                set_line_settings(f, &id, &rid, |s| {
                    s.speed_bonus = (v != 0.0).then_some(v / 100.0)
                })
            });
        }
    }
    if features.productivity && (data.game != GameId::Factorio || recipe.allow_productivity) {
        let mut v = s.prod_bonus.unwrap_or(0.0) * 100.0;
        ui.label(RichText::new("prod +").weak());
        if ui
            .add(DragValue::new(&mut v).suffix("%").speed(1.0))
            .accessible_name("Productivity bonus")
            .on_hover_text("Productivity bonus (outputs only)")
            .changed()
        {
            let (id, rid) = (id.clone(), rid.clone());
            edit(actions, Some(format!("{id}-prod")), move |f, _| {
                set_line_settings(f, &id, &rid, |s| {
                    s.prod_bonus = (v != 0.0).then_some(v / 100.0)
                })
            });
        }
    }
    if features.power && !compact {
        let sign = if line.power_mw < 0.0 { "+" } else { "" };
        ui.label(
            RichText::new(format!("{sign}{} MW", fmt::num(line.power_mw.abs())))
                .monospace()
                .weak(),
        );
    }
    let id2 = id.clone();
    if ed.factory.mode == Mode::Target {
        if line.auto {
            if ui
                .small_button("Lock")
                .on_hover_text("Pin this line at its current machine count")
                .clicked()
            {
                edit(actions, None, move |f, p| lock_line(f, p, &id2));
            }
        } else if line.locked
            && ui
                .small_button("Unlock")
                .on_hover_text("Let the solver size this line again")
                .clicked()
        {
            edit(actions, None, move |f, _| unlock_line(f, &id2));
        }
    } else if line.locked {
        if ui
            .small_button("Auto-size")
            .on_hover_text("Let the solver size this line to meet targets")
            .clicked()
        {
            edit(actions, None, move |f, _| unlock_line(f, &id2));
        }
    } else {
        let m = (line.machines * 1e4).round() / 1e4;
        if ui
            .small_button("Lock")
            .on_hover_text("Fix the machine count")
            .clicked()
        {
            edit(actions, None, move |f, _| {
                update_line(f, &id2, |l| l.machines = Some(m))
            });
        }
    }
}

/// Regenerate / Reset: ways out of a plan that can't balance.
pub fn fix_buttons(ui: &mut Ui, ed: &Editor, actions: &mut Vec<Action>) {
    let can_regen = regenerate(&ed.factory, &ed.plan) != ed.factory;
    let r = ui.add_enabled(
        can_regen,
        egui::Button::new("Regenerate with standard recipes"),
    );
    let r = if can_regen {
        r.on_hover_text(
            "Put the items that depend on the short item back on their standard recipes",
        )
    } else {
        r.on_disabled_hover_text("Everything involved already uses standard recipes")
    };
    if r.clicked() {
        edit(actions, None, regenerate);
    }
    if ui
        .button("Reset")
        .on_hover_text("Start the plan over from your targets: standard recipes, default imports, no line settings. Undo brings it back.")
        .clicked()
    {
        edit(actions, None, |f, _| reset_plan(f));
    }
}

impl FactoryUi {
    pub fn show(
        &mut self,
        ui: &mut Ui,
        ed: &Editor,
        session: &Session,
        view: View,
        pal: &Palette,
        actions: &mut Vec<Action>,
    ) {
        let data = &session.data;
        let f = &ed.factory;
        let plan = &ed.plan;

        // --- header ---
        ui.horizontal(|ui| {
            if ui.link("< Factories").clicked() {
                actions.push(Action::Home);
            }
            let mut name = f.name.clone();
            let r = ui
                .add(
                    egui::TextEdit::singleline(&mut name)
                        .font(egui::TextStyle::Heading)
                        .desired_width(360.0),
                )
                .accessible_name("Factory name");
            if r.changed() {
                edit(actions, Some("name".into()), move |f, _| Factory {
                    name,
                    ..f.clone()
                });
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Export JSON…").clicked() {
                    actions.push(Action::ExportFactory);
                }
                if ui
                    .button("Copy JSON")
                    .on_hover_text("Copy this factory to the clipboard")
                    .clicked()
                {
                    actions.push(Action::CopyFactoryJson);
                }
                if ui
                    .add_enabled(ed.can_redo(), egui::Button::new("Redo"))
                    .on_hover_text("Ctrl+Shift+Z")
                    .clicked()
                {
                    actions.push(Action::Redo);
                }
                if ui
                    .add_enabled(ed.can_undo(), egui::Button::new("Undo"))
                    .on_hover_text("Ctrl+Z")
                    .clicked()
                {
                    actions.push(Action::Undo);
                }
                let status = ed.status();
                let color = if ed.save_error.is_some() {
                    pal.bad
                } else {
                    ui.visuals().weak_text_color()
                };
                let r = ui.label(RichText::new(status).small().color(color));
                if let Some(e) = &ed.save_error {
                    r.on_hover_text(e);
                }
            });
        });

        ui.horizontal_wrapped(|ui| {
            for (mode, label, tip) in [
                (Mode::Target, "Target", "The solver picks lines to meet your targets"),
                (Mode::Manual, "Manual", "You type the lines; switching freezes the current plan"),
            ] {
                if ui.selectable_label(f.mode == mode, label).on_hover_text(tip).clicked() && f.mode != mode {
                    edit(actions, None, move |f, p| set_mode(f, p, mode));
                }
            }
            ui.separator();
            ui.label("Objective");
            let names = [(Objective::MinRaw, "Fewest raw inputs"), (Objective::MinPower, "Least power"), (Objective::MinMachines, "Fewest machines")];
            let current = names.iter().find(|(o, _)| *o == f.objective).map_or("", |(_, n)| n);
            let combo = egui::ComboBox::from_id_salt("objective").selected_text(current).show_ui(ui, |ui| {
                for (o, n) in names {
                    if ui.selectable_label(o == f.objective, n).clicked() {
                        edit(actions, None, move |f, _| Factory { objective: o, ..f.clone() });
                    }
                }
            });
            combo.response.accessible_name("Objective");
            if f.mode == Mode::Target {
                let mut opt = f.optimize;
                if ui
                    .checkbox(&mut opt, "Choose recipes for me")
                    .on_hover_text("Let the solver choose among standard recipes and the alternates unlocked in Settings")
                    .changed()
                {
                    edit(actions, None, move |f, _| Factory { optimize: opt, ..f.clone() });
                }
            }
            if data.features.clock {
                ui.separator();
                ui.label("Spacing ×").on_hover_text("Foundations = floor area × spacing ÷ 64 m²");
                let mut sp = f.spacing;
                if ui.add(DragValue::new(&mut sp).range(1.0..=10.0).speed(0.1)).accessible_name("Spacing").changed() {
                    edit(actions, Some("spacing".into()), move |f, _| Factory { spacing: sp, ..f.clone() });
                }
            }
        });
        ui.add_space(6.0);

        // --- targets ---
        let items = item_options(data);
        egui::Frame::group(ui.style()).inner_margin(10.0).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            crate::widgets::heading_small(ui, "Targets");
            for (i, t) in f.targets.iter().enumerate() {
                ui.horizontal(|ui| {
                    if let Some(item) = picker(ui, ("target", i), &items, &t.item, "Item", 260.0) {
                        edit(actions, None, move |f, _| {
                            let mut g = f.clone();
                            g.targets[i].item = item;
                            g
                        });
                    }
                    let mut rate = t.rate;
                    if ui.add(DragValue::new(&mut rate).range(0.0..=1e9).speed(0.5).max_decimals(4)).accessible_name("Rate").changed() {
                        edit(actions, Some(format!("target-{i}")), move |f, _| {
                            let mut g = f.clone();
                            g.targets[i].rate = rate;
                            g
                        });
                    }
                    ui.label(if t.item == POWER { "MW" } else { "/min" });
                    if ui.small_button("×").on_hover_text("Remove target").clicked() {
                        edit(actions, None, move |f, _| {
                            let mut g = f.clone();
                            g.targets.remove(i);
                            g
                        });
                    }
                });
            }
            ui.horizontal(|ui| {
                let hint = if f.targets.is_empty() { "What do you want to make?" } else { "Add another target…" };
                if let Some(item) = picker(ui, "new-target", &items, "", hint, 260.0) {
                    let rate = if item == POWER { 1000.0 } else { 10.0 };
                    edit(actions, None, move |f, _| {
                        let mut g = f.clone();
                        g.targets.push(Target { item, rate });
                        g
                    });
                }
                if f.mode == Mode::Manual {
                    ui.label(RichText::new("Optional in manual mode: unlocked lines are sized to meet targets.").small().weak());
                }
            });
        });

        // --- problems ---
        if plan.status != PlanStatus::Ok
            && let Some(msg) = &plan.message
        {
            egui::Frame::group(ui.style())
                .stroke(egui::Stroke::new(
                    1.0,
                    if plan.status == PlanStatus::Error {
                        pal.bad
                    } else {
                        pal.warn
                    },
                ))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        ui.colored_label(
                            if plan.status == PlanStatus::Error {
                                pal.bad
                            } else {
                                pal.warn
                            },
                            msg,
                        );
                        if plan.status == PlanStatus::Unbalanced && f.mode == Mode::Target {
                            fix_buttons(ui, ed, actions);
                        }
                    });
                });
        }
        for w in &plan.warnings {
            ui.colored_label(pal.warn, w);
        }

        // --- tiles ---
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if data.features.power {
                let generated = if plan.generated_mw > 0.0 {
                    format!(" · {} generated", fmt::num(plan.generated_mw))
                } else {
                    String::new()
                };
                tile(
                    ui,
                    "Power",
                    &fmt::mw(plan.power_mw),
                    &format!("{} drawn{generated}", fmt::num(plan.consumed_mw)),
                );
            }
            let whole: f64 = plan.lines.iter().map(|l| whole_machines(l.machines)).sum();
            tile(
                ui,
                "Machines",
                &fmt::num(plan.machines),
                &format!("{whole} whole · {} lines", plan.lines.len()),
            );
            if data.features.footprint {
                let unit = data.features.area_unit.as_deref().unwrap_or("");
                match plan.foundations {
                    Some(fd) => tile(
                        ui,
                        "Foundations",
                        &fmt::num(fd),
                        &format!("{} {unit} × {} ÷ 64", fmt::num(plan.footprint), f.spacing),
                    ),
                    None => tile(
                        ui,
                        "Floor",
                        &format!("{} {unit}", fmt::num(plan.footprint)),
                        "",
                    ),
                }
            }
            let n = plan
                .items
                .values()
                .filter(|p| p.item != POWER && p.imported > 1e-6)
                .count();
            tile(ui, "Imports", &n.to_string(), "items brought in");
        });

        // --- tabs ---
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            for (v, label) in [
                (View::Flow, "Flow"),
                (View::Lines, "Lines"),
                (View::Balance, "Balance"),
                (View::Buildings, "Buildings"),
            ] {
                if ui
                    .selectable_label(view == v, RichText::new(label).size(15.0))
                    .clicked()
                {
                    actions.push(Action::SetView(v));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("solved in {:.1} ms", ed.solve_ms))
                        .small()
                        .weak(),
                );
            });
        });
        ui.separator();

        match view {
            View::Flow => crate::pages::flow::show(ui, ed, self, pal, actions),
            View::Lines => crate::pages::lines::show(ui, ed, pal, actions),
            View::Balance => crate::pages::tables::balance(ui, ed, self, pal, actions),
            View::Buildings => crate::pages::tables::buildings(ui, ed),
        }

        // --- notes ---
        ui.add_space(10.0);
        egui::CollapsingHeader::new("Notes")
            .default_open(f.notes.as_deref().is_some_and(|n| !n.trim().is_empty()))
            .show(ui, |ui| {
                let mut notes = f.notes.clone().unwrap_or_default();
                if ui
                    .add(
                        egui::TextEdit::multiline(&mut notes)
                            .desired_width(f32::INFINITY)
                            .code_editor(),
                    )
                    .accessible_name("Notes")
                    .changed()
                {
                    edit(actions, Some("notes".into()), move |f, _| Factory {
                        notes: (!notes.is_empty()).then_some(notes),
                        ..f.clone()
                    });
                }
            });
    }
}
