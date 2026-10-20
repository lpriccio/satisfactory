//! What one machine makes, eats and draws per minute.

use crate::model::{Building, GameData, GameId, LineSettings, Recipe, POWER};
use std::collections::BTreeMap;

pub const MIN_CLOCK: f64 = 0.01;
pub const MAX_CLOCK: f64 = 2.5;

#[derive(Debug, Clone)]
pub struct MachineRates {
    /// Per-minute net flow of one machine by item, including POWER.
    pub rates: BTreeMap<String, f64>,
    /// MW per machine; negative for generators.
    pub power_mw: f64,
    pub footprint: f64,
    pub building: String,
}

/// The building a line runs in: the line's choice, else the preferred one for
/// the recipe's category, else the recipe's default.
pub fn line_building<'a>(
    data: &'a GameData,
    recipe: &Recipe,
    settings: &LineSettings,
    preferred: &BTreeMap<String, String>,
) -> Option<&'a Building> {
    let allowed = |id: &String| recipe.buildings.contains(id);
    let id = settings
        .building
        .as_ref()
        .filter(|b| allowed(b))
        .or_else(|| recipe.category.as_ref().and_then(|c| preferred.get(c)).filter(|b| allowed(b)))
        .or(recipe.buildings.first())?;
    data.buildings.get(id)
}

pub fn clamp_clock(clock: Option<f64>) -> f64 {
    match clock {
        Some(c) if c.is_finite() => c.clamp(MIN_CLOCK, MAX_CLOCK),
        _ => 1.0,
    }
}

/// Somersloop amplification: 1 + filled / slots.
pub fn amplification(building: Option<&Building>, sloops: Option<u32>) -> f64 {
    let slots = building.and_then(|b| b.sloop_slots).unwrap_or(0);
    match (slots, sloops) {
        (0, _) | (_, None) | (_, Some(0)) => 1.0,
        (slots, Some(n)) => 1.0 + n.min(slots) as f64 / slots as f64,
    }
}

pub fn machine_rates(
    data: &GameData,
    recipe: &Recipe,
    settings: &LineSettings,
    preferred: &BTreeMap<String, String>,
) -> MachineRates {
    let building = line_building(data, recipe, settings, preferred);
    let mut rates = BTreeMap::new();
    let mut add = |item: &str, r: f64| *rates.entry(item.to_string()).or_insert(0.0) += r;
    let power_mw;

    if data.features.clock {
        // Satisfactory: output scales with clock and sloops; power with clock^exp × amp².
        let clock = clamp_clock(settings.clock);
        let amp = amplification(building, settings.sloops);
        let cycles = 60.0 / recipe.time * clock;
        for s in &recipe.inputs {
            add(&s.item, -s.amount * cycles);
        }
        for s in &recipe.outputs {
            add(&s.item, s.amount * cycles * amp);
        }
        power_mw = match recipe.generates_mw {
            Some(mw) => -mw * clock,
            None => {
                let base = match recipe.power {
                    Some(p) => (p.min + p.max) / 2.0,
                    None => building.map_or(0.0, |b| b.power_mw),
                };
                let exp = building.and_then(|b| b.power_exponent).unwrap_or(1.0);
                base * clock.powf(exp) * amp * amp
            }
        };
    } else {
        // Factorio / DSP: crafting speed and productivity bonuses.
        let speed = building.and_then(|b| b.speed).unwrap_or(1.0) * (1.0 + settings.speed_bonus.unwrap_or(0.0));
        let cycles = 60.0 / recipe.time * speed;
        let prod_allowed = data.game != GameId::Factorio || recipe.allow_productivity;
        // Built-in productivity (foundry, EM plant) also needs allow_productivity.
        let prod = if prod_allowed {
            1.0 + building.and_then(|b| b.base_productivity).unwrap_or(0.0) + settings.prod_bonus.unwrap_or(0.0)
        } else {
            1.0
        };
        for s in &recipe.inputs {
            add(&s.item, -s.amount * cycles);
        }
        for s in &recipe.outputs {
            let ignored = recipe
                .ignored_by_productivity
                .as_ref()
                .and_then(|m| m.get(&s.item))
                .copied()
                .unwrap_or(0.0)
                .min(s.amount);
            add(&s.item, ((s.amount - ignored) * prod + ignored) * cycles);
        }
        power_mw = match recipe.generates_mw {
            Some(mw) => -mw * speed,
            None => building.map_or(0.0, |b| b.power_mw),
        };
    }

    if data.features.power && power_mw != 0.0 {
        add(POWER, -power_mw);
    }
    MachineRates {
        rates,
        power_mw,
        footprint: building.and_then(|b| b.footprint).unwrap_or(0.0),
        building: building.map_or_else(|| "?".to_string(), |b| b.id.clone()),
    }
}

/// Whole machines needed to house a fractional count.
pub fn whole_machines(machines: f64) -> f64 {
    (machines - 1e-6).ceil().max(0.0)
}
