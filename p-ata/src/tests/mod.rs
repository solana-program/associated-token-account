pub mod mollusk_adapter;
pub mod test_account_parsing;
pub mod test_account_validation;
pub mod test_address_derivation;
pub mod test_instruction_builders;
pub mod test_utils;

// Organized test modules
pub mod bump;
pub mod token_account_len;

#[cfg(test)]
pub(crate) use test_utils::*;

// Migrated tests from /program/tests
mod migrated {
    pub mod create_idempotent;
    pub mod extended_mint;
    pub mod process_create_associated_token_account;
    pub mod recover_nested;
    pub mod spl_token_create;
}

include!(concat!(env!("OUT_DIR"), "/generated_tests.rs"));
