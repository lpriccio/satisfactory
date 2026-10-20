//! Per-game settings: spacing, unlocked alternates, preferred buildings.

use crate::app::{Action, Session};
use eframe::egui::{self, DragValue, RichText, Ui};
use planner_core::solve::item_name;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub struct SettingsUi {
    filter: String,
}

impl SettingsUi {
    pub fn show(&mut self, ui: &mut Ui, session: &Session, actions: &mut Vec<Action>) {
        let data = &session.data;
        let s = &session.settings;
        ui.heading(format!("{} settings", data.name));
        ui.label(
            RichText::new(format!(
                "Data: {} · generated {}",
                data.source,
                data.generated_at.get(..10).unwrap_or("")
            ))
            .small()
            .weak(),
        );
        ui.add_space(8.0);

        if data.features.clock {
            ui.horizontal(|ui| {
                ui.label("Foundation spacing for new factories");
                let mut sp = s.spacing;
                if ui
                    .add(DragValue::new(&mut sp).range(1.0..=10.0).speed(0.1))
                    .accessible_name("Spacing")
                    .changed()
                {
                    let mut next = s.clone();
                    next.spacing = sp;
                    actions.push(Action::SaveSettings(next));
                }
                ui.label(
                    RichText::new("foundations = floor m² × spacing ÷ 64")
                        .small()
                        .weak(),
                );
            });
            ui.add_space(8.0);
        }

        let mut alts: Vec<_> = data.recipes.values().filter(|r| r.alternate).collect();
        alts.sort_by(|a, b| a.name.cmp(&b.name));
        if !alts.is_empty() {
            ui.horizontal(|ui| {
                ui.heading("Unlocked alternates");
                ui.label(RichText::new(format!("{} of {}", s.unlocked.len(), alts.len())).weak());
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .hint_text("Filter")
                        .desired_width(180.0),
                )
                .accessible_name("Filter alternates");
                if ui.button("All").clicked() {
                    let mut next = s.clone();
                    next.unlocked = alts.iter().map(|r| r.id.clone()).collect();
                    actions.push(Action::SaveSettings(next));
                }
                if ui.button("None").clicked() {
                    let mut next = s.clone();
                    next.unlocked.clear();
                    actions.push(Action::SaveSettings(next));
                }
            });
            ui.label(RichText::new("Used when a factory has “Choose recipes for me” on. You can always pick any recipe by hand.").small().weak());
            let f = self.filter.to_lowercase();
            let unlocked: BTreeSet<&String> = s.unlocked.iter().collect();
            egui::Grid::new("alts")
                .num_columns(3)
                .spacing([16.0, 2.0])
                .show(ui, |ui| {
                    let shown: Vec<_> = alts
                        .iter()
                        .filter(|r| {
                            f.is_empty()
                                || format!(
                                    "{} {}",
                                    r.name,
                                    r.outputs
                                        .iter()
                                        .map(|o| item_name(data, &o.item))
                                        .collect::<Vec<_>>()
                                        .join(" ")
                                )
                                .to_lowercase()
                                .contains(&f)
                        })
                        .collect();
                    for (i, r) in shown.iter().enumerate() {
                        let mut on = unlocked.contains(&r.id);
                        let outs = r
                            .outputs
                            .iter()
                            .map(|o| item_name(data, &o.item))
                            .collect::<Vec<_>>()
                            .join(", ");
                        if ui
                            .checkbox(&mut on, &r.name)
                            .on_hover_text(format!("Makes {outs}"))
                            .changed()
                        {
                            let mut next = s.clone();
                            if on {
                                next.unlocked.push(r.id.clone());
                                next.unlocked.sort();
                            } else {
                                next.unlocked.retain(|x| x != &r.id);
                            }
                            actions.push(Action::SaveSettings(next));
                        }
                        if i % 3 == 2 {
                            ui.end_row();
                        }
                    }
                });
            ui.add_space(8.0);
        }

        // Crafting categories more than one building can run.
        let mut cats: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for r in data.recipes.values().filter(|r| r.buildings.len() > 1) {
            if let Some(c) = &r.category {
                cats.entry(c.as_str())
                    .or_default()
                    .extend(r.buildings.iter().map(String::as_str));
            }
        }
        if !cats.is_empty() {
            ui.heading("Preferred building per category");
            ui.label(
                RichText::new("Lines use this unless you pick another building on the line.")
                    .small()
                    .weak(),
            );
            egui::Grid::new("cats")
                .num_columns(2)
                .striped(true)
                .show(ui, |ui| {
                    for (cat, buildings) in cats {
                        ui.label(cat);
                        let current = s.default_building.get(cat);
                        let label = current
                            .and_then(|b| data.buildings.get(b))
                            .map_or("Fastest available", |b| b.name.as_str());
                        let combo = egui::ComboBox::from_id_salt(("cat", cat))
                            .selected_text(label)
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(current.is_none(), "Fastest available")
                                    .clicked()
                                {
                                    let mut next = s.clone();
                                    next.default_building.remove(cat);
                                    actions.push(Action::SaveSettings(next));
                                }
                                for b in buildings {
                                    let name = data.buildings.get(b).map_or(b, |x| x.name.as_str());
                                    if ui
                                        .selectable_label(current.is_some_and(|c| c == b), name)
                                        .clicked()
                                    {
                                        let mut next = s.clone();
                                        next.default_building
                                            .insert(cat.to_string(), b.to_string());
                                        actions.push(Action::SaveSettings(next));
                                    }
                                }
                            });
                        combo
                            .response
                            .accessible_name(format!("Building for {cat}"));
                        ui.end_row();
                    }
                });
        }
    }
}
