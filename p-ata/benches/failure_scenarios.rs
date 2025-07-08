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

// ================================ FAILURE TEST BUILDERS ================================

/// Failure test builders using the consolidated builder pattern where possible.
/// Complex scenarios that require custom logic are implemented directly.

// Helper function for complex cases that need custom logic
fn build_base_failure_accounts(
    base_test: BaseTestType,
    variant: TestVariant,
    ata_implementation: &AtaImplementation,
) -> (Pubkey, Pubkey, Pubkey) {
    let test_number = common_builders::calculate_failure_test_number(base_test, variant);
    let [payer, mint, wallet] = crate::common::structured_pk_multi(
        &ata_implementation.variant,
        crate::common::TestBankId::Failures,
        test_number,
        [
            crate::common::AccountTypeId::Payer,
            crate::common::AccountTypeId::Mint,
            crate::common::AccountTypeId::Wallet,
        ],
    );
    (payer, mint, wallet)
}

struct FailureTestBuilder;

impl FailureTestBuilder {
    fn build_fail_wrong_payer_owner(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::Create,
            TestVariant::BASE,
            ata_impl,
            token_program_id,
            FailureMode::WrongPayerOwner(*token_program_id),
        )
    }

    fn build_fail_payer_not_signed(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::Create,
            TestVariant::BASE,
            ata_impl,
            token_program_id,
            FailureMode::PayerNotSigned,
        )
    }

    fn build_fail_wrong_system_program(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::Create,
            TestVariant::BASE,
            ata_impl,
            token_program_id,
            FailureMode::WrongSystemProgram(FAKE_SYSTEM_PROGRAM_ID),
        )
    }

    fn build_fail_wrong_token_program(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::Create,
            TestVariant::BASE,
            ata_impl,
            token_program_id,
            FailureMode::WrongTokenProgram(FAKE_TOKEN_PROGRAM_ID),
        )
    }

    fn build_fail_insufficient_funds(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::Create,
            TestVariant::BASE,
            ata_impl,
            token_program_id,
            FailureMode::InsufficientFunds(1000),
        )
    }

    fn build_fail_wrong_ata_address(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::Create,
            TestVariant::BASE,
            ata_impl,
            token_program_id,
            FailureMode::WrongAtaAddress(crate::common::structured_pk(
                &ata_impl.variant,
                crate::common::TestBankId::Failures,
                173,
                crate::common::AccountTypeId::Ata,
            )),
        )
    }

    /// Build CREATE failure test with mint owned by wrong program
    fn build_fail_mint_wrong_owner(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::Create,
            TestVariant::BASE,
            ata_impl,
            token_program_id,
            FailureMode::MintWrongOwner(SYSTEM_PROGRAM_ID),
        )
    }

    /// Build CREATE failure test with invalid mint structure
    fn build_fail_invalid_mint_structure(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::Create,
            TestVariant::BASE,
            ata_impl,
            token_program_id,
            FailureMode::InvalidMintStructure(50), // Wrong size - should be MINT_ACCOUNT_SIZE
        )
    }

    /// Build CREATE_IDEMPOTENT failure test with invalid token account structure
    fn build_fail_invalid_token_account_structure(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::CreateIdempotent,
            TestVariant::BASE,
            ata_impl,
            token_program_id,
            FailureMode::InvalidTokenAccountStructure,
        )
    }

    /// Build RECOVER failure test with wallet not signer
    fn build_fail_recover_wallet_not_signer(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::RecoverNested,
            TestVariant::BASE,
            ata_impl,
            token_program_id,
            FailureMode::RecoverWalletNotSigner,
        )
    }

    /// Build RECOVER failure test with multisig insufficient signers
    fn build_fail_recover_multisig_insufficient_signers(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::RecoverMultisig,
            TestVariant::BASE,
            ata_impl,
            token_program_id,
            FailureMode::RecoverMultisigInsufficientSigners,
        )
    }

    /// Build failure test with invalid instruction discriminator
    fn build_fail_invalid_discriminator(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::Create,
            TestVariant::BASE,
            ata_impl,
            token_program_id,
            FailureMode::InvalidDiscriminator(99), // Invalid discriminator (should be 0, 1, or 2)
        )
    }

    /// Build failure test with invalid bump value
    fn build_fail_invalid_bump_value(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::Create,
            TestVariant {
                bump_arg: true,
                ..TestVariant::BASE
            },
            ata_impl,
            token_program_id,
            FailureMode::InvalidBumpValue(99), // Invalid bump (not the correct bump)
        )
    }

    /// Build CREATE failure test with ATA owned by system program (existing ATA with wrong owner)
    fn build_fail_ata_owned_by_system_program(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::Create,
            TestVariant::BASE,
            ata_impl,
            token_program_id,
            FailureMode::AtaWrongOwner(SYSTEM_PROGRAM_ID),
        )
    }

    /// Build RECOVER failure test with wrong nested ATA address
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

        let mut accounts = RecoverAccountSet::new(
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

    /// Build RECOVER failure test with wrong destination address
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

    /// Build RECOVER failure test with invalid bump for RecoverNested
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

    /// Build CREATE failure test with wrong token account size
    fn build_fail_wrong_token_account_size(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let (payer, mint, wallet) = build_base_failure_accounts(
            BaseTestType::CreateIdempotent,
            TestVariant::BASE,
            ata_impl,
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

    /// Build CREATE failure test with token account pointing to wrong mint
    fn build_fail_token_account_wrong_mint(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let (payer, mint, wallet) = build_base_failure_accounts(
            BaseTestType::CreateIdempotent,
            TestVariant::BASE,
            ata_impl,
        );
        let test_number = common_builders::calculate_failure_test_number(
            BaseTestType::CreateIdempotent,
            TestVariant::BASE,
        );
        let wrong_mint = crate::common::structured_pk(
            &ata_impl.variant,
            crate::common::TestBankId::Failures,
            test_number + 1, // offset for different account
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

    /// Build CREATE failure test with token account having wrong owner
    fn build_fail_token_account_wrong_owner(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let (payer, mint, wallet) = build_base_failure_accounts(
            BaseTestType::CreateIdempotent,
            TestVariant::BASE,
            ata_impl,
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

    /// Build CREATE failure test with immutable account (non-writable)
    fn build_fail_immutable_account(
        ata_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::Create,
            TestVariant::BASE,
            ata_impl,
            token_program_id,
            FailureMode::AtaNotWritable,
        )
    }
}

// ================================ FAILURE TEST COMPARISON RUNNER ================================

struct FailureTestRunner;

impl FailureTestRunner {
    /// Print failure test results - detailed only for unexpected successes (security issues)
    fn print_failure_test_result_summary(result: &ComparisonResult) {
        // Check if we need detailed output (security issues or unexpected results)
        let needs_detailed_output = matches!(
            result.compatibility_status,
            CompatibilityStatus::IncompatibleSuccess | CompatibilityStatus::Identical
        ) && (result.p_ata.success || result.spl_ata.success);

        match result.compatibility_status {
            CompatibilityStatus::BothRejected => {
                // Expected: both failed - brief output
                println!("    ❌ Both rejected (expected)");
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

        // Show captured debug output only for security issues
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

        // Build test for P-ATA
        let (p_ata_ix, p_ata_accounts) = test_builder(p_ata_impl, token_program_id);

        // Run P-ATA benchmark with quiet logging first
        let mut p_ata_result = ComparisonRunner::run_single_benchmark(
            name,
            &p_ata_ix,
            &p_ata_accounts,
            p_ata_impl,
            token_program_id,
        );

        let mut comparison_result = if is_p_ata_only {
            // For P-ATA-only tests, create a N/A result for original ATA
            let original_result = BenchmarkResult {
                implementation: "original-ata".to_string(),
                test_name: name.to_string(),
                compute_units: 0,
                success: false,
                error_message: Some(
                    "N/A - Test not applicable to original ATA (uses P-ATA-specific bump args)"
                        .to_string(),
                ),
                captured_output: String::new(),
            };
            ComparisonRunner::create_comparison_result(name, p_ata_result, original_result)
        } else {
            // Build test for Original ATA (separate account set with correct ATA addresses)
            let (original_ix, original_accounts) = test_builder(original_impl, token_program_id);

            // Run Original ATA benchmark with quiet logging first
            let original_result = ComparisonRunner::run_single_benchmark(
                name,
                &original_ix,
                &original_accounts,
                original_impl,
                token_program_id,
            );

            // Create comparison result
            ComparisonRunner::create_comparison_result(name, p_ata_result, original_result)
        };

        // Check if we need debug logging for problematic results
        let needs_debug_logging = Self::is_problematic_result(&comparison_result);

        if needs_debug_logging {
            // Re-run with debug logging to capture verbose output
            p_ata_result = ComparisonRunner::run_single_benchmark_with_debug(
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
                let original_result = ComparisonRunner::run_single_benchmark_with_debug(
                    name,
                    &original_ix,
                    &original_accounts,
                    original_impl,
                    token_program_id,
                );

                // Update comparison result with debug output
                comparison_result =
                    ComparisonRunner::create_comparison_result(name, p_ata_result, original_result);
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
        println!(
            "This validates that P-ATA maintains the same security properties as the original ATA"
        );

        let mut results = Vec::new();

        // Basic account ownership failure tests
        println!("\n--- Basic Account Ownership Failure Tests ---");

        // Type alias for cleaner function pointer types
        type TestFn = fn(&AtaImplementation, &Pubkey) -> (Instruction, Vec<(Pubkey, Account)>);

        let basic_tests: [(&str, TestFn); 5] = [
            (
                "fail_wrong_payer_owner",
                FailureTestBuilder::build_fail_wrong_payer_owner,
            ),
            (
                "fail_payer_not_signed",
                FailureTestBuilder::build_fail_payer_not_signed,
            ),
            (
                "fail_wrong_system_program",
                FailureTestBuilder::build_fail_wrong_system_program,
            ),
            (
                "fail_wrong_token_program",
                FailureTestBuilder::build_fail_wrong_token_program,
            ),
            (
                "fail_insufficient_funds",
                FailureTestBuilder::build_fail_insufficient_funds,
            ),
        ];

        for (test_name, test_builder) in basic_tests {
            let comparison = Self::run_failure_comparison_test(
                test_name,
                test_builder,
                p_ata_impl,
                original_impl,
                token_program_id,
            );
            Self::print_failure_test_result_summary(&comparison);
            results.push(comparison);
        }

        // Address derivation and structure failure tests
        println!("\n--- Address Derivation and Structure Failure Tests ---");

        let structure_tests: [(&str, TestFn); 6] = [
            (
                "fail_wrong_ata_address",
                FailureTestBuilder::build_fail_wrong_ata_address,
            ),
            (
                "fail_mint_wrong_owner",
                FailureTestBuilder::build_fail_mint_wrong_owner,
            ),
            (
                "fail_invalid_mint_structure",
                FailureTestBuilder::build_fail_invalid_mint_structure,
            ),
            (
                "fail_invalid_token_account_structure",
                FailureTestBuilder::build_fail_invalid_token_account_structure,
            ),
            (
                "fail_invalid_discriminator",
                FailureTestBuilder::build_fail_invalid_discriminator,
            ),
            (
                "fail_invalid_bump_value",
                FailureTestBuilder::build_fail_invalid_bump_value,
            ),
        ];

        for (test_name, test_builder) in structure_tests {
            let comparison = Self::run_failure_comparison_test(
                test_name,
                test_builder,
                p_ata_impl,
                original_impl,
                token_program_id,
            );
            Self::print_failure_test_result_summary(&comparison);
            results.push(comparison);
        }

        // Recovery-specific failure tests
        println!("\n--- Recovery Operation Failure Tests ---");

        let recovery_tests: [(&str, TestFn); 5] = [
            (
                "fail_recover_wallet_not_signer",
                FailureTestBuilder::build_fail_recover_wallet_not_signer,
            ),
            (
                "fail_recover_multisig_insufficient_signers",
                FailureTestBuilder::build_fail_recover_multisig_insufficient_signers,
            ),
            (
                "fail_recover_wrong_nested_ata_address",
                FailureTestBuilder::build_fail_recover_wrong_nested_ata_address,
            ),
            (
                "fail_recover_wrong_destination_address",
                FailureTestBuilder::build_fail_recover_wrong_destination_address,
            ),
            (
                "fail_recover_invalid_bump_value",
                FailureTestBuilder::build_fail_recover_invalid_bump_value,
            ),
        ];

        for (test_name, test_builder) in recovery_tests {
            let comparison = Self::run_failure_comparison_test(
                test_name,
                test_builder,
                p_ata_impl,
                original_impl,
                token_program_id,
            );
            Self::print_failure_test_result_summary(&comparison);
            results.push(comparison);
        }

        // Additional validation tests
        println!("\n--- Additional Validation Coverage Tests ---");

        let validation_tests: [(&str, TestFn); 5] = [
            (
                "fail_ata_owned_by_system_program",
                FailureTestBuilder::build_fail_ata_owned_by_system_program,
            ),
            (
                "fail_wrong_token_account_size",
                FailureTestBuilder::build_fail_wrong_token_account_size,
            ),
            (
                "fail_token_account_wrong_mint",
                FailureTestBuilder::build_fail_token_account_wrong_mint,
            ),
            (
                "fail_token_account_wrong_owner",
                FailureTestBuilder::build_fail_token_account_wrong_owner,
            ),
            (
                "fail_immutable_account",
                FailureTestBuilder::build_fail_immutable_account,
            ),
        ];

        for (test_name, test_builder) in validation_tests {
            let comparison = Self::run_failure_comparison_test(
                test_name,
                test_builder,
                p_ata_impl,
                original_impl,
                token_program_id,
            );
            Self::print_failure_test_result_summary(&comparison);
            results.push(comparison);
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
            "Both Implementations Rejected (Compatible): {} ({:.1}%)",
            both_rejected,
            (both_rejected as f64 / total_tests as f64) * 100.0
        );
        println!(
            "Failed with Different Errors: {} ({:.1}%)",
            incompatible_failures,
            (incompatible_failures as f64 / total_tests as f64) * 100.0
        );
        println!(
            "Optimized Behavior: {} ({:.1}%)",
            optimized_behavior,
            (optimized_behavior as f64 / total_tests as f64) * 100.0
        );
        println!(
            "Unexpected Success/Failure: {} ({:.1}%)",
            unexpected_success,
            (unexpected_success as f64 / total_tests as f64) * 100.0
        );
        println!(
            "Both Succeeded (Test Issue): {} ({:.1}%)",
            both_succeeded,
            (both_succeeded as f64 / total_tests as f64) * 100.0
        );

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
        ComparisonRunner::create_mollusk_for_all_ata_implementations(&program_ids.token_program_id);
    let original_mollusk =
        ComparisonRunner::create_mollusk_for_all_ata_implementations(&program_ids.token_program_id);

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
