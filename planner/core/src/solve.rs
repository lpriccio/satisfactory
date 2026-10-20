//! Turns a factory document into a solved plan.
//!
//! Every recipe line is a variable (machine count). Each item gets one balance
//! row: what lines make, minus what they eat, plus imports, minus surplus,
//! equals the target. Byproducts, loops and shared intermediates therefore net
//! out globally instead of being expanded branch by branch.
//!
//! Solved in two phases: first the smallest possible shortfall (items nothing
//! can supply), then the best plan that keeps that shortfall.

use crate::linemath::{line_building, machine_rates, whole_machines, MachineRates};
use crate::model::*;
use good_lp::{microlp, variable, Expression, ProblemVariables, Solution, SolverModel, Variable};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const EPS: f64 = 1e-6;
/// Phase-1 weights: a shortfall on a source item (raw, or nothing makes it) is
/// where it really is; one on a craftable item just passes the blame up.
const SHORT_SOURCE: f64 = 1.0;
const SHORT_CRAFTABLE: f64 = 1000.0;

// --- recipe lookups --------------------------------------------------------

/// item id -> ids of recipes that output it, first-output recipes first.
pub fn producers(data: &GameData) -> &BTreeMap<String, Vec<String>> {
    data.cache.producers.get_or_init(|| {
        let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for r in data.recipes.values() {
            for (i, s) in r.outputs.iter().enumerate() {
                let list = map.entry(s.item.clone()).or_default();
                if i == 0 {
                    list.insert(0, r.id.clone());
                } else {
                    list.push(r.id.clone());
                }
            }
        }
        if data.features.power {
            let gens: Vec<String> = data.recipes.values().filter(|r| r.generates_mw.is_some()).map(|r| r.id.clone()).collect();
            if !gens.is_empty() {
                map.insert(POWER.to_string(), gens);
            }
        }
        map
    })
}

fn has_producer(data: &GameData, item: &str) -> bool {
    producers(data).get(item).is_some_and(|v| !v.is_empty())
}

pub fn default_recipe(data: &GameData, item: &str) -> Option<String> {
    if let Some(r) = data.items.get(item).and_then(|i| i.recipe.as_ref()) {
        if data.recipes.contains_key(r) {
            return Some(r.clone());
        }
    }
    let list = producers(data).get(item)?;
    list.iter().find(|id| !data.recipes[*id].alternate).or(list.first()).cloned()
}

pub fn item_name(data: &GameData, item: &str) -> String {
    if item == POWER {
        return "Power (MW)".into();
    }
    data.items.get(item).map_or_else(|| item.to_string(), |i| i.name.clone())
}

pub fn recipe_name(data: &GameData, recipe: &str) -> String {
    data.recipes.get(recipe).map_or_else(|| recipe.to_string(), |r| r.name.clone())
}

pub fn auto_line_id(recipe: &str) -> String {
    format!("r:{recipe}")
}

pub fn edge_key(item: &str, line_id: &str) -> String {
    format!("{item}>{line_id}")
}

// --- import policy ---------------------------------------------------------

/// Raw items and power: imported unless the user excluded them.
pub fn is_default_kind(data: &GameData, item: &str) -> bool {
    item == POWER || data.default_imports.iter().any(|i| i == item)
}

pub fn is_default_import(data: &GameData, f: &Factory, item: &str) -> bool {
    is_default_kind(data, item) && !f.imports.exclude.iter().any(|i| i == item)
}

pub fn is_item_imported(data: &GameData, f: &Factory, item: &str) -> bool {
    f.imports.items.iter().any(|i| i == item) || is_default_import(data, f, item)
}

// --- line set ---------------------------------------------------------------

fn candidate_recipes(data: &GameData, f: &Factory, settings: &GameSettings, item: &str) -> Vec<String> {
    let Some(all) = producers(data).get(item).filter(|v| !v.is_empty()) else {
        return vec![];
    };
    if f.optimize {
        let pinned = f.recipe_choice.get(item);
        let open: Vec<String> = all
            .iter()
            .filter(|id| !data.recipes[*id].alternate || settings.unlocked.contains(id) || Some(*id) == pinned)
            .cloned()
            .collect();
        return if open.is_empty() { vec![all[0].clone()] } else { open };
    }
    if let Some(chosen) = f.recipe_choice.get(item).filter(|r| data.recipes.contains_key(*r)) {
        return vec![chosen.clone()];
    }
    default_recipe(data, item).into_iter().collect()
}

