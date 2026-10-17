use crate::dsr::{DsrConfig, compute_dsr, expected_max_sharpe};

#[test]
fn test_expected_max_sharpe_single_trial() {
    assert_eq!(expected_max_sharpe(1, 1.0), 0.0);
}

#[test]
fn test_dsr_known_values() {
    // N=100 trials, trial variance = 1.0
    let sr0 = expected_max_sharpe(100, 1.0);
    // Approximately 2.50 to 2.55 per Bailey & López de Prado table 1
    assert!(sr0 > 2.40 && sr0 < 2.65, "Expected ~2.5, got {sr0}");

    // Normal returns with length 250
    let returns = vec![0.01; 250];
    let cfg = DsrConfig {
        num_trials: 100,
        trial_sharpe_variance: 1.0,
        threshold: 0.95,
    };

    // If observed Sharpe is well above expected maximum (e.g. 3.5), DSR passes
    let res_high = compute_dsr(3.5, &returns, cfg);
    assert!(res_high.passed);
    assert!(res_high.dsr > 0.99);

    // If observed Sharpe is below expected maximum (e.g. 1.5 < 2.5), DSR fails
    let res_low = compute_dsr(1.5, &returns, cfg);
    assert!(!res_low.passed);
    assert!(res_low.dsr < 0.01);
}
