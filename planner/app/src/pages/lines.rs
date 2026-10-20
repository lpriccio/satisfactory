//! Lines view: the recipe lines as a table you can type into. Column widths
//! can be dragged; egui remembers them between runs.

use crate::app::Action;
use crate::editor::Editor;
use crate::fmt;
use crate::pages::factory::{all_recipe_options, edit, line_controls};
use crate::widgets::{Palette, picker};
use eframe::egui::{self, RichText, Ui};
use egui_extras::{Column, TableBuilder};
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

/// Starting width of the recipe column; drag the column edges to change it.
const RECIPE_COL: f32 = 280.0;

/// One stack per line; the column clips anything wider than it.
fn io_cell(ui: &mut Ui, text: String) {
    ui.add(egui::Label::new(RichText::new(text).small()).extend());
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

    let note_of = |id: &str| {
        f.lines
            .iter()
            .find(|l| l.id == id)
            .and_then(|l| l.note.clone())
    };
    let small = ui.text_style_height(&egui::TextStyle::Small);
    let body = ui.text_style_height(&egui::TextStyle::Body);
    // Each row fits its tallest cell: the recipe (plus "solver" or a note) or
    // the longest list of inputs/outputs.
    let heights: Vec<f32> = rows
        .iter()
        .map(|line| {
            let stacks = |sign: f64| {
                line.rates
                    .iter()
                    .filter(|(i, r)| i.as_str() != POWER && **r * sign > 1e-9)
                    .count()
            };
            let io = stacks(-1.0).max(stacks(1.0)) as f32 * (small + 2.0);
            let recipe = body
                + 10.0
                + if line.auto || note_of(&line.id).is_some() {
                    small + 4.0
                } else {
                    0.0
                };
            io.max(recipe).max(30.0) + 6.0
        })
        .collect();
    let n_typed = f.lines.len();

    TableBuilder::new(ui)
        .id_salt("lines-table")
        .striped(true)
        .resizable(true)
        .vscroll(false)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Min))
        .column(Column::initial(RECIPE_COL).at_least(120.0).clip(true))
        .column(Column::initial(470.0).at_least(160.0).clip(true))
        .column(Column::initial(210.0).at_least(80.0).clip(true))
        .column(Column::initial(210.0).at_least(80.0).clip(true))
        .column(Column::remainder().at_least(150.0))
        .header(22.0, |mut header| {
            for h in ["Recipe", "Machines and settings", "In /min", "Out /min", ""] {
                header.col(|ui| {
                    ui.label(RichText::new(h).small().strong().weak());
                });
            }
        })
        .body(|body| {
            body.heterogeneous_rows(heights.into_iter(), |mut row| {
                let i = row.index();
                let line = rows[i];
                row.col(|ui| {
                    ui.vertical(|ui| {
                        if line.auto {
                            ui.add(egui::Label::new(&data.recipes[&line.recipe].name).truncate());
                            ui.label(RichText::new("solver").small().weak());
                        } else {
                            let width = ui.available_width() - 4.0;
                            if let Some(rid) = picker(
                                ui,
                                ("line-recipe", &line.id),
                                &options,
                                &line.recipe,
                                "Recipe",
                                width,
                            ) {
                                let id = line.id.clone();
                                edit(actions, None, move |f, _| {
                                    update_line(f, &id, |l| l.recipe = rid)
                                });
                            }
                            if let Some(note) = note_of(&line.id) {
                                ui.add(
                                    egui::Label::new(RichText::new(note).small().weak()).truncate(),
                                );
                            }
                        }
                    });
                });
                row.col(|ui| line_controls(ui, ed, line, actions, true));
                row.col(|ui| io_cell(ui, io_text(data, line, -1.0)));
                row.col(|ui| io_cell(ui, io_text(data, line, 1.0)));
                row.col(|ui| {
                    if line.auto {
                        return;
                    }
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
                });
            });
        });
}
