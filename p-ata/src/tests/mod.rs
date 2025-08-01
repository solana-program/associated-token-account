pub mod test_account_parsing;
pub mod test_account_validation;
pub mod test_address_derivation;
pub mod test_instruction_builders;

// Organized test modules
pub mod bump;
pub mod token_account_len;

// Benchmark modules - compiled unconditionally so that benchmarks have access to their helpers
pub mod benches;

// Always re-export test_utils when benchmarks/tests are enabled (including benches build)
#[cfg(any(test, feature = "std"))]
pub use utils::test_utils::*;
#[cfg(any(test, feature = "std"))]
pub use utils::*;

pub mod utils {
    pub mod account_builder;
    pub mod address_gen;
    pub mod mollusk_adapter;
    pub mod test_utils;
}

// Migrated tests from /program/tests
mod migrated {
    pub mod create_idempotent;
    pub mod extended_mint;
    pub mod process_create_associated_token_account;
    pub mod recover_nested;
    pub mod spl_token_create;
}

// Original tests from /program/tests
include!(concat!(env!("OUT_DIR"), "/generated_tests.rs"));
