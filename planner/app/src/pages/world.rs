//! World page: several factories with multipliers, added up.

use crate::app::Action;
use crate::fmt;
use crate::pages::home::HomeUi;
use crate::widgets::{Opt, Palette, picker, tile};
use eframe::egui::{self, DragValue, RichText, Ui};
use egui_extras::{Column, TableBuilder};
use planner_core::factory::now_iso;
use planner_core::solve::{item_name, solve_factory};
use planner_core::store::Store;
use planner_core::world::{Member, combine_plans, net_position, world_to_factory};
use planner_core::*;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct WorldUi {
    world: World,
    factories: BTreeMap<String, Factory>,
    plans: BTreeMap<String, Plan>,
    data: Arc<GameData>,
    settings: GameSettings,
    dirty_at: Option<Instant>,
}

impl WorldUi {
    pub fn new(
        world: World,
        store: &Store,
        data: Arc<GameData>,
        settings: GameSettings,
    ) -> WorldUi {
        let mut w = WorldUi {
            world,
            factories: BTreeMap::new(),
            plans: BTreeMap::new(),
            data,
            settings,
            dirty_at: None,
        };
        w.load(store);
        w
    }

    fn load(&mut self, store: &Store) {
        for e in &self.world.entries {
            if !self.factories.contains_key(&e.factory)
                && let Ok(f) = store.load_factory(self.world.game, &e.factory)
            {
                self.plans
                    .insert(f.id.clone(), solve_factory(&self.data, &f, &self.settings));
                self.factories.insert(f.id.clone(), f);
            }
        }
    }

