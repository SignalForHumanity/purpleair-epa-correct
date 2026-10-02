//! U.S. EPA PM2.5 Air Quality Index using the breakpoints in effect since
//! May 6, 2024 (40 CFR Part 58 Appendix G, as revised by the Feb 7, 2024
//! PM NAAQS rule), and the PM NowCast.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    Good,
    Moderate,
    UnhealthySensitive,
    Unhealthy,
    VeryUnhealthy,
    Hazardous,
}

impl Category {
    pub const ALL: [Category; 6] = [
        Category::Good,
        Category::Moderate,
        Category::UnhealthySensitive,
        Category::Unhealthy,
        Category::VeryUnhealthy,
        Category::Hazardous,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Category::Good => "Good",
            Category::Moderate => "Moderate",
            Category::UnhealthySensitive => "Unhealthy for Sensitive Groups",
            Category::Unhealthy => "Unhealthy",
            Category::VeryUnhealthy => "Very Unhealthy",
            Category::Hazardous => "Hazardous",
        }
    }
}

/// (C_lo, C_hi, I_lo, I_hi, category)
const BREAKPOINTS: [(f64, f64, f64, f64, Category); 6] = [
    (0.0, 9.0, 0.0, 50.0, Category::Good),
    (9.1, 35.4, 51.0, 100.0, Category::Moderate),
    (35.5, 55.4, 101.0, 150.0, Category::UnhealthySensitive),
    (55.5, 125.4, 151.0, 200.0, Category::Unhealthy),
    (125.5, 225.4, 201.0, 300.0, Category::VeryUnhealthy),
    (225.5, 325.4, 301.0, 500.0, Category::Hazardous),
];

/// Truncate a concentration to one decimal place, as the AQI method requires.
pub fn truncate1(c: f64) -> f64 {
    (c * 10.0 + 1e-9).floor() / 10.0
}

/// AQI for a PM2.5 concentration (µg/m³). Values above 325.4 are reported as
/// 500 (Hazardous, beyond the index). Returns `None` for negative or
/// non-finite input.
pub fn pm25_aqi(c: f64) -> Option<(u32, Category)> {
    if !c.is_finite() || c < 0.0 {
        return None;
    }
    let c = truncate1(c);
    for (clo, chi, ilo, ihi, cat) in BREAKPOINTS {
        if c <= chi + 1e-9 {
            let i = (ihi - ilo) / (chi - clo) * (c - clo) + ilo;
            return Some((i.round() as u32, cat));
        }
    }
    Some((500, Category::Hazardous))
}

/// PM NowCast. `recent[0]` is the most recent hour, up to 12 entries,
/// `None` for missing hours. Requires at least 2 of the 3 most recent hours.
pub fn nowcast(recent: &[Option<f64>]) -> Option<f64> {
    let recent = &recent[..recent.len().min(12)];
    let have_recent = recent.iter().take(3).filter(|v| v.is_some()).count();
    if have_recent < 2 {
        return None;
    }
    let vals: Vec<f64> = recent.iter().flatten().copied().collect();
    let max = vals.iter().cloned().fold(f64::MIN, f64::max);
    let min = vals.iter().cloned().fold(f64::MAX, f64::min);
    if max <= 0.0 {
        return Some(0.0);
    }
    let w = (min / max).max(0.5);
    let mut num = 0.0;
    let mut den = 0.0;
    for (i, v) in recent.iter().enumerate() {
        if let Some(c) = v {
            let f = w.powi(i as i32);
            num += f * c;
            den += f;
        }
    }
    Some(num / den)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breakpoint_edges() {
        assert_eq!(pm25_aqi(0.0), Some((0, Category::Good)));
        assert_eq!(pm25_aqi(9.0), Some((50, Category::Good)));
        assert_eq!(pm25_aqi(9.1), Some((51, Category::Moderate)));
        assert_eq!(pm25_aqi(35.4), Some((100, Category::Moderate)));
        assert_eq!(pm25_aqi(35.5), Some((101, Category::UnhealthySensitive)));
        assert_eq!(pm25_aqi(55.4), Some((150, Category::UnhealthySensitive)));
        assert_eq!(pm25_aqi(55.5), Some((151, Category::Unhealthy)));
        assert_eq!(pm25_aqi(125.4), Some((200, Category::Unhealthy)));
        assert_eq!(pm25_aqi(125.5), Some((201, Category::VeryUnhealthy)));
        assert_eq!(pm25_aqi(225.4), Some((300, Category::VeryUnhealthy)));
        assert_eq!(pm25_aqi(225.5), Some((301, Category::Hazardous)));
        assert_eq!(pm25_aqi(325.4), Some((500, Category::Hazardous)));
        assert_eq!(pm25_aqi(900.0), Some((500, Category::Hazardous)));
    }

    #[test]
    fn truncation_not_rounding() {
        // 9.09 truncates to 9.0 → still Good.
        assert_eq!(pm25_aqi(9.09), Some((50, Category::Good)));
        assert_eq!(pm25_aqi(35.49), Some((100, Category::Moderate)));
    }

    #[test]
    fn interpolates() {
        // 20.0 → (100-51)/(35.4-9.1)*(20-9.1)+51 = 71.3 → 71
        assert_eq!(pm25_aqi(20.0), Some((71, Category::Moderate)));
    }

    #[test]
    fn invalid_input() {
        assert_eq!(pm25_aqi(-1.0), None);
        assert_eq!(pm25_aqi(f64::NAN), None);
    }

    #[test]
    fn nowcast_steady_equals_value() {
        let v = vec![Some(12.0); 12];
        assert!((nowcast(&v).unwrap() - 12.0).abs() < 1e-9);
    }

    #[test]
    fn nowcast_needs_two_of_three_recent() {
        assert_eq!(nowcast(&[Some(5.0), None, None, Some(5.0)]), None);
        assert!(nowcast(&[Some(5.0), None, Some(5.0)]).is_some());
    }

    #[test]
    fn nowcast_weights_recent_hours() {
        // min/max = 0.1 → w = 0.5. Values: 40, 4 → (40 + 0.5*4)/(1.5) = 28
        let n = nowcast(&[Some(40.0), Some(4.0)]).unwrap();
        assert!((n - 28.0).abs() < 1e-9);
    }

    #[test]
    fn nowcast_epa_style_example() {
        // w = min/max = 0.8 when within range
        let v = [Some(10.0), Some(8.0)];
        let w: f64 = 0.8;
        let expected = (10.0 + w * 8.0) / (1.0 + w);
        assert!((nowcast(&v).unwrap() - expected).abs() < 1e-9);
    }

    #[test]
    fn nowcast_all_zero() {
        assert_eq!(nowcast(&[Some(0.0), Some(0.0)]), Some(0.0));
    }
}
