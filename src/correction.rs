//! EPA extended U.S. correction for PurpleAir PM2.5 (Barkjohn et al. 2022,
//! "Correction and Accuracy of PurpleAir PM2.5 Measurements for Extreme
//! Wildfire Smoke", Sensors 22(24):9669). This is the version applied on the
//! AirNow Fire and Smoke Map.
//!
//! `x` is the hourly average of the channel A and B `pm2.5_atm` values in
//! µg/m³ and `rh` is the sensor's own (uncorrected) relative humidity in
//! percent (0–100, not a fraction).

/// Apply the 5-piece correction. Negative results are floored at 0.
pub fn epa_extended(x: f64, rh: f64) -> f64 {
    let y = if x < 30.0 {
        0.524 * x - 0.0862 * rh + 5.75
    } else if x < 50.0 {
        let w = x / 20.0 - 3.0 / 2.0;
        (0.786 * w + 0.524 * (1.0 - w)) * x - 0.0862 * rh + 5.75
    } else if x < 210.0 {
        0.786 * x - 0.0862 * rh + 5.75
    } else if x < 260.0 {
        let w = x / 50.0 - 21.0 / 5.0;
        (0.69 * w + 0.786 * (1.0 - w)) * x - 0.0862 * rh * (1.0 - w)
            + 2.966 * w
            + 5.75 * (1.0 - w)
            + 8.84e-4 * x * x * w
    } else {
        2.966 + 0.69 * x + 8.84e-4 * x * x
    };
    y.max(0.0)
}

/// EPA hourly A/B agreement check: an hour is rejected when the channel
/// averages differ by more than 5 µg/m³ AND by more than 70 % relative
/// percent difference (|A−B| / mean(A,B)).
pub fn channels_agree(a: f64, b: f64) -> bool {
    let diff = (a - b).abs();
    if diff <= 5.0 {
        return true;
    }
    let mean = (a + b) / 2.0;
    if mean <= 0.0 {
        return false;
    }
    diff / mean <= 0.70
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn low_piece_matches_formula() {
        assert!(close(
            epa_extended(10.0, 50.0),
            0.524 * 10.0 - 0.0862 * 50.0 + 5.75
        ));
    }

    #[test]
    fn middle_piece_matches_formula() {
        assert!(close(epa_extended(100.0, 40.0), 78.6 - 3.448 + 5.75));
    }

    #[test]
    fn high_piece_matches_formula() {
        let x: f64 = 400.0;
        assert!(close(
            epa_extended(x, 20.0),
            2.966 + 0.69 * x + 8.84e-4 * x * x
        ));
    }

    #[test]
    fn high_piece_ignores_humidity() {
        assert!(close(epa_extended(300.0, 10.0), epa_extended(300.0, 90.0)));
    }

    #[test]
    fn blend_pieces_hit_their_endpoints() {
        // At x = 30 the 30–50 blend weight is 0 → same as the low piece.
        let rh = 35.0;
        assert!(close(
            epa_extended(30.0, rh),
            0.524 * 30.0 - 0.0862 * rh + 5.75
        ));
        // At x = 210 the 210–260 blend weight is 0 → same as the middle piece.
        assert!(close(
            epa_extended(210.0, rh),
            0.786 * 210.0 - 0.0862 * rh + 5.75
        ));
    }

    #[test]
    fn continuous_at_every_boundary() {
        for rh in [10.0, 50.0, 90.0] {
            for b in [30.0, 50.0, 210.0, 260.0] {
                let below = epa_extended(b - 1e-7, rh);
                let at = epa_extended(b, rh);
                assert!(
                    (below - at).abs() < 1e-4,
                    "jump at {b} rh {rh}: {below} vs {at}"
                );
            }
        }
    }

    #[test]
    fn monotonic_in_concentration() {
        let mut prev = epa_extended(0.0, 50.0);
        let mut x = 0.5;
        while x < 1000.0 {
            let y = epa_extended(x, 50.0);
            assert!(y >= prev - 1e-9, "decrease at {x}");
            prev = y;
            x += 0.5;
        }
    }

    #[test]
    fn negative_floored_to_zero() {
        assert_eq!(epa_extended(0.0, 100.0), 0.0);
    }

    #[test]
    fn ab_agreement_rules() {
        assert!(channels_agree(10.0, 14.9)); // diff < 5
        assert!(channels_agree(100.0, 130.0)); // diff 30 but 26 %
        assert!(!channels_agree(2.0, 20.0)); // diff 18 and 164 %
        assert!(channels_agree(1.0, 6.0)); // diff exactly 5
        assert!(!channels_agree(0.0, 6.0));
    }
}
