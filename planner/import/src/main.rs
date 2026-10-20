//! planner-import: regenerate game data and import saves.
//!
//!   planner-import satisfactory [docs.json]   CommunityResources/Docs/en-US.json
//!   planner-import factorio [dump.json]       factorio --dump-data output (+ locale)
//!   planner-import dsp [recipes.tsv]          ../recipes_dsp.tsv
//!   planner-import t9 [workbook.ods|xlsx]     T9 build tabs -> manual factories
//!   planner-import chains [saved_chains/]     old Streamlit saves -> factories

mod chains;
mod dsp;
mod factorio;
mod output;
mod satisfactory;
mod t9;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "Regenerate game data and import saves for the factory planner")]
struct Cli {
    /// Planner folder holding data/ and factories/ (default: this crate's parent).
    #[arg(long, global = true)]
    root: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Satisfactory recipes from the game's docs export.
    Satisfactory { input: Option<PathBuf> },
    /// Factorio recipes from `factorio --dump-data`.
    Factorio { input: Option<PathBuf> },
    /// Dyson Sphere Program recipes from the TSV.
    Dsp { input: Option<PathBuf> },
    /// T9 workbook build tabs as manual factories.
    T9 { input: Option<PathBuf> },
    /// Old Streamlit saved chains as target-mode factories.
    Chains { input: Option<PathBuf> },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let root = cli.root.unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".."));
    match cli.command {
        Command::Satisfactory { input } => satisfactory::run(&root, input),
        Command::Factorio { input } => factorio::run(&root, input),
        Command::Dsp { input } => dsp::run(&root, input),
        Command::T9 { input } => t9::run(&root, input),
        Command::Chains { input } => chains::run(&root, input),
    }
}
