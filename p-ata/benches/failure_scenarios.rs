mod common;
use common::*;
use pinocchio_ata_program::test_utils::{load_program_ids, AtaImplementation, AtaVariant};

use {
    common::{
        BaseTestType, BenchmarkResult, BenchmarkRunner, BenchmarkSetup, ComparisonResult,
        CompatibilityStatus, TestVariant,
    },
    common_builders::{CommonTestCaseBuilder, FailureMode},
    constants::account_sizes,
    pinocchio_ata_program::{
        debug_log,
        test_helpers::address_gen::{
            random_seeded_pk, structured_pk, structured_pk_multi, AccountTypeId, TestBankId,
        },
        test_utils::{account_builder::AccountBuilder, shared_constants::NATIVE_LOADER_ID},
    },
    solana_account::Account,
    solana_instruction::{AccountMeta, Instruction},
    solana_logger,
    solana_pubkey::Pubkey,
    std::{
        boxed::Box,
        format, println,
        string::{String, ToString},
        vec,
        vec::Vec,
    },
    strum::Display,
};

const FAKE_SYSTEM_PROGRAM_ID: Pubkey = Pubkey::new_from_array([1u8; 32]);
const FAKE_TOKEN_PROGRAM_ID: Pubkey = Pubkey::new_from_array([2u8; 32]);

/// Configuration for a failure test case
#[derive(Clone)]
struct FailureTestConfig {
    name: &'static str,
    category: TestCategory,
    base_test: BaseTestType,
    variant: TestVariant,
    failure_mode: FailureMode,
    builder_type: TestBuilderType,
}

#[derive(Clone, Display)]
#[strum(serialize_all = "title_case")]
enum TestCategory {
    #[strum(to_string = "Basic Account Ownership Failure Tests")]
    BasicAccountOwnership,
    #[strum(to_string = "Address Derivation and Structure Failure Tests")]
    AddressDerivationStructure,
    #[strum(to_string = "Recovery Operation Failure Tests")]
    RecoveryOperations,
    #[strum(to_string = "Additional Validation Coverage Tests")]
    AdditionalValidation,
}

#[derive(Clone)]
enum TestBuilderType {
    /// Use the CommonTestCaseBuilder with the specified failure mode
    Simple,
    /// Use custom logic - these need individual functions
    Custom,
}

// Helper functions for test configuration to reduce repetition
fn basic_create_test(
    name: &'static str,
    category: TestCategory,
    failure_mode: FailureMode,
) -> FailureTestConfig {
    FailureTestConfig {
        name,
        category,
        base_test: BaseTestType::Create,
        variant: TestVariant::BASE,
        failure_mode,
        builder_type: TestBuilderType::Simple,
    }
}

fn basic_idempotent_test(
    name: &'static str,
    category: TestCategory,
    failure_mode: FailureMode,
) -> FailureTestConfig {
    FailureTestConfig {
        name,
        category,
        base_test: BaseTestType::CreateIdempotent,
        variant: TestVariant::BASE,
        failure_mode,
        builder_type: TestBuilderType::Simple,
    }
}

fn recovery_test(
    name: &'static str,
    base_test: BaseTestType,
    failure_mode: FailureMode,
) -> FailureTestConfig {
    FailureTestConfig {
        name,
        category: TestCategory::RecoveryOperations,
        base_test,
        variant: TestVariant::BASE,
        failure_mode,
        builder_type: TestBuilderType::Simple,
    }
}

fn bump_test(
    name: &'static str,
    category: TestCategory,
    failure_mode: FailureMode,
) -> FailureTestConfig {
    FailureTestConfig {
        name,
        category,
        base_test: BaseTestType::Create,
        variant: TestVariant {
            bump_arg: true,
            ..TestVariant::BASE
        },
        failure_mode,
        builder_type: TestBuilderType::Simple,
    }
}

