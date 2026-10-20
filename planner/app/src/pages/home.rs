//! Factory and world lists.

use crate::app::{Action, Session};
use crate::fmt;
use crate::widgets::Palette;
use eframe::egui::{self, RichText, Ui};
use planner_core::solve::item_name;
use planner_core::store::{Store, Summary};
use planner_core::{GameId, Mode, World};

/// A listed factory: its summary, targets (item, rate) and mode.
pub type Listed = (Summary, Vec<(String, f64)>, Mode);

#[derive(Default)]
pub struct HomeUi {
    pub factories: Vec<Listed>,
    pub worlds: Vec<World>,
    filter: String,
    confirm: Option<(bool, String, String)>,
}

impl HomeUi {
    pub fn refresh(&mut self, store: &Store, game: GameId) {
        self.factories = store
            .list_factories(game)
            .into_iter()
            .map(|s| {
                let f = store.load_factory(game, &s.id).ok();
                let targets = f
                    .as_ref()
                    .map(|f| f.targets.iter().map(|t| (t.item.clone(), t.rate)).collect())
                    .unwrap_or_default();
                let mode = f.map_or(Mode::Target, |f| f.mode);
                (s, targets, mode)
            })
            .collect();
        self.worlds = store.list_worlds(game);
    }

    fn factory_list(
        &mut self,
        ui: &mut Ui,
        data: &planner_core::GameData,
        actions: &mut Vec<Action>,
    ) {
        ui.horizontal(|ui| {
            ui.heading("Factories");
            ui.label(RichText::new(self.factories.len().to_string()).weak());
            ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text("Filter")
                    .desired_width(180.0),
            )
            .accessible_name("Filter factories");
        });
        let filter = self.filter.to_lowercase();
        egui::Grid::new("factories")
            .num_columns(4)
            .striped(true)
            .spacing([14.0, 6.0])
            .show(ui, |ui| {
                for (s, targets, mode) in &self.factories {
                    let detail = if targets.is_empty() {
                        s.detail.clone()
                    } else {
                        targets
                            .iter()
                            .map(|(i, r)| format!("{} {}", fmt::num(*r), item_name(data, i)))
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    if !filter.is_empty()
                        && !format!("{} {detail}", s.name)
                            .to_lowercase()
                            .contains(&filter)
                    {
                        continue;
                    }
                    ui.vertical(|ui| {
                        ui.set_min_width(300.0);
                        if ui
                            .add(egui::Link::new(RichText::new(&s.name).strong()))
                            .clicked()
                        {
                            actions.push(Action::OpenFactory(s.id.clone()));
                        }
                        ui.add(egui::Label::new(RichText::new(detail).small().weak()).truncate());
                    });
                    ui.label(
                        RichText::new(if *mode == Mode::Manual {
                            "manual"
                        } else {
                            "target"
                        })
                        .small()
                        .weak(),
                    );
                    ui.label(RichText::new(fmt::when(&s.updated_at)).small().weak());
                    ui.horizontal(|ui| {
                        if ui.small_button("Duplicate").clicked() {
                            actions.push(Action::DuplicateFactory(s.id.clone()));
                        }
                        if ui.small_button("Delete").clicked() {
                            self.confirm = Some((true, s.id.clone(), s.name.clone()));
                        }
                    });
                    ui.end_row();
                }
            });
        if self.factories.is_empty() {
            ui.label(RichText::new("No factories yet. Start one above.").weak());
        }
    }

    fn world_list(&mut self, ui: &mut Ui, actions: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            ui.heading("Worlds");
            if ui.button("New world").clicked() {
                actions.push(Action::NewWorld);
            }
        });
        ui.add(
            egui::Label::new(RichText::new("A world adds up several factories with multipliers and shows what's left over or short.").small().weak())
                .wrap(),
        );
        for w in &self.worlds {
            ui.horizontal(|ui| {
                if ui.link(&w.name).clicked() {
                    actions.push(Action::OpenWorld(w.id.clone()));
                }
                let n = w.entries.len();
                ui.label(
                    RichText::new(format!("{n} factor{}", if n == 1 { "y" } else { "ies" }))
                        .small()
                        .weak(),
                );
                if ui.small_button("Delete").clicked() {
                    self.confirm = Some((false, w.id.clone(), w.name.clone()));
                }
            });
        }
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        session: &Session,
        pal: &Palette,
        actions: &mut Vec<Action>,
    ) {
        let data = &session.data;
        ui.horizontal(|ui| {
            ui.heading(&data.name);
            ui.label(
                RichText::new(format!(
                    "{} recipes · {} items",
                    data.recipes.len(),
                    data.items.len()
                ))
                .weak(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Import JSON…").clicked() {
                    actions.push(Action::ImportFactory);
                }
                if ui
                    .button("New build (manual)")
                    .on_hover_text("Type recipe lines yourself, like a workbook tab")
                    .clicked()
                {
                    actions.push(Action::NewFactory(Mode::Manual));
                }
                if ui
                    .add(
                        egui::Button::new(RichText::new("New factory").strong())
                            .fill(pal.accent.gamma_multiply(0.35)),
                    )
                    .clicked()
                {
                    actions.push(Action::NewFactory(Mode::Target));
                }
            });
        });
        ui.add_space(8.0);

        let width = ui.available_width();
        ui.horizontal_top(|ui| {
            // Factories on the left, worlds in a narrower column on the right.
            ui.vertical(|ui| {
                ui.set_width((width * 0.64).max(420.0));
                self.factory_list(ui, data, actions);
            });
            ui.add_space(24.0);
            ui.vertical(|ui| {
                self.world_list(ui, actions);
            });
        });

        if let Some((is_factory, id, name)) = self.confirm.clone() {
            let modal =
                egui::Modal::new(ui.scope_id().with("confirm-delete")).show(ui.ctx(), |ui| {
                    ui.label(format!("Delete “{name}”? This removes its file."));
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new("Delete").color(pal.bad)).clicked() {
                            actions.push(if is_factory {
                                Action::DeleteFactory(id.clone())
                            } else {
                                Action::DeleteWorld(id.clone())
                            });
                            self.confirm = None;
                        }
                        if ui.button("Cancel").clicked() {
                            self.confirm = None;
                        }
                    });
                });
            if modal.should_close() {
                self.confirm = None;
            }
        }
    }
}
