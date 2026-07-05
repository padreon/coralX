//! Extended analysis functions for coral reef point count data.

use std::collections::{HashMap, HashSet};

use statrs::distribution::{ContinuousCDF, Normal};

use crate::models::{CoralGroup, ImageAnnotation};

/// Number of unique codes present (S).
pub fn species_richness(labels: &[String]) -> usize {
    labels.iter().collect::<HashSet<_>>().len()
}

/// Pielou's J' = H' / ln(S). Range 0-1; 1 = perfectly even distribution.
pub fn pielou_evenness(h_prime: f64, s: usize) -> f64 {
    if s <= 1 || h_prime <= 0.0 {
        return 0.0;
    }
    h_prime / (s as f64).ln()
}

/// Margalef's d = (S - 1) / ln(N). Species richness relative to sample size.
pub fn margalef_richness(s: usize, n: usize) -> f64 {
    if n <= 1 || s == 0 {
        return 0.0;
    }
    (s as f64 - 1.0) / (n as f64).ln()
}

/// Fisher's alpha diversity index via iterative root-finding.
///
/// Solves `S = alpha * ln(1 + N/alpha)` by bisection on `[1e-6, N*10]`
/// (mirrors scipy's `brentq` bracket in the original implementation).
pub fn fisher_alpha(s: usize, n: usize) -> f64 {
    let (s, n) = (s as f64, n as f64);
    if s <= 0.0 || n <= 0.0 || s >= n {
        return 0.0;
    }
    let f = |alpha: f64| alpha * (1.0 + n / alpha).ln() - s;

    let mut lo = 1e-6_f64;
    let mut hi = n * 10.0;
    let mut f_lo = f(lo);
    let f_hi = f(hi);
    if !f_lo.is_finite() || !f_hi.is_finite() || f_lo * f_hi > 0.0 {
        return 0.0;
    }
    for _ in 0..200 {
        let mid = (lo + hi) / 2.0;
        let f_mid = f(mid);
        if f_mid.abs() < 1e-10 || (hi - lo) < 1e-12 {
            return mid;
        }
        if f_lo * f_mid < 0.0 {
            hi = mid;
        } else {
            lo = mid;
            f_lo = f_mid;
        }
    }
    (lo + hi) / 2.0
}

/// 95% CI for a proportion using the Wilson score interval.
///
/// More accurate than the normal approximation for small n or extreme
/// proportions. Returns (lower%, upper%) as percentages (0-100).
pub fn wilson_confidence_interval(n_hits: usize, n_total: usize, confidence: f64) -> (f64, f64) {
    if n_total == 0 {
        return (0.0, 0.0);
    }
    let normal = Normal::new(0.0, 1.0).expect("standard normal is always valid");
    let z = normal.inverse_cdf(1.0 - (1.0 - confidence) / 2.0);
    let n_total = n_total as f64;
    let p = n_hits as f64 / n_total;
    let centre = (p + z.powi(2) / (2.0 * n_total)) / (1.0 + z.powi(2) / n_total);
    let margin = (z / (1.0 + z.powi(2) / n_total))
        * (p * (1.0 - p) / n_total + z.powi(2) / (4.0 * n_total.powi(2))).sqrt();
    let lower = (centre - margin).max(0.0) * 100.0;
    let upper = (centre + margin).min(1.0) * 100.0;
    (round2(lower), round2(upper))
}

/// % cover aggregated per group using the project's `coral_groups` mapping.
///
/// Returns e.g. `{"Hard Coral": 42.3, "Soft / Algae": 18.1, "Uncategorized": 14.6}`.
pub fn group_coverage(labels: &[String], coral_groups: &[CoralGroup]) -> HashMap<String, f64> {
    if labels.is_empty() {
        return HashMap::new();
    }
    let total = labels.len() as f64;
    let counts = count_labels(labels);

    let mut code_to_group: HashMap<&str, &str> = HashMap::new();
    for group in coral_groups {
        for code in &group.codes {
            code_to_group.insert(code.as_str(), group.name.as_str());
        }
    }

    let mut group_counts: HashMap<String, u32> = HashMap::new();
    let mut uncategorized: u32 = 0;
    for (code, cnt) in &counts {
        match code_to_group.get(code.as_str()) {
            Some(grp) => *group_counts.entry(grp.to_string()).or_insert(0) += cnt,
            None => uncategorized += cnt,
        }
    }

    let mut result: HashMap<String, f64> =
        group_counts.into_iter().map(|(k, v)| (k, round2(v as f64 / total * 100.0))).collect();
    if uncategorized > 0 {
        result.insert("Uncategorized".to_string(), round2(uncategorized as f64 / total * 100.0));
    }
    result
}

/// Effective photo area in `scale_unit^2` (cm^2 or m^2).
///
/// Returns `None` if `scale_factor` is not calibrated (== 1.0 or == 0).
pub fn photo_area(annotation: &ImageAnnotation) -> Option<f64> {
    let sf = annotation.scale_factor;
    if sf <= 1.0 {
        return None;
    }
    let eff_w = annotation.image_width as f64;
    let eff_h = annotation.image_height as f64;
    Some(round4((eff_w / sf) * (eff_h / sf)))
}

