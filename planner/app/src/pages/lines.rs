//! Lines view: the recipe lines as a table you can type into.

use crate::app::Action;
use crate::editor::Editor;
use crate::fmt;
use crate::pages::factory::{all_recipe_options, edit, line_controls};
use crate::widgets::{Palette, picker};
use eframe::egui::{self, RichText, Ui};
use planner_core::edits::*;
use planner_core::solve::item_name;
use planner_core::*;

fn io_text(data: &GameData, line: &PlanLine, sign: f64) -> String {
    line.rates
        .iter()
        .filter(|(i, r)| i.as_str() != POWER && **r * sign > 1e-9)
        .map(|(i, r)| format!("{} {}", fmt::num(r.abs()), item_name(data, i)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Wide enough for the longest recipe names ("Alternate: Pure Copper Ingot").
const RECIPE_COL: f32 = 280.0;

/// One stack per line, never squeezed narrower than the longest entry.
fn io_cell(ui: &mut Ui, text: String) {
    ui.vertical(|ui| {
        ui.set_min_width(200.0);
        ui.add(egui::Label::new(RichText::new(text).small()).extend());
    });
}

pub fn show(ui: &mut Ui, ed: &Editor, pal: &Palette, actions: &mut Vec<Action>) {
    let data = &ed.data;
    let f = &ed.factory;
    let plan = &ed.plan;
    let options = all_recipe_options(data);

    ui.horizontal(|ui| {
        if let Some(rid) = picker(ui, "add-line", &options, "", "Add a recipe line…", 320.0) {
            edit(actions, None, move |f, _| add_line(f, &rid, Some(1.0)));
        }
        ui.label(
            RichText::new(if f.mode == Mode::Manual {
                "New lines start at 1 machine; drag the count, or Auto-size to let targets decide."
            } else {
                "Lines added here are locked extras; the solver adds the rest."
            })
            .small()
            .weak(),
        );
    });
    ui.add_space(6.0);

    // Typed lines in their saved order, then the solver's lines.
    let mut rows: Vec<&PlanLine> = f.lines.iter().filter_map(|l| plan.line(&l.id)).collect();
    rows.extend(plan.lines.iter().filter(|l| l.auto));
    if rows.is_empty() {
        ui.label(RichText::new("No lines yet.").weak());
        return;
    }

    egui::Grid::new("lines")
        .num_columns(5)
        .striped(true)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            for h in ["Recipe", "Machines and settings", "In /min", "Out /min", ""] {
                ui.label(RichText::new(h).small().strong().weak());
            }
            ui.end_row();
            let n_typed = f.lines.len();
            for (i, line) in rows.iter().enumerate() {
                if line.auto {
                    ui.vertical(|ui| {
                        ui.set_min_width(RECIPE_COL);
                        ui.add(egui::Label::new(&data.recipes[&line.recipe].name).extend());
                        ui.label(RichText::new("solver").small().weak());
                    });
                } else {
                    ui.vertical(|ui| {
                        ui.set_min_width(RECIPE_COL);
                        if let Some(rid) = picker(
                            ui,
                            ("line-recipe", &line.id),
                            &options,
                            &line.recipe,
                            "Recipe",
                            RECIPE_COL,
                        ) {
                            let id = line.id.clone();
                            edit(actions, None, move |f, _| {
                                update_line(f, &id, |l| l.recipe = rid)
                            });
                        }
                        if let Some(note) = f
                            .lines
                            .iter()
                            .find(|l| l.id == line.id)
                            .and_then(|l| l.note.as_ref())
                        {
                            ui.label(RichText::new(note).small().weak());
                        }
                    });
                }
                ui.horizontal_wrapped(|ui| {
                    ui.set_max_width(520.0);
                    line_controls(ui, ed, line, actions, true);
                });
                io_cell(ui, io_text(data, line, -1.0));
                io_cell(ui, io_text(data, line, 1.0));
                ui.horizontal(|ui| {
                    if !line.auto {
                        let id = line.id.clone();
                        if ui
                            .add_enabled(i > 0, egui::Button::new("Up").small())
                            .clicked()
                        {
                            let id = id.clone();
                            edit(actions, None, move |f, _| move_line(f, &id, -1));
                        }
                        if ui
                            .add_enabled(i + 1 < n_typed, egui::Button::new("Down").small())
                            .clicked()
                        {
                            let id = id.clone();
                            edit(actions, None, move |f, _| move_line(f, &id, 1));
                        }
                        if ui
                            .add(egui::Button::new(RichText::new("×").color(pal.bad)).small())
                            .on_hover_text("Delete line")
                            .clicked()
                        {
                            edit(actions, None, move |f, _| remove_line(f, &id));
                        }
                    }
                });
                ui.end_row();
            }
        });
}