    fn members(&self) -> Vec<Member<'_>> {
        self.world
            .entries
            .iter()
            .filter_map(|e| {
                Some(Member {
                    factory: self.factories.get(&e.factory)?,
                    plan: self.plans.get(&e.factory)?,
                    multiplier: e.multiplier,
                })
            })
            .collect()
    }

    pub fn flatten(&self) -> Factory {
        world_to_factory(&self.world, &self.members())
    }

    pub fn autosave(&mut self, store: &Store, force: bool) -> Option<Duration> {
        let at = self.dirty_at?;
        if !force && at.elapsed() < Duration::from_millis(400) {
            return Some(Duration::from_millis(400) - at.elapsed());
        }
        let _ = store.save_world(&self.world);
        self.dirty_at = None;
        None
    }

    fn changed(&mut self) {
        self.world.updated_at = now_iso();
        self.dirty_at = Some(Instant::now());
    }

    pub fn show(&mut self, ui: &mut Ui, home: &HomeUi, pal: &Palette, actions: &mut Vec<Action>) {
        let data = self.data.clone();
        ui.horizontal(|ui| {
            if ui.link("< Factories").clicked() {
                actions.push(Action::Home);
            }
            if ui
                .add(
                    egui::TextEdit::singleline(&mut self.world.name)
                        .font(egui::TextStyle::Heading)
                        .desired_width(360.0),
                )
                .accessible_name("World name")
                .changed()
            {
                self.changed();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        !self.world.entries.is_empty(),
                        egui::Button::new("Save as one factory"),
                    )
                    .on_hover_text(
                        "One manual factory holding every line, scaled by its multiplier",
                    )
                    .clicked()
                {
                    actions.push(Action::WorldSaveAsFactory);
                }
            });
        });

        egui::Frame::group(ui.style())
            .inner_margin(10.0)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                crate::widgets::heading_small(ui, "Factories in this world");
                let mut remove = None;
                let mut changed = false;
                for (i, e) in self.world.entries.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        changed |= ui
                            .add(
                                DragValue::new(&mut e.multiplier)
                                    .range(0.0..=1e6)
                                    .speed(0.1)
                                    .prefix("× "),
                            )
                            .accessible_name("Multiplier")
                            .changed();
                        match self.factories.get(&e.factory) {
                            Some(f) => {
                                if ui.link(&f.name).clicked() {
                                    actions.push(Action::OpenFactory(f.id.clone()));
                                }
                            }
                            None => {
                                ui.colored_label(pal.bad, format!("missing factory {}", e.factory));
                            }
                        }
                        if ui
                            .small_button("×")
                            .on_hover_text("Remove from world")
                            .clicked()
                        {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    self.world.entries.remove(i);
                    changed = true;
                }
                let options: Vec<Opt> = home
                    .factories
                    .iter()
                    .map(|(s, _, _)| Opt::new(&s.id, &s.name, ""))
                    .collect();
                if let Some(id) = picker(ui, "add-factory", &options, "", "Add a factory…", 300.0)
                {
                    self.world.entries.push(WorldEntry {
                        factory: id,
                        multiplier: 1.0,
                    });
                    changed = true;
                    actions.push(Action::Toast("Added".into()));
                }
                if changed {
                    self.changed();
                }
            });
        // Load anything newly added (needs the store, so it happens on the next frame via OpenWorld refresh).
        let missing: Vec<String> = self
            .world
            .entries
            .iter()
            .filter(|e| !self.factories.contains_key(&e.factory))
            .map(|e| e.factory.clone())
            .collect();
        if !missing.is_empty() {
            let store = Store::new(planner_root());
            self.load(&store);
        }

        let totals = combine_plans(&self.members());
        ui.horizontal_wrapped(|ui| {
            if data.features.power {
                tile(ui, "Power", &fmt::mw(totals.power_mw), "");
            }
            tile(ui, "Machines", &fmt::num(totals.machines), "");
            if let Some(fd) = totals.foundations {
                tile(ui, "Foundations", &fmt::num(fd), "");
            } else if data.features.footprint {
                tile(
                    ui,
                    "Floor",
                    &format!(
                        "{} {}",
                        fmt::num(totals.footprint),
                        data.features.area_unit.as_deref().unwrap_or("")
                    ),
                    "",
                );
            }
        });

        ui.add_space(8.0);
        ui.heading("Net position");
        ui.label(RichText::new("What the world sends out (+) or still needs (−) once factories feed each other. Raw resources show as needs.").small().weak());
        let mut pos: Vec<(String, f64)> = net_position(&totals.items)
            .into_iter()
            .filter(|(i, _)| i != POWER)
            .collect();
        pos.sort_by(|a, b| a.1.total_cmp(&b.1));
        TableBuilder::new(ui)
            .id_salt("net-table")
            .striped(true)
            .resizable(true)
            .vscroll(false)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::initial(280.0).at_least(120.0).clip(true))
            .column(Column::initial(100.0).at_least(50.0).clip(true))
            .column(Column::initial(100.0).at_least(50.0).clip(true))
            .column(Column::remainder().at_least(60.0))
            .header(22.0, |mut header| {
                for h in ["Item", "Net /min", "Out", "In"] {
                    header.col(|ui| {
                        ui.label(RichText::new(h).small().strong().weak());
                    });
                }
            })
            .body(|body| {
                body.rows(22.0, pos.len(), |mut row| {
                    let (item, net) = &pos[row.index()];
                    let p = &totals.items[item];
                    row.col(|ui| {
                        ui.add(egui::Label::new(item_name(&data, item)).truncate());
                    });
                    let c = if *net > 0.0 { pal.good } else { pal.bad };
                    row.col(|ui| {
                        ui.label(RichText::new(fmt::signed(*net)).monospace().color(c));
                    });
                    row.col(|ui| {
                        ui.label(RichText::new(fmt::num(p.target + p.surplus)).monospace());
                    });
                    row.col(|ui| {
                        ui.label(
                            RichText::new(fmt::num(p.imported))
                                .monospace()
                                .color(pal.import),
                        );
                    });
                });
            });
        if pos.is_empty() {
            ui.label(RichText::new("Add factories to see the balance.").weak());
        }
    }
}

pub fn planner_root() -> std::path::PathBuf {
    crate::root()
}
