//! Continuous preference signals. Architecture is frozen; do not change formulas
//! unless a constant here is internally inconsistent.

use serde::{Deserialize, Serialize};

pub const RECENCY_MAX: f32 = 1.35;
pub const RECENCY_MIN: f32 = 0.70;
pub const RECENCY_LAMBDA: f32 = 0.51;
pub const STD_FLOOR: f32 = 0.35;
pub const ABSOLUTE_NEUTRAL: f32 = 3.0;
pub const HEART_WEIGHT: f32 = 1.25;
pub const MIN_RATINGS: usize = 8;
pub const RELATIVE_CLAMP: f32 = 2.5;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RatingProfile {
    pub mean: f32,
    pub std: f32,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preference {
    pub raw_rating: f32,
    pub relative: f32,
    pub absolute: f32,
    pub affinity_preference: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InteractionSignal {
    pub preference: Preference,
    pub recency_weight: f32,
    pub rewatch_weight: f32,
    pub heart_weight: f32,
    pub effective_weight: f32,
}

pub fn rating_profile(ratings: &[f32]) -> Option<RatingProfile> {
    if ratings.len() < MIN_RATINGS {
        return None;
    }
    let n = ratings.len() as f32;
    let mean = ratings.iter().sum::<f32>() / n;
    let var = ratings.iter().map(|r| (r - mean).powi(2)).sum::<f32>() / n;
    Some(RatingProfile {
        mean,
        std: var.sqrt(),
        count: ratings.len(),
    })
}

pub fn relative_preference(rating: f32, profile: &RatingProfile) -> f32 {
    let denom = profile.std.max(STD_FLOOR);
    ((rating - profile.mean) / denom).clamp(-RELATIVE_CLAMP, RELATIVE_CLAMP)
}

pub fn absolute_preference(rating: f32) -> f32 {
    ((rating - ABSOLUTE_NEUTRAL) / 2.0).clamp(-1.0, 1.0)
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

/// Maps z-score into (0.5, 1.0]. Ranks positives; does not scale negatives.
pub fn relative_strength(relative: f32) -> f32 {
    0.5 + 0.5 * sigmoid(relative)
}

pub fn affinity_preference(absolute: f32, relative: f32) -> f32 {
    let positive = absolute.max(0.0) * relative_strength(relative);
    let negative = (-absolute).max(0.0);
    positive - negative
}

pub fn preference_from_rating(rating: f32, profile: &RatingProfile) -> Preference {
    let relative = relative_preference(rating, profile);
    let absolute = absolute_preference(rating);
    Preference {
        raw_rating: rating,
        relative,
        absolute,
        affinity_preference: affinity_preference(absolute, relative),
    }
}

/// Age in years. Missing dates use 2.0 years (weight ≈ 0.93, near typical history).
pub fn recency_weight(age_years: Option<f32>) -> f32 {
    let t = age_years.unwrap_or(2.0).max(0.0);
    RECENCY_MIN + (RECENCY_MAX - RECENCY_MIN) * (-RECENCY_LAMBDA * t).exp()
}

pub fn rewatch_weight(viewings: u32) -> f32 {
    let extra = viewings.saturating_sub(1) as f32;
    1.0 + (1.0 + extra).ln() * 0.2
}

pub fn heart_weight(liked: bool) -> f32 {
    if liked {
        HEART_WEIGHT
    } else {
        1.0
    }
}

pub fn interaction_signal(
    rating: f32,
    profile: &RatingProfile,
    age_years: Option<f32>,
    viewings: u32,
    liked: bool,
) -> InteractionSignal {
    let preference = preference_from_rating(rating, profile);
    let recency_weight = recency_weight(age_years);
    let rewatch_weight = rewatch_weight(viewings);
    let heart_weight = heart_weight(liked);
    InteractionSignal {
        preference,
        recency_weight,
        rewatch_weight,
        heart_weight,
        effective_weight: recency_weight * rewatch_weight * heart_weight,
    }
}

pub fn weighted_mean(pairs: &[(f32, f32)]) -> Option<f32> {
    let sum_w: f32 = pairs.iter().map(|(_, w)| *w).sum();
    if sum_w <= 0.0 {
        return None;
    }
    Some(pairs.iter().map(|(p, w)| p * w).sum::<f32>() / sum_w)
}

pub fn weighted_variance(pairs: &[(f32, f32)], mean: f32) -> f32 {
    let sum_w: f32 = pairs.iter().map(|(_, w)| *w).sum();
    if sum_w <= 0.0 {
        return 0.0;
    }
    pairs
        .iter()
        .map(|(p, w)| w * (p - mean).powi(2))
        .sum::<f32>()
        / sum_w
}

pub fn years_since(iso: &str, now: chrono::DateTime<chrono::Utc>) -> Option<f32> {
    let parsed = chrono::DateTime::parse_from_rfc3339(iso)
        .ok()
        .map(|d| d.with_timezone(&chrono::Utc))
        .or_else(|| {
            chrono::NaiveDate::parse_from_str(iso.get(..10).unwrap_or(iso), "%Y-%m-%d")
                .ok()
                .and_then(|d| d.and_hms_opt(0, 0, 0))
                .map(|d| d.and_utc())
        })?;
    let days = (now - parsed).num_days() as f32;
    Some((days / 365.25).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generous() -> RatingProfile {
        rating_profile(&[4.0, 4.0, 4.0, 4.5, 4.0, 4.5, 4.0, 5.0]).unwrap()
    }

    #[test]
    fn refuses_sparse_logs() {
        assert!(rating_profile(&[4.0, 5.0, 3.0, 4.0, 4.5, 2.0, 3.5]).is_none());
        assert!(rating_profile(&[4.0; 8]).is_some());
    }

    #[test]
    fn absolute_sign_table() {
        assert!((absolute_preference(5.0) - 1.0).abs() < 1e-5);
        assert!((absolute_preference(4.0) - 0.5).abs() < 1e-5);
        assert!((absolute_preference(3.0)).abs() < 1e-5);
        assert!((absolute_preference(2.0) + 0.5).abs() < 1e-5);
        assert!((absolute_preference(0.5) + 1.0).abs() < 1e-5);
        assert!(absolute_preference(1.0) >= -1.0);
    }

    #[test]
    fn generous_four_star_is_positive_evidence() {
        let profile = generous();
        assert!(profile.mean > 4.1);
        let pref = preference_from_rating(4.0, &profile);
        assert!(pref.relative < 0.0, "4.0 is below a generous mean");
        assert!(pref.absolute > 0.0);
        assert!(
            pref.affinity_preference > 0.0,
            "got {}",
            pref.affinity_preference
        );
    }

    #[test]
    fn two_star_is_negative_regardless_of_relative() {
        let harsh = rating_profile(&[1.0, 1.5, 2.0, 2.0, 2.5, 2.0, 3.0, 2.5]).unwrap();
        let pref = preference_from_rating(2.0, &harsh);
        assert!(pref.absolute < 0.0);
        assert!(pref.affinity_preference < 0.0);
    }

    #[test]
    fn recency_table() {
        let w0 = recency_weight(Some(0.0));
        let w6m = recency_weight(Some(0.5));
        let w1 = recency_weight(Some(1.0));
        let w2 = recency_weight(Some(2.0));
        let w5 = recency_weight(Some(5.0));
        let winf = recency_weight(Some(80.0));
        assert!((w0 - 1.35).abs() < 0.02);
        assert!((w6m - 1.20).abs() < 0.02);
        assert!((w1 - 1.09).abs() < 0.03);
        assert!((w2 - 0.93).abs() < 0.03);
        assert!((w5 - 0.75).abs() < 0.02);
        assert!((winf - 0.70).abs() < 0.02);
        assert!(winf > 0.69);
    }

    #[test]
    fn rewatch_and_heart_are_multiplicative() {
        let p = generous();
        let once = interaction_signal(4.5, &p, Some(1.0), 1, false);
        let many = interaction_signal(4.5, &p, Some(1.0), 7, false);
        let heart = interaction_signal(4.5, &p, Some(1.0), 1, true);
        assert!(many.rewatch_weight > once.rewatch_weight);
        assert!((heart.heart_weight - 1.25).abs() < 1e-5);
        assert!(
            (heart.effective_weight - once.effective_weight * 1.25).abs() < 1e-4
        );
        assert!((once.rewatch_weight - 1.0).abs() < 1e-5);
    }

    #[test]
    fn weighted_mean_does_not_fold_weight_into_pref() {
        let mean = weighted_mean(&[(1.2, 1.3), (-0.4, 0.8)]).unwrap();
        let expected = (1.2 * 1.3 + -0.4 * 0.8) / (1.3 + 0.8);
        assert!((mean - expected).abs() < 1e-5);
    }
}
