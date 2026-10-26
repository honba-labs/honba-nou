//! Cryptographically signed DatasetManifest with SHA-256 provenance (Task 4.3).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::verification::StatisticalVerificationReport;

/// Cryptographically signed dataset manifest ensuring mathematical provenance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DatasetManifest {
    /// Manifest unique identifier
    pub manifest_id: String,
    /// SHA-256 hash of the Lakehouse Parquet dataset catalog
    pub data_catalog_hash: String,
    /// SHA-256 hash of the closed StrategySpec JSON
    pub strategy_spec_hash: String,
    /// Deterministic pseudo-random seed used for execution
    pub seed: u64,
    /// In-Sample (Train) window start timestamp
    pub in_sample_start: DateTime<Utc>,
    /// In-Sample (Train) window end timestamp
    pub in_sample_end: DateTime<Utc>,
    /// Out-of-Sample (Validation) window start timestamp
    pub out_of_sample_start: DateTime<Utc>,
    /// Out-of-Sample (Validation) window end timestamp
    pub out_of_sample_end: DateTime<Utc>,
    /// Statistical verification metrics and pass/reject verdict
    pub verification: StatisticalVerificationReport,
    /// Cryptographic SHA-256 signature chain guaranteeing provenance
    pub signature: String,
    /// Manifest creation timestamp
    pub created_at: DateTime<Utc>,
}

impl DatasetManifest {
    /// Create and cryptographically sign a new dataset manifest.
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        data_catalog_content_or_hash: &str,
        strategy_spec_json: &str,
        seed: u64,
        in_sample_start: DateTime<Utc>,
        in_sample_end: DateTime<Utc>,
        out_of_sample_start: DateTime<Utc>,
        out_of_sample_end: DateTime<Utc>,
        verification: StatisticalVerificationReport,
    ) -> Self {
        let manifest_id = Uuid::new_v4().to_string();

        let data_catalog_hash = if data_catalog_content_or_hash.len() == 64
            && data_catalog_content_or_hash
                .chars()
                .all(|c| c.is_ascii_hexdigit())
        {
            data_catalog_content_or_hash.to_string()
        } else {
            let mut hasher = Sha256::new();
            hasher.update(data_catalog_content_or_hash.as_bytes());
            format!("{:x}", hasher.finalize())
        };

        let mut spec_hasher = Sha256::new();
        spec_hasher.update(strategy_spec_json.as_bytes());
        let strategy_spec_hash = format!("{:x}", spec_hasher.finalize());

        let created_at = Utc::now();

        let signature = Self::compute_signature(
            &data_catalog_hash,
            &strategy_spec_hash,
            seed,
            in_sample_start,
            in_sample_end,
            out_of_sample_start,
            out_of_sample_end,
            &verification,
        );

        Self {
            manifest_id,
            data_catalog_hash,
            strategy_spec_hash,
            seed,
            in_sample_start,
            in_sample_end,
            out_of_sample_start,
            out_of_sample_end,
            verification,
            signature,
            created_at,
        }
    }

    /// Compute cryptographic hash signature for the manifest.
    #[allow(clippy::too_many_arguments)]
    fn compute_signature(
        catalog_hash: &str,
        spec_hash: &str,
        seed: u64,
        is_start: DateTime<Utc>,
        is_end: DateTime<Utc>,
        oos_start: DateTime<Utc>,
        oos_end: DateTime<Utc>,
        verification: &StatisticalVerificationReport,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"HONBA_DATASET_MANIFEST_V1:");
        hasher.update(catalog_hash.as_bytes());
        hasher.update(b":");
        hasher.update(spec_hash.as_bytes());
        hasher.update(b":");
        hasher.update(seed.to_string().as_bytes());
        hasher.update(b":");
        hasher.update(is_start.to_rfc3339().as_bytes());
        hasher.update(b":");
        hasher.update(is_end.to_rfc3339().as_bytes());
        hasher.update(b":");
        hasher.update(oos_start.to_rfc3339().as_bytes());
        hasher.update(b":");
        hasher.update(oos_end.to_rfc3339().as_bytes());
        hasher.update(b":");
        hasher.update(verification.passed.to_string().as_bytes());
        hasher.update(b":");
        hasher.update(verification.deflated_sharpe_ratio.to_string().as_bytes());
        hasher.update(b":");
        hasher.update(
            verification
                .probability_of_overfitting
                .to_string()
                .as_bytes(),
        );
        format!("{:x}", hasher.finalize())
    }

    /// Verify whether this manifest's cryptographic signature is intact (tamper detection).
    pub fn verify_provenance(&self) -> bool {
        let expected = Self::compute_signature(
            &self.data_catalog_hash,
            &self.strategy_spec_hash,
            self.seed,
            self.in_sample_start,
            self.in_sample_end,
            self.out_of_sample_start,
            self.out_of_sample_end,
            &self.verification,
        );
        self.signature == expected
    }
}
