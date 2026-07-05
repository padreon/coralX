//! Multivariate community analysis for coralX.
//!
//! All functions require `can_run_multivariate(project).ok == true` before
//! use. Functions are pure (no UI, no side effects).
//!
//! Default `biotic_only = true` and TWS is always excluded so abiotic
//! substrate codes don't dominate Bray-Curtis distances.

use std::collections::{HashMap, HashSet};

use nalgebra::DMatrix;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;

use crate::models::Project;

/// Substrate / artefact codes excluded when `biotic_only = true`.
const ABIOTIC_CODES: &[&str] = &["S", "R", "RK", "SI", "SD", "RB", "TWS", "OT"];

pub struct CompositionMatrix {
    pub sample_names: Vec<String>,
    pub code_names: Vec<String>,
    /// shape (n_stations, n_codes)
    pub matrix: DMatrix<f64>,
}

/// Build a site x species composition matrix (proportions).
///
/// Rows = stations, columns = codes, cells = proportion of labeled points.
/// `transform`: "none" | "sqrt" | "fourth_root", applied element-wise.
pub fn composition_matrix(
    project: &Project,
    biotic_only: bool,
    exclude_codes: &HashSet<String>,
    transform: &str,
) -> CompositionMatrix {
    let mut drop: HashSet<String> = exclude_codes.clone();
    drop.insert("TWS".to_string());
    if biotic_only {
        drop.extend(ABIOTIC_CODES.iter().map(|s| s.to_string()));
    }

    let mut sample_names = Vec::new();
    let mut raw_counts: Vec<HashMap<String, u32>> = Vec::new();

    for st in &project.stations {
        let mut counts: HashMap<String, u32> = HashMap::new();
        for ann in &st.annotations {
            for p in &ann.points {
                if let Some(lbl) = &p.label {
                    if !drop.contains(lbl) {
                        *counts.entry(lbl.clone()).or_insert(0) += 1;
                    }
                }
            }
        }
        sample_names.push(st.name.clone());
        raw_counts.push(counts);
    }

    let mut all_codes: Vec<String> =
        raw_counts.iter().flat_map(|c| c.keys().cloned()).collect::<HashSet<_>>().into_iter().collect();
    all_codes.sort();

    let mut matrix = DMatrix::<f64>::zeros(sample_names.len(), all_codes.len());
    for (i, counts) in raw_counts.iter().enumerate() {
        let total: u32 = counts.values().sum();
        if total > 0 {
            for (j, code) in all_codes.iter().enumerate() {
                matrix[(i, j)] = *counts.get(code).unwrap_or(&0) as f64 / total as f64;
            }
        }
    }

    match transform {
        "sqrt" => matrix.apply(|v| *v = v.sqrt()),
        "fourth_root" => matrix.apply(|v| *v = v.powf(0.25)),
        _ => {}
    }

    CompositionMatrix { sample_names, code_names: all_codes, matrix }
}

/// Bray-Curtis dissimilarity matrix (n_samples x n_samples), range 0..1.
pub fn bray_curtis_matrix(matrix: &DMatrix<f64>) -> DMatrix<f64> {
    let n = matrix.nrows();
    let mut d = DMatrix::<f64>::zeros(n, n);
    for i in 0..n {
        for j in (i + 1)..n {
            let row_i = matrix.row(i);
            let row_j = matrix.row(j);
            let num: f64 = row_i.iter().zip(row_j.iter()).map(|(a, b)| (a - b).abs()).sum();
            let denom: f64 = row_i.iter().zip(row_j.iter()).map(|(a, b)| a + b).sum();
            let bc = num / denom; // NaN for two all-zero rows, matching scipy's pdist behavior
            d[(i, j)] = bc;
            d[(j, i)] = bc;
        }
    }
    d
}

pub struct PcoaResult {
    pub coords: DMatrix<f64>,
    pub eigenvalues: Vec<f64>,
    pub variance_explained: Vec<f64>,
}

