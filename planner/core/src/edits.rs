//! Pure edits on a factory document. Each returns a new Factory; the app keeps
//! the old one for undo.

use crate::factory::new_id;
use crate::model::*;
use crate::solve::{auto_line_id, can_produce, edge_key, is_default_import, is_default_kind};
use std::collections::{BTreeSet, VecDeque};

/// Is `item` imported where `line_id` consumes it?
pub fn occurrence_imported(data: &GameData, f: &Factory, item: &str, line_id: &str) -> bool {
    item_imported(data, f, item) || f.imports.edges.contains(&edge_key(item, line_id))
}

/// Is `item` imported everywhere it's consumed?
pub fn item_imported(data: &GameData, f: &Factory, item: &str) -> bool {
    f.imports.items.iter().any(|i| i == item) || is_default_import(data, f, item)
}

/// Why an import toggle on `item` must stay on, if it must: turning it off
/// would ask the factory to make something nothing can make from scratch.
pub fn import_locked_reason(data: &GameData, f: &Factory, item: &str) -> Option<String> {
    if item_imported(data, f, item) && is_default_kind(data, item) && !can_produce(data, item) {
        Some(format!(
            "Nothing can make {} from scratch, so it has to be imported.",
            crate::solve::item_name(data, item)
        ))
    } else {
        None
    }
}

fn remove(list: &mut Vec<String>, value: &str) {
    list.retain(|v| v != value);
}

fn add(list: &mut Vec<String>, value: &str) {
    if !list.iter().any(|v| v == value) {
        list.push(value.to_string());
    }
}

fn set_item_import(data: &GameData, g: &mut Factory, item: &str, on: bool) {
    let prefix = format!("{item}>");
    g.imports.edges.retain(|e| !e.starts_with(&prefix));
    if is_default_kind(data, item) {
        if on {
            remove(&mut g.imports.exclude, item);
        } else {
            add(&mut g.imports.exclude, item);
        }
        remove(&mut g.imports.items, item);
    } else if on {
        add(&mut g.imports.items, item);
    } else {
        remove(&mut g.imports.items, item);
    }
}

/// Click: toggle this one occurrence. Shift-click (`all`): set every
/// occurrence of the item to the clicked occurrence's new value. Raw items and
/// power toggle as a whole, and never off when nothing could replace them.
pub fn toggle_import(
    data: &GameData,
    f: &Factory,
    plan: &Plan,
    item: &str,
    line_id: &str,
    all: bool,
) -> Factory {
    if import_locked_reason(data, f, item).is_some() {
        return f.clone();
    }
    let mut g = f.clone();
    let next = !occurrence_imported(data, f, item, line_id);
    if all || is_default_kind(data, item) {
        set_item_import(data, &mut g, item, next);
    } else if g.imports.items.iter().any(|i| i == item) {
        // Imported everywhere -> produce here only: keep the others imported per edge.
        remove(&mut g.imports.items, item);
        for l in &plan.lines {
            if l.id != line_id && l.rate(item) < 0.0 {
                add(&mut g.imports.edges, &edge_key(item, &l.id));
            }
        }
    } else {
        let key = edge_key(item, line_id);
        if next {
            add(&mut g.imports.edges, &key);
        } else {
            remove(&mut g.imports.edges, &key);
        }
    }
    g
}

pub fn toggle_item_import(data: &GameData, f: &Factory, item: &str) -> Factory {
    if import_locked_reason(data, f, item).is_some() {
        return f.clone();
    }
    let mut g = f.clone();
    set_item_import(data, &mut g, item, !item_imported(data, f, item));
    g
}

pub fn set_recipe_choice(f: &Factory, item: &str, recipe: Option<&str>) -> Factory {
    let mut g = f.clone();
    match recipe {
        Some(r) => g.recipe_choice.insert(item.to_string(), r.to_string()),
        None => g.recipe_choice.remove(item),
    };
    g
}

fn is_auto(line_id: &str) -> bool {
    line_id.starts_with("r:")
}

fn rename_edges(g: &mut Factory, from: &str, to: &str) {
    let suffix = format!(">{from}");
    for e in &mut g.imports.edges {
        if let Some(head) = e.strip_suffix(&suffix) {
            *e = format!("{head}>{to}");
        }
    }
}

/// Change clock, sloops, building or bonuses on a line, auto or explicit.
pub fn set_line_settings(
    f: &Factory,
    line_id: &str,
    recipe: &str,
    change: impl FnOnce(&mut LineSettings),
) -> Factory {
    let mut g = f.clone();
    if is_auto(line_id) {
        let mut s = g.line_settings.remove(recipe).unwrap_or_default();
        change(&mut s);
        if !s.is_empty() {
            g.line_settings.insert(recipe.to_string(), s);
        }
    } else if let Some(line) = g.lines.iter_mut().find(|l| l.id == line_id) {
        change(&mut line.settings);
    }
    g
}

