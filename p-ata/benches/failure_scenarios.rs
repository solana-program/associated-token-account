use {
    solana_account::Account,
    solana_instruction::{AccountMeta, Instruction},
    solana_logger,
    solana_pubkey::Pubkey,
};

#[path = "common.rs"]
mod common;
use common::{account_templates::*, *};

#[path = "common_builders.rs"]
mod common_builders;
use common_builders::{CommonTestCaseBuilder, FailureMode};

// ================================ FAILURE TEST CONSTANTS ================================

const FAKE_SYSTEM_PROGRAM_ID: Pubkey = Pubkey::new_from_array([1u8; 32]);
const FAKE_TOKEN_PROGRAM_ID: Pubkey = Pubkey::new_from_array([2u8; 32]);

// ================================ FAILURE TEST CONFIGURATION ================================

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

#[derive(Clone)]
enum TestCategory {
    BasicAccountOwnership,
    AddressDerivationStructure,
    RecoveryOperations,
    AdditionalValidation,
}

impl TestCategory {
    fn display_name(&self) -> &'static str {
        match self {
            TestCategory::BasicAccountOwnership => "Basic Account Ownership Failure Tests",
            TestCategory::AddressDerivationStructure => {
                "Address Derivation and Structure Failure Tests"
            }
            TestCategory::RecoveryOperations => "Recovery Operation Failure Tests",
            TestCategory::AdditionalValidation => "Additional Validation Coverage Tests",
        }
    }
}

#[derive(Clone)]
enum TestBuilderType {
    /// Use the CommonTestCaseBuilder with the specified failure mode
    Simple,
    /// Use custom logic - these need individual functions
    Custom,
}

