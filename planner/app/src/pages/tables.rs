//! Balance and Buildings views.

use crate::app::Action;
use crate::editor::Editor;
use crate::fmt;
use crate::pages::factory::{FactoryUi, edit};
use crate::widgets::{Palette, chip};
use eframe::egui::{self, RichText, Ui};
use egui_extras::{Column, TableBuilder};
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

    let mut table = TableBuilder::new(ui)
        .id_salt("balance-table")
        .striped(true)
        .resizable(true)
        .vscroll(false)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::initial(280.0).at_least(120.0).clip(true));
    for _ in 1..COLS.len() {
        table = table.column(Column::initial(100.0).at_least(50.0).clip(true));
    }
    table
        .column(Column::remainder().at_least(90.0))
        .header(22.0, |mut header| {
            for (i, (name, tip)) in COLS.iter().enumerate() {
                header.col(|ui| {
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
                });
            }
            header.col(|ui| {
                ui.label(RichText::new("Import").small().strong());
            });
        })
        .body(|body| {
            body.rows(24.0, rows.len(), |mut row| {
                let p = rows[row.index()];
                let net = p.produced - p.consumed;
                row.col(|ui| {
                    ui.add(egui::Label::new(item_name(data, &p.item)).truncate());
                    if p.unbalanced > 1e-6 {
                        ui.label(
                            RichText::new(format!("{} short", fmt::num(p.unbalanced)))
                                .small()
                                .color(pal.bad),
                        );
                    }
                });
                let number = |row: &mut egui_extras::TableRow,
                              text: String,
                              color: Option<egui::Color32>| {
                    row.col(|ui| {
                        let mut t = RichText::new(text).monospace();
                        if let Some(c) = color {
                            t = t.color(c);
                        }
                        ui.label(t);
                    });
                };
                let shown = |x: f64| if x > 1e-6 { fmt::num(x) } else { String::new() };
                number(&mut row, fmt::num(p.produced), None);
                number(&mut row, fmt::num(p.consumed), None);
                let c = if net > 1e-6 {
                    Some(pal.good)
                } else if net < -1e-6 {
                    Some(pal.bad)
                } else {
                    None
                };
                number(&mut row, fmt::signed(net), c);
                number(&mut row, shown(p.imported), Some(pal.import));
                number(&mut row, shown(p.target), None);
                number(&mut row, shown(p.surplus), Some(pal.warn));
                row.col(|ui| {
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
                });
            });
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
    struct Row {
        name: RichText,
        machines: f64,
        whole: f64,
        clock: Option<String>,
        sloops: Option<String>,
        power: f64,
        floor: f64,
    }
    let mut table_rows = vec![];
    for (b, lines) in &groups {
        let name = data.buildings.get(*b).map_or(*b, |x| x.name.as_str());
        table_rows.push(Row {
            name: RichText::new(name).strong(),
            machines: lines.iter().map(|l| l.machines).sum(),
            whole: lines.iter().map(|l| whole_machines(l.machines)).sum(),
            clock: None,
            sloops: None,
            power: lines.iter().map(|l| l.power_mw).sum(),
            floor: lines.iter().map(|l| l.footprint).sum(),
        });
        let mut lines = lines.clone();
        lines.sort_by(|a, b| b.machines.total_cmp(&a.machines));
        for l in lines {
            table_rows.push(Row {
                name: RichText::new(format!("    {}", recipe_name(data, &l.recipe))),
                machines: l.machines,
                whole: whole_machines(l.machines),
                clock: Some(fmt::pct(l.settings.clock)),
                sloops: l.settings.sloops.map(|s| s.to_string()),
                power: l.power_mw,
                floor: l.footprint,
            });
        }
    }
    table_rows.push(Row {
        name: RichText::new("Total").strong(),
        machines: plan.machines,
        whole: plan.lines.iter().map(|l| whole_machines(l.machines)).sum(),
        clock: None,
        sloops: None,
        power: plan.power_mw,
        floor: plan.footprint,
    });

    let mut table = TableBuilder::new(ui)
        .id_salt("buildings-table")
        .striped(true)
        .resizable(true)
        .vscroll(false)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::initial(300.0).at_least(120.0).clip(true));
    for _ in 2..headers.len() {
        table = table.column(Column::initial(90.0).at_least(50.0).clip(true));
    }
    table
        .column(Column::remainder().at_least(60.0))
        .header(22.0, |mut header| {
            for h in &headers {
                header.col(|ui| {
                    ui.label(RichText::new(h).small().strong().weak());
                });
            }
        })
        .body(|body| {
            body.rows(22.0, table_rows.len(), |mut row| {
                let r = &table_rows[row.index()];
                row.col(|ui| {
                    ui.add(egui::Label::new(r.name.clone()).truncate());
                });
                let mut number = |text: String| {
                    row.col(|ui| {
                        ui.label(RichText::new(text).monospace());
                    });
                };
                number(fmt::num(r.machines));
                number(fmt::num(r.whole));
                if features.clock {
                    number(r.clock.clone().unwrap_or_default());
                    number(r.sloops.clone().unwrap_or_default());
                }
                if features.power {
                    number(fmt::num(r.power));
                }
                if features.footprint {
                    number(fmt::num(r.floor));
                }
                if foundations {
                    number(fmt::num(r.floor * spacing / 64.0));
                }
            });
        });
}
