use crate::math::{SplitMix64, mean, norm_cdf, norm_inv, skewness, variance};

#[test]
fn test_norm_cdf_and_inv() {
    assert!((norm_cdf(0.0) - 0.5).abs() < 1e-5);
    assert!((norm_cdf(1.96) - 0.975002).abs() < 1e-4);
    assert!((norm_cdf(-1.96) - 0.024998).abs() < 1e-4);

    assert!((norm_inv(0.5) - 0.0).abs() < 1e-5);
    assert!((norm_inv(0.975) - 1.95996).abs() < 1e-4);
    assert!((norm_inv(0.025) - (-1.95996)).abs() < 1e-4);
}

#[test]
fn test_moments() {
    let xs = [1.0, 2.0, 3.0, 4.0, 5.0];
    assert_eq!(mean(&xs), 3.0);
    assert_eq!(variance(&xs), 2.5);
    assert_eq!(skewness(&xs), 0.0);
}

#[test]
fn test_splitmix_determinism() {
    let mut rng1 = SplitMix64::new(42);
    let mut rng2 = SplitMix64::new(42);
    for _ in 0..100 {
        assert_eq!(rng1.next_u64(), rng2.next_u64());
    }
}