/// Static configuration for all failure tests
static FAILURE_TESTS: &[FailureTestConfig] = &[
    // Basic Account Ownership Failure Tests
    FailureTestConfig {
        name: "fail_wrong_payer_owner",
        category: TestCategory::BasicAccountOwnership,
        base_test: BaseTestType::Create,
        variant: TestVariant::BASE,
        failure_mode: FailureMode::WrongPayerOwner(FAKE_TOKEN_PROGRAM_ID),
        builder_type: TestBuilderType::Simple,
    },
    FailureTestConfig {
        name: "fail_payer_not_signed",
        category: TestCategory::BasicAccountOwnership,
        base_test: BaseTestType::Create,
        variant: TestVariant::BASE,
        failure_mode: FailureMode::PayerNotSigned,
        builder_type: TestBuilderType::Simple,
    },
    FailureTestConfig {
        name: "fail_wrong_system_program",
        category: TestCategory::BasicAccountOwnership,
        base_test: BaseTestType::Create,
        variant: TestVariant::BASE,
        failure_mode: FailureMode::WrongSystemProgram(FAKE_SYSTEM_PROGRAM_ID),
        builder_type: TestBuilderType::Simple,
    },
    FailureTestConfig {
        name: "fail_wrong_token_program",
        category: TestCategory::BasicAccountOwnership,
        base_test: BaseTestType::Create,
        variant: TestVariant::BASE,
        failure_mode: FailureMode::WrongTokenProgram(FAKE_TOKEN_PROGRAM_ID),
        builder_type: TestBuilderType::Simple,
    },
    FailureTestConfig {
        name: "fail_insufficient_funds",
        category: TestCategory::BasicAccountOwnership,
        base_test: BaseTestType::Create,
        variant: TestVariant::BASE,
        failure_mode: FailureMode::InsufficientFunds(1000),
        builder_type: TestBuilderType::Simple,
    },
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
    FailureTestConfig {
        name: "fail_mint_wrong_owner",
        category: TestCategory::AddressDerivationStructure,
        base_test: BaseTestType::Create,
        variant: TestVariant::BASE,
        failure_mode: FailureMode::MintWrongOwner(SYSTEM_PROGRAM_ID),
        builder_type: TestBuilderType::Simple,
    },
    FailureTestConfig {
        name: "fail_invalid_mint_structure",
        category: TestCategory::AddressDerivationStructure,
        base_test: BaseTestType::Create,
        variant: TestVariant::BASE,
        failure_mode: FailureMode::InvalidMintStructure(50),
        builder_type: TestBuilderType::Simple,
    },
    FailureTestConfig {
        name: "fail_invalid_token_account_structure",
        category: TestCategory::AddressDerivationStructure,
        base_test: BaseTestType::CreateIdempotent,
        variant: TestVariant::BASE,
        failure_mode: FailureMode::InvalidTokenAccountStructure,
        builder_type: TestBuilderType::Simple,
    },
    FailureTestConfig {
        name: "fail_invalid_discriminator",
        category: TestCategory::AddressDerivationStructure,
        base_test: BaseTestType::Create,
        variant: TestVariant::BASE,
        failure_mode: FailureMode::InvalidDiscriminator(99),
        builder_type: TestBuilderType::Simple,
    },
    FailureTestConfig {
        name: "fail_invalid_bump_value",
        category: TestCategory::AddressDerivationStructure,
        base_test: BaseTestType::Create,
        variant: TestVariant {
            bump_arg: true,
            ..TestVariant::BASE
        },
        failure_mode: FailureMode::InvalidBumpValue(99),
        builder_type: TestBuilderType::Simple,
    },
    // Recovery Operation Failure Tests
    FailureTestConfig {
        name: "fail_recover_wallet_not_signer",
        category: TestCategory::RecoveryOperations,
        base_test: BaseTestType::RecoverNested,
        variant: TestVariant::BASE,
        failure_mode: FailureMode::RecoverWalletNotSigner,
        builder_type: TestBuilderType::Simple,
    },
    FailureTestConfig {
        name: "fail_recover_multisig_insufficient_signers",
        category: TestCategory::RecoveryOperations,
        base_test: BaseTestType::RecoverMultisig,
        variant: TestVariant::BASE,
        failure_mode: FailureMode::RecoverMultisigInsufficientSigners,
        builder_type: TestBuilderType::Simple,
    },
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
    FailureTestConfig {
        name: "fail_recover_invalid_bump_value",
        category: TestCategory::RecoveryOperations,
        base_test: BaseTestType::RecoverNested,
        variant: TestVariant::BASE,
        failure_mode: FailureMode::InvalidBumpValue(99),
        builder_type: TestBuilderType::Custom, // Has custom instruction data
    },
    // Additional Validation Coverage Tests
    FailureTestConfig {
        name: "fail_ata_owned_by_system_program",
        category: TestCategory::AdditionalValidation,
        base_test: BaseTestType::Create,
        variant: TestVariant::BASE,
        failure_mode: FailureMode::AtaWrongOwner(SYSTEM_PROGRAM_ID),
        builder_type: TestBuilderType::Simple,
    },
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
    FailureTestConfig {
        name: "fail_immutable_account",
        category: TestCategory::AdditionalValidation,
        base_test: BaseTestType::Create,
        variant: TestVariant::BASE,
        failure_mode: FailureMode::AtaNotWritable,
        builder_type: TestBuilderType::Simple,
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
];

// ================================ FAILURE TEST HELPERS ================================

/// Log test information for debugging - only shown with --full-debug-logs feature
#[allow(unused)]
fn log_test_info(test_name: &str, ata_impl: &AtaImplementation, addresses: &[(&str, &Pubkey)]) {
    #[cfg(feature = "full-debug-logs")]
    {
        let short_addresses: Vec<String> = addresses
            .iter()
            .map(|(name, addr)| format!("{}: {}", name, &addr.to_string()[0..8]))
            .collect();

        println!(
            "🔍 Test: {} | Implementation: {} | {}",
            test_name,
            ata_impl.name,
            short_addresses.join(" | ")
        );

        let full_addresses: Vec<String> = addresses
            .iter()
            .map(|(name, addr)| format!("{}: {}", name, addr))
            .collect();

        println!("    Full addresses: {}", full_addresses.join(" | "));
    }
}

// Helper function for complex cases that need custom logic
fn build_base_failure_accounts(
    base_test: BaseTestType,
    variant: TestVariant,
    ata_implementation: &AtaImplementation,
    token_program_id: &Pubkey,
) -> (Pubkey, Pubkey, Pubkey) {
    let test_number = common_builders::calculate_failure_test_number(base_test, variant);

    let payer = crate::common::structured_pk(
        &ata_implementation.variant,
        crate::common::TestBankId::Failures,
        test_number,
        crate::common::AccountTypeId::Payer,
    );

    // Use consistent variant for mint and wallet to enable byte-for-byte comparison
    let consistent_variant = &crate::common::AtaVariant::SplAta;
    let mint = crate::common::structured_pk(
        consistent_variant,
        crate::common::TestBankId::Failures,
        test_number,
        crate::common::AccountTypeId::Mint,
    );
    let all_ata_program_ids: Vec<Pubkey> = crate::common::AtaImplementation::all()
        .iter()
        .map(|a| a.program_id)
        .collect();
    let wallet = crate::common::structured_pk_with_optimal_common_bump(
        consistent_variant,
        crate::common::TestBankId::Failures,
        test_number,
        crate::common::AccountTypeId::Wallet,
        &all_ata_program_ids,
        &token_program_id,
        &mint,
    );

    (payer, mint, wallet)
}

// ================================ FAILURE TEST BUILDERS ================================

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
                    "fail_recover_invalid_bump_value" => {
                        Self::build_fail_recover_invalid_bump_value(ata_impl, token_program_id)
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
        let wrong_ata_address = crate::common::structured_pk(
            &ata_impl.variant,
            crate::common::TestBankId::Failures,
            173,
            crate::common::AccountTypeId::Ata,
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

    /// Custom builder for recover wrong nested ATA address test
    fn build_fail_recover_wrong_nested_ata_address(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let test_number = common_builders::calculate_failure_test_number(
            BaseTestType::RecoverNested,
            TestVariant::BASE,
        );
        let [wrong_nested_ata, nested_mint, dest_ata, owner_ata, owner_mint, wallet] =
            crate::common::structured_pk_multi(
                &ata_impl.variant,
                crate::common::TestBankId::Failures,
                test_number,
                [
                    crate::common::AccountTypeId::NestedAta, // wrong_nested_ata - will be wrong in the test
                    crate::common::AccountTypeId::NestedMint,
                    crate::common::AccountTypeId::Ata, // dest_ata
                    crate::common::AccountTypeId::OwnerAta,
                    crate::common::AccountTypeId::OwnerMint,
                    crate::common::AccountTypeId::Wallet,
                ],
            );

        // Log test name for identification
        log_test_info(
            "fail_recover_wrong_nested_ata_address",
            ata_impl,
            &[
                ("wrong_nested_ata", &wrong_nested_ata),
                ("nested_mint", &nested_mint),
                ("dest_ata", &dest_ata),
                ("owner_ata", &owner_ata),
                ("owner_mint", &owner_mint),
                ("wallet", &wallet),
            ],
        );

        let accounts = RecoverAccountSet::new(
            wrong_nested_ata, // Use wrong address as provided
            nested_mint,
            dest_ata,
            owner_ata,
            owner_mint,
            wallet,
            token_program_id,
            100, // token amount
        )
        .to_vec();

        let ix = Instruction {
            program_id: ata_impl.program_id,
            accounts: vec![
                AccountMeta::new(wrong_nested_ata, false), // Wrong nested ATA
                AccountMeta::new_readonly(nested_mint, false),
                AccountMeta::new(dest_ata, false),
                AccountMeta::new(owner_ata, false),
                AccountMeta::new_readonly(owner_mint, false),
                AccountMeta::new(wallet, true),
                AccountMeta::new_readonly(*token_program_id, false),
                AccountMeta::new_readonly(Pubkey::from(spl_token_interface::program::ID), false),
            ],
            data: vec![2u8],
        };

        (ix, accounts)
    }

    /// Custom builder for recover wrong destination address test
    fn build_fail_recover_wrong_destination_address(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let test_number = common_builders::calculate_failure_test_number(
            BaseTestType::RecoverNested,
            TestVariant::BASE,
        );
        let [nested_ata, nested_mint, wrong_dest_ata, owner_ata, owner_mint, wallet] =
            crate::common::structured_pk_multi(
                &ata_impl.variant,
                crate::common::TestBankId::Failures,
                test_number,
                [
                    crate::common::AccountTypeId::NestedAta,
                    crate::common::AccountTypeId::NestedMint,
                    crate::common::AccountTypeId::Ata, // wrong_dest_ata
                    crate::common::AccountTypeId::OwnerAta,
                    crate::common::AccountTypeId::OwnerMint,
                    crate::common::AccountTypeId::Wallet,
                ],
            );

        // Log test name for identification
        log_test_info(
            "fail_recover_wrong_destination_address",
            ata_impl,
            &[
                ("nested_ata", &nested_ata),
                ("nested_mint", &nested_mint),
                ("wrong_dest_ata", &wrong_dest_ata),
                ("owner_ata", &owner_ata),
                ("owner_mint", &owner_mint),
                ("wallet", &wallet),
            ],
        );

        let accounts = RecoverAccountSet::new(
            nested_ata,
            nested_mint,
            wrong_dest_ata, // Use wrong destination address as provided
            owner_ata,
            owner_mint,
            wallet,
            token_program_id,
            100, // token amount
        )
        .to_vec();

        let ix = Instruction {
            program_id: ata_impl.program_id,
            accounts: vec![
                AccountMeta::new(nested_ata, false),
                AccountMeta::new_readonly(nested_mint, false),
                AccountMeta::new(wrong_dest_ata, false), // Wrong destination
                AccountMeta::new(owner_ata, false),
                AccountMeta::new_readonly(owner_mint, false),
                AccountMeta::new(wallet, true),
                AccountMeta::new_readonly(*token_program_id, false),
                AccountMeta::new_readonly(Pubkey::from(spl_token_interface::program::ID), false),
            ],
            data: vec![2u8],
        };

        (ix, accounts)
    }

    /// Custom builder for recover invalid bump value test
    fn build_fail_recover_invalid_bump_value(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let test_number = common_builders::calculate_failure_test_number(
            BaseTestType::RecoverNested,
            TestVariant::BASE,
        );
        let [nested_ata, nested_mint, dest_ata, owner_ata, owner_mint, wallet] =
            crate::common::structured_pk_multi(
                &ata_impl.variant,
                crate::common::TestBankId::Failures,
                test_number,
                [
                    crate::common::AccountTypeId::NestedAta,
                    crate::common::AccountTypeId::NestedMint,
                    crate::common::AccountTypeId::Ata, // dest_ata
                    crate::common::AccountTypeId::OwnerAta,
                    crate::common::AccountTypeId::OwnerMint,
                    crate::common::AccountTypeId::Wallet,
                ],
            );

        // Log test name for identification
        log_test_info(
            "fail_recover_invalid_bump_value",
            ata_impl,
            &[
                ("nested_ata", &nested_ata),
                ("nested_mint", &nested_mint),
                ("dest_ata", &dest_ata),
                ("owner_ata", &owner_ata),
                ("owner_mint", &owner_mint),
                ("wallet", &wallet),
            ],
        );

        let accounts = RecoverAccountSet::new(
            nested_ata,
            nested_mint,
            dest_ata,
            owner_ata,
            owner_mint,
            wallet,
            token_program_id,
            100, // token amount
        )
        .to_vec();

        let ix = Instruction {
            program_id: ata_impl.program_id,
            accounts: vec![
                AccountMeta::new(nested_ata, false),
                AccountMeta::new_readonly(nested_mint, false),
                AccountMeta::new(dest_ata, false),
                AccountMeta::new(owner_ata, false),
                AccountMeta::new_readonly(owner_mint, false),
                AccountMeta::new(wallet, true),
                AccountMeta::new_readonly(*token_program_id, false),
                AccountMeta::new_readonly(Pubkey::from(spl_token_interface::program::ID), false),
            ],
            data: vec![2u8, 99u8], // RecoverNested with invalid bump
        };

        (ix, accounts)
    }

    /// Custom builder for wrong token account size test
    fn build_fail_wrong_token_account_size(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let (payer, mint, wallet) = build_base_failure_accounts(
            BaseTestType::CreateIdempotent,
            TestVariant::BASE,
            ata_impl,
            token_program_id,
        );

        // Log test name for identification
        log_test_info(
            "fail_wrong_token_account_size",
            ata_impl,
            &[("payer", &payer), ("mint", &mint), ("wallet", &wallet)],
        );

        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            &ata_impl.program_id,
        );

        let mut accounts = StandardAccountSet::new(payer, ata, wallet, mint, token_program_id)
            .with_existing_ata(&mint, &wallet, token_program_id)
            .to_vec();

        // Apply failure: set ATA to wrong size
        FailureAccountBuilder::set_wrong_data_size(&mut accounts, ata, 100);

        let ix = Instruction {
            program_id: ata_impl.program_id,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new(ata, false),
                AccountMeta::new_readonly(wallet, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new_readonly(*token_program_id, false),
            ],
            data: vec![1u8], // CreateIdempotent instruction
        };

        (ix, accounts)
    }

    /// Custom builder for token account wrong mint test
    fn build_fail_token_account_wrong_mint(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let (payer, mint, wallet) = build_base_failure_accounts(
            BaseTestType::CreateIdempotent,
            TestVariant::BASE,
            ata_impl,
            token_program_id,
        );

        // Log test name for identification
        log_test_info(
            "fail_token_account_wrong_mint",
            ata_impl,
            &[("payer", &payer), ("mint", &mint), ("wallet", &wallet)],
        );

        let test_number = common_builders::calculate_failure_test_number(
            BaseTestType::CreateIdempotent,
            TestVariant::BASE,
        );
        let wrong_mint = crate::common::structured_pk(
            &ata_impl.variant,
            crate::common::TestBankId::Failures,
            test_number + 1,
            crate::common::AccountTypeId::Mint,
        );
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            &ata_impl.program_id,
        );

        let mut accounts =
            StandardAccountSet::new(payer, ata, wallet, mint, token_program_id).to_vec();

        // Replace ATA with one pointing to wrong mint
        if let Some(pos) = accounts.iter().position(|(addr, _)| *addr == ata) {
            accounts[pos].1 =
                AccountBuilder::token_account(&wrong_mint, &wallet, 0, token_program_id);
        }

        // Add the wrong mint account
        accounts.push((
            wrong_mint,
            AccountBuilder::mint_account(0, token_program_id, false),
        ));

        let ix = Instruction {
            program_id: ata_impl.program_id,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new(ata, false),
                AccountMeta::new_readonly(wallet, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new_readonly(*token_program_id, false),
            ],
            data: vec![1u8], // CreateIdempotent instruction
        };

        (ix, accounts)
    }

    /// Custom builder for token account wrong owner test
    fn build_fail_token_account_wrong_owner(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let (payer, mint, wallet) = build_base_failure_accounts(
            BaseTestType::CreateIdempotent,
            TestVariant::BASE,
            ata_impl,
            token_program_id,
        );

        // Log test name for identification
        log_test_info(
            "fail_token_account_wrong_owner",
            ata_impl,
            &[("payer", &payer), ("mint", &mint), ("wallet", &wallet)],
        );

        let test_number = common_builders::calculate_failure_test_number(
            BaseTestType::CreateIdempotent,
            TestVariant::BASE,
        );
        let wrong_owner = crate::common::structured_pk(
            &ata_impl.variant,
            crate::common::TestBankId::Failures,
            test_number + 1,
            crate::common::AccountTypeId::Wallet,
        );
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            &ata_impl.program_id,
        );

        let mut accounts =
            StandardAccountSet::new(payer, ata, wallet, mint, token_program_id).to_vec();

        // Replace ATA with one having wrong owner
        if let Some(pos) = accounts.iter().position(|(addr, _)| *addr == ata) {
            accounts[pos].1 =
                AccountBuilder::token_account(&mint, &wrong_owner, 0, token_program_id);
        }

        let ix = Instruction {
            program_id: ata_impl.program_id,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new(ata, false),
                AccountMeta::new_readonly(wallet, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new_readonly(*token_program_id, false),
            ],
            data: vec![1u8], // CreateIdempotent instruction
        };

        (ix, accounts)
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
            if new_data.len() != crate::constants::account_sizes::MINT_ACCOUNT_SIZE {
                new_data.truncate(crate::constants::account_sizes::MINT_ACCOUNT_SIZE);
            }

            // Increase length to 98 bytes and write the 4-byte TLV header (ImmutableOwner = 7).
            let required_len = crate::constants::account_sizes::MINT_ACCOUNT_SIZE + 16; // header + padding
            new_data.resize(required_len, 0u8);
            new_data[crate::constants::account_sizes::MINT_ACCOUNT_SIZE
                ..crate::constants::account_sizes::MINT_ACCOUNT_SIZE + 4]
                .copy_from_slice(&[7u8, 0u8, 0u8, 0u8]);

            mint_acct.data = new_data;
        }
        (ix, accounts)
    }
}

