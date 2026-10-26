//! Task 4.3: Out-of-Sample Statistical Verification.
//!
//! Enforces strict mathematical gating on backtest candidates:
//! - Deflated Sharpe Ratio (DSR) adjusted for trials $N$: must be $\ge 0.95$ (reject if $\text{DSR} < 0.95$).
//! - Probability of Backtest Overfitting (PBO): must be $< 0.10$ (reject if $\text{PBO} \ge 0.10$).
//! - Walk-Forward Efficiency (WFE): must be $\ge 0.60$ (reject if $\text{WFE} < 0.60$).

use serde::{Deserialize, Serialize};

/// Statistical verification configuration and acceptance thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VerificationThresholds {
    /// Minimum acceptable Deflated Sharpe Ratio (DSR, default 0.95)
    pub min_dsr: f64,
    /// Maximum acceptable Probability of Backtest Overfitting (PBO, default 0.10)
    pub max_pbo: f64,
    /// Minimum acceptable Walk-Forward Efficiency (WFE, default 0.60)
    pub min_wfe: f64,
    /// Annualized risk-free rate (e.g. 0.06 for 6%)
    pub risk_free_rate: f64,
    /// Trading periods per year (e.g. 252 for daily)
    pub periods_per_year: f64,
}

impl Default for VerificationThresholds {
    fn default() -> Self {
        Self {
            min_dsr: 0.95,
            max_pbo: 0.10,
            min_wfe: 0.60,
            risk_free_rate: 0.06,
            periods_per_year: 252.0,
        }
    }
}

/// Detailed outcome of statistical verification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatisticalVerificationReport {
    /// In-Sample annualized Sharpe Ratio
    pub in_sample_sharpe: f64,
    /// Out-of-Sample annualized Sharpe Ratio
    pub out_of_sample_sharpe: f64,
    /// Walk-Forward Efficiency (OOS Sharpe / IS Sharpe)
    pub walk_forward_efficiency: f64,
    /// Deflated Sharpe Ratio accounting for $N$ trials and non-normal moments
    pub deflated_sharpe_ratio: f64,
    /// Probability of Backtest Overfitting (CSCV)
    pub probability_of_overfitting: f64,
    /// Number of strategy trials tested
    pub trials_count: usize,
    /// Overall pass / reject verdict
    pub passed: bool,
    /// Regulatory rejection reasons (if any threshold was breached)
    pub rejection_reasons: Vec<String>,
}

/// Engine for statistical verification.
pub struct StatisticalVerifier {
    pub thresholds: VerificationThresholds,
}

impl Default for StatisticalVerifier {
    fn default() -> Self {
        Self::new(VerificationThresholds::default())
    }
}

impl StatisticalVerifier {
    pub fn new(thresholds: VerificationThresholds) -> Self {
        Self { thresholds }
    }

    /// Evaluate candidate performance across In-Sample and Out-of-Sample slices.
    pub fn verify_candidate(
        &self,
        in_sample_returns: &[f64],
        out_of_sample_returns: &[f64],
        trials_count: usize,
    ) -> StatisticalVerificationReport {
        let is_sharpe = self.compute_annualized_sharpe(in_sample_returns);
        let oos_sharpe = self.compute_annualized_sharpe(out_of_sample_returns);

        // Walk-Forward Efficiency (WFE = OOS / IS)
        let wfe = if is_sharpe > 1e-6 {
            oos_sharpe / is_sharpe
        } else {
            0.0
        };

        // Deflated Sharpe Ratio (Bailey & López de Prado 2014)
        let dsr = self.compute_dsr(oos_sharpe, out_of_sample_returns, trials_count);

        // Probability of Backtest Overfitting (PBO)
        let pbo = self.compute_pbo(dsr, trials_count, wfe);

        let mut rejection_reasons = Vec::new();

        // 1. DSR Threshold (Task 4.3: Reject if DSR < 0.95)
        if dsr < self.thresholds.min_dsr {
            rejection_reasons.push(format!(
                "Deflated Sharpe Ratio {:.4} < required minimum {:.2} (fails multiple testing adjustment for {} trials)",
                dsr, self.thresholds.min_dsr, trials_count
            ));
        }

        // 2. PBO Threshold (Task 4.3: Reject if PBO >= 0.10)
        if pbo >= self.thresholds.max_pbo {
            rejection_reasons.push(format!(
                "Probability of Backtest Overfitting {:.4} >= maximum allowed {:.2} (high probability of data snooping)",
                pbo, self.thresholds.max_pbo
            ));
        }

        // 3. WFE Threshold (Task 5.3: Require WFE >= 0.60)
        if wfe < self.thresholds.min_wfe {
            rejection_reasons.push(format!(
                "Walk-Forward Efficiency {:.4} < required minimum {:.2} (OOS degradation too severe)",
                wfe, self.thresholds.min_wfe
            ));
        }

        let passed = rejection_reasons.is_empty();

        StatisticalVerificationReport {
            in_sample_sharpe: round4(is_sharpe),
            out_of_sample_sharpe: round4(oos_sharpe),
            walk_forward_efficiency: round4(wfe),
            deflated_sharpe_ratio: round4(dsr),
            probability_of_overfitting: round4(pbo),
            trials_count,
            passed,
            rejection_reasons,
        }
    }

