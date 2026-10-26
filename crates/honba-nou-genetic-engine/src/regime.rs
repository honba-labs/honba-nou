//! StockSharp Analytics Port: Market Regime Analytics
//!
//! Provides statistical market state classifiers:
//! - Hidden Markov Models (HMM) for bull / bear / sideways regime detection via Viterbi decoding.
//! - Gaussian Mixture Models (GMM) with Expectation-Maximization (EM) clustering.
//! - K-Means clustering on volatility, trend slope, and order book imbalance.
//! - Regime-conditional rule execution.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Market regimes detectable by statistical classifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DetectedRegime {
    Bull,
    Bear,
    Sideways,
    HighVolatility,
    LowVolatility,
    #[default]
    Unknown,
}

impl DetectedRegime {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Bull => "bull",
            Self::Bear => "bear",
            Self::Sideways => "sideways",
            Self::HighVolatility => "high_volatility",
            Self::LowVolatility => "low_volatility",
            Self::Unknown => "unknown",
        }
    }
}

/// Market features for regime classification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketFeatures {
    pub trend_slope: Decimal,
    pub volatility: Decimal,
    pub order_book_imbalance: Decimal,
    pub mean_reversion_score: Decimal,
    pub volume_ratio: Decimal,
}

impl MarketFeatures {
    pub fn to_f64_slice(&self) -> [f64; 5] {
        [
            self.trend_slope.to_string().parse::<f64>().unwrap_or(0.0),
            self.volatility.to_string().parse::<f64>().unwrap_or(0.0),
            self.order_book_imbalance
                .to_string()
                .parse::<f64>()
                .unwrap_or(0.0),
            self.mean_reversion_score
                .to_string()
                .parse::<f64>()
                .unwrap_or(0.0),
            self.volume_ratio.to_string().parse::<f64>().unwrap_or(0.0),
        ]
    }
}

/// Detected regime result with confidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegimeDetectionResult {
    pub regime: DetectedRegime,
    pub confidence: f64,
}

/// Simple heuristic regime detection based on thresholds.
pub fn detect_regime(features: &MarketFeatures) -> DetectedRegime {
    let trending = features.trend_slope > Decimal::ZERO;
    let bear = features.trend_slope < Decimal::new(-1, 2);
    let high_vol = features.volatility > Decimal::new(25, 3); // 2.5%

    if high_vol {
        DetectedRegime::HighVolatility
    } else if trending {
        DetectedRegime::Bull
    } else if bear {
        DetectedRegime::Bear
    } else {
        DetectedRegime::Sideways
    }
}

/// Hidden Markov Model (HMM) for regime classification using Gaussian emissions and Viterbi decoding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HmmClassifier {
    pub n_states: usize,
    /// State mapping (e.g. 0 -> Bull, 1 -> Bear, 2 -> Sideways)
    pub states: Vec<DetectedRegime>,
    /// Initial state probabilities
    pub initial_probs: Vec<f64>,
    /// Transition probability matrix (row-major n_states x n_states)
    pub transition_matrix: Vec<Vec<f64>>,
    /// Gaussian emission mean per state
    pub emission_means: Vec<f64>,
    /// Gaussian emission standard deviation per state
    pub emission_stds: Vec<f64>,
}

impl Default for HmmClassifier {
    fn default() -> Self {
        Self::standard_three_state()
    }
}

impl HmmClassifier {
    /// Constructs a standard calibrated 3-state HMM:
    /// State 0: Bull (mean return +0.015, std 0.008)
    /// State 1: Bear (mean return -0.015, std 0.010)
    /// State 2: Sideways (mean return 0.0000, std 0.004)
    pub fn standard_three_state() -> Self {
        Self {
            n_states: 3,
            states: vec![
                DetectedRegime::Bull,
                DetectedRegime::Bear,
                DetectedRegime::Sideways,
            ],
            initial_probs: vec![0.333, 0.333, 0.334],
            transition_matrix: vec![
                vec![0.85, 0.05, 0.10], // Bull -> [Bull, Bear, Sideways]
                vec![0.05, 0.85, 0.10], // Bear -> [Bull, Bear, Sideways]
                vec![0.15, 0.15, 0.70], // Sideways -> [Bull, Bear, Sideways]
            ],
            emission_means: vec![0.015, -0.015, 0.0000],
            emission_stds: vec![0.008, 0.010, 0.004],
        }
    }

