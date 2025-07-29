mod mollusk_adapter;
mod test_account_length_limits;
mod test_account_parsing;
mod test_account_validation;
mod test_address_derivation;
mod test_extension_size_exhaustive;
mod test_extension_size_validation;
mod test_extension_utils;
mod test_idemp_oncurve_attack;
mod test_instruction_builders;
mod test_mollusk_non_canonical_bump;
mod test_utils;

// Migrated tests from /program/tests
mod migrated {
    pub mod create_idempotent;
    pub mod extended_mint;
    pub mod process_create_associated_token_account;
    pub mod recover_nested;
    pub mod spl_token_create;
}

include!(concat!(env!("OUT_DIR"), "/generated_tests.rs"));
