//! Number formatting: more decimals for small numbers, separators for big ones.

pub fn num(n: f64) -> String {
    num_with(n, 2)
}

pub fn num_with(n: f64, small: usize) -> String {
    if !n.is_finite() {
        return "—".into();
    }
    let a = n.abs();
    if a < 0.0005 {
        return "0".into();
    }
    if a < 10.0 {
        let digits = small + usize::from(a < 1.0);
        return trim(format!("{n:.digits$}"));
    }
    if a < 1000.0 {
        return trim(format!("{n:.1}"));
    }
    group(n.round() as i64)
}

fn trim(s: String) -> String {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

fn group(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

pub fn rate(n: f64) -> String {
    format!("{}/min", num(n))
}

pub fn mw(n: f64) -> String {
    format!("{} MW", num(n))
}

pub fn signed(n: f64) -> String {
    let s = num(n);
    if n > 0.0005 { format!("+{s}") } else { s }
}

pub fn pct(fraction: Option<f64>) -> String {
    format!(
        "{}%",
        trim(format!("{:.2}", fraction.unwrap_or(1.0) * 100.0))
    )
}

/// "2026-10-19T21:30:00.000Z" -> "2026-10-19 21:30".
pub fn when(iso: &str) -> String {
    if iso.len() >= 16 {
        format!("{} {}", &iso[..10], &iso[11..16])
    } else {
        iso.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(num(0.0), "0");
        assert_eq!(num(0.3333333), "0.333");
        assert_eq!(num(3.3333333), "3.33");
        assert_eq!(num(12.26), "12.3");
        assert_eq!(num(7440.0), "7,440");
        assert_eq!(num(-21360.0), "-21,360");
        assert_eq!(num(1_234_567.0), "1,234,567");
        assert_eq!(signed(5.0), "+5");
        assert_eq!(pct(Some(2.5)), "250%");
        assert_eq!(pct(None), "100%");
    }
}
