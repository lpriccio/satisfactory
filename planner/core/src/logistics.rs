//! Trains, drones and trucks, and the to-do list: one document per game,
//! factories/<game>/logistics.json. Only Satisfactory uses it.
//!
//! A station has ports. An output port offers an item at a rate; an input
//! port takes some, either from a named output port (`from`) or from a
//! partner described in words. An output's drain is the sum of the inputs
//! that name it, so its net is what's still free.

use crate::factory::{new_id, now_iso};
use crate::model::GameId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    Train,
    Drone,
    Truck,
}

impl Transport {
    pub const ALL: [Transport; 3] = [Transport::Train, Transport::Drone, Transport::Truck];

    pub fn name(self) -> &'static str {
        match self {
            Transport::Train => "Trains",
            Transport::Drone => "Drones",
            Transport::Truck => "Trucks",
        }
    }

    /// What one station is called.
    pub fn station_word(self) -> &'static str {
        match self {
            Transport::Train => "Station",
            Transport::Drone => "Installation",
            Transport::Truck => "Station",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dir {
    In,
    Out,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Port {
    pub id: String,
    /// Platform or port letter ("A", "B", …); may be empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub slot: String,
    pub dir: Dir,
    /// An item id, or free text for anything the game data doesn't name.
    /// Empty means an open slot.
    #[serde(default)]
    pub item: String,
    /// Per minute.
    #[serde(default)]
    pub rate: f64,
    /// Inputs: the output port this one draws from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// Who's on the other end, in words ("Babylon 4", "Personal", "DEAD").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Station {
    pub id: String,
    pub kind: Transport,
    pub name: String,
    /// Trains serving the station.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trains: Option<u32>,
    #[serde(default)]
    pub ports: Vec<Port>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Todo {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Logistics {
    pub schema: u32,
    pub game: GameId,
    #[serde(default)]
    pub stations: Vec<Station>,
    #[serde(default)]
    pub todos: Vec<Todo>,
    pub updated_at: String,
}

pub fn new_logistics(game: GameId) -> Logistics {
    Logistics {
        schema: 1,
        game,
        stations: vec![],
        todos: vec![],
        updated_at: now_iso(),
    }
}

pub fn new_port(dir: Dir) -> Port {
    Port {
        id: new_id(),
        slot: String::new(),
        dir,
        item: String::new(),
        rate: 0.0,
        from: None,
        partner: None,
        note: None,
    }
}

pub fn new_station(kind: Transport, name: &str) -> Station {
    Station {
        id: new_id(),
        kind,
        name: name.into(),
        trains: None,
        ports: vec![new_port(Dir::Out)],
    }
}

pub fn new_todo(text: &str) -> Todo {
    Todo {
        id: new_id(),
        text: text.into(),
        done: false,
    }
}

impl Logistics {
    pub fn ports(&self) -> impl Iterator<Item = (&Station, &Port)> {
        self.stations
            .iter()
            .flat_map(|s| s.ports.iter().map(move |p| (s, p)))
    }

    pub fn port(&self, id: &str) -> Option<(&Station, &Port)> {
        self.ports().find(|(_, p)| p.id == id)
    }

    /// How much the inputs naming this output take from it.
    pub fn drain(&self, port: &str) -> f64 {
        self.ports()
            .filter(|(_, p)| p.dir == Dir::In && p.from.as_deref() == Some(port))
            .map(|(_, p)| p.rate)
            .sum()
    }

    /// The inputs drawing from this output.
    pub fn takers(&self, port: &str) -> Vec<(&Station, &Port)> {
        self.ports()
            .filter(|(_, p)| p.dir == Dir::In && p.from.as_deref() == Some(port))
            .collect()
    }

    /// Outputs of one transport that offer an item: what an input can draw from.
    pub fn sources(&self, kind: Transport, item: &str) -> Vec<(&Station, &Port)> {
        self.ports()
            .filter(|(s, p)| s.kind == kind && p.dir == Dir::Out && p.item == item)
            .collect()
    }

    /// Per item, for one transport: total out, total in.
    pub fn totals(&self, kind: Transport) -> BTreeMap<String, (f64, f64)> {
        let mut out: BTreeMap<String, (f64, f64)> = BTreeMap::new();
        for (s, p) in self.ports() {
            if s.kind != kind || p.item.is_empty() {
                continue;
            }
            let t = out.entry(p.item.clone()).or_default();
            match p.dir {
                Dir::Out => t.0 += p.rate,
                Dir::In => t.1 += p.rate,
            }
        }
        out
    }

    /// Remove a port, and unhook any inputs that drew from it.
    pub fn remove_port(&mut self, id: &str) {
        for s in &mut self.stations {
            s.ports.retain(|p| p.id != id);
            for p in &mut s.ports {
                if p.from.as_deref() == Some(id) {
                    p.from = None;
                }
            }
        }
    }

    pub fn remove_station(&mut self, id: &str) {
        let ports: Vec<String> = self
            .stations
            .iter()
            .filter(|s| s.id == id)
            .flat_map(|s| s.ports.iter().map(|p| p.id.clone()))
            .collect();
        for p in ports {
            self.remove_port(&p);
        }
        self.stations.retain(|s| s.id != id);
    }
}
