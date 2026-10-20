//! The window: top bar, routing between pages, and the action loop. Pages
//! draw from the current state and push `Action`s; they're applied after the
//! frame, so every change goes through one place (undo, autosave, re-solve).

use crate::editor::{Edit, Editor};
use crate::pages::{factory::FactoryUi, home::HomeUi, settings::SettingsUi, world::WorldUi};
use crate::widgets::{Palette, palette};
use eframe::egui::{self, Color32, Key, KeyboardShortcut, Modifiers, RichText};
use planner_core::factory::{new_factory, new_id, new_world, now_iso};
use planner_core::store::{Store, read_json, write_json};
use planner_core::{Factory, GameData, GameId, GameSettings, Mode};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct Session {
    pub data: Arc<GameData>,
    pub settings: GameSettings,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Flow,
    Lines,
    Balance,
    Buildings,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Route {
    Home,
    Factory,
    World,
    Settings,
}

pub enum Action {
    Edit(Edit, Option<String>),
    Undo,
    Redo,
    SetGame(GameId),
    Home,
    Settings,
    OpenFactory(String),
    NewFactory(Mode),
    DuplicateFactory(String),
    DeleteFactory(String),
    ImportFactory,
    ExportFactory,
    CopyFactoryJson,
    OpenWorld(String),
    NewWorld,
    DeleteWorld(String),
    WorldSaveAsFactory,
    SaveSettings(GameSettings),
    SetView(View),
    Toast(String),
}

pub struct PlannerApp {
    store: Store,
    game: GameId,
    sessions: BTreeMap<GameId, Session>,
    pub(crate) route: Route,
    view: View,
    pub(crate) editor: Option<Editor>,
    factory_ui: FactoryUi,
    home: HomeUi,
    world: Option<WorldUi>,
    settings_ui: SettingsUi,
    toast: Option<(String, Instant)>,
    error: Option<String>,
    themed_for: Option<(GameId, bool)>,
    /// Text to put on the clipboard at the end of the frame.
    clipboard: Option<String>,
    /// PLANNER_SCREENSHOT=path: save one frame as an image, then quit.
    screenshot: Option<(std::path::PathBuf, u32)>,
}

fn accent(game: GameId) -> Color32 {
    match game {
        GameId::Satisfactory => Color32::from_rgb(0xf2, 0xa3, 0x3a),
        GameId::Factorio => Color32::from_rgb(0xe8, 0x93, 0x2d),
        GameId::Dsp => Color32::from_rgb(0x4f, 0xc3, 0xe8),
    }
}

impl PlannerApp {
    pub fn new(cc: &eframe::CreationContext<'_>, store: Store) -> PlannerApp {
        let game = cc
            .storage
            .and_then(|s| s.get_string("game"))
            .and_then(|g| GameId::from_key(&g))
            .unwrap_or(GameId::Satisfactory);
        let mut app = PlannerApp {
            store,
            game,
            sessions: BTreeMap::new(),
            route: Route::Home,
            view: View::Flow,
            editor: None,
            factory_ui: FactoryUi::default(),
            home: HomeUi::default(),
            world: None,
            settings_ui: SettingsUi::default(),
            toast: None,
            error: None,
            themed_for: None,
            clipboard: None,
            screenshot: std::env::var_os("PLANNER_SCREENSHOT").map(|p| (p.into(), 0)),
        };
        app.enter_game(game);
        app
    }

    /// Apply command-line choices: game, view, and a factory to open.
    pub fn launch(mut self, l: crate::Launch) -> PlannerApp {
        if let Some(g) = l.game {
            self.enter_game(g);
        }
        if let Some(v) = l.view {
            self.view = v;
        }
        if let Some(id) = l.factory {
            self.apply(Action::OpenFactory(id));
        }
        self
    }

    fn session(&mut self) -> Option<&Session> {
        if !self.sessions.contains_key(&self.game) {
            match self.store.load_game_data(self.game) {
                Ok(data) => {
                    let settings = self.store.load_settings(self.game);
                    self.sessions.insert(
                        self.game,
                        Session {
                            data: Arc::new(data),
                            settings,
                        },
                    );
                }
                Err(e) => {
                    self.error = Some(format!("{e:#}"));
                    return None;
                }
            }
        }
        self.sessions.get(&self.game)
    }

    fn enter_game(&mut self, game: GameId) {
        self.flush(true);
        self.game = game;
        self.error = None;
        self.editor = None;
        self.world = None;
        self.route = Route::Home;
        if self.session().is_some() {
            self.home.refresh(&self.store, game);
        }
    }

    fn flush(&mut self, force: bool) -> Option<Duration> {
        let mut next = None;
        if let Some(ed) = &mut self.editor {
            next = ed.autosave(&self.store, force);
        }
        if let Some(w) = &mut self.world {
            next = next.or(w.autosave(&self.store, force));
        }
        next
    }

    fn toast(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), Instant::now()));
    }

    fn open_factory(&mut self, f: Factory) {
        let Some(s) = self.session() else { return };
        let (data, settings) = (s.data.clone(), s.settings.clone());
        self.flush(true);
        self.editor = Some(Editor::new(f, data, settings));
        self.factory_ui = FactoryUi::default();
        self.route = Route::Factory;
    }

    fn apply(&mut self, action: Action) {
        let game = self.game;
        match action {
            Action::Edit(edit, merge) => {
                if let Some(ed) = &mut self.editor {
                    ed.apply(edit, merge);
                }
            }
            Action::Undo => {
                if let Some(ed) = &mut self.editor {
                    ed.undo();
                }
            }
            Action::Redo => {
                if let Some(ed) = &mut self.editor {
                    ed.redo();
                }
            }
            Action::SetGame(g) => self.enter_game(g),
            Action::Home => {
                self.flush(true);
                self.route = Route::Home;
                self.editor = None;
                self.world = None;
                self.home.refresh(&self.store, game);
            }
            Action::Settings => {
                self.flush(true);
                self.route = Route::Settings;
            }
            Action::SetView(v) => self.view = v,
            Action::OpenFactory(id) => match self.store.load_factory(game, &id) {
                Ok(f) => self.open_factory(f),
                Err(e) => self.toast(format!("Couldn't open: {e:#}")),
            },
            Action::NewFactory(mode) => {
                let settings = self
                    .session()
                    .map(|s| s.settings.clone())
                    .unwrap_or_default();
                let name = if mode == Mode::Manual {
                    "New build"
                } else {
                    "New factory"
                };
                let f = new_factory(game, &settings, name, mode);
                if let Err(e) = self.store.save_factory(&f) {
                    self.toast(format!("Couldn't save: {e:#}"));
                }
                self.view = if mode == Mode::Manual {
                    View::Lines
                } else {
                    View::Flow
                };
                self.open_factory(f);
            }
            Action::DuplicateFactory(id) => {
                if let Ok(mut f) = self.store.load_factory(game, &id) {
                    f.id = new_id();
                    f.name = format!("{} copy", f.name);
                    f.created_at = now_iso();
                    f.updated_at = f.created_at.clone();
                    let _ = self.store.save_factory(&f);
                }
                self.home.refresh(&self.store, game);
            }
            Action::DeleteFactory(id) => {
                if let Err(e) = self.store.delete_factory(game, &id) {
                    self.toast(format!("Couldn't delete: {e:#}"));
                }
                self.home.refresh(&self.store, game);
            }
            Action::ImportFactory => {
                let Some(path) = rfd::FileDialog::new()
                    .add_filter("Factory JSON", &["json"])
                    .pick_file()
                else {
                    return;
                };
                match read_json::<Factory>(&path) {
                    Ok(f) if f.game != game => {
                        self.toast(format!("That file is a {} factory.", f.game.name()))
                    }
                    Ok(f) => {
                        let _ = self.store.save_factory(&f);
                        self.toast(format!("Imported “{}”", f.name));
                        self.home.refresh(&self.store, game);
                    }
                    Err(e) => self.toast(format!("Couldn't import: {e:#}")),
                }
            }
            Action::ExportFactory => {
                let Some(ed) = &self.editor else { return };
                let file = format!(
                    "{}.json",
                    ed.factory
                        .name
                        .replace(|c: char| !c.is_alphanumeric() && c != '-', "_")
                );
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name(file)
                    .add_filter("Factory JSON", &["json"])
                    .save_file()
                {
                    match write_json(&path, &ed.factory) {
                        Ok(()) => self.toast(format!("Exported to {}", path.display())),
                        Err(e) => self.toast(format!("Couldn't export: {e:#}")),
                    }
                }
            }
            Action::CopyFactoryJson => {
                if let Some(ed) = &self.editor {
                    self.clipboard = serde_json::to_string_pretty(&ed.factory).ok();
                    self.toast("Copied this factory's JSON to the clipboard");
                }
            }
            Action::OpenWorld(id) => match self.store.load_world(game, &id) {
                Ok(w) => {
                    self.flush(true);
                    if let Some(s) = self.session() {
                        let (data, settings) = (s.data.clone(), s.settings.clone());
                        self.world = Some(WorldUi::new(w, &self.store, data, settings));
                        self.route = Route::World;
                    }
                }
                Err(e) => self.toast(format!("Couldn't open: {e:#}")),
            },
            Action::NewWorld => {
                let w = new_world(game, "World");
                let _ = self.store.save_world(&w);
                self.apply(Action::OpenWorld(w.id));
            }
            Action::DeleteWorld(id) => {
                let _ = self.store.delete_world(game, &id);
                self.home.refresh(&self.store, game);
            }
            Action::WorldSaveAsFactory => {
                let Some(w) = &self.world else { return };
                let f = w.flatten();
                let _ = self.store.save_factory(&f);
                self.view = View::Lines;
                self.world = None;
                self.open_factory(f);
            }
            Action::SaveSettings(s) => {
                if let Err(e) = self.store.save_settings(game, &s) {
                    self.toast(format!("Couldn't save settings: {e:#}"));
                }
                if let Some(ed) = &mut self.editor {
                    ed.set_settings(s.clone());
                }
                if let Some(sess) = self.sessions.get_mut(&game) {
                    sess.settings = s;
                }
            }
            Action::Toast(t) => self.toast(t),
        }
    }

    fn theme(&mut self, ctx: &egui::Context) {
        let dark = ctx.global_style().visuals.dark_mode;
        if self.themed_for == Some((self.game, dark)) {
            return;
        }
        self.themed_for = Some((self.game, dark));
        let a = accent(self.game);
        ctx.global_style_mut(|style| {
            // Opaque blend of accent over the panel colour (no alpha in the fill).
            let bg = style.visuals.panel_fill;
            let mix = |x: u8, y: u8| ((x as u16 * 45 + y as u16 * 55) / 100) as u8;
            style.visuals.selection.bg_fill =
                Color32::from_rgb(mix(bg.r(), a.r()), mix(bg.g(), a.g()), mix(bg.b(), a.b()));
            style.visuals.selection.stroke.color = a;
            style.visuals.hyperlink_color = a;
            style.spacing.item_spacing = egui::vec2(8.0, 6.0);
            style.spacing.button_padding = egui::vec2(8.0, 3.0);
        });
    }

    fn top_bar(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Factory Planner")
                        .strong()
                        .color(accent(self.game)),
                );
                ui.add_space(12.0);
                for g in GameId::ALL {
                    if ui.selectable_label(g == self.game, g.name()).clicked() && g != self.game {
                        actions.push(Action::SetGame(g));
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(self.store.root().join("factories").display().to_string())
                            .small()
                            .weak(),
                    )
                    .on_hover_text("Saves are JSON files in this folder");
                    if ui
                        .selectable_label(self.route == Route::Settings, "Settings")
                        .clicked()
                    {
                        actions.push(Action::Settings);
                    }
                    if ui
                        .selectable_label(self.route == Route::Home, "Factories")
                        .clicked()
                    {
                        actions.push(Action::Home);
                    }
                });
            });
            ui.add_space(2.0);
        });
    }

    fn shortcuts(&mut self, ui: &egui::Ui, actions: &mut Vec<Action>) {
        if self.editor.is_none() || ui.ctx().memory(|m| m.focused().is_some()) {
            return;
        }
        let redo = ui.input_mut(|i| {
            i.consume_shortcut(&KeyboardShortcut::new(
                Modifiers::COMMAND | Modifiers::SHIFT,
                Key::Z,
            )) || i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Y))
        });
        if redo {
            actions.push(Action::Redo);
        } else if ui
            .input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Z)))
        {
            actions.push(Action::Undo);
        }
    }
}

