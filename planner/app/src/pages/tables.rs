//! Balance and Buildings views.

use crate::app::Action;
use crate::editor::Editor;
use crate::fmt;
use crate::pages::factory::{FactoryUi, edit};
use crate::widgets::{Palette, chip};
use eframe::egui::{self, RichText, Ui};
use planner_core::edits::{import_locked_reason, item_imported, toggle_item_import};
use planner_core::linemath::whole_machines;
use planner_core::solve::{item_name, recipe_name};
use planner_core::*;
use std::collections::BTreeMap;

const COLS: [(&str, &str); 7] = [
    ("Item", ""),
    ("Produced", "Made by lines in this factory"),
    ("Consumed", "Eaten by lines in this factory"),
    ("Net", "Produced − consumed"),
    ("Imported", "Brought in from outside"),
    ("Target", "Sent out as the factory's output"),
    ("Surplus", "Made but not used: byproducts or rounding"),
];

fn value(data: &GameData, p: &PlanItem, col: usize) -> Result<f64, String> {
    match col {
        0 => Err(item_name(data, &p.item).to_lowercase()),
        1 => Ok(p.produced),
        2 => Ok(p.consumed),
        3 => Ok(p.produced - p.consumed),
        4 => Ok(p.imported),
        5 => Ok(p.target),
        _ => Ok(p.surplus),
    }
}

pub fn balance(
    ui: &mut Ui,
    ed: &Editor,
    st: &mut FactoryUi,
    pal: &Palette,
    actions: &mut Vec<Action>,
) {
    let data = &ed.data;
    let hide_zero = st.balance_hide_zero.get_or_insert(true);
    ui.horizontal(|ui| {
        ui.checkbox(hide_zero, "Hide items with no flow");
        ui.label(
            RichText::new(
                "All figures per minute. The import toggle applies to every line using the item.",
            )
            .small()
            .weak(),
        );
    });
    let hide = *hide_zero;
    let (sort_col, desc) = st.balance_sort;
    let mut rows: Vec<&PlanItem> = ed
        .plan
        .items
        .values()
        .filter(|p| {
            p.item != POWER && (!hide || p.produced + p.consumed + p.imported + p.target > 1e-6)
        })
        .collect();
    rows.sort_by(|a, b| {
        let o = match (value(data, a, sort_col), value(data, b, sort_col)) {
            (Ok(x), Ok(y)) => x.total_cmp(&y),
            (Err(x), Err(y)) => x.cmp(&y),
            _ => std::cmp::Ordering::Equal,
        };
        if desc { o.reverse() } else { o }
    });

    egui::Grid::new("balance")
        .num_columns(8)
        .striped(true)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            for (i, (name, tip)) in COLS.iter().enumerate() {
                let arrow = if sort_col == i {
                    if desc {
                        " (high first)"
                    } else {
                        " (low first)"
                    }
                } else {
                    ""
                };
                let r = ui.selectable_label(
                    sort_col == i,
                    RichText::new(format!("{name}{arrow}")).small().strong(),
                );
                let r = if tip.is_empty() {
                    r
                } else {
                    r.on_hover_text(*tip)
                };
                if r.clicked() {
                    st.balance_sort = if sort_col == i {
                        (i, !desc)
                    } else {
                        (i, i != 0)
                    };
                }
            }
            ui.label(RichText::new("Import").small().strong());
            ui.end_row();
            for p in rows {
                let net = p.produced - p.consumed;
                ui.horizontal(|ui| {
                    ui.label(item_name(data, &p.item));
                    if p.unbalanced > 1e-6 {
                        ui.label(
                            RichText::new(format!("{} short", fmt::num(p.unbalanced)))
                                .small()
                                .color(pal.bad),
                        );
                    }
                });
                ui.label(RichText::new(fmt::num(p.produced)).monospace());
                ui.label(RichText::new(fmt::num(p.consumed)).monospace());
                let c = if net > 1e-6 {
                    pal.good
                } else if net < -1e-6 {
                    pal.bad
                } else {
                    ui.visuals().text_color()
                };
                ui.label(RichText::new(fmt::signed(net)).monospace().color(c));
                ui.label(
                    RichText::new(if p.imported > 1e-6 {
                        fmt::num(p.imported)
                    } else {
                        String::new()
                    })
                    .monospace()
                    .color(pal.import),
                );
                ui.label(
                    RichText::new(if p.target > 1e-6 {
                        fmt::num(p.target)
                    } else {
                        String::new()
                    })
                    .monospace(),
                );
                ui.label(
                    RichText::new(if p.surplus > 1e-6 {
                        fmt::num(p.surplus)
                    } else {
                        String::new()
                    })
                    .monospace()
                    .color(pal.warn),
                );
                let on = item_imported(data, &ed.factory, &p.item);
                let locked = import_locked_reason(data, &ed.factory, &p.item);
                let r = ui
                    .add_enabled_ui(locked.is_none(), |ui| {
                        chip(
                            ui,
                            if on { "imported" } else { "import" },
                            on.then_some(pal.import),
                            "",
                        )
                    })
                    .inner;
                let r = match &locked {
                    Some(why) => r.on_disabled_hover_text(why),
                    None => r,
                };
                if r.clicked() {
                    let (d, item) = (ed.data.clone(), p.item.clone());
                    edit(actions, None, move |f, _| toggle_item_import(&d, f, &item));
                }
                ui.end_row();
            }
        });
}