/// Get all failure test configurations
fn get_failure_tests() -> Vec<FailureTestConfig> {
    vec![
        // Basic Account Ownership Failure Tests
        basic_create_test(
            "fail_wrong_payer_owner",
            TestCategory::BasicAccountOwnership,
            FailureMode::WrongPayerOwner(FAKE_TOKEN_PROGRAM_ID),
        ),
        basic_create_test(
            "fail_payer_not_signed",
            TestCategory::BasicAccountOwnership,
            FailureMode::PayerNotSigned,
        ),
        basic_create_test(
            "fail_wrong_system_program",
            TestCategory::BasicAccountOwnership,
            FailureMode::WrongSystemProgram(FAKE_SYSTEM_PROGRAM_ID),
        ),
        basic_create_test(
            "fail_wrong_token_program",
            TestCategory::BasicAccountOwnership,
            FailureMode::WrongTokenProgram(FAKE_TOKEN_PROGRAM_ID),
        ),
        basic_create_test(
            "fail_insufficient_funds",
            TestCategory::BasicAccountOwnership,
            FailureMode::InsufficientFunds(1000),
        ),
        // Address Derivation and Structure Failure Tests
        FailureTestConfig {
            name: "fail_wrong_ata_address",
            category: TestCategory::AddressDerivationStructure,
            base_test: BaseTestType::Create,
            variant: TestVariant::BASE,
            failure_mode: FailureMode::WrongAtaAddress(
                // This will be dynamically generated in the builder
                Pubkey::new_from_array([173u8; 32]), // Placeholder
            ),
            builder_type: TestBuilderType::Custom, // Needs dynamic address generation
        },
        basic_create_test(
            "fail_mint_wrong_owner",
            TestCategory::AddressDerivationStructure,
            FailureMode::MintWrongOwner(solana_system_interface::program::id()),
        ),
        basic_create_test(
            "fail_invalid_mint_structure",
            TestCategory::AddressDerivationStructure,
            FailureMode::InvalidMintStructure(50),
        ),
        basic_idempotent_test(
            "fail_invalid_token_account_structure",
            TestCategory::AddressDerivationStructure,
            FailureMode::InvalidTokenAccountStructure,
        ),
        basic_create_test(
            "fail_invalid_discriminator",
            TestCategory::AddressDerivationStructure,
            FailureMode::InvalidDiscriminator(99),
        ),
        bump_test(
            "fail_invalid_bump_value",
            TestCategory::AddressDerivationStructure,
            FailureMode::InvalidBumpValue(99),
        ),
        // Recovery Operation Failure Tests
        recovery_test(
            "fail_recover_wallet_not_signer",
            BaseTestType::RecoverNested,
            FailureMode::RecoverWalletNotSigner,
        ),
        recovery_test(
            "fail_recover_multisig_insufficient_signers",
            BaseTestType::RecoverMultisig,
            FailureMode::RecoverMultisigInsufficientSigners,
        ),
        FailureTestConfig {
            name: "fail_recover_multisig_duplicate_signers",
            category: TestCategory::RecoveryOperations,
            base_test: BaseTestType::RecoverMultisig,
            variant: TestVariant::BASE,
            failure_mode: FailureMode::RecoverMultisigDuplicateSigners,
            builder_type: TestBuilderType::Custom,
        },
        FailureTestConfig {
            name: "fail_recover_multisig_non_signer_account",
            category: TestCategory::RecoveryOperations,
            base_test: BaseTestType::RecoverMultisig,
            variant: TestVariant::BASE,
            failure_mode: FailureMode::RecoverMultisigNonSignerAccount,
            builder_type: TestBuilderType::Custom,
        },
        recovery_test(
            "fail_recover_multisig_wrong_wallet_owner",
            BaseTestType::RecoverMultisig,
            FailureMode::RecoverMultisigWrongWalletOwner(solana_system_interface::program::id()),
        ),
        FailureTestConfig {
            name: "fail_recover_wrong_nested_ata_address",
            category: TestCategory::RecoveryOperations,
            base_test: BaseTestType::RecoverNested,
            variant: TestVariant::BASE,
            failure_mode: FailureMode::RecoverWrongNestedAta(Pubkey::new_from_array([0u8; 32])), // Placeholder
            builder_type: TestBuilderType::Custom, // Has complex custom logic
        },
        FailureTestConfig {
            name: "fail_recover_wrong_destination_address",
            category: TestCategory::RecoveryOperations,
            base_test: BaseTestType::RecoverNested,
            variant: TestVariant::BASE,
            failure_mode: FailureMode::RecoverWrongDestination(Pubkey::new_from_array([0u8; 32])), // Placeholder
            builder_type: TestBuilderType::Custom, // Has complex custom logic
        },
        // Additional Validation Coverage Tests
        basic_create_test(
            "fail_ata_owned_by_system_program",
            TestCategory::AdditionalValidation,
            FailureMode::AtaWrongOwner(solana_system_interface::program::id()),
        ),
        FailureTestConfig {
            name: "fail_wrong_token_account_size",
            category: TestCategory::AdditionalValidation,
            base_test: BaseTestType::CreateIdempotent,
            variant: TestVariant::BASE,
            failure_mode: FailureMode::TokenAccountWrongSize(100),
            builder_type: TestBuilderType::Custom, // Has custom account setup
        },
        FailureTestConfig {
            name: "fail_token_account_wrong_mint",
            category: TestCategory::AdditionalValidation,
            base_test: BaseTestType::CreateIdempotent,
            variant: TestVariant::BASE,
            failure_mode: FailureMode::TokenAccountWrongMint(Pubkey::new_from_array([0u8; 32])), // Placeholder
            builder_type: TestBuilderType::Custom, // Has custom account setup
        },
        FailureTestConfig {
            name: "fail_token_account_wrong_owner",
            category: TestCategory::AdditionalValidation,
            base_test: BaseTestType::CreateIdempotent,
            variant: TestVariant::BASE,
            failure_mode: FailureMode::TokenAccountWrongOwner(Pubkey::new_from_array([0u8; 32])), // Placeholder
            builder_type: TestBuilderType::Custom, // Has custom account setup
        },
        basic_create_test(
            "fail_immutable_account",
            TestCategory::AdditionalValidation,
            FailureMode::AtaNotWritable,
        ),
        FailureTestConfig {
            name: "fail_drain_lamports_from_uninitialized_ata",
            category: TestCategory::AdditionalValidation,
            base_test: BaseTestType::Create,
            variant: TestVariant::BASE,
            failure_mode: FailureMode::AtaAddressMismatchLamportDrain,
            builder_type: TestBuilderType::Custom,
        },
        // Additional Validation: Using Token-v1 program with an extended (Token-2022 style) mint
        FailureTestConfig {
            name: "fail_create_extended_mint_v1",
            category: TestCategory::AdditionalValidation,
            base_test: BaseTestType::Create,
            variant: TestVariant::BASE,
            // failure_mode placeholder – actual mutation done in custom builder
            failure_mode: FailureMode::InvalidMintStructure(98),
            builder_type: TestBuilderType::Custom,
        },
    ]
}

/// Log test information for debugging - only shown with --full-debug-logs feature
#[allow(unused)]
fn log_test_info(test_name: &str, ata_impl: &AtaImplementation, addresses: &[(&str, &Pubkey)]) {
    let short_addresses: Vec<String> = addresses
        .iter()
        .map(|(name, address)| format!("{}: {}", name, &address.to_string()[0..8]))
        .collect();

    debug_log!(
        "🔍 Test: {} | Implementation: {} | {}",
        test_name,
        ata_impl.name,
        short_addresses.join(" | ")
    );

    let full_addresses: Vec<String> = addresses
        .iter()
        .map(|(name, address)| format!("{}: {}", name, address))
        .collect();

    debug_log!("    Full addresses: {}", full_addresses.join(" | "));
}

// Helper function for complex cases that need custom logic
fn build_base_failure_accounts(
    base_test: BaseTestType,
    variant: TestVariant,
    ata_implementation: &AtaImplementation,
) -> (Pubkey, Pubkey, Pubkey) {
    let test_number = common_builders::calculate_failure_test_number(base_test, variant);

    let payer = structured_pk(
        &ata_implementation.variant,
        TestBankId::Failures,
        test_number,
        AccountTypeId::Payer,
    );

    // Use consistent variant for mint and wallet to enable byte-for-byte comparison
    let consistent_variant = &AtaVariant::SplAta;
    let mint = structured_pk(
        consistent_variant,
        TestBankId::Failures,
        test_number,
        AccountTypeId::Mint,
    );
    let simple_entropy = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let wallet = random_seeded_pk(
        consistent_variant,
        TestBankId::Failures,
        test_number,
        AccountTypeId::Wallet,
        42, // Fixed seed ensures consistency across implementations
        simple_entropy,
    );

    (payer, mint, wallet)
}

/// Holds the set of accounts used in RecoverNested scenarios.
struct RecoverNestedAccounts {
    nested_ata: Pubkey,
    nested_mint: Pubkey,
    dest_ata: Pubkey,
    owner_ata: Pubkey,
    owner_mint: Pubkey,
    wallet: Pubkey,
}

impl RecoverNestedAccounts {
    /// Creates a new set of accounts for RecoverNested tests.
    fn new(ata_impl: &AtaImplementation) -> Self {
        let test_number = common_builders::calculate_failure_test_number(
            BaseTestType::RecoverNested,
            TestVariant::BASE,
        );
        let pubkeys = structured_pk_multi(
            &ata_impl.variant,
            TestBankId::Failures,
            test_number,
            &[
                AccountTypeId::NestedAta,
                AccountTypeId::NestedMint,
                AccountTypeId::Ata, // dest_ata
                AccountTypeId::OwnerAta,
                AccountTypeId::OwnerMint,
                AccountTypeId::Wallet,
            ],
        );
        let [nested_ata, nested_mint, dest_ata, owner_ata, owner_mint, wallet]: [Pubkey; 6] =
            pubkeys
                .try_into()
                .expect("structured_pk_multi should return exactly 6 pubkeys");
        Self {
            nested_ata,
            nested_mint,
            dest_ata,
            owner_ata,
            owner_mint,
            wallet,
        }
    }
}

