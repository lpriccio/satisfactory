//! Small shared widgets.

use eframe::egui::{self, Color32, RichText, Ui};

#[derive(Clone)]
pub struct Opt {
    pub id: String,
    pub label: String,
    pub hint: String,
}

impl Opt {
    pub fn new(id: impl Into<String>, label: impl Into<String>, hint: impl Into<String>) -> Opt {
        Opt {
            id: id.into(),
            label: label.into(),
            hint: hint.into(),
        }
    }
}

/// A combo box with a search field. Returns the picked id.
pub fn picker(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    options: &[Opt],
    current: &str,
    placeholder: &str,
    width: f32,
) -> Option<String> {
    let id = ui.make_persistent_id(&id_salt);
    let label = options
        .iter()
        .find(|o| o.id == current)
        .map_or(placeholder, |o| o.label.as_str());
    let mut picked = None;
    let shown = egui::ComboBox::from_id_salt(id)
        .width(width)
        .height(360.0)
        .selected_text(if current.is_empty() {
            RichText::new(label).weak()
        } else {
            RichText::new(label)
        })
        .show_ui(ui, |ui| {
            let query_id = id.with("query");
            let mut query: String =
                ui.data_mut(|d| d.get_temp::<String>(query_id).unwrap_or_default());
            let edit = ui.add(
                egui::TextEdit::singleline(&mut query)
                    .hint_text("Type to search…")
                    .desired_width(f32::INFINITY),
            );
            if !edit.has_focus() && ui.data(|d| d.get_temp::<bool>(id.with("focused")).is_none()) {
                edit.request_focus();
                ui.data_mut(|d| d.insert_temp(id.with("focused"), true));
            }
            let q = query.trim().to_lowercase();
            let words: Vec<&str> = q.split_whitespace().collect();
            let mut hits: Vec<&Opt> = options
                .iter()
                .filter(|o| {
                    let hay = format!("{} {} {}", o.label, o.hint, o.id).to_lowercase();
                    words.iter().all(|w| hay.contains(w))
                })
                .collect();
            hits.sort_by_key(|o| !o.label.to_lowercase().starts_with(&q));
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            if enter && let Some(first) = hits.first() {
                picked = Some(first.id.clone());
            }
            for o in hits.iter().take(80) {
                let text = if o.hint.is_empty() {
                    RichText::new(&o.label)
                } else {
                    RichText::new(format!("{}   ·  {}", o.label, o.hint))
                };
                if ui.selectable_label(o.id == current, text).clicked() {
                    picked = Some(o.id.clone());
                }
            }
            if hits.len() > 80 {
                ui.label(RichText::new(format!("…{} more, keep typing", hits.len() - 80)).weak());
            }
            ui.data_mut(|d| d.insert_temp(query_id, query));
            if picked.is_some() {
                ui.close();
            }
        });
    shown.response.accessible_name(placeholder);
    // Closed (or just picked): forget the search so it opens fresh next time.
    if picked.is_some() || shown.inner.is_none() {
        ui.data_mut(|d| {
            d.remove::<String>(id.with("query"));
            d.remove::<bool>(id.with("focused"));
        });
    }
    picked
}

/// A stat tile: small label, big value, quiet detail line.
pub fn tile(ui: &mut Ui, label: &str, value: &str, detail: &str) {
    egui::Frame::group(ui.style())
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.set_min_width(170.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(label).small().weak());
                ui.label(RichText::new(value).size(22.0).strong().monospace());
                ui.label(RichText::new(detail).small().weak());
            });
        });
}

pub fn heading_small(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text.to_uppercase()).small().strong().weak());
}

/// A rounded chip-like button. `color` tints it.
pub fn chip(ui: &mut Ui, text: &str, color: Option<Color32>, tooltip: &str) -> egui::Response {
    let mut t = RichText::new(text).small();
    if let Some(c) = color {
        t = t.color(c);
    }
    let mut b = egui::Button::new(t)
        .corner_radius(10.0)
        .wrap_mode(egui::TextWrapMode::Extend);
    if let Some(c) = color {
        b = b.stroke(egui::Stroke::new(1.0, c));
    }
    let r = ui.add(b);
    if tooltip.is_empty() {
        r
    } else {
        r.on_hover_text(tooltip)
    }
}

pub struct Palette {
    pub accent: Color32,
    pub good: Color32,
    pub warn: Color32,
    pub bad: Color32,
    pub import: Color32,
}

pub fn palette(dark: bool, accent: Color32) -> Palette {
    if dark {
        Palette {
            accent,
            good: Color32::from_rgb(0x5c, 0xc2, 0x8a),
            warn: Color32::from_rgb(0xe7, 0xb8, 0x4b),
            bad: Color32::from_rgb(0xef, 0x6b, 0x6b),
            import: Color32::from_rgb(0x6c, 0xb4, 0xee),
        }
    } else {
        Palette {
            accent,
            good: Color32::from_rgb(0x23, 0x82, 0x4f),
            warn: Color32::from_rgb(0x9a, 0x6b, 0x00),
            bad: Color32::from_rgb(0xc2, 0x3b, 0x3b),
            import: Color32::from_rgb(0x1f, 0x6f, 0xae),
        }
    }
}