// ================================ FAILURE TEST COMPARISON RUNNER ================================

struct FailureTestRunner;

impl FailureTestRunner {
    /// Print failure test results - detailed only for unexpected successes (security issues)
    fn print_failure_test_result_summary(result: &ComparisonResult) {
        println!("Test: {}", result.test_name);
        // Check if we need detailed output (security issues or unexpected results)
        let needs_detailed_output = matches!(
            result.compatibility_status,
            CompatibilityStatus::IncompatibleSuccess | CompatibilityStatus::Identical
        ) && (result.p_ata.success || result.spl_ata.success);

        match result.compatibility_status {
            CompatibilityStatus::BothRejected => {
                // Expected: both failed - brief output
                println!("    ❌ Both failed (expected)");
            }
            CompatibilityStatus::OptimizedBehavior => {
                // P-ATA-only feature - brief output
                if result
                    .spl_ata
                    .error_message
                    .as_ref()
                    .map_or(false, |msg| msg.contains("N/A"))
                {
                    println!("    🚀 P-ATA-only feature");
                } else {
                    println!("    🚀 P-ATA optimization");
                }
            }
            CompatibilityStatus::IncompatibleFailure => {
                // Different error messages but both failed - brief output (detailed in summary)
                println!("    ⚠️ Different error messages (both failed)");
            }
            _ => {
                // Unexpected successes or critical issues - FULL detailed output
                println!("    🚨 UNEXPECTED RESULT - DETAILED ANALYSIS:");

                let p_ata_status = if result.p_ata.success {
                    "✅ SUCCESS (UNEXPECTED!)".to_string()
                } else {
                    "❌ Failed (expected)".to_string()
                };

                let spl_ata_status = if result.spl_ata.success {
                    "✅ SUCCESS (UNEXPECTED!)".to_string()
                } else {
                    "❌ Failed (expected)".to_string()
                };

                println!("      P-ATA: {}", p_ata_status);
                println!("      SPL ATA: {}", spl_ata_status);

                // Show error details for failures
                if !result.p_ata.success {
                    if let Some(ref error) = result.p_ata.error_message {
                        println!("      P-ATA Error: {}", error);
                    }
                }
                if !result.spl_ata.success {
                    if let Some(ref error) = result.spl_ata.error_message {
                        println!("      SPL ATA Error: {}", error);
                    }
                }

                // Show compatibility assessment
                match result.compatibility_status {
                    CompatibilityStatus::IncompatibleSuccess => {
                        if result.p_ata.success && !result.spl_ata.success {
                            println!("      🔴 SECURITY ISSUE: P-ATA bypassed validation!");
                        } else if !result.p_ata.success && result.spl_ata.success {
                            println!("      🔴 SECURITY ISSUE: SPL ATA bypassed validation!");
                        }
                    }
                    CompatibilityStatus::Identical => {
                        if result.p_ata.success && result.spl_ata.success {
                            println!("      🔴 TEST ISSUE: Both succeeded when they should fail!");
                        }
                    }
                    _ => {
                        println!("      Status: {:?}", result.compatibility_status);
                    }
                }
            }
        }

        // Show captured debug output only for unexpected results
        if needs_detailed_output {
            if !result.p_ata.captured_output.is_empty() {
                println!("      P-ATA Debug Output:");
                for line in result.p_ata.captured_output.lines() {
                    println!("        {}", line);
                }
            }
            if !result.spl_ata.captured_output.is_empty() {
                println!("      SPL ATA Debug Output:");
                for line in result.spl_ata.captured_output.lines() {
                    println!("        {}", line);
                }
            }
        }
    }
    /// Run a failure test with configuration against both implementations and compare results
    fn run_failure_comparison_test_with_config(
        config: &FailureTestConfig,
        p_ata_impl: &AtaImplementation,
        original_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> ComparisonResult {
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
        let is_p_ata_only =
            name == "fail_invalid_bump_value" || name == "fail_recover_invalid_bump_value";

        // Build P-ATA test case
        let (p_ata_ix, p_ata_accounts) = test_builder(p_ata_impl, token_program_id);
        let mut p_ata_result = BenchmarkRunner::run_single_benchmark(
            name,
            &p_ata_ix,
            &p_ata_accounts,
            p_ata_impl,
            token_program_id,
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
            let original_result = BenchmarkRunner::run_single_benchmark(
                name,
                &original_ix,
                &original_accounts,
                original_impl,
                token_program_id,
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
        let mut tests_by_category: std::collections::HashMap<String, Vec<&FailureTestConfig>> =
            std::collections::HashMap::new();

        for config in FAILURE_TESTS {
            let category_name = config.category.display_name().to_string();
            tests_by_category
                .entry(category_name)
                .or_insert_with(Vec::new)
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
                Self::print_failure_test_result_summary(&comparison);
                results.push(comparison);
            }
        }

        Self::print_failure_summary(&results);
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
        if let Err(e) = std::fs::write("benchmark_results/failure_results.json", output) {
            eprintln!("Failed to write failure results: {}", e);
        } else {
            println!("\n🧪 Failure test results written to benchmark_results/failure_results.json");
        }
    }

    /// Print failure test summary with compatibility analysis
    fn print_failure_summary(results: &[ComparisonResult]) {
        println!("\n=== FAILURE TEST COMPATIBILITY SUMMARY ===");

        let total_tests = results.len();
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

        println!("Total Failure Tests: {}", total_tests);
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
                        if result.p_ata.success {
                            println!("    P-ATA:     Success");
                        } else {
                            println!(
                                "    P-ATA:     {}",
                                result
                                    .p_ata
                                    .error_message
                                    .as_deref()
                                    .unwrap_or("Unknown error")
                            );
                        }
                        if result.spl_ata.success {
                            println!("    SPL ATA:  Success");
                        } else {
                            println!(
                                "   SPL ATA:  {}",
                                result
                                    .spl_ata
                                    .error_message
                                    .as_deref()
                                    .unwrap_or("Unknown error")
                            );
                        }
                    }
                    CompatibilityStatus::OptimizedBehavior => {
                        println!("  {} - Optimized Behavior:", result.test_name);
                        if result.p_ata.success {
                            println!("    P-ATA:     Success");
                        } else {
                            println!(
                                "    P-ATA:     {}",
                                result
                                    .p_ata
                                    .error_message
                                    .as_deref()
                                    .unwrap_or("Unknown error")
                            );
                        }
                        if result.spl_ata.success {
                            println!("    SPL ATA:  Success");
                        } else {
                            println!(
                                "    Original:  {}",
                                result
                                    .spl_ata
                                    .error_message
                                    .as_deref()
                                    .unwrap_or("Unknown error")
                            );
                        }
                    }
                    CompatibilityStatus::IncompatibleSuccess => {
                        println!("  {} - Incompatible Success/Failure:", result.test_name);
                        if result.p_ata.success {
                            println!("    P-ATA:     Success");
                        } else {
                            println!(
                                "    P-ATA:     {}",
                                result
                                    .p_ata
                                    .error_message
                                    .as_deref()
                                    .unwrap_or("Unknown error")
                            );
                        }
                        if result.spl_ata.success {
                            println!("    SPL ATA:  Success");
                        } else {
                            println!(
                                "    SPL ATA:  {}",
                                result
                                    .spl_ata
                                    .error_message
                                    .as_deref()
                                    .unwrap_or("Unknown error")
                            );
                        }
                    }
                    _ => {
                        println!("  {} - {:?}", result.test_name, result.compatibility_status);
                    }
                }
            }
        } else if both_rejected == total_tests {
            println!("\n✅ ALL FAILURE TESTS SHOW IDENTICAL ERRORS");
        }
    }
}

