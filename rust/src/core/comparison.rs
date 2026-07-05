//! Statistical comparison utilities for coralX.
//!
//! Bootstrap CI, group comparison tests, and temporal/depth trend analyses.
//! All functions are pure: no UI, no side effects.

use std::collections::HashMap;

use rand::{Rng, RngExt, SeedableRng};
use rand::rngs::StdRng;
use statrs::distribution::{ChiSquared, ContinuousCDF, FisherSnedecor, StudentsT};
use time::{macros::format_description, Date};

use crate::core::statistics::station_summary;
use crate::core::validation::validate_metadata_completeness;
use crate::models::Project;

#[derive(Debug, Clone)]
pub struct BootstrapCi {
    pub value: f64,
    pub ci_lower: f64,
    pub ci_upper: f64,
}

/// Bootstrap percentile confidence interval for a label-based metric.
///
/// Resamples `labels` with replacement `n_boot` times, computes `metric_fn`
/// on each resample, and returns the percentile-based CI. Uses a seeded RNG
/// for reproducibility (not bit-identical to the Python/numpy PCG64 stream,
/// but stable across runs of this program).
pub fn bootstrap_ci(
    labels: &[String],
    metric_fn: impl Fn(&[String]) -> f64,
    n_boot: usize,
    confidence: f64,
    seed: u64,
) -> BootstrapCi {
    if labels.is_empty() {
        return BootstrapCi { value: 0.0, ci_lower: 0.0, ci_upper: 0.0 };
    }

    let observed = metric_fn(labels);
    let mut rng = StdRng::seed_from_u64(seed);
    let n = labels.len();
    let mut boot_vals: Vec<f64> = Vec::with_capacity(n_boot);
    for _ in 0..n_boot {
        let sample: Vec<String> = (0..n).map(|_| labels[rng.random_range(0..n)].clone()).collect();
        boot_vals.push(metric_fn(&sample));
    }
    boot_vals.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let alpha = 1.0 - confidence;
    let lo = percentile(&boot_vals, alpha / 2.0 * 100.0);
    let hi = percentile(&boot_vals, (1.0 - alpha / 2.0) * 100.0);
    BootstrapCi { value: round4(observed), ci_lower: round4(lo), ci_upper: round4(hi) }
}

/// `numpy.percentile` with default linear interpolation, `p` in `[0, 100]`.
fn percentile(sorted_vals: &[f64], p: f64) -> f64 {
    let n = sorted_vals.len();
    if n == 0 {
        return 0.0;
    }
    if n == 1 {
        return sorted_vals[0];
    }
    let rank = p / 100.0 * (n as f64 - 1.0);
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    if lo == hi {
        return sorted_vals[lo];
    }
    let frac = rank - lo as f64;
    sorted_vals[lo] + (sorted_vals[hi] - sorted_vals[lo]) * frac
}

#[derive(Debug, Clone)]
pub struct GroupComparison {
    pub method: &'static str,
    pub statistic: f64,
    pub p_value: f64,
    pub significant: bool,
}

/// Compare a metric across groups using one-way ANOVA or Kruskal-Wallis.
///
/// `method`: "anova" | "kruskal" | "auto". "auto" chooses Kruskal-Wallis when
/// any group has n < 10, else ANOVA. Returns `Err` when data requirements
/// (>=2 groups, each with >=2 values) are not met.
pub fn compare_groups(
    values_by_group: &HashMap<String, Vec<f64>>,
    method: &str,
) -> Result<GroupComparison, String> {
    let groups: HashMap<&String, &Vec<f64>> =
        values_by_group.iter().filter(|(_, v)| !v.is_empty()).collect();
    if groups.len() < 2 {
        return Err(format!("Need >=2 groups; got {}.", groups.len()));
    }
    for (name, vals) in &groups {
        if vals.len() < 2 {
            return Err(format!("Group '{name}' has only {} value(s); need >=2.", vals.len()));
        }
    }

    let chosen = if method == "auto" {
        if groups.values().any(|v| v.len() < 10) { "kruskal" } else { "anova" }
    } else {
        method
    };

    let group_vecs: Vec<&Vec<f64>> = groups.values().copied().collect();
    let (stat, p) = if chosen == "anova" {
        f_oneway(&group_vecs)
    } else {
        kruskal(&group_vecs)
    };

    Ok(GroupComparison {
        method: if chosen == "anova" { "anova" } else { "kruskal" },
        statistic: round6(stat),
        p_value: round6(p),
        significant: p < 0.05,
    })
}

