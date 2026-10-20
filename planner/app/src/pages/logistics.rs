//! Trains, drones and trucks (one table of ports per transport, plus item
//! totals) and the to-do list. Both edit factories/<game>/logistics.json in
//! place, with their own undo history and the same 400 ms autosave as worlds.

use crate::fmt;
use crate::pages::factory::item_options;
use crate::widgets::{Opt, Palette, picker};
use eframe::egui::{self, DragValue, RichText, TextEdit, Ui};
use egui_extras::{Column, TableBuilder};
use planner_core::logistics::*;
use planner_core::solve::item_name;
use planner_core::store::Store;
use planner_core::{GameData, POWER};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Edits made while drawing, applied once the frame's done.
type Change = Box<dyn FnOnce(&mut Logistics)>;

pub struct LogisticsUi {
    pub doc: Logistics,
    data: Arc<GameData>,
    items: Vec<Opt>,
    dirty_at: Option<Instant>,
    undo: Vec<Logistics>,
    redo: Vec<Logistics>,
    /// When the last undo step was taken, so typing merges into one step.
    last_step: Option<Instant>,
    pub tab: Transport,
    show_totals: bool,
    filter: String,
    new_station: String,
    new_todo: String,
    show_done: bool,
}

fn port_mut<'a>(doc: &'a mut Logistics, id: &str) -> Option<&'a mut Port> {
    doc.stations
        .iter_mut()
        .flat_map(|s| s.ports.iter_mut())
        .find(|p| p.id == id)
}

fn station_mut<'a>(doc: &'a mut Logistics, id: &str) -> Option<&'a mut Station> {
    doc.stations.iter_mut().find(|s| s.id == id)
}

fn none_if_empty(s: String) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

impl LogisticsUi {
    pub fn new(doc: Logistics, data: Arc<GameData>) -> LogisticsUi {
        let items = item_options(&data)
            .into_iter()
            .filter(|o| o.id != POWER)
            .collect();
        LogisticsUi {
            doc,
            data,
            items,
            dirty_at: None,
            undo: vec![],
            redo: vec![],
            last_step: None,
            tab: Transport::Train,
            show_totals: false,
            filter: String::new(),
            new_station: String::new(),
            new_todo: String::new(),
            show_done: false,
        }
    }

    pub fn autosave(&mut self, store: &Store, force: bool) -> Option<Duration> {
        let at = self.dirty_at?;
        if !force && at.elapsed() < Duration::from_millis(400) {
            return Some(Duration::from_millis(400) - at.elapsed());
        }
        let _ = store.save_logistics(&self.doc);
        self.dirty_at = None;
        None
    }

    fn apply(&mut self, changes: Vec<Change>) {
        if changes.is_empty() {
            return;
        }
        let before = self.doc.clone();
        for c in changes {
            c(&mut self.doc);
        }
        if self.doc == before {
            return;
        }
        // Keystrokes within a second of each other are one undo step.
        let merge = self
            .last_step
            .is_some_and(|t| t.elapsed() < Duration::from_secs(1));
        if !merge {
            self.undo.push(before);
        }
        self.last_step = Some(Instant::now());
        self.redo.clear();
        self.changed();
    }

