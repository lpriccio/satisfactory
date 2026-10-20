//! Adding factories up.

use crate::factory::{new_id, now_iso};
use crate::model::*;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub struct WorldTotals {
    pub items: BTreeMap<String, PlanItem>,
    pub power_mw: f64,
    pub consumed_mw: f64,
    pub generated_mw: f64,
    pub machines: f64,
    pub footprint: f64,
    pub foundations: Option<f64>,
}

pub struct Member<'a> {
    pub factory: &'a Factory,
    pub plan: &'a Plan,
    pub multiplier: f64,
}

/// Sum solved plans, each scaled by its multiplier. Plans are linear, so
/// scaling is exact.
pub fn combine_plans(members: &[Member]) -> WorldTotals {
    let mut t = WorldTotals::default();
    for m in members {
        let k = m.multiplier;
        for p in m.plan.items.values() {
            let acc = t.items.entry(p.item.clone()).or_insert_with(|| PlanItem {
                item: p.item.clone(),
                ..Default::default()
            });
            acc.produced += p.produced * k;
            acc.consumed += p.consumed * k;
            acc.imported += p.imported * k;
            acc.surplus += p.surplus * k;
            acc.target += p.target * k;
            acc.unbalanced += p.unbalanced * k;
        }
        t.power_mw += m.plan.power_mw * k;
        t.consumed_mw += m.plan.consumed_mw * k;
        t.generated_mw += m.plan.generated_mw * k;
        t.machines += m.plan.machines * k;
        t.footprint += m.plan.footprint * k;
        if m.plan.foundations.is_some() {
            *t.foundations.get_or_insert(0.0) += m.plan.footprint * k * m.factory.spacing / 64.0;
        }
    }
    t
}

/// Net position once factories feed each other: what one factory sends out
/// (target or surplus) offsets another's import. Positive = left over.
pub fn net_position(items: &BTreeMap<String, PlanItem>) -> Vec<(String, f64)> {
    items
        .values()
        .map(|p| (p.item.clone(), p.target + p.surplus - p.imported))
        .filter(|(_, n)| n.abs() > 1e-6)
        .collect()
}

/// Flatten a world into one manual factory that keeps every line, scaled and locked.
pub fn world_to_factory(world: &World, members: &[Member]) -> Factory {
    let mut lines = vec![];
    for m in members {
        for pl in &m.plan.lines {
            if pl.machines * m.multiplier < 1e-9 {
                continue;
            }
            let scale = if m.multiplier != 1.0 {
                format!(" ×{}", m.multiplier)
            } else {
                String::new()
            };
            lines.push(Line {
                id: new_id(),
                recipe: pl.recipe.clone(),
                settings: pl.settings.clone(),
                machines: Some(pl.machines * m.multiplier),
                note: Some(format!("from {}{scale}", m.factory.name)),
            });
        }
    }
    let now = now_iso();
    let names: Vec<String> = members
        .iter()
        .map(|m| format!("{} ×{}", m.factory.name, m.multiplier))
        .collect();
    Factory {
        schema: 1,
        id: new_id(),
        game: world.game,
        name: format!("{} (combined)", world.name),
        notes: Some(format!("Combined from: {}", names.join(", "))),
        mode: Mode::Manual,
        objective: Objective::MinRaw,
        targets: vec![],
        recipe_choice: BTreeMap::new(),
        optimize: false,
        line_settings: BTreeMap::new(),
        lines,
        imports: FactoryImports::default(),
        spacing: members.first().map_or(4.0, |m| m.factory.spacing),
        created_at: now.clone(),
        updated_at: now,
    }
}
