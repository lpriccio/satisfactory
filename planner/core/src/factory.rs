//! Creating documents.

use crate::model::{Factory, FactoryImports, GameId, GameSettings, Mode, Objective, World};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Short random-looking id, unique enough for files in one folder.
pub fn new_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    // SplitMix64 over time + counter + process id.
    let mut x = nanos ^ COUNTER.fetch_add(1, Ordering::Relaxed).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (std::process::id() as u64) << 32;
    let mut out = String::with_capacity(12);
    for _ in 0..12 {
        x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = x;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        out.push(char::from_digit((z % 36) as u32, 36).unwrap());
    }
    out
}

/// Current time as an ISO-8601 UTC string, e.g. 2026-10-19T21:30:00.000Z.
pub fn now_iso() -> String {
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    iso_from_unix(d.as_secs() as i64, d.subsec_millis())
}

pub fn iso_from_unix(secs: i64, millis: u32) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

pub fn new_factory(game: GameId, settings: &GameSettings, name: &str, mode: Mode) -> Factory {
    let now = now_iso();
    Factory {
        schema: 1,
        id: new_id(),
        game,
        name: name.to_string(),
        notes: None,
        mode,
        objective: Objective::MinRaw,
        targets: vec![],
        recipe_choice: BTreeMap::new(),
        optimize: false,
        line_settings: BTreeMap::new(),
        lines: vec![],
        imports: FactoryImports::default(),
        spacing: settings.spacing,
        created_at: now.clone(),
        updated_at: now,
    }
}

pub fn new_world(game: GameId, name: &str) -> World {
    let now = now_iso();
    World { schema: 1, id: new_id(), game, name: name.to_string(), entries: vec![], created_at: now.clone(), updated_at: now }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_dates() {
        assert_eq!(iso_from_unix(0, 0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso_from_unix(1_792_468_552, 40), "2026-10-20T03:55:52.040Z");
        assert_eq!(iso_from_unix(951_782_400, 0), "2000-02-29T00:00:00.000Z");
    }

    #[test]
    fn ids_are_distinct() {
        let a = new_id();
        let b = new_id();
        assert_eq!(a.len(), 12);
        assert_ne!(a, b);
    }
}