/// Lines the solver may run: explicit lines plus, in target mode, one auto
/// line per candidate recipe. Returns the lines and the ids of the auto ones.
pub fn build_lines(data: &GameData, f: &Factory, settings: &GameSettings) -> (Vec<Line>, BTreeSet<String>) {
    let mut lines: Vec<Line> = f.lines.iter().filter(|l| data.recipes.contains_key(&l.recipe)).cloned().collect();
    let mut auto = BTreeSet::new();
    if f.mode != Mode::Target {
        return (lines, auto);
    }
    // A locked line is capacity you already have; the solver may still add an
    // auto line of the same recipe for demand beyond it.
    let mut have: BTreeSet<String> = lines.iter().filter(|l| l.machines.is_none()).map(|l| l.recipe.clone()).collect();
    let mut seen = BTreeSet::new();
    let mut queue: VecDeque<String> = f.targets.iter().map(|t| t.item.clone()).collect();
    let local_power = data.features.power && !is_item_imported(data, f, POWER);
    for l in &lines {
        queue.extend(data.recipes[&l.recipe].inputs.iter().map(|s| s.item.clone()));
    }
    if local_power {
        queue.push_back(POWER.into());
    }
    while let Some(item) = queue.pop_front() {
        if !seen.insert(item.clone()) || is_item_imported(data, f, &item) {
            continue;
        }
        for rid in candidate_recipes(data, f, settings, &item) {
            if !have.insert(rid.clone()) {
                continue;
            }
            let id = auto_line_id(&rid);
            auto.insert(id.clone());
            let settings = f.line_settings.get(&rid).cloned().unwrap_or_default();
            queue.extend(data.recipes[&rid].inputs.iter().map(|s| s.item.clone()));
            lines.push(Line { id, recipe: rid, settings, machines: None, note: None });
        }
    }
    (lines, auto)
}

// --- LP ---------------------------------------------------------------------

struct Col {
    /// Phase-2 cost.
    cost: f64,
    /// Phase-1 cost (shortfall columns only).
    short: f64,
}

struct Lp {
    cols: Vec<Col>,
    /// Sparse rows: (coefficients, rhs), each an equality.
    rows: Vec<(Vec<(usize, f64)>, f64)>,
}

impl Lp {
    fn col(&mut self, cost: f64, short: f64) -> usize {
        self.cols.push(Col { cost, short });
        self.cols.len() - 1
    }

    fn solve_phase(&self, costs: impl Fn(&Col) -> f64, cap: Option<f64>) -> anyhow::Result<Vec<f64>> {
        let mut vars = ProblemVariables::new();
        let xs: Vec<Variable> = self.cols.iter().map(|_| vars.add(variable().min(0.0))).collect();
        let mut objective = Expression::from(0.0);
        for (c, x) in self.cols.iter().zip(&xs) {
            let w = costs(c);
            if w != 0.0 {
                objective += w * *x;
            }
        }
        let mut model = vars.minimise(objective).using(microlp);
        for (terms, rhs) in &self.rows {
            let mut e = Expression::from(0.0);
            for &(j, c) in terms {
                e += c * xs[j];
            }
            model = model.with(e.eq(*rhs));
        }
        if let Some(cap) = cap {
            let mut e = Expression::from(0.0);
            for (c, x) in self.cols.iter().zip(&xs) {
                if c.short != 0.0 {
                    e += c.short * *x;
                }
            }
            model = model.with(e.leq(cap));
        }
        let sol = model.solve().map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(xs.iter().map(|x| sol.value(*x)).collect())
    }
}

struct Weights {
    machine_base: f64,
    machine_per_mw: f64,
    power_import: f64,
    raw_scale: f64,
}