pub fn add_line(f: &Factory, recipe: &str, machines: Option<f64>) -> Factory {
    let mut g = f.clone();
    g.lines.push(Line {
        id: new_id(),
        recipe: recipe.to_string(),
        machines,
        ..Default::default()
    });
    g
}

pub fn update_line(f: &Factory, line_id: &str, change: impl FnOnce(&mut Line)) -> Factory {
    let mut g = f.clone();
    if let Some(line) = g.lines.iter_mut().find(|l| l.id == line_id) {
        change(line);
    }
    g
}

pub fn remove_line(f: &Factory, line_id: &str) -> Factory {
    let mut g = f.clone();
    g.lines.retain(|l| l.id != line_id);
    let suffix = format!(">{line_id}");
    g.imports.edges.retain(|e| !e.ends_with(&suffix));
    g
}

pub fn move_line(f: &Factory, line_id: &str, delta: isize) -> Factory {
    let Some(i) = f.lines.iter().position(|l| l.id == line_id) else {
        return f.clone();
    };
    let j = i as isize + delta;
    if j < 0 || j as usize >= f.lines.len() {
        return f.clone();
    }
    let mut g = f.clone();
    g.lines.swap(i, j as usize);
    g
}

fn round6(n: f64) -> f64 {
    (n * 1e6).round() / 1e6
}

/// Target mode: pin an auto line at its current machine count.
pub fn lock_line(f: &Factory, plan: &Plan, line_id: &str) -> Factory {
    let Some(pl) = plan.line(line_id).filter(|_| is_auto(line_id)) else {
        return f.clone();
    };
    let mut g = f.clone();
    let id = new_id();
    let settings = g.line_settings.get(&pl.recipe).cloned().unwrap_or_default();
    g.lines.push(Line {
        id: id.clone(),
        recipe: pl.recipe.clone(),
        settings,
        machines: Some(round6(pl.machines)),
        note: None,
    });
    rename_edges(&mut g, line_id, &id);
    g
}

/// Let the solver size a locked line again.
pub fn unlock_line(f: &Factory, line_id: &str) -> Factory {
    let mut g = f.clone();
    let Some(pos) = g.lines.iter().position(|l| l.id == line_id) else {
        return g;
    };
    if g.mode == Mode::Manual {
        g.lines[pos].machines = None;
        return g;
    }
    let line = g.lines.remove(pos);
    if !line.settings.is_empty() {
        g.line_settings
            .insert(line.recipe.clone(), line.settings.clone());
    }
    rename_edges(&mut g, line_id, &auto_line_id(&line.recipe));
    g
}

/// Switch modes. Going to manual freezes the current plan into locked lines.
pub fn set_mode(f: &Factory, plan: &Plan, mode: Mode) -> Factory {
    if f.mode == mode {
        return f.clone();
    }
    let mut g = f.clone();
    g.mode = mode;
    if mode == Mode::Manual {
        let mut lines = vec![];
        for pl in &plan.lines {
            if let Some(existing) = f.lines.iter().find(|l| l.id == pl.id) {
                lines.push(existing.clone());
                continue;
            }
            let id = new_id();
            let settings = f.line_settings.get(&pl.recipe).cloned().unwrap_or_default();
            lines.push(Line {
                id: id.clone(),
                recipe: pl.recipe.clone(),
                settings,
                machines: Some(round6(pl.machines)),
                note: None,
            });
            rename_edges(&mut g, &pl.id, &id);
        }
        g.lines = lines;
    }
    g
}

/// After a "can't balance": put every item that depends on a short item back
/// on its standard recipe, so the plan is re-filled from defaults. Walks from
/// each short item to the lines that consume it and on to what they make.
/// Returns an identical factory when there is nothing to reset.
pub fn regenerate(f: &Factory, plan: &Plan) -> Factory {
    let short: Vec<String> = plan
        .items
        .values()
        .filter(|p| p.unbalanced > 1e-6)
        .map(|p| p.item.clone())
        .collect();
    let mut seen: BTreeSet<String> = short.iter().cloned().collect();
    let mut queue: VecDeque<String> = short.into();
    while let Some(item) = queue.pop_front() {
        for l in plan.lines.iter().filter(|l| l.rate(&item) < 0.0) {
            for (out, &r) in &l.rates {
                if r > 0.0 && seen.insert(out.clone()) {
                    queue.push_back(out.clone());
                }
            }
        }
    }
    let mut g = f.clone();
    g.recipe_choice.retain(|item, _| !seen.contains(item));
    g
}

/// Start the plan over: standard recipes, default imports, no per-line
/// settings or locked lines. Targets, name, notes, mode, objective and spacing
/// stay. Undo brings everything back.
pub fn reset_plan(f: &Factory) -> Factory {
    let mut g = f.clone();
    g.recipe_choice.clear();
    g.line_settings.clear();
    g.optimize = false;
    g.imports = FactoryImports::default();
    if g.mode == Mode::Target {
        g.lines.clear();
    }
    g
}
