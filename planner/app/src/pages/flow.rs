//! Flow view: each line from the targets down, with where its inputs come
//! from and where its outputs go. Import toggles live on the inputs.

use crate::app::Action;
use crate::editor::Editor;
use crate::fmt;
use crate::pages::factory::{
    FactoryUi, edit, line_controls, line_label, main_item, order_lines, recipe_options_for,
};
use crate::widgets::{Palette, chip, heading_small};
use eframe::egui::{self, RichText, Ui};
use planner_core::edits::*;
use planner_core::solve::item_name;
use planner_core::*;

pub fn show(
    ui: &mut Ui,
    ed: &Editor,
    st: &mut FactoryUi,
    pal: &Palette,
    actions: &mut Vec<Action>,
) {
    let data = &ed.data;
    let plan = &ed.plan;
    let f = &ed.factory;

    ui.horizontal_top(|ui| {
        // --- side: imports and surplus ---
        ui.vertical(|ui| {
            ui.set_width(230.0);
            egui::Frame::group(ui.style()).inner_margin(10.0).show(ui, |ui| {
                ui.set_width(208.0);
                heading_small(ui, "Imports");
                let mut imports: Vec<&PlanItem> = plan.items.values().filter(|p| p.item != POWER && p.imported > 1e-6).collect();
                imports.sort_by(|a, b| b.imported.total_cmp(&a.imported));
                if imports.is_empty() {
                    ui.label(RichText::new("Nothing imported.").weak());
                }
                egui::Grid::new("imports").num_columns(2).striped(true).show(ui, |ui| {
                    for p in imports {
                        ui.label(item_name(data, &p.item));
                        ui.label(RichText::new(fmt::num(p.imported)).monospace().color(pal.import));
                        ui.end_row();
                    }
                });
                let surplus: Vec<&PlanItem> = plan.items.values().filter(|p| p.item != POWER && p.surplus > 1e-6).collect();
                if !surplus.is_empty() {
                    ui.add_space(8.0);
                    heading_small(ui, "Surplus");
                    egui::Grid::new("surplus").num_columns(2).show(ui, |ui| {
                        for p in surplus {
                            ui.label(item_name(data, &p.item));
                            ui.label(RichText::new(fmt::num(p.surplus)).monospace().color(pal.warn));
                            ui.end_row();
                        }
                    });
                }
                ui.add_space(8.0);
                ui.add(
                    egui::Label::new(
                        RichText::new("Click “import” on an input to import it for that line only. Shift-click to set every line using that item the same way.")
                            .small()
                            .weak(),
                    )
                    .wrap(),
                );
            });
        });

        // --- lines ---
        ui.vertical(|ui| {
            let lines = order_lines(plan);
            if lines.is_empty() {
                ui.label(if f.mode == Mode::Target {
                    "Add a target above and the plan appears here."
                } else {
                    "No lines yet. Add them in the Lines view."
                });
            }
            for line in lines {
                line_card(ui, ed, line, st, pal, actions);
            }
            unbalanced_panel(ui, ed, pal, actions);
        });
    });
}

