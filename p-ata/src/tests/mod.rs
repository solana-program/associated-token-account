mod mollusk_adapter;
mod test_account_length_limits;
mod test_account_parsing;
mod test_account_validation;
mod test_address_derivation;
mod test_extension_size_validation;
mod test_instruction_builders;
mod test_mollusk_non_canonical_bump;
mod test_utils;

include!(concat!(env!("OUT_DIR"), "/generated_tests.rs"));