/// Principal Coordinates Analysis (classical MDS).
///
/// 1. Square the distance matrix. 2. Double-center: `A = -0.5*D^2`, `G = H*A*H`.
/// 3. Eigen-decompose `G`, keep `n_axes` with the largest positive eigenvalues.
pub fn pcoa(distance_matrix: &DMatrix<f64>, n_axes: usize) -> PcoaResult {
    let n = distance_matrix.nrows();
    let d2 = distance_matrix.map(|v| v * v);
    let h = DMatrix::<f64>::identity(n, n) - DMatrix::<f64>::from_element(n, n, 1.0 / n as f64);
    let g = &h * d2 * &h * -0.5;

    let eigen = nalgebra::SymmetricEigen::new(g);
    let mut pairs: Vec<(f64, usize)> = eigen.eigenvalues.iter().copied().enumerate().map(|(i, v)| (v, i)).collect();
    pairs.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());

    let pos: Vec<(f64, usize)> = pairs.into_iter().filter(|(v, _)| *v > 1e-10).collect();
    let k = n_axes.min(pos.len());
    let total_var: f64 = if pos.is_empty() { 1.0 } else { pos.iter().map(|(v, _)| v).sum() };

    let mut coords = DMatrix::<f64>::zeros(n, k);
    let mut eigenvalues = Vec::with_capacity(k);
    let mut variance_explained = Vec::with_capacity(k);
    for (col, &(eval, orig_idx)) in pos.iter().take(k).enumerate() {
        let scale = eval.sqrt();
        for row in 0..n {
            coords[(row, col)] = eigen.eigenvectors[(row, orig_idx)] * scale;
        }
        eigenvalues.push(eval);
        variance_explained.push(round4(eval / total_var));
    }

    PcoaResult { coords, eigenvalues, variance_explained }
}

pub struct LinkageResult {
    /// scipy-format linkage rows: `[cluster_a, cluster_b, distance, count]`.
    pub linkage: Vec<[f64; 4]>,
    pub method: String,
}

/// Hierarchical agglomerative clustering from a distance matrix.
///
/// `method`: "average" (UPGMA, default), "complete", "single", or "ward"
/// (Lance-Williams update formulas).
pub fn hierarchical_clusters(distance_matrix: &DMatrix<f64>, method: &str) -> LinkageResult {
    let n = distance_matrix.nrows();
    let mut dist: HashMap<(usize, usize), f64> = HashMap::new();
    for i in 0..n {
        for j in (i + 1)..n {
            dist.insert((i, j), distance_matrix[(i, j)]);
        }
    }
    let mut active: Vec<usize> = (0..n).collect();
    let mut size: HashMap<usize, usize> = (0..n).map(|i| (i, 1)).collect();
    let mut linkage = Vec::with_capacity(n.saturating_sub(1));
    let mut next_id = n;

    let key = |a: usize, b: usize| if a < b { (a, b) } else { (b, a) };

    while active.len() > 1 {
        let mut best = (f64::INFINITY, active[0], active[1]);
        for i in 0..active.len() {
            for j in (i + 1)..active.len() {
                let (a, b) = (active[i], active[j]);
                let d = dist[&key(a, b)];
                if d < best.0 {
                    best = (d, a, b);
                }
            }
        }
        let (d_ab, a, b) = best;
        let (size_a, size_b) = (size[&a], size[&b]);
        linkage.push([a as f64, b as f64, d_ab.max(0.0), (size_a + size_b) as f64]);

        for &c in &active {
            if c == a || c == b {
                continue;
            }
            let d_ac = dist[&key(a, c)];
            let d_bc = dist[&key(b, c)];
            let size_c = size[&c];
            let d_new = match method {
                "complete" => d_ac.max(d_bc),
                "single" => d_ac.min(d_bc),
                "ward" => {
                    let num = (size_a + size_c) as f64 * d_ac.powi(2) + (size_b + size_c) as f64 * d_bc.powi(2)
                        - size_c as f64 * d_ab.powi(2);
                    (num / (size_a + size_b + size_c) as f64).max(0.0).sqrt()
                }
                _ => (size_a as f64 * d_ac + size_b as f64 * d_bc) / (size_a + size_b) as f64, // average / UPGMA
            };
            dist.insert(key(next_id, c), d_new);
        }

        active.retain(|&c| c != a && c != b);
        active.push(next_id);
        size.insert(next_id, size_a + size_b);
        next_id += 1;
    }

    LinkageResult { linkage, method: method.to_string() }
}

pub struct PermanovaResult {
    pub pseudo_f: f64,
    pub p_value: f64,
    pub permutations: usize,
    pub significant: bool,
}