/// One-way ANOVA F-test.
fn f_oneway(groups: &[&Vec<f64>]) -> (f64, f64) {
    let k = groups.len();
    let n: usize = groups.iter().map(|g| g.len()).sum();
    let grand_mean: f64 = groups.iter().flat_map(|g| g.iter()).sum::<f64>() / n as f64;

    let ssb: f64 = groups
        .iter()
        .map(|g| {
            let mean = g.iter().sum::<f64>() / g.len() as f64;
            g.len() as f64 * (mean - grand_mean).powi(2)
        })
        .sum();
    let ssw: f64 = groups
        .iter()
        .map(|g| {
            let mean = g.iter().sum::<f64>() / g.len() as f64;
            g.iter().map(|x| (x - mean).powi(2)).sum::<f64>()
        })
        .sum();

    let df1 = (k - 1) as f64;
    let df2 = (n - k) as f64;
    let f_stat = (ssb / df1) / (ssw / df2);
    let p = FisherSnedecor::new(df1, df2)
        .map(|dist| dist.sf(f_stat))
        .unwrap_or(f64::NAN);
    (f_stat, p)
}

/// Kruskal-Wallis H-test with tie correction.
fn kruskal(groups: &[&Vec<f64>]) -> (f64, f64) {
    let n: usize = groups.iter().map(|g| g.len()).sum();

    // Flatten with group index, then rank (average rank for ties).
    let mut all: Vec<(f64, usize)> =
        groups.iter().enumerate().flat_map(|(gi, g)| g.iter().map(move |&v| (v, gi))).collect();
    all.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    let mut ranks = vec![0.0; all.len()];
    let mut i = 0;
    let mut tie_term = 0.0;
    while i < all.len() {
        let mut j = i;
        while j + 1 < all.len() && all[j + 1].0 == all[i].0 {
            j += 1;
        }
        let avg_rank = ((i + 1) + (j + 1)) as f64 / 2.0;
        for r in ranks.iter_mut().take(j + 1).skip(i) {
            *r = avg_rank;
        }
        let t = (j - i + 1) as f64;
        tie_term += t.powi(3) - t;
        i = j + 1;
    }

    let mut rank_sums = vec![0.0; groups.len()];
    for (idx, &(_, gi)) in all.iter().enumerate() {
        rank_sums[gi] += ranks[idx];
    }

    let h_raw: f64 = (12.0 / (n as f64 * (n as f64 + 1.0)))
        * groups.iter().enumerate().map(|(gi, g)| rank_sums[gi].powi(2) / g.len() as f64).sum::<f64>()
        - 3.0 * (n as f64 + 1.0);

    let n_f64 = n as f64;
    let correction = 1.0 - tie_term / (n_f64.powi(3) - n_f64);
    let h = if correction > 0.0 { h_raw / correction } else { h_raw };

    let df = (groups.len() - 1) as f64;
    let p = ChiSquared::new(df).map(|dist| dist.sf(h)).unwrap_or(f64::NAN);
    (h, p)
}

#[derive(Debug, Clone)]
pub struct LinRegress {
    pub slope: f64,
    pub intercept: f64,
    pub r_squared: f64,
    pub p_value: f64,
}

/// Ordinary least-squares simple linear regression with a two-tailed
/// significance test on the slope (`scipy.stats.linregress` equivalent).
fn linregress(x: &[f64], y: &[f64]) -> LinRegress {
    let n = x.len() as f64;
    let x_bar = x.iter().sum::<f64>() / n;
    let y_bar = y.iter().sum::<f64>() / n;

    let sxy: f64 = x.iter().zip(y).map(|(xi, yi)| (xi - x_bar) * (yi - y_bar)).sum();
    let sxx: f64 = x.iter().map(|xi| (xi - x_bar).powi(2)).sum();
    let syy: f64 = y.iter().map(|yi| (yi - y_bar).powi(2)).sum();

    let slope = sxy / sxx;
    let intercept = y_bar - slope * x_bar;
    let r = sxy / (sxx * syy).sqrt();
    let r_squared = r.powi(2);

    let df = n - 2.0;
    let sse = (1.0 - r_squared) * syy;
    let se_slope = (sse / df / sxx).sqrt();
    let t = slope / se_slope;
    let p_value =
        StudentsT::new(0.0, 1.0, df).map(|dist| 2.0 * dist.sf(t.abs())).unwrap_or(f64::NAN);

    LinRegress { slope, intercept, r_squared, p_value }
}

#[derive(Debug, Clone)]
pub struct StationTrend {
    pub dates: Vec<String>,
    pub values: Vec<f64>,
    pub slope: f64,
    pub p_value: f64,
    /// "naik" (up) | "turun" (down) | "stabil" (stable)
    pub trend: &'static str,
}

