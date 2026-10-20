//! Computer Modern Unicode (CMU Serif, its bold for headings, CMU Typewriter
//! for numbers), built into the binary. egui's own fonts stay behind them as
//! fallbacks for any glyph CMU lacks. The fonts are under the SIL OFL; see
//! fonts/OFL.txt.

use eframe::egui::{self, FontData, FontDefinitions, FontFamily, FontId, TextStyle};
use std::sync::Arc;

const SERIF: &[u8] = include_bytes!("../fonts/cmunrm.ttf");
const SERIF_BOLD: &[u8] = include_bytes!("../fonts/cmunbx.ttf");
const TYPEWRITER: &[u8] = include_bytes!("../fonts/cmuntt.ttf");

/// Headings use this family.
pub fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    for (name, bytes) in [
        ("cmu-serif", SERIF),
        ("cmu-serif-bold", SERIF_BOLD),
        ("cmu-typewriter", TYPEWRITER),
    ] {
        fonts
            .font_data
            .insert(name.into(), Arc::new(FontData::from_static(bytes)));
    }
    let fallbacks = fonts.families[&FontFamily::Proportional].clone();
    let family = |first: &str| {
        let mut v = vec![first.to_string()];
        v.extend(fallbacks.iter().cloned());
        v
    };
    fonts
        .families
        .insert(FontFamily::Proportional, family("cmu-serif"));
    fonts.families.insert(bold(), family("cmu-serif-bold"));
    fonts
        .families
        .get_mut(&FontFamily::Monospace)
        .unwrap()
        .insert(0, "cmu-typewriter".into());
    ctx.set_fonts(fonts);
    // Computer Modern runs small next to egui's default, so size up a little.
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (
                TextStyle::Small,
                FontId::new(12.0, FontFamily::Proportional),
            ),
            (TextStyle::Body, FontId::new(15.0, FontFamily::Proportional)),
            (
                TextStyle::Button,
                FontId::new(15.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(14.0, FontFamily::Monospace),
            ),
            (TextStyle::Heading, FontId::new(21.0, bold())),
        ]
        .into();
    });
}