pub fn buildings(ui: &mut Ui, ed: &Editor) {
    let data = &ed.data;
    let plan = &ed.plan;
    let features = &data.features;
    let spacing = ed.factory.spacing;
    let mut groups: BTreeMap<&str, Vec<&PlanLine>> = BTreeMap::new();
    for l in plan.lines.iter().filter(|l| l.machines > 1e-9) {
        groups.entry(l.building.as_str()).or_default().push(l);
    }
    let mut groups: Vec<(&str, Vec<&PlanLine>)> = groups.into_iter().collect();
    groups.sort_by(|a, b| {
        let w = |g: &Vec<&PlanLine>| g.iter().map(|l| whole_machines(l.machines)).sum::<f64>();
        w(&b.1).total_cmp(&w(&a.1))
    });
    let foundations = plan.foundations.is_some();
    let unit = features.area_unit.clone().unwrap_or_default();

    let mut headers = vec![
        "Building / recipe".to_string(),
        "Machines".into(),
        "Whole".into(),
    ];
    if features.clock {
        headers.extend(["Clock".into(), "Sloops".into()]);
    }
    if features.power {
        headers.push("MW".into());
    }
    if features.footprint {
        headers.push(format!("Floor {unit}"));
    }
    if foundations {
        headers.push("Foundations".into());
    }
    let row = |ui: &mut Ui,
               name: RichText,
               machines: f64,
               whole: f64,
               clock: Option<String>,
               sloops: Option<String>,
               power: f64,
               floor: f64| {
        ui.label(name);
        ui.label(RichText::new(fmt::num(machines)).monospace());
        ui.label(RichText::new(fmt::num(whole)).monospace());
        if features.clock {
            ui.label(RichText::new(clock.unwrap_or_default()).monospace());
            ui.label(RichText::new(sloops.unwrap_or_default()).monospace());
        }
        if features.power {
            ui.label(RichText::new(fmt::num(power)).monospace());
        }
        if features.footprint {
            ui.label(RichText::new(fmt::num(floor)).monospace());
        }
        if foundations {
            ui.label(RichText::new(fmt::num(floor * spacing / 64.0)).monospace());
        }
        ui.end_row();
    };
    egui::Grid::new("buildings")
        .num_columns(headers.len())
        .striped(true)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            for h in &headers {
                ui.label(RichText::new(h).small().strong().weak());
            }
            ui.end_row();
            for (b, lines) in &groups {
                let name = data.buildings.get(*b).map_or(*b, |x| x.name.as_str());
                let m: f64 = lines.iter().map(|l| l.machines).sum();
                let w: f64 = lines.iter().map(|l| whole_machines(l.machines)).sum();
                let p: f64 = lines.iter().map(|l| l.power_mw).sum();
                let fl: f64 = lines.iter().map(|l| l.footprint).sum();
                row(ui, RichText::new(name).strong(), m, w, None, None, p, fl);
                let mut lines = lines.clone();
                lines.sort_by(|a, b| b.machines.total_cmp(&a.machines));
                for l in lines {
                    row(
                        ui,
                        RichText::new(format!("    {}", recipe_name(data, &l.recipe))),
                        l.machines,
                        whole_machines(l.machines),
                        Some(fmt::pct(l.settings.clock)),
                        l.settings.sloops.map(|s| s.to_string()),
                        l.power_mw,
                        l.footprint,
                    );
                }
            }
            let whole: f64 = plan.lines.iter().map(|l| whole_machines(l.machines)).sum();
            row(
                ui,
                RichText::new("Total").strong(),
                plan.machines,
                whole,
                None,
                None,
                plan.power_mw,
                plan.footprint,
            );
        });
}
