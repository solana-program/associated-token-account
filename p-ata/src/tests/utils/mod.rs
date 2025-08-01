#![cfg(test)]

pub mod address_gen;
pub mod account_builder;
pub mod test_utils;
pub mod mollusk_adapter;

// Re-export core address generation utilities for convenience
pub use address_gen::{
    const_pk_with_optimal_bump, derive_address_with_bump, find_optimal_wallet_for_mints,
    find_optimal_wallet_for_nested_ata, is_off_curve, random_seeded_pk, structured_pk,
    structured_pk_multi,
};