fn line_card(
    ui: &mut Ui,
    ed: &Editor,
    line: &PlanLine,
    st: &mut FactoryUi,
    pal: &Palette,
    actions: &mut Vec<Action>,
) {
    let data = &ed.data;
    let plan = &ed.plan;
    let f = &ed.factory;
    let recipe = &data.recipes[&line.recipe];
    let open = !st.collapsed.contains(&line.id);
    egui::Frame::group(ui.style())
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                if ui.small_button(if open { "−" } else { "+" }).clicked() {
                    if open {
                        st.collapsed.insert(line.id.clone());
                    } else {
                        st.collapsed.remove(&line.id);
                    }
                }
                let item = main_item(data, line);
                let options = recipe_options_for(data, &item);
                if options.len() > 1 && !(line.auto && f.optimize) {
                    let combo = egui::ComboBox::from_id_salt(("recipe", &line.id))
                        .selected_text(RichText::new(&recipe.name).strong())
                        .width(260.0)
                        .show_ui(ui, |ui| {
                            for o in &options {
                                let text = if o.hint.is_empty() {
                                    o.label.clone()
                                } else {
                                    format!("{}  ({})", o.label, o.hint)
                                };
                                if ui.selectable_label(o.id == line.recipe, text).clicked()
                                    && o.id != line.recipe
                                {
                                    let (rid, item, id, auto) =
                                        (o.id.clone(), item.clone(), line.id.clone(), line.auto);
                                    edit(actions, None, move |f, _| {
                                        if auto {
                                            set_recipe_choice(f, &item, Some(&rid))
                                        } else {
                                            update_line(f, &id, |l| l.recipe = rid)
                                        }
                                    });
                                }
                            }
                        });
                    combo.response.accessible_name("Recipe");
                } else {
                    ui.label(RichText::new(&recipe.name).strong());
                }
                if recipe.alternate {
                    ui.label(RichText::new("alt").small().weak());
                }
                if !line.auto {
                    ui.label(
                        RichText::new(if line.locked { "locked" } else { "sized" })
                            .small()
                            .color(pal.accent),
                    )
                    .on_hover_text("Typed or locked by you");
                }
                ui.add_space(12.0);
                line_controls(ui, ed, line, actions, false);
            });
            if !open {
                return;
            }
            ui.columns(2, |cols| {
                heading_small(&mut cols[0], "In");
                egui::Grid::new(("in", &line.id))
                    .num_columns(4)
                    .spacing([8.0, 4.0])
                    .show(&mut cols[0], |ui| {
                        for (item, rate) in line
                            .rates
                            .iter()
                            .filter(|(i, r)| i.as_str() != POWER && **r < -1e-9)
                        {
                            ui.label(item_name(data, item));
                            ui.label(RichText::new(fmt::rate(-rate)).monospace());
                            ui.horizontal_wrapped(|ui| {
                                for e in plan
                                    .edges
                                    .iter()
                                    .filter(|e| e.to == line.id && &e.item == item)
                                {
                                    let color = (e.from == "import").then_some(pal.import);
                                    let _ = chip(
                                        ui,
                                        &line_label(data, plan, &e.from),
                                        color,
                                        &format!("{}/min", fmt::num(e.rate)),
                                    );
                                }
                            });
                            import_toggle(ui, ed, item, &line.id, pal, actions);
                            ui.end_row();
                        }
                    });
                heading_small(&mut cols[1], "Out");
                egui::Grid::new(("out", &line.id))
                    .num_columns(3)
                    .spacing([8.0, 4.0])
                    .show(&mut cols[1], |ui| {
                        for (item, _) in line
                            .rates
                            .iter()
                            .filter(|(i, r)| i.as_str() != POWER && **r > 1e-9)
                        {
                            ui.label(item_name(data, item));
                            ui.label(RichText::new(fmt::rate(line.rate(item))).monospace());
                            ui.horizontal_wrapped(|ui| {
                                for e in plan
                                    .edges
                                    .iter()
                                    .filter(|e| e.from == line.id && &e.item == item)
                                {
                                    let color = match e.to.as_str() {
                                        "target" => Some(pal.accent),
                                        "surplus" => Some(pal.warn),
                                        _ => None,
                                    };
                                    let _ = chip(
                                        ui,
                                        &line_label(data, plan, &e.to),
                                        color,
                                        &format!("{}/min", fmt::num(e.rate)),
                                    );
                                }
                            });
                            ui.end_row();
                        }
                    });
            });
        });
}

fn import_toggle(
    ui: &mut Ui,
    ed: &Editor,
    item: &str,
    line_id: &str,
    pal: &Palette,
    actions: &mut Vec<Action>,
) {
    let data = &ed.data;
    let on = occurrence_imported(data, &ed.factory, item, line_id);
    let locked = import_locked_reason(data, &ed.factory, item);
    let label = if on { "imported" } else { "import" };
    let color = if on { Some(pal.import) } else { None };
    let r = ui
        .add_enabled_ui(locked.is_none(), |ui| chip(ui, label, color, ""))
        .inner;
    let r = match &locked {
        Some(why) => r.on_disabled_hover_text(why),
        None => r.on_hover_text("Click: this line only. Shift-click: every line using it."),
    };
    if r.clicked() {
        let shift = ui.input(|i| i.modifiers.shift);
        let (d, item, id) = (ed.data.clone(), item.to_string(), line_id.to_string());
        edit(actions, None, move |f, p| {
            toggle_import(&d, f, p, &item, &id, shift)
        });
    }
}

fn unbalanced_panel(ui: &mut Ui, ed: &Editor, pal: &Palette, actions: &mut Vec<Action>) {
    let data = &ed.data;
    let short: Vec<&PlanItem> = ed
        .plan
        .items
        .values()
        .filter(|p| p.unbalanced > 1e-6)
        .collect();
    if short.is_empty() {
        return;
    }
    egui::Frame::group(ui.style())
        .stroke(egui::Stroke::new(1.0, pal.bad))
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                heading_small(ui, "Can't balance");
                if ed.factory.mode == Mode::Target {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        crate::pages::factory::fix_buttons(ui, ed, actions);
                    });
                }
            });
            for p in short {
                ui.horizontal(|ui| {
                    ui.label(item_name(data, &p.item));
                    ui.label(
                        RichText::new(format!("{} short", fmt::rate(p.unbalanced)))
                            .monospace()
                            .color(pal.bad),
                    );
                    if !item_imported(data, &ed.factory, &p.item)
                        && ui.button("Import it").clicked()
                    {
                        let (d, item) = (ed.data.clone(), p.item.clone());
                        edit(actions, None, move |f, _| toggle_item_import(&d, f, &item));
                    }
                });
            }
        });
}
