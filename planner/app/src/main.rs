//! Factory planner: a native window for planning Satisfactory, Factorio and
//! Dyson Sphere Program factories. Saves are JSON files under factories/.
//!
//!   cargo run --release -p planner-app
//!
//!   planner [--game satisfactory|factorio|dsp] [--view flow|lines|balance|buildings] [factory-id]
//!
//! opens straight to a factory (its id is the file name under factories/<game>/).
//! PLANNER_ROOT points at the folder holding data/ and factories/ (default:
//! the planner folder this was built from).

mod app;
mod editor;
mod fmt;
mod pages;
#[cfg(test)]
mod ui_tests;
mod widgets;

use std::path::PathBuf;

pub fn root() -> PathBuf {
    std::env::var_os("PLANNER_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".."))
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".."))
}

/// What to open at start, from the command line.
#[derive(Default, Clone)]
pub struct Launch {
    pub game: Option<planner_core::GameId>,
    pub view: Option<app::View>,
    pub factory: Option<String>,
}

fn parse_args() -> Result<Launch, String> {
    let mut launch = Launch::default();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--game" => {
                let g = args.next().ok_or("--game needs a value")?;
                launch.game =
                    Some(planner_core::GameId::from_key(&g).ok_or(format!("unknown game {g}"))?);
            }
            "--view" => {
                launch.view = Some(match args.next().as_deref() {
                    Some("flow") => app::View::Flow,
                    Some("lines") => app::View::Lines,
                    Some("balance") => app::View::Balance,
                    Some("buildings") => app::View::Buildings,
                    other => return Err(format!("unknown view {other:?}")),
                })
            }
            "-h" | "--help" => {
                return Err("usage: planner [--game satisfactory|factorio|dsp] [--view flow|lines|balance|buildings] [factory-id]".into());
            }
            id => launch.factory = Some(id.to_string()),
        }
    }
    Ok(launch)
}

fn main() -> eframe::Result {
    let launch = match parse_args() {
        Ok(l) => l,
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(2);
        }
    };
    let store = planner_core::store::Store::new(root());
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Factory Planner")
            .with_inner_size([1360.0, 900.0])
            .with_min_inner_size([720.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "factory-planner",
        options,
        Box::new(|cc| Ok(Box::new(app::PlannerApp::new(cc, store).launch(launch)))),
    )
}