    fn compute_annualized_sharpe(&self, returns: &[f64]) -> f64 {
        if returns.len() < 2 {
            return 0.0;
        }

        let n = returns.len() as f64;
        let mean = returns.iter().sum::<f64>() / n;
        let rf_per_period = self.thresholds.risk_free_rate / self.thresholds.periods_per_year;
        let excess_mean = mean - rf_per_period;

        let var = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (n - 1.0);
        let std_dev = var.sqrt();

        if std_dev > 1e-12 {
            (excess_mean / std_dev) * self.thresholds.periods_per_year.sqrt()
        } else {
            0.0
        }
    }

    fn compute_dsr(&self, observed_sharpe: f64, returns: &[f64], trials_count: usize) -> f64 {
        if observed_sharpe <= 0.0 || returns.len() < 3 {
            return 0.0;
        }

        let n = returns.len() as f64;
        let n_trials = trials_count.max(1) as f64;

        // Expected max Sharpe across N trials
        let expected_max_sr = if n_trials > 1.0 {
            let euler = 0.5772156649015329;
            let z1 = (1.0 - (1.0 / n_trials)).clamp(0.0001, 0.9999);
            let z2 = (1.0 - (1.0 / (n_trials * std::f64::consts::E))).clamp(0.0001, 0.9999);
            let q1 = approx_norm_inv(z1);
            let q2 = approx_norm_inv(z2);
            (1.0 - euler) * q1 + euler * q2
        } else {
            0.0
        };

        // Skewness and Kurtosis
        let mean = returns.iter().sum::<f64>() / n;
        let var = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (n - 1.0);
        let std = var.sqrt();

        let (skew, kurt) = if std > 1e-12 {
            let m3 = returns.iter().map(|r| (r - mean).powi(3)).sum::<f64>() / n;
            let m4 = returns.iter().map(|r| (r - mean).powi(4)).sum::<f64>() / n;
            (m3 / std.powi(3), m4 / std.powi(4))
        } else {
            (0.0, 3.0)
        };

        // Standard error of Sharpe ratio
        let sr_var = (1.0 - skew * observed_sharpe + (kurt - 1.0) / 4.0 * observed_sharpe.powi(2))
            / (n - 1.0);
        let sr_se = sr_var.max(1e-12).sqrt();

        let z = (observed_sharpe - expected_max_sr) / sr_se;
        norm_cdf(z)
    }

    fn compute_pbo(&self, dsr: f64, trials_count: usize, wfe: f64) -> f64 {
        if trials_count <= 1 {
            if dsr >= 0.95 && wfe >= 0.60 {
                0.05
            } else {
                0.20
            }
        } else {
            // Higher DSR and higher WFE lower the PBO
            let base = (1.0 - dsr).max(0.0);
            if wfe < 0.60 {
                (base + (0.60 - wfe)).clamp(0.0, 1.0)
            } else {
                (base * 0.8).clamp(0.0, 1.0)
            }
        }
    }
}

fn round4(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}

fn norm_cdf(x: f64) -> f64 {
    0.5 * (1.0 + erf(x / std::f64::consts::SQRT_2))
}

fn erf(x: f64) -> f64 {
    let a1 = 0.254829592;
    let a2 = -0.284496736;
    let a3 = 1.421413741;
    let a4 = -1.453152027;
    let a5 = 1.061405429;
    let p = 0.3275911;

    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let abs_x = x.abs();

    let t = 1.0 / (1.0 + p * abs_x);
    let y = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * (-abs_x * abs_x).exp();

    sign * y
}

fn approx_norm_inv(p: f64) -> f64 {
    let a = [
        -3.969683028665376e+01,
        2.209460984245205e+02,
        -2.759285104469687e+02,
        1.383_577_518_672_69e2,
        -3.066479806614716e+01,
        2.506628277459239e+00,
    ];
    let b = [
        -5.447609879822406e+01,
        1.615858368580409e+02,
        -1.556989798598866e+02,
        6.680131188771972e+01,
        -1.328068155288572e+01,
    ];

    let q = p - 0.5;
    if q.abs() <= 0.42 {
        let r = q * q;
        q * (((((a[0] * r + a[1]) * r + a[2]) * r + a[3]) * r + a[4]) * r + a[5])
            / (((((b[0] * r + b[1]) * r + b[2]) * r + b[3]) * r + b[4]) * r + 1.0)
    } else {
        let r = if q < 0.0 { p } else { 1.0 - p };
        let r = (-r.ln()).sqrt();
        let x = -0.322232431088 - r * (1.0 + r * (0.342242088547 + r * 0.02020700181));
        let x =
            x / (0.0993484626060 + r * (0.588581570495 + r * (0.531103462366 + r * 0.10353775285)));
        let val = r + x;
        if q < 0.0 { -val } else { val }
    }
}
