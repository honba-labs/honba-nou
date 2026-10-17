use crate::validation::ValidationBlock;

#[test]
fn test_result_carries_validation_block() {
    let returns = vec![0.002; 150];
    let block = ValidationBlock::evaluate_backtest(&returns, 2.5, 1);
    assert!(block.dsr.is_some());
    assert!(block.walk_forward.is_some());
    assert!(block.holdout.is_some());
    assert!(block.monte_carlo.is_some());
    assert!(block.cost_stress.is_some());
    assert!(block.passed);
    assert!(block.warnings.is_empty());
}