struct FailureTestBuilder;

impl FailureTestBuilder {
    /// Build a failure test case from configuration
    fn build_failure_test(
        config: &FailureTestConfig,
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        match config.builder_type {
            TestBuilderType::Simple => CommonTestCaseBuilder::build_failure_test_case_with_name(
                config.base_test,
                config.variant,
                ata_impl,
                token_program_id,
                config.failure_mode.clone(),
                config.name,
            ),
            TestBuilderType::Custom => {
                // Route to the appropriate custom builder
                match config.name {
                    "fail_wrong_ata_address" => {
                        Self::build_fail_wrong_ata_address(ata_impl, token_program_id)
                    }
                    "fail_recover_wrong_nested_ata_address" => {
                        Self::build_fail_recover_wrong_nested_ata_address(
                            ata_impl,
                            token_program_id,
                        )
                    }
                    "fail_recover_wrong_destination_address" => {
                        Self::build_fail_recover_wrong_destination_address(
                            ata_impl,
                            token_program_id,
                        )
                    }
                    "fail_wrong_token_account_size" => {
                        Self::build_fail_wrong_token_account_size(ata_impl, token_program_id)
                    }
                    "fail_token_account_wrong_mint" => {
                        Self::build_fail_token_account_wrong_mint(ata_impl, token_program_id)
                    }
                    "fail_token_account_wrong_owner" => {
                        Self::build_fail_token_account_wrong_owner(ata_impl, token_program_id)
                    }
                    "fail_create_extended_mint_v1" => {
                        Self::build_fail_create_extended_mint_v1(ata_impl, token_program_id)
                    }
                    "fail_recover_multisig_duplicate_signers" => {
                        Self::build_fail_recover_multisig_duplicate_signers(
                            ata_impl,
                            token_program_id,
                        )
                    }
                    "fail_recover_multisig_non_signer_account" => {
                        Self::build_fail_recover_multisig_non_signer_account(
                            ata_impl,
                            token_program_id,
                        )
                    }
                    "fail_drain_lamports_from_uninitialized_ata" => {
                        Self::build_fail_drain_lamports_from_uninitialized_ata(
                            ata_impl,
                            token_program_id,
                        )
                    }
                    _ => panic!("Unknown custom test: {}", config.name),
                }
            }
        }
    }

    /// Custom builder for wrong ATA address test
    fn build_fail_wrong_ata_address(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let wrong_ata_address = structured_pk(
            &ata_impl.variant,
            TestBankId::Failures,
            173,
            AccountTypeId::Ata,
        );

        CommonTestCaseBuilder::build_failure_test_case_with_name(
            BaseTestType::Create,
            TestVariant::BASE,
            ata_impl,
            token_program_id,
            FailureMode::WrongAtaAddress(wrong_ata_address),
            "fail_wrong_ata_address",
        )
    }

    /// Generic helper for RecoverNested failure tests
    fn build_recover_nested_failure<F>(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
        test_name: &'static str,
        mutator: F,
    ) -> (Instruction, Vec<(Pubkey, Account)>)
    where
        F: FnOnce(&mut RecoverNestedAccounts, &mut Vec<u8>),
    {
        let mut accounts_struct = RecoverNestedAccounts::new(ata_impl);
        let mut instruction_data = vec![2u8]; // Base RecoverNested instruction

        // Apply the custom mutation to accounts or instruction data
        mutator(&mut accounts_struct, &mut instruction_data);

        log_test_info(
            test_name,
            ata_impl,
            &[
                ("nested_ata", &accounts_struct.nested_ata),
                ("nested_mint", &accounts_struct.nested_mint),
                ("dest_ata", &accounts_struct.dest_ata),
                ("owner_ata", &accounts_struct.owner_ata),
                ("owner_mint", &accounts_struct.owner_mint),
                ("wallet", &accounts_struct.wallet),
            ],
        );

        let accounts = account_templates::RecoverAccountSet::new(
            accounts_struct.nested_ata,
            accounts_struct.nested_mint,
            accounts_struct.dest_ata,
            accounts_struct.owner_ata,
            accounts_struct.owner_mint,
            accounts_struct.wallet,
            token_program_id,
            100, // token amount
        )
        .to_vec();

        let ix = Instruction {
            program_id: ata_impl.program_id,
            accounts: vec![
                AccountMeta::new(accounts_struct.nested_ata, false),
                AccountMeta::new_readonly(accounts_struct.nested_mint, false),
                AccountMeta::new(accounts_struct.dest_ata, false),
                AccountMeta::new(accounts_struct.owner_ata, false),
                AccountMeta::new_readonly(accounts_struct.owner_mint, false),
                AccountMeta::new(accounts_struct.wallet, true),
                AccountMeta::new_readonly(*token_program_id, false),
                AccountMeta::new_readonly(Pubkey::from(spl_token_interface::program::ID), false),
            ],
            data: instruction_data,
        };

        (ix, accounts)
    }

    /// Custom builder for recover wrong nested ATA address test
    fn build_fail_recover_wrong_nested_ata_address(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        Self::build_recover_nested_failure(
            ata_impl,
            token_program_id,
            "fail_recover_wrong_nested_ata_address",
            |accs, _data| {
                let test_number = common_builders::calculate_failure_test_number(
                    BaseTestType::RecoverNested,
                    TestVariant::BASE,
                );
                // Overwrite the nested_ata with a new, different key to force a mismatch.
                accs.nested_ata = structured_pk(
                    &ata_impl.variant,
                    TestBankId::Failures,
                    test_number.wrapping_add(10), // Use a distinct offset to guarantee a different address
                    AccountTypeId::NestedAta,
                );
            },
        )
    }

    /// Custom builder for recover wrong destination address test
    fn build_fail_recover_wrong_destination_address(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        Self::build_recover_nested_failure(
            ata_impl,
            token_program_id,
            "fail_recover_wrong_destination_address",
            |accs, _data| {
                let test_number = common_builders::calculate_failure_test_number(
                    BaseTestType::RecoverNested,
                    TestVariant::BASE,
                );
                // Overwrite the dest_ata with a new, different key to force a mismatch.
                accs.dest_ata = structured_pk(
                    &ata_impl.variant,
                    TestBankId::Failures,
                    test_number.wrapping_add(11), // Use a distinct offset to guarantee a different address
                    AccountTypeId::Ata,
                );
            },
        )
    }

    /// Generic helper for CreateIdempotent failure tests that have an existing ATA
    fn build_create_idempotent_failure_test<F>(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
        test_name: &'static str,
        failure_applicator: F,
    ) -> (Instruction, Vec<(Pubkey, Account)>)
    where
        F: FnOnce(
            &mut Vec<(Pubkey, Account)>,
            &Pubkey, // ata
            &Pubkey, // mint
            &Pubkey, // wallet
            &AtaImplementation,
        ),
    {
        let (payer, mint, wallet) = build_base_failure_accounts(
            BaseTestType::CreateIdempotent,
            TestVariant::BASE,
            ata_impl,
        );

        log_test_info(
            test_name,
            ata_impl,
            &[("payer", &payer), ("mint", &mint), ("wallet", &wallet)],
        );

        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            &ata_impl.program_id,
        );