    /// Gaussian probability density function: N(x; mu, sigma).
    fn gaussian_pdf(x: f64, mean: f64, std: f64) -> f64 {
        if std <= 1e-8 {
            return 1e-8;
        }
        let exponent = -0.5 * ((x - mean) / std).powi(2);
        (1.0 / (std * (2.0 * std::f64::consts::PI).sqrt())) * exponent.exp()
    }

    /// Viterbi decoding algorithm: finds most likely sequence of hidden states given observations.
    pub fn viterbi(&self, observations: &[f64]) -> Vec<DetectedRegime> {
        if observations.is_empty() {
            return Vec::new();
        }

        let t_len = observations.len();
        let k = self.n_states;

        // viterbi table (t_len x k) of log probabilities
        let mut v = vec![vec![f64::NEG_INFINITY; k]; t_len];
        let mut backpointer = vec![vec![0usize; k]; t_len];

        // Initialization at t = 0
        for i in 0..k {
            let emission = Self::gaussian_pdf(
                observations[0],
                self.emission_means[i],
                self.emission_stds[i],
            );
            let prob = (self.initial_probs[i] * emission).max(1e-300);
            v[0][i] = prob.ln();
        }

        // Recursion for t = 1..t_len
        for t in 1..t_len {
            for j in 0..k {
                let emission = Self::gaussian_pdf(
                    observations[t],
                    self.emission_means[j],
                    self.emission_stds[j],
                );
                let emission_log = emission.max(1e-300).ln();

                let mut max_log_prob = f64::NEG_INFINITY;
                let mut best_prev_state = 0;

                for i in 0..k {
                    let trans_log = self.transition_matrix[i][j].max(1e-300).ln();
                    let log_prob = v[t - 1][i] + trans_log + emission_log;
                    if log_prob > max_log_prob {
                        max_log_prob = log_prob;
                        best_prev_state = i;
                    }
                }

                v[t][j] = max_log_prob;
                backpointer[t][j] = best_prev_state;
            }
        }

        // Termination: find state with max prob at t = t_len - 1
        let mut best_final_state = 0;
        let mut max_final_prob = f64::NEG_INFINITY;
        for i in 0..k {
            if v[t_len - 1][i] > max_final_prob {
                max_final_prob = v[t_len - 1][i];
                best_final_state = i;
            }
        }

        // Path backtracking
        let mut path = vec![0usize; t_len];
        path[t_len - 1] = best_final_state;
        for t in (1..t_len).rev() {
            path[t - 1] = backpointer[t][path[t]];
        }

        path.into_iter().map(|idx| self.states[idx]).collect()
    }

    /// Predict the current regime from recent observation sequence.
    pub fn predict_current_regime(&self, observations: &[f64]) -> DetectedRegime {
        let path = self.viterbi(observations);
        path.last().copied().unwrap_or(DetectedRegime::Unknown)
    }
}

/// Gaussian Mixture Model (GMM) Component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GmmComponent {
    pub weight: f64,
    pub mean: f64,
    pub variance: f64,
    pub regime: DetectedRegime,
}

/// Gaussian Mixture Model classifier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GmmClassifier {
    pub components: Vec<GmmComponent>,
}