fn weights(objective: Objective) -> Weights {
    match objective {
        Objective::MinRaw => Weights { machine_base: 1e-3, machine_per_mw: 0.0, power_import: 1e-4, raw_scale: 1.0 },
        Objective::MinPower => Weights { machine_base: 1e-3, machine_per_mw: 1e-3, power_import: 1.0, raw_scale: 1e-3 },
        Objective::MinMachines => Weights { machine_base: 1.0, machine_per_mw: 0.0, power_import: 1e-4, raw_scale: 1e-3 },
    }
}
const SURPLUS_COST: f64 = 1e-5;
/// Manual mode may import anything, but sized lines should be used before
/// importing an item the factory doesn't normally import.
const FALLBACK_IMPORT_COST: f64 = 1e4;

struct Work {
    line: Line,
    per: MachineRates,
    auto: bool,
    col: Option<usize>,
    locked: Option<f64>,
}

pub fn solve_factory(data: &GameData, f: &Factory, settings: &GameSettings) -> Plan {
    let mut warnings = vec![];
    for l in &f.lines {
        if !data.recipes.contains_key(&l.recipe) {
            warnings.push(format!("Unknown recipe \"{}\" skipped.", l.recipe));
        }
    }
    let (lines, auto) = build_lines(data, f, settings);
    let w = weights(f.objective);
    let mut lp = Lp { cols: vec![], rows: vec![] };

    let mut work: Vec<Work> = lines
        .into_iter()
        .map(|line| {
            let recipe = &data.recipes[&line.recipe];
            let per = machine_rates(data, recipe, &line.settings, &settings.default_building);
            let locked = line.machines.filter(|m| m.is_finite()).map(|m| m.max(0.0));
            Work { auto: auto.contains(&line.id), line, per, col: None, locked }
        })
        .collect();
    for wk in &mut work {
        if wk.locked.is_none() {
            wk.col = Some(lp.col(w.machine_base + wk.per.power_mw.max(0.0) * w.machine_per_mw, 0.0));
        }
    }

    // Items touched by the plan.
    let mut items: BTreeSet<String> = f.targets.iter().map(|t| t.item.clone()).collect();
    for wk in &work {
        items.extend(wk.per.rates.keys().cloned());
    }
    if items.is_empty() {
        let mut p = Plan::empty(PlanStatus::Ok, None, warnings);
        p.foundations = data.features.clock.then_some(0.0);
        return p;
    }

    let edge_imports: BTreeSet<&str> = f.imports.edges.iter().map(String::as_str).collect();
    let mut target_of: BTreeMap<&str, f64> = BTreeMap::new();
    for t in &f.targets {
        *target_of.entry(t.item.as_str()).or_default() += t.rate;
    }
    let importable = |item: &str| {
        f.mode == Mode::Manual
            || is_item_imported(data, f, item)
            // Items nothing makes are imported unless the user excluded them;
            // then the plan reports them short.
            || (!has_producer(data, item) && !f.imports.exclude.iter().any(|i| i == item))
    };
    let raw_weight = |item: &str| data.resource_weights.as_ref().and_then(|m| m.get(item)).copied().unwrap_or(1.0);

    struct ItemCols {
        import: Option<usize>,
        short: Option<usize>,
        surplus: usize,
    }
    let mut cols: BTreeMap<String, ItemCols> = BTreeMap::new();
    for item in &items {
        let mut terms = vec![];
        let mut rhs = target_of.get(item.as_str()).copied().unwrap_or(0.0);
        for wk in &work {
            let Some(&rate) = wk.per.rates.get(item) else { continue };
            // An edge import cancels this line's own consumption of the item.
            let net = if rate < 0.0 && edge_imports.contains(edge_key(item, &wk.line.id).as_str()) { 0.0 } else { rate };
            match wk.col {
                Some(j) => terms.push((j, net)),
                None => rhs -= net * wk.locked.unwrap_or(0.0),
            }
        }
        let (import, short) = if importable(item) {
            let cost = if f.mode == Mode::Manual && !is_item_imported(data, f, item) {
                FALLBACK_IMPORT_COST
            } else if item == POWER {
                w.power_import
            } else {
                w.raw_scale * raw_weight(item)
            };
            (Some(lp.col(cost, 0.0)), None)
        } else {
            // Raw items count as sources even when a recipe "makes" them:
            // crude oil's only recipe is unpackaging, which can't create oil.
            let source = is_default_kind(data, item) || !has_producer(data, item);
            (None, Some(lp.col(0.0, if source { SHORT_SOURCE } else { SHORT_CRAFTABLE })))
        };
        let surplus = lp.col(SURPLUS_COST, 0.0);
        terms.extend(import.map(|j| (j, 1.0)));
        terms.extend(short.map(|j| (j, 1.0)));
        terms.push((surplus, -1.0));
        lp.rows.push((terms, rhs));
        cols.insert(item.clone(), ItemCols { import, short, surplus });
    }

    let any_short = cols.values().any(|c| c.short.is_some());
    let solved = (|| -> anyhow::Result<Vec<f64>> {
        if !any_short {
            return lp.solve_phase(|c| c.cost, None);
        }
        let first = lp.solve_phase(|c| c.short, None)?;
        let worst: f64 = lp.cols.iter().zip(&first).map(|(c, x)| c.short * x).sum();
        lp.solve_phase(|c| c.cost, Some(worst * (1.0 + 1e-7) + 1e-7))
    })();
    let x = match solved {
        Ok(x) => x,
        Err(e) => return Plan::empty(PlanStatus::Error, Some(format!("Solver error: {e}")), warnings),
    };
    let value = |j: usize| {
        let v = x[j];
        if v.abs() < 1e-9 { 0.0 } else { v }
    };

    // --- assemble plan ---
    let mut plan_lines = vec![];
    for wk in &work {
        let machines = match wk.col {
            Some(j) => value(j),
            None => wk.locked.unwrap_or(0.0),
        };
        // Auto lines the solver left idle are noise; explicit lines always show.
        if wk.auto && machines < EPS {
            continue;
        }
        let recipe = &data.recipes[&wk.line.recipe];
        plan_lines.push(PlanLine {
            id: wk.line.id.clone(),
            recipe: recipe.id.clone(),
            building: line_building(data, recipe, &wk.line.settings, &settings.default_building)
                .map_or_else(|| "?".into(), |b| b.id.clone()),
            machines,
            locked: wk.locked.is_some(),
            auto: wk.auto,
            settings: wk.line.settings.clone(),
            rates: wk.per.rates.iter().map(|(k, r)| (k.clone(), r * machines)).collect(),
            power_mw: wk.per.power_mw * machines,
            footprint: wk.per.footprint * whole_machines(machines),
        });
    }

    let mut plan_items = BTreeMap::new();
    for item in &items {
        let c = &cols[item];
        let (mut produced, mut consumed, mut edge_imported) = (0.0, 0.0, 0.0);
        for pl in &plan_lines {
            let r = pl.rate(item);
            if r > 0.0 {
                produced += r;
            } else if r < 0.0 {
                consumed -= r;
                if edge_imports.contains(edge_key(item, &pl.id).as_str()) {
                    edge_imported -= r;
                }
            }
        }
        plan_items.insert(
            item.clone(),
            PlanItem {
                item: item.clone(),
                produced,
                consumed,
                imported: c.import.map_or(0.0, value) + edge_imported,
                surplus: value(c.surplus),
                target: target_of.get(item.as_str()).copied().unwrap_or(0.0),
                unbalanced: c.short.map_or(0.0, value),
            },
        );
    }

    let consumed_mw: f64 = plan_lines.iter().map(|l| l.power_mw.max(0.0)).sum();
    let generated_mw: f64 = plan_lines.iter().map(|l| (-l.power_mw).max(0.0)).sum();
    let footprint: f64 = plan_lines.iter().map(|l| l.footprint).sum();
    let missing: Vec<&PlanItem> = plan_items.values().filter(|p| p.unbalanced > EPS).collect();
    let message = (!missing.is_empty()).then(|| {
        let names: Vec<String> =
            missing.iter().map(|p| format!("{} ({}/min short)", item_name(data, &p.item), round2(p.unbalanced))).collect();
        let (them, are) = if missing.len() > 1 { ("them", "they aren't") } else { ("it", "it isn't") };
        format!("Can't balance {}: nothing in this factory can supply {them}, and {are} imported.", names.join(", "))
    });

    Plan {
        status: if missing.is_empty() { PlanStatus::Ok } else { PlanStatus::Unbalanced },
        message,
        edges: allocate_edges(&plan_lines, &plan_items, &edge_imports),
        machines: plan_lines.iter().map(|l| l.machines).sum(),
        lines: plan_lines,
        items: plan_items,
        power_mw: consumed_mw - generated_mw,
        consumed_mw,
        generated_mw,
        footprint,
        foundations: data.features.clock.then(|| footprint * f.spacing / 64.0),
        warnings,
    }
}

