//! This module contains all the tests for the program.
//! It is not top-level marked as #[cfg(test)] since
//! helpers are used by the benches module.

// Organized test modules
pub mod bump;
pub mod token_account_len;

// Always re-export utils when benchmarks/tests are enabled (including benches build)
#[cfg(any(test, feature = "std"))]
pub use utils::*;

pub mod utils {
    pub mod mollusk_adapter;
}

#[cfg(test)]
// Migrated tests from /program/tests
mod migrated;

// Original tests from /program/tests
include!(concat!(env!("OUT_DIR"), "/generated_tests.rs"));