    fn changed(&mut self) {
        self.doc.updated_at = planner_core::factory::now_iso();
        self.dirty_at = Some(Instant::now());
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.doc, prev));
            self.last_step = None;
            self.changed();
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.doc, next));
            self.last_step = None;
            self.changed();
        }
    }

    /// Drawn right to left, so Redo comes first.
    fn undo_buttons(&mut self, ui: &mut Ui) {
        if ui
            .add_enabled(!self.redo.is_empty(), egui::Button::new("Redo"))
            .on_hover_text("Ctrl+Shift+Z")
            .clicked()
        {
            self.redo();
        }
        if ui
            .add_enabled(!self.undo.is_empty(), egui::Button::new("Undo"))
            .on_hover_text("Ctrl+Z")
            .clicked()
        {
            self.undo();
        }
    }

    fn item_label(&self, item: &str) -> String {
        if item.is_empty() {
            "(open)".into()
        } else {
            item_name(&self.data, item)
        }
    }

    fn port_label(&self, s: &Station, p: &Port) -> String {
        if p.slot.is_empty() {
            s.name.clone()
        } else {
            format!("{} {}", s.name, p.slot)
        }
    }

    // --- trains / drones / trucks ---------------------------------------------

    pub fn show_transport(&mut self, ui: &mut Ui, pal: &Palette) {
        let mut changes: Vec<Change> = vec![];
        ui.horizontal(|ui| {
            for t in Transport::ALL {
                if ui.selectable_label(self.tab == t, t.name()).clicked() {
                    self.tab = t;
                }
            }
            ui.separator();
            ui.selectable_value(&mut self.show_totals, false, "Ports");
            ui.selectable_value(&mut self.show_totals, true, "Item totals");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.undo_buttons(ui);
            });
        });
        ui.add_space(4.0);
        if self.show_totals {
            self.totals(ui, pal);
            return;
        }
        let kind = self.tab;
        ui.horizontal(|ui| {
            ui.add(
                TextEdit::singleline(&mut self.filter)
                    .hint_text("Filter by station or item…")
                    .desired_width(260.0),
            )
            .accessible_name("Filter");
            ui.separator();
            let r = ui
                .add(
                    TextEdit::singleline(&mut self.new_station)
                        .hint_text(format!("New {}…", kind.station_word().to_lowercase()))
                        .desired_width(200.0),
                )
                .accessible_name("New station name");
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button("Add").clicked() || enter) && !self.new_station.trim().is_empty() {
                let name = std::mem::take(&mut self.new_station).trim().to_string();
                changes.push(Box::new(move |d| d.stations.push(new_station(kind, &name))));
            }
        });
        ui.label(
            RichText::new(match kind {
                Transport::Drone => {
                    "An input names the output it draws from; Used and Free add those up. Rates per minute."
                }
                _ => "Link an input to the output it draws from to see what's free. Rates per minute.",
            })
            .small()
            .weak(),
        );
        ui.add_space(4.0);
        self.ports_table(ui, pal, kind, &mut changes);
        self.apply(changes);
    }

    fn ports_table(&self, ui: &mut Ui, pal: &Palette, kind: Transport, changes: &mut Vec<Change>) {
        let q = self.filter.trim().to_lowercase();
        let rows: Vec<(&Station, usize)> = self
            .doc
            .stations
            .iter()
            .filter(|s| s.kind == kind)
            .filter(|s| {
                q.is_empty()
                    || s.name.to_lowercase().contains(&q)
                    || s.ports
                        .iter()
                        .any(|p| self.item_label(&p.item).to_lowercase().contains(&q))
            })
            .flat_map(|s| (0..s.ports.len().max(1)).map(move |i| (s, i)))
            .collect();
        if rows.is_empty() {
            ui.label(RichText::new("Nothing here yet.").weak());
            return;
        }
        let row_h = ui.text_style_height(&egui::TextStyle::Body) + 12.0;
        let heights: Vec<f32> = rows
            .iter()
            // A one-port station needs a second line for its buttons.
            .map(|(s, i)| {
                if *i == 0 && s.ports.len() < 2 {
                    row_h * 2.0
                } else {
                    row_h
                }
            })
            .collect();
        TableBuilder::new(ui)
            .id_salt(("ports-table", kind))
            .striped(true)
            .resizable(true)
            .vscroll(false)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::initial(190.0).at_least(110.0).clip(true))
            .column(Column::initial(44.0).at_least(32.0).clip(true))
            .column(Column::initial(48.0).at_least(40.0))
            .column(Column::initial(210.0).at_least(110.0).clip(true))
            .column(Column::initial(80.0).at_least(60.0))
            .column(Column::initial(230.0).at_least(120.0).clip(true))
            .column(Column::initial(150.0).at_least(80.0).clip(true))
            .column(Column::initial(70.0).at_least(50.0).clip(true))
            .column(Column::initial(70.0).at_least(50.0).clip(true))
            .column(Column::remainder().at_least(140.0).clip(true))
            .header(22.0, |mut header| {
                for (h, tip) in [
                    (kind.station_word(), ""),
                    ("Slot", ""),
                    ("I/O", "Click to flip between out and in"),
                    ("Item", ""),
                    ("Rate", "Per minute"),
                    (
                        "Draws from / taken by",
                        "Inputs: pick the output this draws from",
                    ),
                    ("Partner", "Who's on the other end, in words"),
                    ("Used", "What linked inputs take from this output"),
                    ("Free", "Rate − used"),
                    ("Note", ""),
                ] {
                    header.col(|ui| {
                        let r = ui.label(RichText::new(h).small().strong().weak());
                        if !tip.is_empty() {
                            r.on_hover_text(tip);
                        }
                    });
                }
            })
            .body(|body| {
                body.heterogeneous_rows(heights.into_iter(), |mut row| {
                    let (s, i) = rows[row.index()];
                    let sid = s.id.clone();
                    row.col(|ui| {
                        // Name on the first row, buttons on the second.
                        let one = s.ports.len() < 2;
                        ui.vertical(|ui| {
                            if i == 0 {
                                self.station_name(ui, s, changes);
                            }
                            if (i == 0 && one) || i == 1 {
                                self.station_buttons(ui, s, changes);
                            }
                        });
                    });
                    let Some(p) = s.ports.get(i) else {
                        return;
                    };
                    let pid = p.id.clone();
                    row.col(|ui| {
                        let mut slot = p.slot.clone();
                        if ui
                            .add(TextEdit::singleline(&mut slot).desired_width(f32::INFINITY))
                            .accessible_name("Slot")
                            .changed()
                        {
                            let pid = pid.clone();
                            changes.push(Box::new(move |d| {
                                if let Some(p) = port_mut(d, &pid) {
                                    p.slot = slot.trim().to_string();
                                }
                            }));
                        }
                    });
                    row.col(|ui| {
                        let (text, color) = match p.dir {
                            Dir::Out => ("out", pal.good),
                            Dir::In => ("in", pal.import),
                        };
                        if ui
                            .add(egui::Button::new(RichText::new(text).color(color)).small())
                            .on_hover_text("Flip between out and in")
                            .clicked()
                        {
                            let pid = pid.clone();
                            changes.push(Box::new(move |d| {
                                if let Some(p) = port_mut(d, &pid) {
                                    p.dir = match p.dir {
                                        Dir::Out => Dir::In,
                                        Dir::In => Dir::Out,
                                    };
                                    p.from = None;
                                }
                                // An output that becomes an input loses its takers.
                                for q in d.stations.iter_mut().flat_map(|s| &mut s.ports) {
                                    if q.from.as_deref() == Some(&pid) {
                                        q.from = None;
                                    }
                                }
                            }));
                        }
                    });
                    row.col(|ui| {
                        let mut options = self.items.clone();
                        if !p.item.is_empty() && !self.data.items.contains_key(&p.item) {
                            options.insert(0, Opt::new(&p.item, &p.item, "not a game item"));
                        }
                        let width = ui.available_width() - 4.0;
                        if let Some(item) =
                            picker(ui, ("lg-item", &p.id), &options, &p.item, "(open)", width)
                        {
                            let pid = pid.clone();
                            changes.push(Box::new(move |d| {
                                if let Some(p) = port_mut(d, &pid) {
                                    if p.item != item {
                                        p.from = None;
                                    }
                                    p.item = item;
                                }
                            }));
                        }
                    });
                    row.col(|ui| {
                        let mut rate = p.rate;
                        if ui
                            .add(DragValue::new(&mut rate).range(0.0..=1e6).speed(1.0))
                            .accessible_name("Rate")
                            .changed()
                        {
                            let pid = pid.clone();
                            changes.push(Box::new(move |d| {
                                if let Some(p) = port_mut(d, &pid) {
                                    p.rate = rate;
                                }
                            }));
                        }
                    });
                    row.col(|ui| self.link_cell(ui, s, p, changes));
                    row.col(|ui| {
                        let mut partner = p.partner.clone().unwrap_or_default();
                        if ui
                            .add(TextEdit::singleline(&mut partner).desired_width(f32::INFINITY))
                            .accessible_name("Partner")
                            .changed()
                        {
                            let pid = pid.clone();
                            changes.push(Box::new(move |d| {
                                if let Some(p) = port_mut(d, &pid) {
                                    p.partner = none_if_empty(partner);
                                }
                            }));
                        }
                    });
                    let used = self.doc.drain(&p.id);
                    row.col(|ui| {
                        if p.dir == Dir::Out && used > 0.0 {
                            ui.label(RichText::new(fmt::num(used)).monospace());
                        }
                    });
                    row.col(|ui| {
                        if p.dir == Dir::Out {
                            let free = p.rate - used;
                            let c = if free < -1e-6 {
                                pal.bad
                            } else if free > 1e-6 {
                                pal.good
                            } else {
                                ui.visuals().weak_text_color()
                            };
                            ui.label(RichText::new(fmt::num(free)).monospace().color(c));
                        }
                    });
                    row.col(|ui| {
                        let mut note = p.note.clone().unwrap_or_default();
                        let w = ui.available_width() - 34.0;
                        if ui
                            .add(TextEdit::singleline(&mut note).desired_width(w))
                            .accessible_name("Note")
                            .changed()
                        {
                            let pid = pid.clone();
                            changes.push(Box::new(move |d| {
                                if let Some(p) = port_mut(d, &pid) {
                                    p.note = none_if_empty(note);
                                }
                            }));
                        }
                        if ui
                            .add(egui::Button::new(RichText::new("×").color(pal.bad)).small())
                            .on_hover_text("Delete this port (the last one takes its station)")
                            .clicked()
                        {
                            changes.push(Box::new(move |d| {
                                d.remove_port(&pid);
                                if d.stations.iter().any(|s| s.id == sid && s.ports.is_empty()) {
                                    d.remove_station(&sid);
                                }
                            }));
                        }
                    });
                });
            });
    }

    fn station_name(&self, ui: &mut Ui, s: &Station, changes: &mut Vec<Change>) {
        let mut name = s.name.clone();
        if ui
            .add(TextEdit::singleline(&mut name).desired_width(f32::INFINITY))
            .accessible_name("Station name")
            .changed()
        {
            let sid = s.id.clone();
            changes.push(Box::new(move |d| {
                if let Some(s) = station_mut(d, &sid) {
                    s.name = name;
                }
            }));
        }
    }

    fn station_buttons(&self, ui: &mut Ui, s: &Station, changes: &mut Vec<Change>) {
        ui.horizontal(|ui| {
            if ui
                .small_button("+ port")
                .on_hover_text("Add a port to this station")
                .clicked()
            {
                let sid = s.id.clone();
                let slot = next_slot(s);
                changes.push(Box::new(move |d| {
                    if let Some(s) = station_mut(d, &sid) {
                        s.ports.push(Port {
                            slot,
                            ..new_port(Dir::Out)
                        });
                    }
                }));
            }
            if s.kind == Transport::Train {
                let mut trains = s.trains.unwrap_or(0);
                let suffix = if trains == 1 { " train" } else { " trains" };
                if ui
                    .add(DragValue::new(&mut trains).range(0..=99).suffix(suffix))
                    .accessible_name("Trains")
                    .changed()
                {
                    let sid = s.id.clone();
                    changes.push(Box::new(move |d| {
                        if let Some(s) = station_mut(d, &sid) {
                            s.trains = (trains > 0).then_some(trains);
                        }
                    }));
                }
            }
        });
    }

    /// Inputs pick the output they draw from; outputs list who takes from them.
    fn link_cell(&self, ui: &mut Ui, s: &Station, p: &Port, changes: &mut Vec<Change>) {
        match p.dir {
            Dir::In => {
                let mut options = vec![Opt::new("", "(not linked)", "")];
                options.extend(
                    self.doc
                        .sources(s.kind, &p.item)
                        .into_iter()
                        .filter(|(_, o)| o.id != p.id)
                        .map(|(os, o)| {
                            let free = o.rate - self.doc.drain(&o.id);
                            Opt::new(
                                &o.id,
                                self.port_label(os, o),
                                format!("{} /min, {} free", fmt::num(o.rate), fmt::num(free)),
                            )
                        }),
                );
                let current = p.from.clone().unwrap_or_default();
                let width = ui.available_width() - 4.0;
                if let Some(from) = picker(
                    ui,
                    ("lg-from", &p.id),
                    &options,
                    &current,
                    "Draws from…",
                    width,
                ) {
                    let pid = p.id.clone();
                    changes.push(Box::new(move |d| {
                        if let Some(p) = port_mut(d, &pid) {
                            p.from = none_if_empty(from);
                        }
                    }));
                }
            }
            Dir::Out => {
                let takers: Vec<String> = self
                    .doc
                    .takers(&p.id)
                    .into_iter()
                    .map(|(ts, t)| format!("{} {}", self.port_label(ts, t), fmt::num(t.rate)))
                    .collect();
                if !takers.is_empty() {
                    let text = takers.join(", ");
                    ui.add(egui::Label::new(RichText::new(&text).small()).truncate())
                        .on_hover_text(text);
                }
            }
        }
    }

    fn totals(&self, ui: &mut Ui, pal: &Palette) {
        let mut rows: Vec<(String, f64, f64)> = self
            .doc
            .totals(self.tab)
            .into_iter()
            .map(|(item, (o, i))| (self.item_label(&item), o, i))
            .collect();
        rows.sort_by_key(|r| r.0.to_lowercase());
        if rows.is_empty() {
            ui.label(RichText::new("Nothing here yet.").weak());
            return;
        }
        ui.label(
            RichText::new(
                "Everything sent out (out) and taken in (in) by this transport, per minute.",
            )
            .small()
            .weak(),
        );
        let h = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
        TableBuilder::new(ui)
            .id_salt(("totals-table", self.tab))
            .striped(true)
            .resizable(true)
            .vscroll(false)
            .column(Column::initial(260.0).at_least(120.0).clip(true))
            .column(Column::initial(100.0).at_least(50.0))
            .column(Column::initial(100.0).at_least(50.0))
            .column(Column::remainder().at_least(80.0))
            .header(22.0, |mut header| {
                for h in ["Item", "Out", "In", "Free"] {
                    header.col(|ui| {
                        ui.label(RichText::new(h).small().strong().weak());
                    });
                }
            })
            .body(|body| {
                body.rows(h, rows.len(), |mut row| {
                    let (name, o, i) = &rows[row.index()];
                    let free = o - i;
                    row.col(|ui| {
                        ui.label(name);
                    });
                    row.col(|ui| {
                        ui.label(RichText::new(fmt::num(*o)).monospace());
                    });
                    row.col(|ui| {
                        ui.label(RichText::new(fmt::num(*i)).monospace().color(pal.import));
                    });
                    row.col(|ui| {
                        let c = if free < -1e-6 { pal.bad } else { pal.good };
                        ui.label(RichText::new(fmt::signed(free)).monospace().color(c));
                    });
                });
            });
    }

    // --- to-do ----------------------------------------------------------------

    pub fn show_todo(&mut self, ui: &mut Ui, pal: &Palette) {
        let mut changes: Vec<Change> = vec![];
        let open = self.doc.todos.iter().filter(|t| !t.done).count();
        let done = self.doc.todos.len() - open;
        ui.horizontal(|ui| {
            ui.heading("To-do");
            ui.label(RichText::new(format!("{open} open, {done} done")).weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.undo_buttons(ui);
            });
        });
        ui.horizontal(|ui| {
            let r = ui
                .add(
                    TextEdit::singleline(&mut self.new_todo)
                        .hint_text("Add a task…")
                        .desired_width(420.0),
                )
                .accessible_name("New task");
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button("Add").clicked() || enter) && !self.new_todo.trim().is_empty() {
                let text = std::mem::take(&mut self.new_todo).trim().to_string();
                changes.push(Box::new(move |d| d.todos.push(new_todo(&text))));
                r.request_focus();
            }
            ui.checkbox(&mut self.show_done, "Show done");
        });
        ui.add_space(6.0);

        let visible: Vec<usize> = (0..self.doc.todos.len())
            .filter(|&i| self.show_done || !self.doc.todos[i].done)
            .collect();
        if visible.is_empty() {
            ui.label(RichText::new("Nothing to do. Suspicious.").weak());
        }
        for (k, &i) in visible.iter().enumerate() {
            let t = &self.doc.todos[i];
            let id = t.id.clone();
            ui.horizontal(|ui| {
                let mut on = t.done;
                if ui.checkbox(&mut on, "").accessible_name(&t.text).changed() {
                    let id = id.clone();
                    changes.push(Box::new(move |d| {
                        if let Some(t) = d.todos.iter_mut().find(|t| t.id == id) {
                            t.done = on;
                        }
                    }));
                }
                if t.done {
                    ui.label(RichText::new(&t.text).strikethrough().weak());
                } else {
                    let mut text = t.text.clone();
                    if ui
                        .add(TextEdit::singleline(&mut text).desired_width(520.0))
                        .accessible_name("Task")
                        .changed()
                    {
                        let id = id.clone();
                        changes.push(Box::new(move |d| {
                            if let Some(t) = d.todos.iter_mut().find(|t| t.id == id) {
                                t.text = text;
                            }
                        }));
                    }
                }
                // Moves swap with the neighbour you can see.
                let swap = |other: usize| -> Change {
                    Box::new(move |d: &mut Logistics| d.todos.swap(i, other))
                };
                if ui
                    .add_enabled(k > 0, egui::Button::new("Up").small())
                    .clicked()
                {
                    changes.push(swap(visible[k - 1]));
                }
                if ui
                    .add_enabled(k + 1 < visible.len(), egui::Button::new("Down").small())
                    .clicked()
                {
                    changes.push(swap(visible[k + 1]));
                }
                if ui
                    .add(egui::Button::new(RichText::new("×").color(pal.bad)).small())
                    .on_hover_text("Delete task")
                    .clicked()
                {
                    changes.push(Box::new(move |d| d.todos.retain(|t| t.id != id)));
                }
            });
        }
        self.apply(changes);
    }
}

/// The letter after the station's last slot: A, B, … (empty if it has none).
fn next_slot(s: &Station) -> String {
    let last = s.ports.iter().filter_map(|p| p.slot.chars().next()).max();
    match last {
        Some(c) if c.is_ascii_uppercase() && c < 'Z' => ((c as u8 + 1) as char).to_string(),
        _ if s.ports.is_empty() => "A".into(),
        _ => String::new(),
    }
}
