//! Transport links and the logistics file.
use planner_core::GameId;
use planner_core::logistics::*;
use planner_core::store::Store;

fn port(dir: Dir, item: &str, rate: f64) -> Port {
    Port {
        item: item.into(),
        rate,
        ..new_port(dir)
    }
}

/// A hub sending 600 plastic, two installations taking 273 and 400 of it.
fn sample() -> (Logistics, String) {
    let mut l = new_logistics(GameId::Satisfactory);
    let mut hub = new_station(Transport::Drone, "Coal Hole");
    hub.ports = vec![port(Dir::Out, "Plastic", 600.0)];
    let out = hub.ports[0].id.clone();
    let mut a = new_station(Transport::Drone, "Kidney");
    a.ports = vec![Port {
        from: Some(out.clone()),
        ..port(Dir::In, "Plastic", 273.0)
    }];
    let mut b = new_station(Transport::Drone, "Going Turbo");
    b.ports = vec![Port {
        from: Some(out.clone()),
        ..port(Dir::In, "Plastic", 400.0)
    }];
    let mut train = new_station(Transport::Train, "Purple Iron");
    train.ports = vec![port(Dir::Out, "Plastic", 480.0)];
    l.stations = vec![hub, a, b, train];
    (l, out)
}

#[test]
fn drain_and_totals_follow_the_links() {
    let (l, out) = sample();
    assert_eq!(l.drain(&out), 673.0);
    assert_eq!(l.takers(&out).len(), 2);
    assert_eq!(l.totals(Transport::Drone)["Plastic"], (600.0, 673.0));
    // Other transports keep their own books.
    assert_eq!(l.totals(Transport::Train)["Plastic"], (480.0, 0.0));
    assert_eq!(l.sources(Transport::Drone, "Plastic").len(), 1);
}

#[test]
fn removing_an_output_unhooks_its_takers() {
    let (mut l, _) = sample();
    let hub = l.stations[0].id.clone();
    l.remove_station(&hub);
    assert_eq!(l.stations.len(), 3);
    assert!(l.ports().all(|(_, p)| p.from.is_none()));
}

#[test]
fn saves_beside_factories_without_showing_up_as_one() {
    let root = std::env::temp_dir().join(format!("planner-logistics-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let store = Store::new(&root);
    // No file yet: an empty one.
    assert!(
        store
            .load_logistics(GameId::Satisfactory)
            .unwrap()
            .stations
            .is_empty()
    );
    let (l, _) = sample();
    store.save_logistics(&l).unwrap();
    assert_eq!(store.load_logistics(GameId::Satisfactory).unwrap(), l);
    assert!(store.list_factories(GameId::Satisfactory).is_empty());
    let _ = std::fs::remove_dir_all(&root);
}
