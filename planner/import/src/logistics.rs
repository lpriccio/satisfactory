//! Imports the T9 workbook's trains, drones and trucks sheets and its
//! `planner` to-do sheet into factories/satisfactory/logistics.json.
//!
//!   planner-import logistics [workbook.ods|xlsx] [--force]
//!
//! The file is yours to edit in the app afterwards, so an existing one is
//! only replaced with --force.
//!
//! Sheet layouts (row 1 = A1):
//!   trains  header row 1: A Station, B Slot, C I/O, D Item, E Amt,
//!           F Partner(s), G Train Count
//!   drones  header row 2: A Installation, B ID, C I/O, D Item, E Qty,
//!           F Drain, G Net (both computed), H/I notes, M port name
//!           ("O Item Installation Qty ID"), N destination (an input's
//!           source port name, or words)
//!   trucks  header row 1: A Station, B I/O, C Item, D Amt, E Partner(s)
//!   planner A task, B "x" when done
//! Drain and net are recomputed from the links, not read.

use crate::t9::{Grid, Resolver, col, default_workbook};
use anyhow::{Context, Result};
use calamine::{Reader, open_workbook_auto};
use planner_core::logistics::*;
use planner_core::model::{GameData, GameId};
use planner_core::store::Store;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Shorthand the transport sheets use beyond the build tabs' aliases.
const ALIASES: &[(&str, &str)] = &[
    ("Crystal Oscillators", "CrystalOscillator"),
    ("ECR", "ElectromagneticControlRod"),
    ("FMF", "ModularFrameFused"),
    ("Fiscite Trigons", "FicsiteMesh"),
    ("HMF", "ModularFrameHeavy"),
    ("RCU", "ModularFrameLightweight"),
    // Truck sheet: straight off a quartz node.
    ("Quartz", "RawQuartz"),
];

struct Items<'a> {
    resolver: Resolver,
    data: &'a GameData,
}

impl Items<'_> {
    /// An item id when the game knows the name; otherwise the words as written.
    fn id(&self, name: &str) -> String {
        if name.eq_ignore_ascii_case("open") {
            return String::new();
        }
        ALIASES
            .iter()
            .find(|(a, _)| *a == name)
            .map(|(_, id)| id.to_string())
            .or_else(|| self.resolver.resolve(name))
            .filter(|id| self.data.items.contains_key(id))
            .unwrap_or_else(|| name.to_string())
    }
}

fn text(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_string())
}

fn dir(s: &str) -> Dir {
    if s.eq_ignore_ascii_case("I") {
        Dir::In
    } else {
        Dir::Out
    }
}

/// Stations in first-seen order, merged by name.
#[derive(Default)]
struct Stations(Vec<Station>);

impl Stations {
    fn get(&mut self, kind: Transport, name: &str) -> &mut Station {
        let i = match self.0.iter().position(|s| s.name == name) {
            Some(i) => i,
            None => {
                let mut s = new_station(kind, name);
                s.ports.clear();
                self.0.push(s);
                self.0.len() - 1
            }
        };
        &mut self.0[i]
    }
}

fn port(dir: Dir, slot: &str, item: String, rate: Option<f64>) -> Port {
    Port {
        slot: slot.to_string(),
        item,
        rate: rate.unwrap_or(0.0),
        ..new_port(dir)
    }
}

fn trains(g: &Grid, items: &Items) -> Vec<Station> {
    let mut st = Stations::default();
    for r in 1..g.rows() {
        let name = g.str(r, col(b'A'));
        if name.is_empty() {
            continue;
        }
        let s = st.get(Transport::Train, name);
        if let Some(n) = g.num(r, col(b'G')) {
            s.trains = Some(s.trains.unwrap_or(0).max(n as u32));
        }
        s.ports.push(Port {
            partner: text(g.str(r, col(b'F'))),
            ..port(
                dir(g.str(r, col(b'C'))),
                g.str(r, col(b'B')),
                items.id(g.str(r, col(b'D'))),
                g.num(r, col(b'E')),
            )
        });
    }
    st.0
}