impl GmmClassifier {
    /// Fit a GMM with EM algorithm on scalar feature observations.
    pub fn fit(observations: &[f64], n_components: usize, max_iter: usize) -> Self {
        if observations.is_empty() || n_components == 0 {
            return Self {
                components: Vec::new(),
            };
        }

        let n = observations.len() as f64;
        let min_val = observations.iter().copied().fold(f64::INFINITY, f64::min);
        let max_val = observations
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let span = (max_val - min_val).max(1e-4);

        // Initialize components evenly across the data range
        let mut weights = vec![1.0 / n_components as f64; n_components];
        let mut means: Vec<f64> = (0..n_components)
            .map(|i| min_val + (i as f64 + 0.5) * (span / n_components as f64))
            .collect();
        let mut variances =
            vec![(span / (2.0 * n_components as f64)).powi(2).max(1e-4); n_components];

        // EM loop
        for _ in 0..max_iter {
            // E-step: responsibilities gamma[i][k]
            let mut responsibilities = vec![vec![0.0; n_components]; observations.len()];
            for (i, &x) in observations.iter().enumerate() {
                let mut sum_prob = 0.0;
                for k in 0..n_components {
                    let std = variances[k].sqrt();
                    let p = HmmClassifier::gaussian_pdf(x, means[k], std);
                    responsibilities[i][k] = weights[k] * p;
                    sum_prob += responsibilities[i][k];
                }
                if sum_prob > 1e-300 {
                    for k in 0..n_components {
                        responsibilities[i][k] /= sum_prob;
                    }
                }
            }

            // M-step: update parameters
            for k in 0..n_components {
                let n_k: f64 = responsibilities.iter().map(|r| r[k]).sum();
                if n_k > 1e-8 {
                    weights[k] = n_k / n;
                    means[k] = responsibilities
                        .iter()
                        .enumerate()
                        .map(|(i, r)| r[k] * observations[i])
                        .sum::<f64>()
                        / n_k;
                    let var: f64 = responsibilities
                        .iter()
                        .enumerate()
                        .map(|(i, r)| r[k] * (observations[i] - means[k]).powi(2))
                        .sum::<f64>()
                        / n_k;
                    variances[k] = var.max(1e-6);
                }
            }
        }

        let regimes = [
            DetectedRegime::Bear,
            DetectedRegime::Sideways,
            DetectedRegime::Bull,
        ];
        let components = (0..n_components)
            .map(|k| GmmComponent {
                weight: weights[k],
                mean: means[k],
                variance: variances[k],
                regime: regimes[k % regimes.len()],
            })
            .collect();

        Self { components }
    }

    /// Predict the most likely regime component for an observation.
    pub fn predict(&self, x: f64) -> RegimeDetectionResult {
        if self.components.is_empty() {
            return RegimeDetectionResult {
                regime: DetectedRegime::Unknown,
                confidence: 0.0,
            };
        }

        let mut best_prob = -1.0;
        let mut best_regime = DetectedRegime::Unknown;
        let mut sum_prob = 0.0;

        for comp in &self.components {
            let p = comp.weight * HmmClassifier::gaussian_pdf(x, comp.mean, comp.variance.sqrt());
            sum_prob += p;
            if p > best_prob {
                best_prob = p;
                best_regime = comp.regime;
            }
        }

        let confidence = if sum_prob > 1e-12 {
            (best_prob / sum_prob).min(1.0)
        } else {
            0.5
        };

        RegimeDetectionResult {
            regime: best_regime,
            confidence,
        }
    }
}

/// K-Means Clustering on multi-dimensional feature space.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KMeansClassifier {
    pub n_clusters: usize,
    pub centroids: Vec<Vec<f64>>,
    pub cluster_regimes: Vec<DetectedRegime>,
}