#[derive(Debug, Clone)]
pub struct TemporalTrendResult {
    pub ok: bool,
    pub reason: Option<String>,
    pub stations: HashMap<String, StationTrend>,
}

/// Linear temporal trend for a metric across survey dates, per station.
///
/// `metric`: one of "live_coral_pct", "mortality_index", "shannon". Requires
/// `validate_metadata_completeness(project)["temporal"].ok`.
pub fn temporal_trend(project: &Project, metric: &str) -> TemporalTrendResult {
    let meta = validate_metadata_completeness(project);
    let temporal = &meta["temporal"];
    if !temporal.ok {
        return TemporalTrendResult {
            ok: false,
            reason: Some(temporal.reasons.join("; ")),
            stations: HashMap::new(),
        };
    }

    let coral_groups = &project.coral_groups;
    let mut collected: HashMap<String, (Vec<String>, Vec<f64>)> = HashMap::new();
    for st in &project.stations {
        let Some(d) = &st.date else { continue };
        if d.is_empty() {
            continue;
        }
        let Some(summary) = station_summary(st, coral_groups) else { continue };
        let Some(value) = extract_metric(&summary, metric) else { continue };
        let entry = collected.entry(st.name.clone()).or_insert_with(|| (Vec::new(), Vec::new()));
        entry.0.push(d.clone());
        entry.1.push(value);
    }

    let fmt = format_description!("[year]-[month]-[day]");
    let mut station_trends = HashMap::new();
    for (sname, (dates, values)) in collected {
        if dates.len() < 2 {
            continue;
        }
        let x: Vec<f64> = dates
            .iter()
            .map(|d| Date::parse(d, &fmt).map(|dt| dt.to_julian_day() as f64).unwrap_or(0.0))
            .collect();
        let result = linregress(&x, &values);
        let trend = if result.p_value >= 0.05 {
            "stabil"
        } else if result.slope > 0.0 {
            "naik"
        } else {
            "turun"
        };
        station_trends.insert(
            sname,
            StationTrend {
                dates,
                values,
                slope: round6(result.slope),
                p_value: round6(result.p_value),
                trend,
            },
        );
    }

    TemporalTrendResult { ok: true, reason: None, stations: station_trends }
}

#[derive(Debug, Clone)]
pub struct DepthGradientResult {
    pub ok: bool,
    pub reason: Option<String>,
    pub slope: Option<f64>,
    pub r_squared: Option<f64>,
    pub p_value: Option<f64>,
    pub points: Vec<(f64, f64)>,
}

/// Linear regression of a metric against depth across stations.
///
/// Requires `validate_metadata_completeness(project)["depth"].ok`.
pub fn depth_gradient(project: &Project, metric: &str) -> DepthGradientResult {
    let meta = validate_metadata_completeness(project);
    let depth = &meta["depth"];
    if !depth.ok {
        return DepthGradientResult {
            ok: false,
            reason: Some(depth.reasons.join("; ")),
            slope: None,
            r_squared: None,
            p_value: None,
            points: Vec::new(),
        };
    }

    let coral_groups = &project.coral_groups;
    let mut points: Vec<(f64, f64)> = Vec::new();
    for st in &project.stations {
        let Some(dm) = st.depth_m else { continue };
        if dm <= 0.0 {
            continue;
        }
        let Some(summary) = station_summary(st, coral_groups) else { continue };
        let Some(value) = extract_metric(&summary, metric) else { continue };
        points.push((dm, value));
    }

    if points.len() < 3 {
        return DepthGradientResult {
            ok: false,
            reason: Some(format!("Need >=3 stations with both depth and metric; found {}.", points.len())),
            slope: None,
            r_squared: None,
            p_value: None,
            points,
        };
    }

    let x: Vec<f64> = points.iter().map(|p| p.0).collect();
    let y: Vec<f64> = points.iter().map(|p| p.1).collect();
    let result = linregress(&x, &y);

    DepthGradientResult {
        ok: true,
        reason: None,
        slope: Some(round6(result.slope)),
        r_squared: Some(round4(result.r_squared)),
        p_value: Some(round6(result.p_value)),
        points,
    }
}

/// Extract a named metric from a `station_summary` result.
fn extract_metric(summary: &crate::core::statistics::Summary, metric: &str) -> Option<f64> {
    match metric {
        "live_coral_pct" => summary.group_coverage.get("Hard Coral").copied(),
        "mortality_index" => summary.mortality_index,
        "shannon" => Some(summary.shannon_diversity),
        _ => None,
    }
}

fn round4(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}

fn round6(v: f64) -> f64 {
    (v * 1_000_000.0).round() / 1_000_000.0
}