// ================================ MAIN FUNCTION ================================

fn main() {
    // Completely suppress debug output from Mollusk and Solana runtime
    std::env::set_var("RUST_LOG", "error");

    // Setup quiet logging by default - only show warnings and errors
    let _ = solana_logger::setup_with(
        "error,solana_runtime=error,solana_program_runtime=error,mollusk=error",
    );

    // Get manifest directory and setup environment
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    println!("CARGO_MANIFEST_DIR: {}", manifest_dir);
    println!("🔨 P-ATA vs Original ATA Failure Scenarios Test Suite");

    BenchmarkSetup::setup_sbf_environment(manifest_dir);

    // Load program IDs
    let program_ids = BenchmarkSetup::load_program_ids(manifest_dir);

    // Create implementation structures
    let p_ata_impl = AtaImplementation::p_ata_prefunded(program_ids.pata_prefunded_program_id);

    println!("P-ATA Program ID: {}", program_ids.pata_legacy_program_id);
    println!(
        "Prefunded Program ID: {}",
        program_ids.pata_prefunded_program_id
    );
    println!(
        "Original ATA Program ID: {}",
        program_ids.spl_ata_program_id
    );
    println!("Token Program ID: {}", program_ids.token_program_id);

    let spl_ata_impl = AtaImplementation::spl_ata(program_ids.spl_ata_program_id);
    println!(
        "Original ATA Program ID: {}",
        program_ids.spl_ata_program_id
    );

    println!("\n🔍 Running comprehensive failure comparison between implementations");

    // Validate both setups work
    let p_ata_mollusk =
        BenchmarkRunner::create_mollusk_for_all_ata_implementations(&program_ids.token_program_id);
    let original_mollusk =
        BenchmarkRunner::create_mollusk_for_all_ata_implementations(&program_ids.token_program_id);

    if let Err(e) = BenchmarkSetup::validate_setup(
        &p_ata_mollusk,
        &p_ata_impl.program_id,
        &program_ids.token_program_id,
    ) {
        panic!("P-ATA failure test setup validation failed: {}", e);
    }

    if let Err(e) = BenchmarkSetup::validate_setup(
        &original_mollusk,
        &spl_ata_impl.program_id,
        &program_ids.token_program_id,
    ) {
        panic!("Original ATA failure test setup validation failed: {}", e);
    }

    // Run comprehensive failure comparison
    let comparison_results = FailureTestRunner::run_comprehensive_failure_comparison(
        &p_ata_impl,
        &spl_ata_impl,
        &program_ids.token_program_id,
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