fn round2(n: f64) -> String {
    let s = format!("{n:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Split each item's supply across its consumers in proportion: every consumer
/// gets the same mix of producers and imports. Edge imports go straight from
/// "import" to their consumer. Targets and surplus appear as consumers.
pub fn allocate_edges(lines: &[PlanLine], items: &BTreeMap<String, PlanItem>, edge_imports: &BTreeSet<&str>) -> Vec<PlanEdge> {
    let mut edges = vec![];
    for pi in items.values() {
        let item = &pi.item;
        let mut sources: Vec<(String, f64)> = vec![];
        let mut sinks: Vec<(String, f64)> = vec![];
        let mut edge_imported = 0.0;
        for l in lines {
            let r = l.rate(item);
            if r > 0.0 {
                sources.push((l.id.clone(), r));
            } else if r < 0.0 {
                if edge_imports.contains(edge_key(item, &l.id).as_str()) {
                    edges.push(PlanEdge { item: item.clone(), from: "import".into(), to: l.id.clone(), rate: -r });
                    edge_imported -= r;
                } else {
                    sinks.push((l.id.clone(), -r));
                }
            }
        }
        let pooled = pi.imported - edge_imported;
        if pooled > EPS {
            sources.push(("import".into(), pooled));
        }
        if pi.target > EPS {
            sinks.push(("target".into(), pi.target));
        }
        if pi.surplus > EPS {
            sinks.push(("surplus".into(), pi.surplus));
        }
        let supply: f64 = sources.iter().map(|(_, r)| r).sum();
        if supply < EPS {
            continue;
        }
        for (to, need) in &sinks {
            for (from, have) in &sources {
                let rate = need * have / supply;
                if rate > EPS {
                    edges.push(PlanEdge { item: item.clone(), from: from.clone(), to: to.clone(), rate });
                }
            }
        }
    }
    edges
}

/// Can anything actually make `item` from other sources? Crude oil "has" a
/// recipe (unpackaging) that needs crude oil, so the answer there is no.
/// Tested by asking for 1/min with its import off and every recipe open.
pub fn can_produce(data: &GameData, item: &str) -> bool {
    if let Some(&v) = data.cache.producible.lock().unwrap().get(item) {
        return v;
    }
    let ok = has_producer(data, item) && {
        let probe = Factory {
            schema: 1,
            id: "probe".into(),
            game: data.game,
            name: "probe".into(),
            notes: None,
            mode: Mode::Target,
            objective: Objective::MinRaw,
            targets: vec![Target { item: item.into(), rate: 1.0 }],
            recipe_choice: BTreeMap::new(),
            optimize: true,
            line_settings: BTreeMap::new(),
            lines: vec![],
            imports: FactoryImports { items: vec![], edges: vec![], exclude: vec![item.into()] },
            spacing: 4.0,
            created_at: String::new(),
            updated_at: String::new(),
        };
        let unlocked = data.recipes.values().filter(|r| r.alternate).map(|r| r.id.clone()).collect();
        let settings = GameSettings { unlocked, ..Default::default() };
        solve_factory(data, &probe, &settings).status == PlanStatus::Ok
    };
    data.cache.producible.lock().unwrap().insert(item.to_string(), ok);
    ok
}