impl KMeansClassifier {
    /// Fit K-Means on multi-dimensional feature rows using Lloyd's algorithm.
    pub fn fit(data: &[Vec<f64>], n_clusters: usize, max_iter: usize) -> Self {
        if data.is_empty() || n_clusters == 0 {
            return Self {
                n_clusters: 0,
                centroids: Vec::new(),
                cluster_regimes: Vec::new(),
            };
        }

        let dim = data[0].len();
        let mut centroids: Vec<Vec<f64>> = (0..n_clusters)
            .map(|i| data[i % data.len()].clone())
            .collect();

        let mut assignments = vec![0usize; data.len()];

        for _ in 0..max_iter {
            let mut changed = false;

            for (i, row) in data.iter().enumerate() {
                let mut best_dist = f64::INFINITY;
                let mut best_c = 0;
                for (c_idx, c) in centroids.iter().enumerate() {
                    let dist: f64 = row.iter().zip(c.iter()).map(|(a, b)| (a - b).powi(2)).sum();
                    if dist < best_dist {
                        best_dist = dist;
                        best_c = c_idx;
                    }
                }
                if assignments[i] != best_c {
                    assignments[i] = best_c;
                    changed = true;
                }
            }

            if !changed {
                break;
            }

            for c_idx in 0..n_clusters {
                let mut sum = vec![0.0; dim];
                let mut count = 0;
                for (i, &assigned) in assignments.iter().enumerate() {
                    if assigned == c_idx {
                        for d in 0..dim {
                            sum[d] += data[i][d];
                        }
                        count += 1;
                    }
                }
                if count > 0 {
                    for d in 0..dim {
                        centroids[c_idx][d] = sum[d] / count as f64;
                    }
                }
            }
        }

        let regimes = [
            DetectedRegime::Bull,
            DetectedRegime::Bear,
            DetectedRegime::Sideways,
            DetectedRegime::HighVolatility,
        ];
        let cluster_regimes = (0..n_clusters)
            .map(|i| regimes[i % regimes.len()])
            .collect();

        Self {
            n_clusters,
            centroids,
            cluster_regimes,
        }
    }

    /// Predict cluster regime for a new feature row.
    pub fn predict(&self, features: &[f64]) -> DetectedRegime {
        if self.centroids.is_empty() {
            return DetectedRegime::Unknown;
        }

        let mut best_dist = f64::INFINITY;
        let mut best_c = 0;
        for (c_idx, c) in self.centroids.iter().enumerate() {
            let dist: f64 = features
                .iter()
                .zip(c.iter())
                .map(|(a, b)| (a - b).powi(2))
                .sum();
            if dist < best_dist {
                best_dist = dist;
                best_c = c_idx;
            }
        }

        self.cluster_regimes[best_c]
    }
}

/// Backwards compatibility functions matching previous interface.
pub fn fit_gmm(_features: &[MarketFeatures], n_components: usize) -> Vec<Value> {
    let obs: Vec<f64> = _features
        .iter()
        .map(|f| f.trend_slope.to_string().parse::<f64>().unwrap_or(0.0))
        .collect();
    let gmm = GmmClassifier::fit(&obs, n_components, 10);
    gmm.components
        .into_iter()
        .map(|c| {
            serde_json::json!({
                "weight": c.weight,
                "mean": c.mean,
                "covariance": c.variance,
                "regime": c.regime
            })
        })
        .collect()
}

pub fn kmeans_cluster(features: &[MarketFeatures], n_clusters: usize, n_iterations: u32) -> Value {
    let data: Vec<Vec<f64>> = features.iter().map(|f| f.to_f64_slice().to_vec()).collect();
    let kmeans = KMeansClassifier::fit(&data, n_clusters, n_iterations as usize);
    let assignments: Vec<usize> = data
        .iter()
        .map(|row| {
            let mut best_dist = f64::INFINITY;
            let mut best_c = 0;
            for (c_idx, c) in kmeans.centroids.iter().enumerate() {
                let dist: f64 = row.iter().zip(c.iter()).map(|(a, b)| (a - b).powi(2)).sum();
                if dist < best_dist {
                    best_dist = dist;
                    best_c = c_idx;
                }
            }
            best_c
        })
        .collect();

    serde_json::json!({
        "cluster_assignments": assignments,
        "n_clusters": kmeans.n_clusters,
        "centroids": kmeans.centroids
    })
}
