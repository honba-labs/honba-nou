use crate::walk_forward::{WalkForwardConfig, compute_walk_forward};

#[test]
fn test_walk_forward_consistent_strategy() {
    let returns = vec![0.001; 100];
    let res = compute_walk_forward(&returns, WalkForwardConfig::default());
    assert!(res.passed);
    assert!((res.wfe - 1.0).abs() < 0.1);
}

#[test]
fn test_walk_forward_overfitted_strategy() {
    let mut returns = vec![0.005; 60];
    returns.extend(vec![-0.002; 40]);
    let res = compute_walk_forward(&returns, WalkForwardConfig::default());
    assert!(!res.passed);
    assert!(res.wfe < 0.0);
}