fn trucks(g: &Grid, items: &Items) -> Vec<Station> {
    let mut st = Stations::default();
    for r in 1..g.rows() {
        let name = g.str(r, col(b'A'));
        if name.is_empty() {
            continue;
        }
        st.get(Transport::Truck, name).ports.push(Port {
            partner: text(g.str(r, col(b'E'))),
            ..port(
                dir(g.str(r, col(b'B'))),
                "",
                items.id(g.str(r, col(b'C'))),
                g.num(r, col(b'D')),
            )
        });
    }
    let mut stations = st.0;
    link_partners(&mut stations);
    stations
}

/// An input whose partner names the one station offering its item becomes
/// a link to that station's output.
fn link_partners(stations: &mut [Station]) {
    let mut links = vec![];
    for (si, s) in stations.iter().enumerate() {
        for (pi, p) in s.ports.iter().enumerate() {
            let Some(partner) = &p.partner else { continue };
            if p.dir != Dir::In {
                continue;
            }
            let offers: Vec<&Port> = stations
                .iter()
                .filter(|o| &o.name == partner)
                .flat_map(|o| &o.ports)
                .filter(|o| o.dir == Dir::Out && o.item == p.item)
                .collect();
            if let [only] = offers[..] {
                links.push((si, pi, only.id.clone()));
            }
        }
    }
    for (si, pi, from) in links {
        let p = &mut stations[si].ports[pi];
        p.from = Some(from);
        p.partner = None;
    }
}

fn drones(g: &Grid, items: &Items) -> Vec<Station> {
    let mut st = Stations::default();
    // Port name (column M) -> port id, and each input's destination (N).
    let mut by_name: HashMap<String, String> = HashMap::new();
    let mut wants: Vec<(String, String)> = vec![];
    for r in 2..g.rows() {
        let name = g.str(r, col(b'A'));
        if name.is_empty() {
            continue;
        }
        let d = dir(g.str(r, col(b'C')));
        let notes: Vec<&str> = [g.str(r, col(b'H')), g.str(r, col(b'I'))]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect();
        let dest = g.str(r, col(b'N'));
        let mut p = Port {
            note: text(&notes.join("; ")),
            ..port(
                d,
                g.str(r, col(b'B')),
                items.id(g.str(r, col(b'D'))),
                g.num(r, col(b'E')),
            )
        };
        // "I …"/"O …" in N names a port; anything else is a partner in words.
        // An output's "I …" is the reverse of a link the input already holds.
        let names_port = dest.starts_with("I ") || dest.starts_with("O ");
        if !dest.is_empty() && !names_port {
            p.partner = Some(dest.to_string());
        }
        if d == Dir::In && names_port {
            wants.push((p.id.clone(), dest.to_string()));
        }
        let own = g.str(r, col(b'M'));
        if !own.is_empty() {
            by_name.insert(own.to_string(), p.id.clone());
        }
        st.get(Transport::Drone, name).ports.push(p);
    }
    let mut stations = st.0;
    for (id, dest) in wants {
        let found = by_name.get(&dest).cloned();
        for p in stations.iter_mut().flat_map(|s| &mut s.ports) {
            if p.id == id {
                match &found {
                    Some(from) => p.from = Some(from.clone()),
                    None => p.partner = Some(dest.clone()),
                }
            }
        }
    }
    stations
}

fn todos(g: &Grid) -> Vec<Todo> {
    (0..g.rows())
        .filter_map(|r| {
            let t = g.str(r, col(b'A'));
            (!t.is_empty()).then(|| Todo {
                done: g.str(r, col(b'B')).eq_ignore_ascii_case("x"),
                ..new_todo(t)
            })
        })
        .collect()
}

pub fn import(data: &GameData, workbook: &Path) -> Result<Logistics> {
    let mut wb =
        open_workbook_auto(workbook).with_context(|| format!("opening {}", workbook.display()))?;
    let mut sheet = |name: &str| -> Result<Grid> {
        Ok(Grid {
            range: wb
                .worksheet_range(name)
                .with_context(|| format!("the workbook has no `{name}` sheet"))?,
        })
    };
    let items = Items {
        resolver: Resolver::new(data),
        data,
    };
    let mut l = new_logistics(GameId::Satisfactory);
    l.stations.extend(trains(&sheet("trains")?, &items));
    l.stations.extend(drones(&sheet("drones")?, &items));
    l.stations.extend(trucks(&sheet("trucks")?, &items));
    l.todos = todos(&sheet("planner")?);
    Ok(l)
}