/// Actual area (unit^2) per code = photo_area * (count_code / total_labeled).
///
/// Returns `None` if not calibrated.
pub fn cover_area_per_code(annotation: &ImageAnnotation) -> Option<HashMap<String, f64>> {
    let p_area = photo_area(annotation)?;
    let labels: Vec<String> =
        annotation.points.iter().filter_map(|p| p.label.clone()).collect();
    if labels.is_empty() {
        return None;
    }
    let total = labels.len() as f64;
    let counts = count_labels(&labels);
    Some(counts.into_iter().map(|(code, cnt)| (code, round4(p_area * cnt as f64 / total))).collect())
}

/// Per-code coverage % with Wilson confidence intervals.
///
/// Returns `{"HC": {"pct": 42.3, "ci_lower": 38.1, "ci_upper": 46.7}, ...}`.
pub fn coverage_with_ci(labels: &[String], confidence: f64) -> HashMap<String, (f64, f64, f64)> {
    let total = labels.len();
    if total == 0 {
        return HashMap::new();
    }
    let counts = count_labels(labels);
    counts
        .into_iter()
        .map(|(code, cnt)| {
            let pct = round2(cnt as f64 / total as f64 * 100.0);
            let (ci_lo, ci_hi) = wilson_confidence_interval(cnt as usize, total, confidence);
            (code, (pct, ci_lo, ci_hi))
        })
        .collect()
}

fn codes_of_group(coral_groups: &[CoralGroup], group_name: &str) -> HashSet<String> {
    let target = group_name.trim().to_lowercase();
    for g in coral_groups {
        if g.name.trim().to_lowercase() == target {
            return g.codes.iter().cloned().collect();
        }
    }
    HashSet::new()
}

/// Mortality Index (MI) = dead / (live_hard_coral + dead).
///
/// Range 0..1; higher = more coral mortality. `None` when the denominator is 0.
pub fn mortality_index(labels: &[String], coral_groups: &[CoralGroup]) -> Option<f64> {
    let dead_codes = codes_of_group(coral_groups, "Dead Coral");
    let hc_codes = codes_of_group(coral_groups, "Hard Coral");
    let dead = labels.iter().filter(|l| dead_codes.contains(*l)).count();
    let live = labels.iter().filter(|l| hc_codes.contains(*l)).count();
    let denom = live + dead;
    if denom == 0 {
        return None;
    }
    Some(round4(dead as f64 / denom as f64))
}

#[derive(Debug, Clone)]
pub struct ReefHealth {
    pub category: &'static str,
    pub live_coral_pct: f64,
}

/// Classify reef health by Gomez & Yap (1988) / KepMen LH No.4/2001 thresholds.
pub fn reef_health_category(live_coral_pct: f64) -> ReefHealth {
    let category = if live_coral_pct < 25.0 {
        "Poor"
    } else if live_coral_pct < 50.0 {
        "Fair"
    } else if live_coral_pct < 75.0 {
        "Good"
    } else {
        "Excellent"
    };
    ReefHealth { category, live_coral_pct: round2(live_coral_pct) }
}

/// Coral-to-algae ratio = live_hard_coral_pct / algae_pct.
///
/// `>1` = coral-dominated; `<1` = algae-dominated. `None` when algae_pct == 0.
pub fn coral_algae_ratio(labels: &[String], coral_groups: &[CoralGroup]) -> Option<f64> {
    if labels.is_empty() {
        return None;
    }
    let total = labels.len() as f64;
    let hc_codes = codes_of_group(coral_groups, "Hard Coral");
    let algae_codes = codes_of_group(coral_groups, "Algae");
    let hc_pct = labels.iter().filter(|l| hc_codes.contains(*l)).count() as f64 / total * 100.0;
    let algae_pct = labels.iter().filter(|l| algae_codes.contains(*l)).count() as f64 / total * 100.0;
    if algae_pct == 0.0 {
        return None;
    }
    Some(round4(hc_pct / algae_pct))
}

/// Berger-Parker dominance d = n_max / N. Range 0..1.
pub fn berger_parker_dominance(labels: &[String]) -> f64 {
    if labels.is_empty() {
        return 0.0;
    }
    let counts = count_labels(labels);
    round4(*counts.values().max().unwrap() as f64 / labels.len() as f64)
}

#[derive(Debug, Clone)]
pub struct HillNumbers {
    pub q0: usize,
    pub q1: f64,
    pub q2: f64,
}

/// Hill numbers - effective number of species at three diversity orders.
///
/// q0 = species richness; q1 = exp(Shannon H'); q2 = 1 / Simpson_D.
pub fn hill_numbers(labels: &[String]) -> HillNumbers {
    if labels.is_empty() {
        return HillNumbers { q0: 0, q1: 0.0, q2: 0.0 };
    }
    let counts = count_labels(labels);
    let n = labels.len() as f64;
    let q0 = counts.len();
    let h: f64 = -counts.values().map(|&c| (c as f64 / n) * (c as f64 / n).ln()).sum::<f64>();
    let d: f64 = counts.values().map(|&c| (c as f64 / n).powi(2)).sum();
    let q1 = if h > 0.0 { round4(h.exp()) } else { 1.0 };
    let q2 = if d > 0.0 { round4(1.0 / d) } else { f64::INFINITY };
    HillNumbers { q0, q1, q2 }
}

fn count_labels(labels: &[String]) -> HashMap<String, u32> {
    let mut counts: HashMap<String, u32> = HashMap::new();
    for l in labels {
        *counts.entry(l.clone()).or_insert(0) += 1;
    }
    counts
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn round4(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}