/// PERMANOVA (Anderson 2001) — permutation-based multivariate ANOVA.
///
/// Tests whether community composition differs significantly between groups
/// using a pseudo-F statistic and a permutation-derived p-value.
pub fn permanova(
    distance_matrix: &DMatrix<f64>,
    group_labels: &[String],
    permutations: usize,
    seed: u64,
) -> Result<PermanovaResult, String> {
    let n = distance_matrix.nrows();
    let groups: HashSet<&String> = group_labels.iter().collect();
    let a = groups.len();
    if a < 2 {
        return Err(format!("Need >=2 groups; got {a}."));
    }
    for g in &groups {
        if group_labels.iter().filter(|l| l == g).count() < 2 {
            return Err(format!("Group '{g}' has only 1 sample; need >=2 per group."));
        }
    }

    let d2 = distance_matrix.map(|v| v * v);

    let pseudo_f = |labels: &[String]| -> f64 {
        let grp_names: HashSet<&String> = labels.iter().collect();
        let a_val = grp_names.len();
        let ss_total = d2.sum() / (2.0 * n as f64);
        let mut ss_within = 0.0;
        for g in &grp_names {
            let idx: Vec<usize> = labels.iter().enumerate().filter(|(_, l)| l == g).map(|(i, _)| i).collect();
            let ng = idx.len();
            if ng < 2 {
                continue;
            }
            let mut sub_sum = 0.0;
            for &i in &idx {
                for &j in &idx {
                    sub_sum += d2[(i, j)];
                }
            }
            ss_within += sub_sum / (2.0 * ng as f64);
        }
        let ss_between = ss_total - ss_within;
        if n == a_val {
            return 0.0;
        }
        let denom = ss_within / (n - a_val) as f64;
        if ss_between == 0.0 && denom == 0.0 {
            return 0.0;
        }
        if denom == 0.0 {
            return f64::INFINITY;
        }
        (ss_between / (a_val - 1) as f64) / denom
    };

    let observed_f = pseudo_f(group_labels);

    let mut rng = StdRng::seed_from_u64(seed);
    let mut perm: Vec<String> = group_labels.to_vec();
    let mut count_ge = 0usize;
    for _ in 0..permutations {
        perm.shuffle(&mut rng);
        if pseudo_f(&perm) >= observed_f {
            count_ge += 1;
        }
    }

    let p_value = (count_ge + 1) as f64 / (permutations + 1) as f64;
    Ok(PermanovaResult {
        pseudo_f: round4(observed_f),
        p_value: round4(p_value),
        permutations,
        significant: p_value < 0.05,
    })
}

pub struct SimperRow {
    pub code: String,
    pub avg_contribution: f64,
    pub pct_contribution: f64,
    pub cumulative_pct: f64,
}

/// SIMPER — species contributions to average Bray-Curtis dissimilarity
/// between two groups, sorted from largest to smallest.
pub fn simper(
    matrix: &DMatrix<f64>,
    code_names: &[String],
    group_labels: &[String],
    group_a: &str,
    group_b: &str,
) -> Vec<SimperRow> {
    let idx_a: Vec<usize> = group_labels.iter().enumerate().filter(|(_, g)| *g == group_a).map(|(i, _)| i).collect();
    let idx_b: Vec<usize> = group_labels.iter().enumerate().filter(|(_, g)| *g == group_b).map(|(i, _)| i).collect();
    if idx_a.is_empty() || idx_b.is_empty() {
        return Vec::new();
    }

    let n_codes = code_names.len();
    let mut contributions = vec![0.0; n_codes];
    let n_pairs = (idx_a.len() * idx_b.len()) as f64;

    for &i in &idx_a {
        for &j in &idx_b {
            let row_i = matrix.row(i);
            let row_j = matrix.row(j);
            let denom: f64 = row_i.sum() + row_j.sum();
            if denom > 0.0 {
                for k in 0..n_codes {
                    contributions[k] += (row_i[k] - row_j[k]).abs() / denom;
                }
            }
        }
    }
    for c in &mut contributions {
        *c /= n_pairs;
    }

    let total: f64 = contributions.iter().sum();
    let mut order: Vec<usize> = (0..n_codes).collect();
    order.sort_by(|&a, &b| contributions[b].partial_cmp(&contributions[a]).unwrap());

    let mut cumulative = 0.0;
    let mut rows = Vec::with_capacity(n_codes);
    for idx in order {
        let contrib = contributions[idx];
        let pct = if total > 0.0 { round2(contrib / total * 100.0) } else { 0.0 };
        cumulative += pct;
        rows.push(SimperRow {
            code: code_names[idx].clone(),
            avg_contribution: round6(contrib),
            pct_contribution: pct,
            cumulative_pct: round2(cumulative),
        });
    }
    rows
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn round4(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}

fn round6(v: f64) -> f64 {
    (v * 1_000_000.0).round() / 1_000_000.0
}