impl PlannerApp {
    /// Debug aid: after the UI settles, grab egui's own framebuffer, write it
    /// as a binary PPM, and close the window.
    fn screenshot_step(&mut self, ctx: &egui::Context) {
        let Some((path, frames)) = &mut self.screenshot else {
            return;
        };
        *frames += 1;
        if *frames == 20 {
            let (path, ctx2) = (path.clone(), ctx.clone());
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                egui::ScreenshotCallback::new(move |image| {
                    let [w, h] = image.size;
                    let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
                    for p in &image.pixels {
                        out.extend_from_slice(&[p.r(), p.g(), p.b()]);
                    }
                    if let Err(e) = std::fs::write(&path, out) {
                        eprintln!("screenshot: {e}");
                    }
                    ctx2.send_viewport_cmd(egui::ViewportCommand::Close);
                    ctx2.request_repaint();
                }),
            ));
        }
        ctx.request_repaint();
    }
}

impl eframe::App for PlannerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.theme(&ctx);
        let pal: Palette = palette(ctx.global_style().visuals.dark_mode, accent(self.game));
        let mut actions = vec![];
        self.top_bar(ui, &mut actions);
        self.shortcuts(ui, &mut actions);

        if let Some((text, at)) = &self.toast {
            if at.elapsed() > Duration::from_secs(4) {
                self.toast = None;
            } else {
                let text = text.clone();
                egui::Panel::bottom("toast").show(ui, |ui| {
                    ui.label(RichText::new(text).color(pal.accent));
                });
                ctx.request_repaint_after(Duration::from_millis(500));
            }
        }

        egui::CentralPanel::default().show(ui, |ui| {
            if let Some(err) = &self.error {
                ui.colored_label(pal.bad, err);
                return;
            }
            let Some(session) = self.sessions.get(&self.game) else {
                return;
            };
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    ui.set_max_width(1280.0);
                    match self.route {
                        Route::Home => self.home.show(ui, session, &pal, &mut actions),
                        Route::Factory => match &self.editor {
                            Some(ed) => {
                                self.factory_ui
                                    .show(ui, ed, session, self.view, &pal, &mut actions)
                            }
                            None => {
                                ui.label("No factory open.");
                            }
                        },
                        Route::World => {
                            if let Some(w) = &mut self.world {
                                w.show(ui, &self.home, &pal, &mut actions);
                            }
                        }
                        Route::Settings => self.settings_ui.show(ui, session, &mut actions),
                    }
                });
        });

        for a in actions {
            self.apply(a);
        }
        if let Some(text) = self.clipboard.take() {
            ctx.copy_text(text);
        }
        self.screenshot_step(&ctx);
        if let Some(wait) = self.flush(false) {
            ctx.request_repaint_after(wait);
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string("game", self.game.key().to_string());
        self.flush(true);
    }
}