pub fn run(root: &Path, input: Option<PathBuf>, force: bool) -> Result<()> {
    let store = Store::new(root);
    let path = store.logistics_path(GameId::Satisfactory);
    anyhow::ensure!(
        force || !path.exists(),
        "{} exists (you may have edited it in the app); pass --force to replace it",
        path.display()
    );
    let data = store.load_game_data(GameId::Satisfactory)?;
    let workbook = input.unwrap_or_else(|| default_workbook(root));
    let l = import(&data, &workbook)?;
    store.save_logistics(&l)?;
    for kind in Transport::ALL {
        let st: Vec<&Station> = l.stations.iter().filter(|s| s.kind == kind).collect();
        let ports: usize = st.iter().map(|s| s.ports.len()).sum();
        println!("{}: {} stations, {ports} ports", kind.name(), st.len());
    }
    let free: Vec<&str> = l
        .ports()
        .filter(|(_, p)| !p.item.is_empty() && !data.items.contains_key(&p.item))
        .map(|(_, p)| p.item.as_str())
        .collect();
    if !free.is_empty() {
        println!("kept as text (not game items): {}", free.join(", "));
    }
    println!(
        "to-do: {} ({} done)",
        l.todos.len(),
        l.todos.iter().filter(|t| t.done).count()
    );
    println!("wrote {}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::t9::tests::planner_dir;

    fn imported() -> Logistics {
        let data: GameData =
            planner_core::store::read_json(&planner_dir().join("data/satisfactory.json")).unwrap();
        import(&data, &default_workbook(&planner_dir())).unwrap()
    }

    fn out_port<'a>(l: &'a Logistics, station: &str, item: &str, slot: &str) -> &'a Port {
        l.ports()
            .find(|(s, p)| {
                s.name == station && p.item == item && p.slot == slot && p.dir == Dir::Out
            })
            .map(|(_, p)| p)
            .unwrap_or_else(|| panic!("no {item} {slot} out at {station}"))
    }

    #[test]
    fn drone_links_reproduce_the_sheets_drain() {
        let l = imported();
        // Coal Hole Plastic A: 600 out, Kidney takes 273 (sheet F23).
        let p = out_port(&l, "Coal Hole", "Plastic", "A");
        assert_eq!(l.drain(&p.id), 273.0);
        // Rubber B is over-drawn: 653 by Rocky Desert (sheet G33 = −53).
        let p = out_port(&l, "Coal Hole", "Rubber", "B");
        assert_eq!(p.rate - l.drain(&p.id), -53.0);
        // Launch Sulfur D/E/F each feed one Big Unit port.
        let p = out_port(&l, "Launch", "Sulfur", "E");
        assert_eq!(l.drain(&p.id), 400.0);
        // Rocky Desert is listed in two blocks; they merge into one installation.
        let rd: Vec<&Station> = l
            .stations
            .iter()
            .filter(|s| s.name == "Rocky Desert" && s.kind == Transport::Drone)
            .collect();
        assert_eq!(rd.len(), 1);
    }

    #[test]
    fn trains_trucks_and_todos() {
        let l = imported();
        let purple = l.stations.iter().find(|s| s.name == "Purple Iron").unwrap();
        assert_eq!(purple.kind, Transport::Train);
        assert_eq!(purple.trains, Some(2));
        assert_eq!(purple.ports.len(), 4);
        assert_eq!(purple.ports[0].item, "OreIron");
        assert_eq!(purple.ports[0].partner.as_deref(), Some("Babylon 4"));
        // An open platform has no item.
        assert!(
            l.ports()
                .any(|(s, p)| s.name == "Blue Crater 2" && p.item.is_empty())
        );
        // Trucks: RCU <- Casing draws 128 from Babylon's 112.5 (sheet K30 = −15.5).
        let casing = out_port(&l, "Babylon Aluminum Casing", "AluminumCasing", "");
        assert_eq!(casing.rate - l.drain(&casing.id), -15.5);
        let totals = l.totals(Transport::Truck);
        assert_eq!(totals["AluminumCasing"], (112.5, 128.0));
        assert!(
            l.todos
                .iter()
                .any(|t| t.text == "Drain.  The.  Swamp." && !t.done)
        );
        assert!(l.todos.iter().any(|t| t.text == "big girl hsc" && t.done));
    }
}
