use crate::pbo::{PboConfig, compute_pbo, generate_combinations};

#[test]
fn test_combinations_count() {
    let combs = generate_combinations(8, 4);
    // 8 choose 4 = 70
    assert_eq!(combs.len(), 70);
}

#[test]
fn test_pbo_calculation() {
    // Model 1: consistently good
    let m1 = vec![0.01; 160];
    // Model 2: pure noise
    let m2: Vec<f64> = (0..160)
        .map(|i| if i % 2 == 0 { 0.02 } else { -0.02 })
        .collect();

    let trials = vec![m1, m2];
    let res = compute_pbo(
        &trials,
        PboConfig {
            num_slices: 4,
            ..Default::default()
        },
    );
    assert_eq!(res.combinations_evaluated, 6); // 4 choose 2 = 6
    assert!(res.passed);
}
