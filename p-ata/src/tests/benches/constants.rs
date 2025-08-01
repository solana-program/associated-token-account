#![cfg_attr(feature = "std", allow(dead_code, unused_imports))]

// Re-export shared constants to maintain compatibility while using unified values
pub use crate::tests::test_utils::shared_constants::*;

/// Lamport amounts used in tests
pub mod lamports {
    pub use crate::tests::test_utils::shared_constants::*;
}

/// Account data sizes used in tests
pub mod account_sizes {
    pub use crate::tests::test_utils::shared_constants::*;
}
