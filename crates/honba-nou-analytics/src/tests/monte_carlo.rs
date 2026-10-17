use crate::monte_carlo::{MonteCarloConfig, run_block_bootstrap};

#[test]
fn test_bootstrap_seeded() {
    let returns: Vec<f64> = (0..200)
        .map(|i| if i % 2 == 0 { 0.015 } else { -0.010 })
        .collect();

    let cfg = MonteCarloConfig {
        num_simulations: 200,
        seed: 12345,
        ..Default::default()
    };

    let res1 = run_block_bootstrap(&returns, cfg);
    let res2 = run_block_bootstrap(&returns, cfg);

    // Identical seed must yield byte-for-byte identical outcomes
    assert_eq!(res1.median_max_drawdown, res2.median_max_drawdown);
    assert_eq!(res1.drawdown_95, res2.drawdown_95);
    assert_eq!(res1.drawdown_99, res2.drawdown_99);
    assert_eq!(res1.prob_loss, res2.prob_loss);
    assert_eq!(res1.prob_ruin, res2.prob_ruin);
    assert_eq!(res1.passed, res2.passed);
}