        let mut accounts =
            account_templates::StandardAccountSet::new(payer, ata, wallet, mint, token_program_id)
                .with_existing_ata(&mint, &wallet, token_program_id)
                .to_vec();

        // Apply the specific failure condition to the accounts
        failure_applicator(&mut accounts, &ata, &mint, &wallet, ata_impl);

        let ix = Instruction {
            program_id: ata_impl.program_id,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new(ata, false),
                AccountMeta::new_readonly(wallet, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(solana_system_interface::program::id(), false),
                AccountMeta::new_readonly(*token_program_id, false),
            ],
            data: vec![1u8], // CreateIdempotent instruction
        };

        (ix, accounts)
    }

    /// Custom builder for wrong token account size test
    fn build_fail_wrong_token_account_size(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        Self::build_create_idempotent_failure_test(
            ata_impl,
            token_program_id,
            "fail_wrong_token_account_size",
            |accounts, ata, _mint, _wallet, _ata_impl| {
                // Apply failure: set ATA to wrong size
                account_templates::FailureAccountBuilder::set_wrong_data_size(accounts, *ata, 100);
            },
        )
    }

    /// Custom builder for token account wrong mint test
    fn build_fail_token_account_wrong_mint(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        Self::build_create_idempotent_failure_test(
            ata_impl,
            token_program_id,
            "fail_token_account_wrong_mint",
            |accounts, ata, _mint, wallet, ata_impl| {
                let test_number = common_builders::calculate_failure_test_number(
                    BaseTestType::CreateIdempotent,
                    TestVariant::BASE,
                );
                let wrong_mint = structured_pk(
                    &ata_impl.variant,
                    TestBankId::Failures,
                    test_number.wrapping_add(10),
                    AccountTypeId::Mint,
                );

                // Replace ATA with one pointing to wrong mint
                if let Some(pos) = accounts.iter().position(|(address, _)| *address == *ata) {
                    accounts[pos].1 =
                        AccountBuilder::token_account(&wrong_mint, &wallet, 0, &token_program_id);
                }

                // Add the wrong mint account
                accounts.push((wrong_mint, AccountBuilder::mint(0, &token_program_id)));
            },
        )
    }

    /// Custom builder for token account wrong owner test
    fn build_fail_token_account_wrong_owner(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        Self::build_create_idempotent_failure_test(
            ata_impl,
            token_program_id,
            "fail_token_account_wrong_owner",
            |accounts, ata, mint, _wallet, ata_impl| {
                let test_number = common_builders::calculate_failure_test_number(
                    BaseTestType::CreateIdempotent,
                    TestVariant::BASE,
                );
                let wrong_owner = structured_pk(
                    &ata_impl.variant,
                    TestBankId::Failures,
                    test_number.wrapping_add(11),
                    AccountTypeId::Wallet,
                );

                // Replace ATA with one having wrong owner
                if let Some(pos) = accounts.iter().position(|(address, _)| *address == *ata) {
                    accounts[pos].1 =
                        AccountBuilder::token_account(mint, &wrong_owner, 0, &token_program_id);
                }
            },
        )
    }

    /// Custom builder: use original Token program but provide an extended (Token-2022 style) mint
    fn build_fail_create_extended_mint_v1(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        // Start from a standard, passing create test case
        let (ix, mut accounts) = CommonTestCaseBuilder::build_test_case(
            BaseTestType::Create,
            TestVariant::BASE,
            ata_impl,
            token_program_id,
        );

        // Mutate the existing mint account into an "extended" mint by
        // appending an ImmutableOwner TLV header (4-byte discriminator + padding).
        if let Some((_key, mint_acct)) = accounts.get_mut(3) {
            let mut new_data = mint_acct.data.clone();

            // Ensure starting from the canonical 82-byte layout.
            if new_data.len() != account_sizes::MINT_ACCOUNT_SIZE {
                new_data.truncate(account_sizes::MINT_ACCOUNT_SIZE);
            }

            // Increase length to 98 bytes and write the 4-byte TLV header (ImmutableOwner = 7).
            let required_len = account_sizes::MINT_ACCOUNT_SIZE + 16; // header + padding
            new_data.resize(required_len, 0u8);
            new_data[account_sizes::MINT_ACCOUNT_SIZE..account_sizes::MINT_ACCOUNT_SIZE + 4]
                .copy_from_slice(&[7u8, 0u8, 0u8, 0u8]);

            mint_acct.data = new_data;
        }
        (ix, accounts)
    }

    /// Custom builder for multisig duplicate signers vulnerability test
    /// This test exploits the vulnerability where the same signer can be counted multiple times
    fn build_fail_recover_multisig_duplicate_signers(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        // Start with a standard RecoverMultisig test case
        let (mut ix, accounts) = CommonTestCaseBuilder::build_test_case(
            BaseTestType::RecoverMultisig,
            TestVariant::BASE,
            ata_impl,
            token_program_id,
        );

        log_test_info(
            "fail_recover_multisig_duplicate_signers",
            ata_impl,
            &[("wallet", &ix.accounts[5].pubkey)],
        );

        // The standard RecoverMultisig test creates a 2-of-3 multisig with 2 signers
        // Correct instruction layout:
        // 0: nested_ata, 1: nested_mint, 2: dest_ata, 3: owner_ata, 4: owner_mint,
        // 5: wallet, 6: token_program, 7: signer1, 8: signer2

        // We'll exploit the vulnerability by replacing the second signer with the first signer
        // This should allow us to bypass the 2-of-3 requirement with only 1 actual signer
        if ix.accounts.len() >= 9 {
            let first_signer = ix.accounts[7].pubkey;
            // Replace the second signer with the first signer (duplicate)
            ix.accounts[8].pubkey = first_signer;
            // Make sure both are marked as signers
            ix.accounts[7].is_signer = true;
            ix.accounts[8].is_signer = true;
        }

        (ix, accounts)
    }

    /// Custom builder for multisig non-signer account test
    /// This test passes a multisig account but doesn't mark it as a signer
    fn build_fail_recover_multisig_non_signer_account(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        // Start with a standard RecoverMultisig test case
        let (mut ix, accounts) = CommonTestCaseBuilder::build_test_case(
            BaseTestType::RecoverMultisig,
            TestVariant::BASE,
            ata_impl,
            token_program_id,
        );

        log_test_info(
            "fail_recover_multisig_non_signer_account",
            ata_impl,
            &[("wallet", &ix.accounts[5].pubkey)],
        );

        // The standard RecoverMultisig test creates a 2-of-3 multisig with 2 signers
        // We'll modify it so that one of the required signers is not marked as a signer
        // This should fail because we don't have enough valid signers

        // Find the second signer account and mark it as NOT a signer
        if ix.accounts.len() > 8 {
            if let Some(second_signer_meta) = ix.accounts.get_mut(8) {
                second_signer_meta.is_signer = false; // This should cause the test to fail
            }
        }

        (ix, accounts)
    }

    /// Exploit test: Drain lamports from a valid PDA that was never initialized.
    ///
    /// This test attempts to exploit a potential vulnerability where an attacker could:
    /// 1. Find a valid ATA address (victim_ata) that has lamports but was never initialized
    /// 2. Use that ATA as the "payer" in a CreateAssociatedTokenAccount instruction
    /// 3. Create their own legitimate ATA (attacker_ata) using the victim's lamports
    ///
    /// The attack works by:
    /// - victim_ata: A valid PDA with lamports but owned by System Program (uninitialized)
    /// - attacker_ata: Attacker's legitimate ATA derived from their wallet + mint
    /// - The instruction uses victim_ata as payer, but creates attacker_ata as the target
    ///
    /// EXPECTED BEHAVIOR (if exploit succeeds):
    /// - victim_ata: 5_000_000 lamports → 3_000_000 lamports (loses 2_000_000 for rent-exempt balance)
    /// - attacker_ata: 0 lamports → 2_000_000 lamports (gains rent-exempt balance)
    ///
    /// SECURE BEHAVIOR (exploit should fail):
    /// - The ATA program should verify that the payer is properly derived from the instruction parameters
    /// - The instruction should fail with an error about invalid payer or address derivation
    /// - No lamports should be transferred
    fn build_fail_drain_lamports_from_uninitialized_ata(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let test_number =
            common_builders::calculate_failure_test_number(BaseTestType::Create, TestVariant::BASE);

        // Generate attacker, victim wallet, and victim mint efficiently
        let [attacker_wallet, victim_wallet, victim_mint] = [
            (test_number, AccountTypeId::Payer),
            (test_number.wrapping_add(10), AccountTypeId::Wallet),
            (test_number.wrapping_add(1), AccountTypeId::Mint),
        ]
        .map(|(num, account_type)| {
            structured_pk(&ata_impl.variant, TestBankId::Failures, num, account_type)
        });

        // Victim's ATA - properly derived PDA from victim's wallet and mint
        let (victim_ata, _victim_bump) = Pubkey::find_program_address(
            &[
                victim_wallet.as_ref(),
                token_program_id.as_ref(),
                victim_mint.as_ref(),
            ],
            &ata_impl.program_id,
        );

        // Attacker's ATA (properly derived from attacker's wallet and the mint)
        let (attacker_ata, attacker_bump) = Pubkey::find_program_address(
            &[
                attacker_wallet.as_ref(),
                token_program_id.as_ref(),
                victim_mint.as_ref(),
            ],
            &ata_impl.program_id,
        );

        log_test_info(
            "fail_drain_lamports_from_uninitialized_ata",
            ata_impl,
            &[
                ("attacker_wallet (tx payer)", &attacker_wallet),
                ("victim_wallet", &victim_wallet),
                ("victim_ata (instruction payer)", &victim_ata),
                ("attacker_ata (target)", &attacker_ata),
                ("attacker_mint", &victim_mint),
            ],
        );

        let accounts = vec![
            // Transaction payer (attacker)
            (
                attacker_wallet,
                AccountBuilder::system_account(1_000_000_000),
            ),
            // Victim's ATA with lamports but uninitialized
            // Should lose TOKEN_ACCOUNT_RENT_EXEMPT_BALANCE if exploit succeeds
            (
                victim_ata,
                Account {
                    lamports: 5_000_000, // Initial balance - should go to 3_000_000 if exploit succeeds
                    data: vec![],
                    owner: solana_system_interface::program::id(),
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            // Attacker's legitimate ATA (doesn't exist yet)
            // Should gain TOKEN_ACCOUNT_RENT_EXEMPT_BALANCE if exploit succeeds
            (attacker_ata, AccountBuilder::system_account(0)), // Should go to 2_000_000 if exploit succeeds
            // The victim's wallet (used as seed in instruction but not for derivation)
            (victim_wallet, AccountBuilder::system_account(0)),
            // Attacker mint
            (victim_mint, AccountBuilder::mint(0, &token_program_id)),
            // System program
            (
                solana_system_interface::program::id(),
                AccountBuilder::executable_program(NATIVE_LOADER_ID),
            ),
            // Token program
            (
                *token_program_id,
                AccountBuilder::executable_program(mollusk_svm::program::loader_keys::LOADER_V3),
            ),
        ];

        // The exploit: victim_ata pays for attacker_ata's creation
        let ix = Instruction {
            program_id: ata_impl.program_id,
            accounts: vec![
                AccountMeta::new(victim_ata, false), // payer within ATA instruction (has lamports, not a signer)
                AccountMeta::new(attacker_ata, false), // associated_token_account (attacker's legitimate ATA)
                AccountMeta::new_readonly(attacker_wallet, false), // wallet = seed for derivation
                AccountMeta::new_readonly(victim_mint, false), // attacker's chosen mint
                AccountMeta::new_readonly(solana_system_interface::program::id(), false),
                AccountMeta::new_readonly(*token_program_id, false),
            ],
            data: vec![0u8, attacker_bump], // Create with bump for attacker's ATA
        };

        (ix, accounts)
    }

    /// Helper to find account balance in account list
    fn find_account_balance(accounts: &[(Pubkey, Account)], target: &Pubkey) -> u64 {
        accounts
            .iter()
            .find(|(pubkey, _)| pubkey == target)
            .map(|(_, account)| account.lamports)
            .unwrap_or(0)
    }

    /// Verify the post-execution state for the drain lamports exploit test.
    /// This function should be called after the instruction is executed to verify
    /// that the lamport transfer occurred as expected (if the exploit succeeded).
    ///
    /// Returns (victim_final_balance, attacker_final_balance, transfer_occurred)
    fn verify_drain_lamports_exploit_result(
        pre_execution_accounts: &[(Pubkey, Account)],
        post_execution_accounts: &[(Pubkey, Account)],
        victim_ata: &Pubkey,
        attacker_ata: &Pubkey,
    ) -> (u64, u64, bool) {
        const INITIAL_VICTIM_BALANCE: u64 = 5_000_000;

        let (initial_victim_balance, initial_attacker_balance) = (
            Self::find_account_balance(pre_execution_accounts, victim_ata),
            Self::find_account_balance(pre_execution_accounts, attacker_ata),
        );
        let (final_victim_balance, final_attacker_balance) = (
            Self::find_account_balance(post_execution_accounts, victim_ata),
            Self::find_account_balance(post_execution_accounts, attacker_ata),
        );

        // Check if the expected transfer occurred
        let expected_transfer = initial_victim_balance == INITIAL_VICTIM_BALANCE
            && final_victim_balance < INITIAL_VICTIM_BALANCE
            && initial_attacker_balance == 0
            && final_attacker_balance > 0;

        // Always print the verification details
        println!("🔍 Drain Lamports Exploit Verification:");
        println!("  Victim ATA ({}):", victim_ata);
        println!("    Initial: {} lamports", initial_victim_balance);
        println!("    Final: {} lamports", final_victim_balance);
        println!("  Attacker ATA ({}):", attacker_ata);
        println!("    Initial: {} lamports", initial_attacker_balance);
        println!("    Final: {} lamports", final_attacker_balance);
        println!("  Transfer occurred as expected: {}", expected_transfer);

        (
            final_victim_balance,
            final_attacker_balance,
            expected_transfer,
        )
    }
}

struct FailureTestRunner;

impl FailureTestRunner {
    /// Print a single failure test result with detailed compatibility info
    fn print_single_failure_result(result: &ComparisonResult) {
        let (status_icon, status_text) = match result.compatibility_status {
            CompatibilityStatus::BothRejected => ("✅", "Both failed as expected (Same Error)"),
            CompatibilityStatus::Identical => {
                ("🚨", "Both succeeded (TEST ISSUE: should have failed)")
            }
            CompatibilityStatus::IncompatibleFailure => {
                ("⚠️", "Both failed but with DIFFERENT errors")
            }
            CompatibilityStatus::IncompatibleSuccess => {
                if result.p_ata.success && !result.spl_ata.success {
                    (
                        "🚨",
                        "P-ATA succeeded where SPL ATA failed (SECURITY ISSUE)",
                    )
                } else if !result.p_ata.success && result.spl_ata.success {
                    (
                        "🚨",
                        "SPL ATA succeeded where P-ATA failed (SECURITY ISSUE)",
                    )
                } else {
                    ("❓", "Incompatible success/failure status unknown")
                }
            }
            _ => ("❓", "Unexpected compatibility status"),
        };

        println!(
            "  {} {:<45} | {}",
            status_icon, result.test_name, status_text
        );

        if result.compatibility_status == CompatibilityStatus::IncompatibleFailure {
            if let (Some(p_err), Some(s_err)) =
                (&result.p_ata.error_message, &result.spl_ata.error_message)
            {
                println!("    - P-ATA Error: {}", p_err);
                println!("    - SPL ATA Error: {}", s_err);
            }
        }

        if !result.p_ata.captured_output.is_empty() {
            println!("    P-ATA Verification:");
            for line in result.p_ata.captured_output.lines() {
                println!("    {}", line);
            }
        }
        if !result.spl_ata.captured_output.is_empty() {
            println!("    SPL ATA Verification:");
            for line in result.spl_ata.captured_output.lines() {
                println!("    {}", line);
            }
        }
    }

    /// Run a failure test with configuration against both implementations and compare results.
    /// First, a baseline test is run to ensure the un-mutated case succeeds.
    fn run_failure_comparison_test_with_config(
        config: &FailureTestConfig,
        p_ata_impl: &AtaImplementation,
        original_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> ComparisonResult {
        // baseline sanity check: the un-mutated case must suceed

        let (baseline_ix, baseline_accounts) = CommonTestCaseBuilder::build_test_case(
            config.base_test,
            config.variant,
            p_ata_impl,
            token_program_id,
        );
        let baseline_result = BenchmarkRunner::run_single_benchmark(
            &format!("{}_baseline", config.name),
            &baseline_ix,
            &baseline_accounts,
            p_ata_impl,
            token_program_id,
            1,
        );
        assert!(
            baseline_result.success,
            "Baseline {} test should succeed",
            config.name
        );
        debug_log!("Baseline {} test succeeded", config.name);

        // Now mutate the test case.
        let test_builder = |ata_impl: &AtaImplementation, token_program_id: &Pubkey| {
            FailureTestBuilder::build_failure_test(config, ata_impl, token_program_id)
        };

        Self::run_failure_comparison_test(
            config.name,
            test_builder,
            p_ata_impl,
            original_impl,
            token_program_id,
        )
    }

    /// Run a failure test against both implementations and compare results
    fn run_failure_comparison_test<F>(
        name: &str,
        test_builder: F,
        p_ata_impl: &AtaImplementation,
        original_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> ComparisonResult
    where
        F: Fn(&AtaImplementation, &Pubkey) -> (Instruction, Vec<(Pubkey, Account)>),
    {
        // Check if this is a P-ATA-only test (uses bump args that original ATA doesn't support)
        let is_p_ata_only = name == "fail_invalid_bump_value";

        // Create verification function for P-ATA if needed
        let p_ata_verification = if name == "fail_drain_lamports_from_uninitialized_ata" {
            println!("🔍 Creating P-ATA verification function for drain lamports test");
            Some(Box::new(
                |pre_accounts: &[(Pubkey, Account)],
                 post_accounts: &[(Pubkey, Account)],
                 ix: &Instruction|
                 -> String {
                    println!("🔍 P-ATA Verification function called for drain lamports test");
                    let victim_ata = ix.accounts[0].pubkey;
                    let attacker_ata = ix.accounts[1].pubkey;

                    let (victim_final, attacker_final, transfer_occurred) =
                        FailureTestBuilder::verify_drain_lamports_exploit_result(
                            pre_accounts,
                            post_accounts,
                            &victim_ata,
                            &attacker_ata,
                        );

                    let result_msg = if transfer_occurred {
                        format!("🚨 EXPLOIT SUCCEEDED: Lamports transferred from victim to attacker!\n  Victim: 5,000,000 → {} lamports\n  Attacker: 0 → {} lamports", victim_final, attacker_final)
                    } else {
                        format!("✅ Exploit failed: No unexpected lamport transfer\n  Victim: {} lamports\n  Attacker: {} lamports", victim_final, attacker_final)
                    };

                    println!("🔍 P-ATA Verification result: {}", result_msg);
                    result_msg
                },
            ) as common::PostExecutionVerificationFn)
        } else {
            None
        };

        // Build P-ATA test case
        let (p_ata_ix, p_ata_accounts) = test_builder(p_ata_impl, token_program_id);
        let mut p_ata_result = BenchmarkRunner::run_single_benchmark_with_post_account_inspection(
            name,
            &p_ata_ix,
            &p_ata_accounts,
            p_ata_impl,
            token_program_id,
            p_ata_verification,
        );

        // Build comparison result
        let mut comparison_result = if is_p_ata_only {
            // For P-ATA-only tests, create a dummy result for original ATA
            let original_result = BenchmarkResult {
                implementation: original_impl.name.to_string(),
                test_name: name.to_string(),
                compute_units: 0,
                success: false,
                error_message: Some(
                    "N/A - Test not applicable to original ATA (uses P-ATA-specific bump args)"
                        .to_string(),
                ),
                captured_output: String::new(),
            };

            let mut result = BenchmarkRunner::create_comparison_result(
                name,
                p_ata_result.clone(),
                original_result,
            );
            result.compatibility_status = CompatibilityStatus::OptimizedBehavior;
            result
        } else {
            // Build Original ATA test case
            let (original_ix, original_accounts) = test_builder(original_impl, token_program_id);

            // Create verification function for Original ATA if needed
            let original_verification = if name == "fail_drain_lamports_from_uninitialized_ata" {
                println!("🔍 Creating SPL ATA verification function for drain lamports test");
                Some(Box::new(
                    |pre_accounts: &[(Pubkey, Account)],
                     post_accounts: &[(Pubkey, Account)],
                     ix: &Instruction|
                     -> String {
                        println!("🔍 SPL ATA Verification function called for drain lamports test");
                        let victim_ata = ix.accounts[0].pubkey;
                        let attacker_ata = ix.accounts[1].pubkey;

                        let (victim_final, attacker_final, transfer_occurred) =
                            FailureTestBuilder::verify_drain_lamports_exploit_result(
                                pre_accounts,
                                post_accounts,
                                &victim_ata,
                                &attacker_ata,
                            );

                        let result_msg = if transfer_occurred {
                            format!("🚨 EXPLOIT SUCCEEDED: Lamports transferred from victim to attacker!\n  Victim: 5,000,000 → {} lamports\n  Attacker: 0 → {} lamports", victim_final, attacker_final)
                        } else {
                            format!("✅ Exploit failed: No unexpected lamport transfer\n  Victim: {} lamports\n  Attacker: {} lamports", victim_final, attacker_final)
                        };

                        println!("🔍 SPL ATA Verification result: {}", result_msg);
                        result_msg
                    },
                ) as common::PostExecutionVerificationFn)
            } else {
                None
            };

            let original_result =
                BenchmarkRunner::run_single_benchmark_with_post_account_inspection(
                    name,
                    &original_ix,
                    &original_accounts,
                    original_impl,
                    token_program_id,
                    original_verification,
                );

            // Create comparison result
            BenchmarkRunner::create_comparison_result(name, p_ata_result.clone(), original_result)
        };

        // Check if we need debug logging for problematic results
        let needs_debug_logging = Self::is_problematic_result(&comparison_result);

        if needs_debug_logging {
            // Re-run with debug logging to capture verbose output
            p_ata_result = BenchmarkRunner::run_single_benchmark_with_debug(
                name,
                &p_ata_ix,
                &p_ata_accounts,
                p_ata_impl,
                token_program_id,
            );

            if !is_p_ata_only {
                // Also re-run original ATA with debug logging
                let (original_ix, original_accounts) =
                    test_builder(original_impl, token_program_id);
                let original_result = BenchmarkRunner::run_single_benchmark_with_debug(
                    name,
                    &original_ix,
                    &original_accounts,
                    original_impl,
                    token_program_id,
                );

                // Update comparison result with debug output
                comparison_result =
                    BenchmarkRunner::create_comparison_result(name, p_ata_result, original_result);
            } else {
                // For P-ATA-only tests, just update the P-ATA result
                comparison_result.p_ata = p_ata_result;
            }
        }

        comparison_result
    }

    /// Check if a comparison result is problematic and needs debug logging
    fn is_problematic_result(result: &ComparisonResult) -> bool {
        match result.compatibility_status {
            // Security issues - definitely need debug logs
            CompatibilityStatus::IncompatibleSuccess => true,
            // Both succeeded when they should fail - test issue
            CompatibilityStatus::Identical if result.p_ata.success && result.spl_ata.success => {
                true
            }
            // All other cases are expected or acceptable
            _ => false,
        }
    }

    /// Run comprehensive failure test comparison between implementations
    fn run_comprehensive_failure_comparison(
        p_ata_impl: &AtaImplementation,
        original_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> Vec<ComparisonResult> {
        println!("\n=== P-ATA VS ORIGINAL ATA FAILURE SCENARIOS COMPARISON ===");

        let mut results = Vec::new();

        // Group tests by category and run them in organized sections
        let failure_tests = get_failure_tests();
        let mut tests_by_category: std::collections::HashMap<String, Vec<&FailureTestConfig>> =
            std::collections::HashMap::new();

        for config in &failure_tests {
            let category_name = config.category.to_string();
            tests_by_category
                .entry(category_name)
                .or_default()
                .push(config);
        }

        // Run tests organized by category
        for (category_name, configs) in tests_by_category {
            println!("\n--- {} ---", category_name);

            for config in configs {
                let comparison = Self::run_failure_comparison_test_with_config(
                    config,
                    p_ata_impl,
                    original_impl,
                    token_program_id,
                );
                Self::print_single_failure_result(&comparison);
                results.push(comparison);
            }
        }

        Self::output_failure_test_data(&results);
        results
    }

    fn output_failure_test_data(results: &[ComparisonResult]) {
        let mut json_entries = Vec::new();

        for result in results {
            let status = match (&result.p_ata.success, &result.spl_ata.success) {
                (true, true) => "pass", // Both succeeded (might be unexpected for failure tests)
                (false, false) => {
                    // Both failed - check if errors are the same type
                    let p_ata_error = result.p_ata.error_message.as_deref().unwrap_or("Unknown");
                    let spl_ata_error =
                        result.spl_ata.error_message.as_deref().unwrap_or("Unknown");

                    // Simple error type comparison - look for key differences
                    if p_ata_error.contains("InvalidInstructionData")
                        != spl_ata_error.contains("InvalidInstructionData")
                        || p_ata_error.contains("Custom(") != spl_ata_error.contains("Custom(")
                        || p_ata_error.contains("PrivilegeEscalation")
                            != spl_ata_error.contains("PrivilegeEscalation")
                    {
                        "failed, but different error"
                    } else {
                        "failed with same error"
                    }
                }
                (true, false) => "pass", // P-ATA works, spl_ata fails (P-ATA optimization)
                (false, true) => "fail", // P-ATA fails, spl_ata works (concerning)
            };

            let p_ata_error_json = match &result.p_ata.error_message {
                Some(msg) => format!(r#""{}""#, msg.replace('"', r#"\""#)),
                None => "null".to_string(),
            };

            let spl_ata_error_json = match &result.spl_ata.error_message {
                Some(msg) => format!(r#""{}""#, msg.replace('"', r#"\""#)),
                None => "null".to_string(),
            };

            let entry = format!(
                r#"    "{}": {{
      "status": "{}",
      "p_ata_success": {},
      "spl_ata_success": {},
      "p_ata_error": {},
      "spl_ata_error": {},
      "type": "failure_test"
    }}"#,
                result.test_name,
                status,
                result.p_ata.success,
                result.spl_ata.success,
                p_ata_error_json,
                spl_ata_error_json
            );
            json_entries.push(entry);
        }

        let output = format!(
            r#"{{
  "timestamp": "{}",
  "failure_tests": {{
{}
  }}
}}"#,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            json_entries.join(",\n")
        );

        // Create benchmark_results directory if it doesn't exist
        std::fs::create_dir_all("benchmark_results").ok();

        // Write failure test results
        std::fs::write("benchmark_results/failure_results.json", output).unwrap();
    }

    /// Print P-ATA and SPL ATA results for a comparison
    fn print_result_comparison(result: &ComparisonResult, spl_label: &str) {
        for (impl_name, bench_result) in [("P-ATA", &result.p_ata), (spl_label, &result.spl_ata)] {
            if bench_result.success {
                println!("    {}:     Success", impl_name);
            } else {
                println!(
                    "    {}:     {}",
                    impl_name,
                    bench_result
                        .error_message
                        .as_deref()
                        .unwrap_or("Unknown error")
                );
            }
        }
    }

    /// Print failure test summary with compatibility analysis
    fn print_failure_summary(results: &[ComparisonResult]) {
        println!("\n--- FAILURE TEST SUMMARY ---");
        let total = results.len();
        let both_rejected = results
            .iter()
            .filter(|r| matches!(r.compatibility_status, CompatibilityStatus::BothRejected))
            .count();
        let incompatible_failures = results
            .iter()
            .filter(|r| {
                matches!(
                    r.compatibility_status,
                    CompatibilityStatus::IncompatibleFailure
                )
            })
            .count();
        let unexpected_success = results
            .iter()
            .filter(|r| {
                matches!(
                    r.compatibility_status,
                    CompatibilityStatus::IncompatibleSuccess
                )
            })
            .count();
        let both_succeeded = results
            .iter()
            .filter(|r| matches!(r.compatibility_status, CompatibilityStatus::Identical))
            .count();
        let optimized_behavior = results
            .iter()
            .filter(|r| {
                matches!(
                    r.compatibility_status,
                    CompatibilityStatus::OptimizedBehavior
                )
            })
            .count();

        println!("Total Failure Tests: {}", total);
        println!(
            "Both Implementations Failed as Expected (Same Errors): {}",
            both_rejected,
        );
        println!("Failed with Different Errors: {}", incompatible_failures,);
        println!(
            "Fails in p-ATA as Expected (SPL ATA not relevant): {}",
            optimized_behavior,
        );
        println!("**Unexpected Success/Failure**: {}", unexpected_success,);
        println!("**Both Succeeded Unexpectedly**: {}", both_succeeded,);

        if incompatible_failures > 0 || unexpected_success > 0 || optimized_behavior > 0 {
            println!("\n⚠️  TESTS WITH DIFFERENT BEHAVIORS:");
            for result in results
                .iter()
                .filter(|r| !matches!(r.compatibility_status, CompatibilityStatus::BothRejected))
            {
                match &result.compatibility_status {
                    CompatibilityStatus::IncompatibleFailure => {
                        println!("  {} - Different Error Messages:", result.test_name);
                        Self::print_result_comparison(result, "SPL ATA");
                    }
                    CompatibilityStatus::OptimizedBehavior => {
                        println!("  {} - Optimized Behavior:", result.test_name);
                        Self::print_result_comparison(result, "Original");
                    }
                    CompatibilityStatus::IncompatibleSuccess => {
                        println!("  {} - Incompatible Success/Failure:", result.test_name);
                        Self::print_result_comparison(result, "SPL ATA");
                    }
                    _ => {
                        println!("  {} - {:?}", result.test_name, result.compatibility_status);
                    }
                }
            }
        } else if both_rejected == total {
            println!("\n✅ ALL FAILURE TESTS SHOW IDENTICAL ERRORS");
        }
    }
}

fn main() {
    // Completely suppress debug output from Mollusk and Solana runtime
    std::env::set_var("RUST_LOG", "error");

    // Setup quiet logging by default - only show warnings and errors
    solana_logger::setup_with(
        "error,solana_runtime=error,solana_program_runtime=error,mollusk=error",
    );

    // Get manifest directory and setup environment
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    println!("CARGO_MANIFEST_DIR: {}", manifest_dir);
    println!("🔨 P-ATA vs Original ATA Failure Scenarios Test Suite");

    BenchmarkSetup::setup_sbf_environment(manifest_dir);

    // Load program IDs
    let program_ids = load_program_ids(manifest_dir);

    // Create implementation structures
    let p_ata_impl = AtaImplementation::p_ata_prefunded(Pubkey::new_from_array(
        program_ids.pata_prefunded_program_id,
    ));

    println!(
        "P-ATA Program ID: {}",
        Pubkey::new_from_array(program_ids.pata_legacy_program_id)
    );
    println!(
        "Prefunded Program ID: {}",
        Pubkey::new_from_array(program_ids.pata_prefunded_program_id)
    );
    println!(
        "Original ATA Program ID: {}",
        Pubkey::new_from_array(program_ids.spl_ata_program_id)
    );
    println!(
        "Token Program ID: {}",
        Pubkey::new_from_array(program_ids.token_program_id)
    );

    let spl_ata_impl =
        AtaImplementation::spl_ata(Pubkey::new_from_array(program_ids.spl_ata_program_id));
    println!(
        "Original ATA Program ID: {}",
        Pubkey::new_from_array(program_ids.spl_ata_program_id)
    );

    println!("\n🔍 Running comprehensive failure comparison between implementations");

    // Validate both setups work
    let p_ata_mollusk = BenchmarkRunner::create_mollusk_for_all_ata_implementations(
        &Pubkey::new_from_array(program_ids.token_program_id),
    );
    let original_mollusk = BenchmarkRunner::create_mollusk_for_all_ata_implementations(
        &Pubkey::new_from_array(program_ids.token_program_id),
    );

    if let Err(e) = BenchmarkSetup::validate_setup(
        &p_ata_mollusk,
        &p_ata_impl.program_id,
        &Pubkey::new_from_array(program_ids.token_program_id),
    ) {
        panic!("P-ATA failure test setup validation failed: {}", e);
    }

    if let Err(e) = BenchmarkSetup::validate_setup(
        &original_mollusk,
        &spl_ata_impl.program_id,
        &Pubkey::new_from_array(program_ids.token_program_id),
    ) {
        panic!("Original ATA failure test setup validation failed: {}", e);
    }

    // Run comprehensive failure comparison
    let comparison_results = FailureTestRunner::run_comprehensive_failure_comparison(
        &p_ata_impl,
        &spl_ata_impl,
        &Pubkey::new_from_array(program_ids.token_program_id),
    );

    // Print summary
    FailureTestRunner::print_failure_summary(&comparison_results);

    // Check for critical issues that indicate security problems or test failures
    let unexpected_success = comparison_results
        .iter()
        .filter(|r| {
            matches!(
                r.compatibility_status,
                CompatibilityStatus::IncompatibleSuccess
            )
        })
        .count();
    let both_succeeded = comparison_results
        .iter()
        .filter(|r| {
            matches!(r.compatibility_status, CompatibilityStatus::Identical)
                && r.p_ata.success
                && r.spl_ata.success
        })
        .count();

    if unexpected_success == 0 && both_succeeded == 0 {
        println!(
            "\n✅ Failure comparison completed successfully - No critical security issues detected"
        );
    } else {
        println!("\n🚨 FAILURE COMPARISON - ISSUES DETECTED");
        if unexpected_success > 0 {
            println!(
                "    {} SECURITY VULNERABILITIES: P-ATA succeeded where original correctly failed",
                unexpected_success
            );
        }
        if both_succeeded > 0 {
            println!(
                "    {} TEST ISSUES: Both implementations succeeded when they should have failed",
                both_succeeded
            );
        }
    }
}
