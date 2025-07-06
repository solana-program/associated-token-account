use {
    mollusk_svm::{program::loader_keys::LOADER_V3, Mollusk},
    mollusk_svm_bencher::MolluskComputeUnitBencher,
    solana_account::Account,
    solana_instruction::{AccountMeta, Instruction},
    solana_logger,
    solana_pubkey::Pubkey,
    solana_sysvar::rent,
    std::fs,
};

#[path = "common.rs"]
mod common;
use common::*;

// ========================== ATA IMPLEMENTATION ABSTRACTION ============================

// ========================== TEST CASE BUILDERS ============================

struct TestCaseBuilder;

impl TestCaseBuilder {
    #[allow(clippy::too_many_arguments)]
    fn build_create(
        ata_implementation: &AtaImplementation,
        token_program_id: &Pubkey,
        extended_mint: bool,
        with_rent: bool,
        topup: bool,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let base_offset = calculate_base_offset(extended_mint, with_rent, topup);
        let (payer, mint, wallet) = build_base_test_accounts(
            base_offset,
            token_program_id,
            &ata_implementation.program_id,
        );

        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            &ata_implementation.program_id,
        );

        let mut accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            (ata, AccountBuilder::system_account(0)),
            (wallet, AccountBuilder::system_account(0)),
            (
                mint,
                AccountBuilder::mint_account(0, token_program_id, extended_mint),
            ),
        ];
        accounts.extend(create_standard_program_accounts(token_program_id));

        if with_rent {
            accounts.push((rent::id(), AccountBuilder::rent_sysvar()));
        }

        // Setup topup scenario if requested
        if topup {
            if let Some((_, ata_acc)) = accounts.iter_mut().find(|(k, _)| *k == ata) {
                modify_account_for_topup(ata_acc);
            }
        }

        let mut metas = vec![
            AccountMeta::new(payer, true),
            AccountMeta::new(ata, false),
            AccountMeta::new_readonly(wallet, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
            AccountMeta::new_readonly(*token_program_id, false),
        ];

        if with_rent {
            metas.push(AccountMeta::new_readonly(rent::id(), false));
        }

        let raw_data = build_instruction_data(0, &[]); // Create instruction (discriminator 0 with no bump)
        let ix = Instruction {
            program_id: ata_implementation.program_id,
            accounts: metas,
            data: ata_implementation.adapt_instruction_data(raw_data),
        };

        (ix, accounts)
    }

    fn build_create_idempotent(
        ata_implementation: &AtaImplementation,
        token_program_id: &Pubkey,
        with_rent: bool,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let payer = const_pk(1);
        let mint = const_pk(2);
        // Wallets are independent of ATA program - use fixed wallet address
        let wallet = const_pk(3);

        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            &ata_implementation.program_id,
        );

        let mut accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            (
                ata,
                AccountBuilder::token_account(&mint, &wallet, 0, token_program_id),
            ),
            (wallet, AccountBuilder::system_account(0)),
            (
                mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (
                SYSTEM_PROGRAM_ID,
                AccountBuilder::executable_program(NATIVE_LOADER_ID),
            ),
            (
                *token_program_id,
                AccountBuilder::executable_program(LOADER_V3),
            ),
        ];

        if with_rent {
            accounts.push((rent::id(), AccountBuilder::rent_sysvar()));
        }

        let mut metas = vec![
            AccountMeta::new(payer, true),
            AccountMeta::new(ata, false),
            AccountMeta::new_readonly(wallet, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
            AccountMeta::new_readonly(*token_program_id, false),
        ];

        if with_rent {
            metas.push(AccountMeta::new_readonly(rent::id(), false));
        }

        let raw_data = build_instruction_data(1, &[]); // CreateIdempotent discriminator
        let ix = Instruction {
            program_id: ata_implementation.program_id,
            accounts: metas,
            data: ata_implementation.adapt_instruction_data(raw_data),
        };

        (ix, accounts)
    }

    fn build_recover(
        ata_implementation: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        // Fixed mints and wallets - independent of ATA program
        let owner_mint = const_pk(20);
        let wallet = const_pk(30);
        let nested_mint = const_pk(40);

        let (owner_ata, _) = Pubkey::find_program_address(
            &[
                wallet.as_ref(),
                token_program_id.as_ref(),
                owner_mint.as_ref(),
            ],
            &ata_implementation.program_id,
        );

        let (nested_ata, _) = Pubkey::find_program_address(
            &[
                owner_ata.as_ref(),
                token_program_id.as_ref(),
                nested_mint.as_ref(),
            ],
            &ata_implementation.program_id,
        );

        let (dest_ata, _) = Pubkey::find_program_address(
            &[
                wallet.as_ref(),
                token_program_id.as_ref(),
                nested_mint.as_ref(),
            ],
            &ata_implementation.program_id,
        );

        let accounts = vec![
            (
                nested_ata,
                AccountBuilder::token_account(&nested_mint, &owner_ata, 100, token_program_id),
            ),
            (
                nested_mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (
                dest_ata,
                AccountBuilder::token_account(&nested_mint, &wallet, 0, token_program_id),
            ),
            (
                owner_ata,
                AccountBuilder::token_account(&owner_mint, &wallet, 0, token_program_id),
            ),
            (
                owner_mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (wallet, AccountBuilder::system_account(1_000_000_000)),
            (
                *token_program_id,
                AccountBuilder::executable_program(LOADER_V3),
            ),
            (
                Pubkey::from(spl_token_interface::program::ID),
                AccountBuilder::executable_program(LOADER_V3),
            ),
        ];

        let raw_data = vec![2u8]; // RecoverNested discriminator
        let ix = Instruction {
            program_id: ata_implementation.program_id,
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
            data: ata_implementation.adapt_instruction_data(raw_data),
        };

        (ix, accounts)
    }

    #[allow(clippy::too_many_arguments)]
    fn build_create_with_bump(
        ata_implementation: &AtaImplementation,
        token_program_id: &Pubkey,
        extended_mint: bool,
        with_rent: bool,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let base_offset = calculate_bump_base_offset(extended_mint, with_rent);
        let (payer, mint, wallet) = build_base_test_accounts(
            base_offset,
            token_program_id,
            &ata_implementation.program_id,
        );

        let (ata, bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            &ata_implementation.program_id,
        );

        let mut accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            (ata, AccountBuilder::system_account(0)),
            (wallet, AccountBuilder::system_account(0)),
            (
                mint,
                AccountBuilder::mint_account(0, token_program_id, extended_mint),
            ),
            (
                SYSTEM_PROGRAM_ID,
                AccountBuilder::executable_program(NATIVE_LOADER_ID),
            ),
            (
                *token_program_id,
                AccountBuilder::executable_program(LOADER_V3),
            ),
        ];

        if with_rent {
            accounts.push((rent::id(), AccountBuilder::rent_sysvar()));
        }

        let mut metas = vec![
            AccountMeta::new(payer, true),
            AccountMeta::new(ata, false),
            AccountMeta::new_readonly(wallet, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
            AccountMeta::new_readonly(*token_program_id, false),
        ];

        if with_rent {
            metas.push(AccountMeta::new_readonly(rent::id(), false));
        }

        let raw_data = build_instruction_data(0, &[bump]); // Create instruction (discriminator 0) with bump
        let ix = Instruction {
            program_id: ata_implementation.program_id,
            accounts: metas,
            data: ata_implementation.adapt_instruction_data(raw_data),
        };

        (ix, accounts)
    }

    fn build_worst_case_bump_scenario(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (
        (Instruction, Vec<(Pubkey, Account)>),
        (Instruction, Vec<(Pubkey, Account)>),
    ) {
        // Use fixed wallet and mint - independent of ATA program
        // These values were chosen to produce a low bump for worst-case testing
        let worst_wallet = const_pk(200);
        let mint = const_pk(199); // Fixed mint for consistency

        let (ata, bump) = Pubkey::find_program_address(
            &[
                worst_wallet.as_ref(),
                token_program_id.as_ref(),
                mint.as_ref(),
            ],
            program_id,
        );

        println!(
            "Worst case bump scenario: wallet={}, bump={} (lower = more expensive)",
            worst_wallet, bump
        );

        let accounts = vec![
            (const_pk(198), AccountBuilder::system_account(1_000_000_000)), // payer
            (ata, AccountBuilder::system_account(0)),
            (worst_wallet, AccountBuilder::system_account(0)),
            (
                mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (
                SYSTEM_PROGRAM_ID,
                AccountBuilder::executable_program(NATIVE_LOADER_ID),
            ),
            (
                *token_program_id,
                AccountBuilder::executable_program(LOADER_V3),
            ),
        ];

        let metas = vec![
            AccountMeta::new(const_pk(198), true), // payer
            AccountMeta::new(ata, false),
            AccountMeta::new_readonly(worst_wallet, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
            AccountMeta::new_readonly(*token_program_id, false),
        ];

        // Create instruction (expensive find_program_address)
        let create_ix = Instruction {
            program_id: *program_id,
            accounts: metas.clone(),
            data: vec![0u8], // Create discriminator
        };

        // CreateWithBump instruction (skips find_program_address)
        let create_with_bump_ix = Instruction {
            program_id: *program_id,
            accounts: metas,
            data: vec![0u8, bump], // Create discriminator + bump
        };

        (
            (create_ix, accounts.clone()),
            (create_with_bump_ix, accounts),
        )
    }

    fn build_recover_with_bump(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        // Fixed mints and wallets - independent of ATA program
        let owner_mint = const_pk(21); // Different from regular recover to avoid collisions
        let wallet = const_pk(31);
        let nested_mint = const_pk(41);

        let (owner_ata, bump) = Pubkey::find_program_address(
            &[
                wallet.as_ref(),
                token_program_id.as_ref(),
                owner_mint.as_ref(),
            ],
            program_id,
        );

        let (nested_ata, _) = Pubkey::find_program_address(
            &[
                owner_ata.as_ref(),
                token_program_id.as_ref(),
                nested_mint.as_ref(),
            ],
            program_id,
        );

        let (dest_ata, _) = Pubkey::find_program_address(
            &[
                wallet.as_ref(),
                token_program_id.as_ref(),
                nested_mint.as_ref(),
            ],
            program_id,
        );

        let accounts = vec![
            (
                nested_ata,
                AccountBuilder::token_account(&nested_mint, &owner_ata, 100, token_program_id),
            ),
            (
                nested_mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (
                dest_ata,
                AccountBuilder::token_account(&nested_mint, &wallet, 0, token_program_id),
            ),
            (
                owner_ata,
                AccountBuilder::token_account(&owner_mint, &wallet, 0, token_program_id),
            ),
            (
                owner_mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (wallet, AccountBuilder::system_account(1_000_000_000)),
            (
                *token_program_id,
                AccountBuilder::executable_program(LOADER_V3),
            ),
            (
                Pubkey::from(spl_token_interface::program::ID),
                AccountBuilder::executable_program(LOADER_V3),
            ),
        ];

        let ix = Instruction {
            program_id: *program_id,
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
            data: vec![2u8, bump], // RecoverNested discriminator + bump
        };

        (ix, accounts)
    }

    fn build_recover_multisig(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        // Fixed mints and wallets - independent of ATA program
        let owner_mint = const_pk(20);
        let nested_mint = const_pk(40);
        let wallet_ms = const_pk(60);

        let signer1 = Pubkey::new_unique();
        let signer2 = Pubkey::new_unique();
        let signer3 = Pubkey::new_unique();

        let (owner_ata_ms, _) = Pubkey::find_program_address(
            &[
                wallet_ms.as_ref(),
                token_program_id.as_ref(),
                owner_mint.as_ref(),
            ],
            program_id,
        );

        let (nested_ata_ms, _) = Pubkey::find_program_address(
            &[
                owner_ata_ms.as_ref(),
                token_program_id.as_ref(),
                nested_mint.as_ref(),
            ],
            program_id,
        );

        let (dest_ata_ms, _) = Pubkey::find_program_address(
            &[
                wallet_ms.as_ref(),
                token_program_id.as_ref(),
                nested_mint.as_ref(),
            ],
            program_id,
        );

        let accounts = vec![
            (
                nested_ata_ms,
                AccountBuilder::token_account(&nested_mint, &owner_ata_ms, 100, token_program_id),
            ),
            (
                nested_mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (
                dest_ata_ms,
                AccountBuilder::token_account(&nested_mint, &wallet_ms, 0, token_program_id),
            ),
            (
                owner_ata_ms,
                AccountBuilder::token_account(&owner_mint, &wallet_ms, 0, token_program_id),
            ),
            (
                owner_mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (
                wallet_ms,
                Account {
                    lamports: 1_000_000_000,
                    data: AccountBuilder::multisig_data(2, &[signer1, signer2, signer3]),
                    owner: *token_program_id,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                *token_program_id,
                AccountBuilder::executable_program(LOADER_V3),
            ),
            (
                Pubkey::from(spl_token_interface::program::ID),
                AccountBuilder::executable_program(LOADER_V3),
            ),
            (signer1, AccountBuilder::system_account(1_000_000_000)),
            (signer2, AccountBuilder::system_account(1_000_000_000)),
            (signer3, AccountBuilder::system_account(1_000_000_000)),
        ];

        let mut metas = vec![
            AccountMeta::new(nested_ata_ms, false),
            AccountMeta::new_readonly(nested_mint, false),
            AccountMeta::new(dest_ata_ms, false),
            AccountMeta::new(owner_ata_ms, false),
            AccountMeta::new_readonly(owner_mint, false),
            AccountMeta::new(wallet_ms, false),
            AccountMeta::new_readonly(*token_program_id, false),
            AccountMeta::new_readonly(Pubkey::from(spl_token_interface::program::ID), false),
        ];

        // Add signer metas
        metas.push(AccountMeta::new_readonly(signer1, true));
        metas.push(AccountMeta::new_readonly(signer2, true));
        metas.push(AccountMeta::new_readonly(signer3, false));

        let ix = Instruction {
            program_id: *program_id,
            accounts: metas,
            data: vec![2u8], // RecoverNested discriminator
        };

        (ix, accounts)
    }

    fn build_recover_multisig_with_bump(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        // Fixed mints and wallets - independent of ATA program
        let owner_mint = const_pk(22); // Different from regular recover to avoid collisions
        let nested_mint = const_pk(42);
        let wallet_ms = const_pk(61);

        let signer1 = Pubkey::new_unique();
        let signer2 = Pubkey::new_unique();
        let signer3 = Pubkey::new_unique();

        let (owner_ata_ms, bump) = Pubkey::find_program_address(
            &[
                wallet_ms.as_ref(),
                token_program_id.as_ref(),
                owner_mint.as_ref(),
            ],
            program_id,
        );

        let (nested_ata_ms, _) = Pubkey::find_program_address(
            &[
                owner_ata_ms.as_ref(),
                token_program_id.as_ref(),
                nested_mint.as_ref(),
            ],
            program_id,
        );

        let (dest_ata_ms, _) = Pubkey::find_program_address(
            &[
                wallet_ms.as_ref(),
                token_program_id.as_ref(),
                nested_mint.as_ref(),
            ],
            program_id,
        );

        let accounts = vec![
            (
                nested_ata_ms,
                AccountBuilder::token_account(&nested_mint, &owner_ata_ms, 100, token_program_id),
            ),
            (
                nested_mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (
                dest_ata_ms,
                AccountBuilder::token_account(&nested_mint, &wallet_ms, 0, token_program_id),
            ),
            (
                owner_ata_ms,
                AccountBuilder::token_account(&owner_mint, &wallet_ms, 0, token_program_id),
            ),
            (
                owner_mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (
                wallet_ms,
                Account {
                    lamports: 1_000_000_000,
                    data: AccountBuilder::multisig_data(2, &[signer1, signer2, signer3]),
                    owner: *token_program_id,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                *token_program_id,
                AccountBuilder::executable_program(LOADER_V3),
            ),
            (
                Pubkey::from(spl_token_interface::program::ID),
                AccountBuilder::executable_program(LOADER_V3),
            ),
            (signer1, AccountBuilder::system_account(1_000_000_000)),
            (signer2, AccountBuilder::system_account(1_000_000_000)),
            (signer3, AccountBuilder::system_account(1_000_000_000)),
        ];

        let mut metas = vec![
            AccountMeta::new(nested_ata_ms, false),
            AccountMeta::new_readonly(nested_mint, false),
            AccountMeta::new(dest_ata_ms, false),
            AccountMeta::new(owner_ata_ms, false),
            AccountMeta::new_readonly(owner_mint, false),
            AccountMeta::new(wallet_ms, false),
            AccountMeta::new_readonly(*token_program_id, false),
            AccountMeta::new_readonly(Pubkey::from(spl_token_interface::program::ID), false),
        ];

        // Add signer metas
        metas.push(AccountMeta::new_readonly(signer1, true));
        metas.push(AccountMeta::new_readonly(signer2, true));
        metas.push(AccountMeta::new_readonly(signer3, false));

        let ix = Instruction {
            program_id: *program_id,
            accounts: metas,
            data: vec![2u8, bump], // RecoverNested discriminator + bump
        };

        (ix, accounts)
    }
}

// ============================ SETUP AND CONFIGURATION =============================

impl BenchmarkSetup {
    fn validate_ata_setup(
        mollusk: &Mollusk,
        ata_implementation: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> Result<(), String> {
        let (test_ix, test_accounts) = TestCaseBuilder::build_create(
            ata_implementation,
            token_program_id,
            false, // not extended
            false, // no rent
            false, // no topup
        );

        let result = mollusk.process_instruction(&test_ix, &test_accounts);

        match result.program_result {
            mollusk_svm::result::ProgramResult::Success => {
                println!(
                    "✓ Benchmark setup validation passed for {}",
                    ata_implementation.name
                );
                Ok(())
            }
            _ => Err(format!(
                "Setup validation failed for {}: {:?}",
                ata_implementation.name, result.program_result
            )),
        }
    }
}

// =============================== COMPARISON FRAMEWORK ===============================

struct ComparisonRunner;

impl ComparisonRunner {
    fn run_full_comparison(
        p_ata_impl: &AtaImplementation,
        original_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> Vec<ComparisonResult> {
        println!("\n=== P-ATA VS ORIGINAL ATA COMPREHENSIVE COMPARISON ===");
        println!("P-ATA Program ID: {}", p_ata_impl.program_id);
        println!("Original Program ID: {}", original_impl.program_id);
        println!("Token Program ID: {}", token_program_id);

        let mut results = Vec::new();

        // Test scenarios that work with both implementations
        let test_scenarios = [
            // Create instruction variants
            ("create_base", false, false, false),
            ("create_with_rent", false, true, false),
            ("create_topup", false, false, true),
        ];

        for (test_name, extended, with_rent, topup) in test_scenarios {
            let comparison = Self::run_create_test(
                test_name,
                p_ata_impl,
                original_impl,
                token_program_id,
                extended,
                with_rent,
                topup,
            );
            common::ComparisonRunner::print_comparison_result(&comparison);
            results.push(comparison);
        }

        // CreateIdempotent variants
        let idempotent_tests = [
            ("create_idempotent_base", false),
            ("create_idempotent_rent", true),
        ];

        for (test_name, with_rent) in idempotent_tests {
            let comparison = Self::run_create_idempotent_test(
                test_name,
                p_ata_impl,
                original_impl,
                token_program_id,
                with_rent,
            );
            common::ComparisonRunner::print_comparison_result(&comparison);
            results.push(comparison);
        }

        // RecoverNested test
        let comparison = Self::run_recover_test(
            "recover_nested",
            p_ata_impl,
            original_impl,
            token_program_id,
        );
        common::ComparisonRunner::print_comparison_result(&comparison);
        results.push(comparison);

        // Worst-case create scenario (expensive find_program_address)
        let comparison = Self::run_worst_case_create_test(
            "worst_case_create",
            p_ata_impl,
            original_impl,
            token_program_id,
        );
        common::ComparisonRunner::print_comparison_result(&comparison);
        results.push(comparison);

        // Token-2022 test (uses actual Token-2022 program)
        let comparison = Self::run_token2022_test("create_token2022", p_ata_impl, original_impl);
        common::ComparisonRunner::print_comparison_result(&comparison);
        results.push(comparison);

        // Test P-ATA specific optimizations (these may fail on original)
        let comparison = Self::run_create_with_bump_test(
            "create_with_bump",
            p_ata_impl,
            original_impl,
            token_program_id,
        );
        common::ComparisonRunner::print_comparison_result(&comparison);
        results.push(comparison);

        let comparison = Self::run_recover_with_bump_test(
            "recover_with_bump",
            p_ata_impl,
            original_impl,
            token_program_id,
        );
        common::ComparisonRunner::print_comparison_result(&comparison);
        results.push(comparison);

        Self::print_summary(&results);
        Self::output_structured_data(&results);
        results
    }

    fn output_structured_data(results: &[ComparisonResult]) {
        let mut json_entries = Vec::new();

        for result in results {
            // Only include successful comparisons or known optimization cases
            let (p_ata_cu, original_cu, savings_percent, compatibility) =
                match (&result.p_ata.success, &result.original.success) {
                    (true, true) => {
                        let savings = result.original.compute_units as i64
                            - result.p_ata.compute_units as i64;
                        let percentage = if result.original.compute_units > 0 {
                            (savings as f64 / result.original.compute_units as f64) * 100.0
                        } else {
                            0.0
                        };

                        let compat = match result.compatibility_status {
                            common::CompatibilityStatus::Identical => "identical",
                            common::CompatibilityStatus::OptimizedBehavior => "optimized",
                            common::CompatibilityStatus::ExpectedDifferences => {
                                "expected_difference"
                            }
                            _ => "unknown",
                        };

                        (
                            result.p_ata.compute_units,
                            result.original.compute_units,
                            percentage,
                            compat,
                        )
                    }
                    (true, false) => {
                        // P-ATA works, Original fails - optimization case
                        (result.p_ata.compute_units, 0, 0.0, "optimized")
                    }
                    _ => continue, // Skip cases where P-ATA fails
                };

            let entry = format!(
                r#"    "{}": {{
      "p_ata_cu": {},
      "original_cu": {},
      "savings_percent": {:.1},
      "compatibility": "{}",
      "type": "performance_test"
    }}"#,
                result.test_name, p_ata_cu, original_cu, savings_percent, compatibility
            );
            json_entries.push(entry);
        }

        let output = format!(
            r#"{{
  "timestamp": "{}",
  "performance_tests": {{
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

        // Write performance results
        if let Err(e) = std::fs::write("benchmark_results/performance_results.json", output) {
            eprintln!("Failed to write performance results: {}", e);
        } else {
            println!(
                "\n📊 Performance results written to benchmark_results/performance_results.json"
            );
        }
    }

    fn print_summary(results: &[ComparisonResult]) {
        println!("\n=== BYTE-FOR-BYTE TEST SUMMARY ===");

        // Print each test with color-coded status
        for result in results {
            let status_indicator = match result.compatibility_status {
                CompatibilityStatus::Identical => {
                    // Special handling for create_with_bump - it's a P-ATA optimization
                    if result.test_name == "create_with_bump" {
                        "P-ATA OPTIMIZATION"
                    } else {
                        "\x1b[32m🟢 IDENTICAL\x1b[0m"
                    }
                }
                CompatibilityStatus::OptimizedBehavior => "P-ATA OPTIMIZATION",
                CompatibilityStatus::ExpectedDifferences => {
                    "\x1b[33m🟡 EXPECTED DIFFERENCES\x1b[0m"
                }
                CompatibilityStatus::BothRejected => "\x1b[31m🔴 BOTH REJECTED\x1b[0m",
                CompatibilityStatus::AccountMismatch => "\x1b[31m🔴 ACCOUNT MISMATCH\x1b[0m",
                CompatibilityStatus::IncompatibleFailure => {
                    "\x1b[31m🔴 INCOMPATIBLE FAILURE\x1b[0m"
                }
                CompatibilityStatus::IncompatibleSuccess => {
                    "\x1b[31m🔴 INCOMPATIBLE SUCCESS\x1b[0m"
                }
            };

            let differences = Self::get_test_differences(result);
            let differences_str = if differences.is_empty() {
                String::new()
            } else {
                format!(" ({})", differences.join(", "))
            };

            println!(
                "  {} {:<18}{}",
                status_indicator, result.test_name, differences_str
            );
        }
    }

    fn get_test_differences(result: &ComparisonResult) -> Vec<String> {
        let mut differences = Vec::new();

        match result.test_name.as_str() {
            "create_with_bump" => {
                differences.push("P-ATA uses CreateWithBump".to_string());
            }
            "recover_with_bump" => {
                if !result.original.success {
                    differences.push("Original fails".to_string());
                }
            }
            _ => {}
        }

        differences
    }

    fn format_compute_savings(result: &ComparisonResult) -> String {
        if result.p_ata.success && result.original.success {
            let savings = result.original.compute_units as i64 - result.p_ata.compute_units as i64;
            let percentage = if result.original.compute_units > 0 {
                (savings as f64 / result.original.compute_units as f64) * 100.0
            } else {
                0.0
            };
            format!("[-{:.1}% CUs]", percentage)
        } else if result.p_ata.success && !result.original.success {
            "[P-ATA works]".to_string()
        } else if !result.p_ata.success && result.original.success {
            "[P-ATA fails]".to_string()
        } else {
            "[Both fail]".to_string()
        }
    }

    // Test scenario functions
    fn run_create_test(
        test_name: &str,
        p_ata_impl: &AtaImplementation,
        original_impl: &AtaImplementation,
        token_program_id: &Pubkey,
        extended: bool,
        with_rent: bool,
        topup: bool,
    ) -> ComparisonResult {
        let (p_ata_ix, p_ata_accounts) =
            TestCaseBuilder::build_create(p_ata_impl, token_program_id, extended, with_rent, topup);
        let (original_ix, original_accounts) = TestCaseBuilder::build_create(
            original_impl,
            token_program_id,
            extended,
            with_rent,
            topup,
        );

        if common::VerboseComparison::is_enabled() {
            common::ComparisonRunner::run_verbose_comparison(
                test_name,
                &p_ata_ix,
                &p_ata_accounts,
                &original_ix,
                &original_accounts,
                p_ata_impl,
                original_impl,
                token_program_id,
            )
        } else {
            let p_ata_result = common::ComparisonRunner::run_single_benchmark(
                test_name,
                &p_ata_ix,
                &p_ata_accounts,
                p_ata_impl,
                token_program_id,
            );
            let original_result = common::ComparisonRunner::run_single_benchmark(
                test_name,
                &original_ix,
                &original_accounts,
                original_impl,
                token_program_id,
            );

            common::ComparisonRunner::create_comparison_result(
                test_name,
                p_ata_result,
                original_result,
            )
        }
    }

    fn run_create_idempotent_test(
        test_name: &str,
        p_ata_impl: &AtaImplementation,
        original_impl: &AtaImplementation,
        token_program_id: &Pubkey,
        with_rent: bool,
    ) -> ComparisonResult {
        let (p_ata_ix, p_ata_accounts) =
            TestCaseBuilder::build_create_idempotent(p_ata_impl, token_program_id, with_rent);
        let (original_ix, original_accounts) =
            TestCaseBuilder::build_create_idempotent(original_impl, token_program_id, with_rent);

        if common::VerboseComparison::is_enabled() {
            common::ComparisonRunner::run_verbose_comparison(
                test_name,
                &p_ata_ix,
                &p_ata_accounts,
                &original_ix,
                &original_accounts,
                p_ata_impl,
                original_impl,
                token_program_id,
            )
        } else {
            let p_ata_result = common::ComparisonRunner::run_single_benchmark(
                test_name,
                &p_ata_ix,
                &p_ata_accounts,
                p_ata_impl,
                token_program_id,
            );
            let original_result = common::ComparisonRunner::run_single_benchmark(
                test_name,
                &original_ix,
                &original_accounts,
                original_impl,
                token_program_id,
            );

            common::ComparisonRunner::create_comparison_result(
                test_name,
                p_ata_result,
                original_result,
            )
        }
    }

    fn run_recover_test(
        test_name: &str,
        p_ata_impl: &AtaImplementation,
        original_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> ComparisonResult {
        let (p_ata_ix, p_ata_accounts) =
            TestCaseBuilder::build_recover(p_ata_impl, token_program_id);
        let (original_ix, original_accounts) =
            TestCaseBuilder::build_recover(original_impl, token_program_id);

        if common::VerboseComparison::is_enabled() {
            common::ComparisonRunner::run_verbose_comparison(
                test_name,
                &p_ata_ix,
                &p_ata_accounts,
                &original_ix,
                &original_accounts,
                p_ata_impl,
                original_impl,
                token_program_id,
            )
        } else {
            let p_ata_result = common::ComparisonRunner::run_single_benchmark(
                test_name,
                &p_ata_ix,
                &p_ata_accounts,
                p_ata_impl,
                token_program_id,
            );
            let original_result = common::ComparisonRunner::run_single_benchmark(
                test_name,
                &original_ix,
                &original_accounts,
                original_impl,
                token_program_id,
            );

            common::ComparisonRunner::create_comparison_result(
                test_name,
                p_ata_result,
                original_result,
            )
        }
    }

    fn run_create_with_bump_test(
        test_name: &str,
        p_ata_impl: &AtaImplementation,
        original_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> ComparisonResult {
        let (p_ata_ix, p_ata_accounts) =
            TestCaseBuilder::build_create_with_bump(p_ata_impl, token_program_id, false, false);
        let (original_ix, original_accounts) =
            TestCaseBuilder::build_create_with_bump(original_impl, token_program_id, false, false);

        if common::VerboseComparison::is_enabled() {
            common::ComparisonRunner::run_verbose_comparison(
                test_name,
                &p_ata_ix,
                &p_ata_accounts,
                &original_ix,
                &original_accounts,
                p_ata_impl,
                original_impl,
                token_program_id,
            )
        } else {
            let p_ata_result = common::ComparisonRunner::run_single_benchmark(
                test_name,
                &p_ata_ix,
                &p_ata_accounts,
                p_ata_impl,
                token_program_id,
            );
            let original_result = common::ComparisonRunner::run_single_benchmark(
                test_name,
                &original_ix,
                &original_accounts,
                original_impl,
                token_program_id,
            );

            common::ComparisonRunner::create_comparison_result(
                test_name,
                p_ata_result,
                original_result,
            )
        }
    }

    fn run_worst_case_create_test(
        test_name: &str,
        p_ata_impl: &AtaImplementation,
        original_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> ComparisonResult {
        // Build worst-case create scenario (low bump = expensive find_program_address)
        // Use only the regular Create instruction so both implementations can be compared
        let ((p_ata_ix, p_ata_accounts), _) = TestCaseBuilder::build_worst_case_bump_scenario(
            &p_ata_impl.program_id,
            token_program_id,
        );
        let ((original_ix, original_accounts), _) = TestCaseBuilder::build_worst_case_bump_scenario(
            &original_impl.program_id,
            token_program_id,
        );

        if common::VerboseComparison::is_enabled() {
            common::ComparisonRunner::run_verbose_comparison(
                test_name,
                &p_ata_ix,
                &p_ata_accounts,
                &original_ix,
                &original_accounts,
                p_ata_impl,
                original_impl,
                token_program_id,
            )
        } else {
            let p_ata_result = common::ComparisonRunner::run_single_benchmark(
                test_name,
                &p_ata_ix,
                &p_ata_accounts,
                p_ata_impl,
                token_program_id,
            );
            let original_result = common::ComparisonRunner::run_single_benchmark(
                test_name,
                &original_ix,
                &original_accounts,
                original_impl,
                token_program_id,
            );

            common::ComparisonRunner::create_comparison_result(
                test_name,
                p_ata_result,
                original_result,
            )
        }
    }

    fn run_token2022_test(
        test_name: &str,
        p_ata_impl: &AtaImplementation,
        original_impl: &AtaImplementation,
    ) -> ComparisonResult {
        // Build Token-2022 test using the actual Token-2022 program ID
        let (p_ata_ix, p_ata_accounts) =
            common::build_create_token2022_simulation(&p_ata_impl.program_id);
        let (original_ix, original_accounts) =
            common::build_create_token2022_simulation(&original_impl.program_id);

        // Use a dummy token program ID for the benchmark runner (Token-2022 program is added separately)
        let token_2022_program_id = Pubkey::new_from_array(pinocchio_pubkey::pubkey!(
            "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
        ));

        if common::VerboseComparison::is_enabled() {
            common::ComparisonRunner::run_verbose_comparison(
                test_name,
                &p_ata_ix,
                &p_ata_accounts,
                &original_ix,
                &original_accounts,
                p_ata_impl,
                original_impl,
                &token_2022_program_id,
            )
        } else {
            let p_ata_result = common::ComparisonRunner::run_single_benchmark(
                test_name,
                &p_ata_ix,
                &p_ata_accounts,
                p_ata_impl,
                &token_2022_program_id,
            );
            let original_result = common::ComparisonRunner::run_single_benchmark(
                test_name,
                &original_ix,
                &original_accounts,
                original_impl,
                &token_2022_program_id,
            );

            common::ComparisonRunner::create_comparison_result(
                test_name,
                p_ata_result,
                original_result,
            )
        }
    }

    fn run_recover_with_bump_test(
        test_name: &str,
        p_ata_impl: &AtaImplementation,
        original_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> ComparisonResult {
        let (p_ata_ix, p_ata_accounts) =
            TestCaseBuilder::build_recover_with_bump(&p_ata_impl.program_id, token_program_id);
        let (original_ix, original_accounts) =
            TestCaseBuilder::build_recover_with_bump(&original_impl.program_id, token_program_id);

        if common::VerboseComparison::is_enabled() {
            common::ComparisonRunner::run_verbose_comparison(
                test_name,
                &p_ata_ix,
                &p_ata_accounts,
                &original_ix,
                &original_accounts,
                p_ata_impl,
                original_impl,
                token_program_id,
            )
        } else {
            let p_ata_result = common::ComparisonRunner::run_single_benchmark(
                test_name,
                &p_ata_ix,
                &p_ata_accounts,
                p_ata_impl,
                token_program_id,
            );
            let original_result = common::ComparisonRunner::run_single_benchmark(
                test_name,
                &original_ix,
                &original_accounts,
                original_impl,
                token_program_id,
            );

            common::ComparisonRunner::create_comparison_result(
                test_name,
                p_ata_result,
                original_result,
            )
        }
    }
}

// =============================== BENCHMARK RUNNER ===============================

struct BenchmarkRunner;

impl BenchmarkRunner {
    fn run_isolated_benchmark(
        name: &str,
        ix: &Instruction,
        accounts: &[(Pubkey, Account)],
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) {
        println!("\n=== Running benchmark: {} ===", name);

        let must_pass = name != "create_token2022_sim";
        run_benchmark_with_validation(name, ix, accounts, program_id, token_program_id, must_pass);
    }

    fn run_all_benchmarks(ata_implementation: &AtaImplementation, token_program_id: &Pubkey) {
        println!(
            "\n=== Running all benchmarks for {} ===",
            ata_implementation.name
        );

        let test_cases = [
            (
                "create_base",
                TestCaseBuilder::build_create(
                    ata_implementation,
                    token_program_id,
                    false,
                    false,
                    false,
                ),
            ),
            (
                "create_rent",
                TestCaseBuilder::build_create(
                    ata_implementation,
                    token_program_id,
                    false,
                    true,
                    false,
                ),
            ),
            (
                "create_topup",
                TestCaseBuilder::build_create(
                    ata_implementation,
                    token_program_id,
                    false,
                    false,
                    true,
                ),
            ),
            (
                "create_idemp",
                TestCaseBuilder::build_create_idempotent(
                    ata_implementation,
                    token_program_id,
                    false,
                ),
            ),
            (
                "create_with_bump_base",
                TestCaseBuilder::build_create_with_bump(
                    ata_implementation,
                    token_program_id,
                    false,
                    false,
                ),
            ),
            (
                "create_with_bump_rent",
                TestCaseBuilder::build_create_with_bump(
                    ata_implementation,
                    token_program_id,
                    false,
                    true,
                ),
            ),
            (
                "recover",
                TestCaseBuilder::build_recover(ata_implementation, token_program_id),
            ),
        ];

        for (name, (ix, accounts)) in test_cases {
            Self::run_isolated_benchmark(
                name,
                &ix,
                &accounts,
                &ata_implementation.program_id,
                token_program_id,
            );
        }

        // Run worst-case bump scenario comparison
        Self::run_worst_case_bump_comparison(&ata_implementation.program_id, token_program_id);
    }

    fn run_worst_case_bump_comparison(program_id: &Pubkey, token_program_id: &Pubkey) {
        println!("\n=== Worst-Case Bump Scenario Comparison ===");
        let ((create_ix, create_accounts), (create_with_bump_ix, create_with_bump_accounts)) =
            TestCaseBuilder::build_worst_case_bump_scenario(program_id, token_program_id);

        // Benchmark regular Create (expensive)
        Self::run_isolated_benchmark(
            "worst_case_create",
            &create_ix,
            &create_accounts,
            program_id,
            token_program_id,
        );

        // Benchmark CreateWithBump (optimized)
        Self::run_isolated_benchmark(
            "worst_case_create_with_bump",
            &create_with_bump_ix,
            &create_with_bump_accounts,
            program_id,
            token_program_id,
        );
    }
}

// ================================= MAIN =====================================

fn main() {
    // Setup logging
    let _ = solana_logger::setup_with(
        "info,solana_runtime=info,solana_program_runtime=info,mollusk=debug",
    );

    // Get manifest directory and setup environment
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    println!("CARGO_MANIFEST_DIR: {}", manifest_dir);
    println!("🔨 P-ATA vs Original ATA Benchmark Suite");

    BenchmarkSetup::setup_sbf_environment(manifest_dir);
    let (p_ata_program_id, original_ata_program_id, token_program_id) =
        BenchmarkSetup::load_both_program_ids(manifest_dir);

    // Create implementation structures
    let p_ata_impl = AtaImplementation::p_ata(p_ata_program_id);

    println!("Token Program ID: {}", token_program_id);

    if let Some(original_program_id) = original_ata_program_id {
        // COMPARISON MODE: Both implementations available
        let original_impl = AtaImplementation::original(original_program_id);

        println!("\n🔍 Running comprehensive comparison between implementations");

        // Validate both setups work
        let p_ata_mollusk = common::ComparisonRunner::create_mollusk_for_implementation(
            &p_ata_impl,
            &token_program_id,
        );
        let original_mollusk = common::ComparisonRunner::create_mollusk_for_implementation(
            &original_impl,
            &token_program_id,
        );

        if let Err(e) =
            BenchmarkSetup::validate_ata_setup(&p_ata_mollusk, &p_ata_impl, &token_program_id)
        {
            panic!("P-ATA benchmark setup validation failed: {}", e);
        }

        if let Err(e) =
            BenchmarkSetup::validate_ata_setup(&original_mollusk, &original_impl, &token_program_id)
        {
            panic!("Original ATA benchmark setup validation failed: {}", e);
        }

        // Run comprehensive comparison
        let _comparison_results =
            ComparisonRunner::run_full_comparison(&p_ata_impl, &original_impl, &token_program_id);

        println!("\n✅ Comprehensive comparison completed successfully");
    } else {
        // P-ATA ONLY MODE: Original ATA not available
        println!("\n🔧 Running P-ATA only benchmarks (original ATA not built)");
        println!("   💡 To enable comparison, run: cargo bench --features build-programs");

        // Setup Mollusk with P-ATA only
        let mollusk = fresh_mollusk(&p_ata_program_id, &token_program_id);

        // Validate the setup works
        if let Err(e) = BenchmarkSetup::validate_ata_setup(&mollusk, &p_ata_impl, &token_program_id)
        {
            panic!("P-ATA benchmark setup validation failed: {}", e);
        }

        // Run P-ATA benchmarks
        BenchmarkRunner::run_all_benchmarks(&p_ata_impl, &token_program_id);

        println!("\n✅ P-ATA benchmarks completed successfully");
    }
}

// ================================= HELPERS =====================================

fn build_account_meta(pubkey: &Pubkey, writable: bool, signer: bool) -> AccountMeta {
    AccountMeta {
        pubkey: *pubkey,
        is_writable: writable,
        is_signer: signer,
    }
}

fn build_ata_instruction_metas(
    payer: &Pubkey,
    ata: &Pubkey,
    wallet: &Pubkey,
    mint: &Pubkey,
    system_prog: &Pubkey,
    token_prog: &Pubkey,
) -> Vec<AccountMeta> {
    vec![
        build_account_meta(payer, true, true), // payer (writable, signer)
        build_account_meta(ata, true, false),  // ata (writable, not signer)
        build_account_meta(wallet, false, false), // wallet (readonly, not signer)
        build_account_meta(mint, false, false), // mint (readonly, not signer)
        build_account_meta(system_prog, false, false), // system program (readonly, not signer)
        build_account_meta(token_prog, false, false), // token program (readonly, not signer)
    ]
}

fn build_base_test_accounts(
    base_offset: u8,
    token_program_id: &Pubkey,
    program_id: &Pubkey,
) -> (Pubkey, Pubkey, Pubkey) {
    let payer = const_pk(base_offset);
    let mint = const_pk(base_offset + 1);
    // Wallets are independent of ATA program - use fixed wallet address
    let wallet = const_pk(base_offset + 2);
    (payer, mint, wallet)
}

fn build_standard_account_vec(accounts: &[(Pubkey, Account)]) -> Vec<(Pubkey, Account)> {
    accounts.iter().map(|(k, v)| (*k, v.clone())).collect()
}

fn modify_account_for_topup(account: &mut Account) {
    account.lamports = 1_000_000; // Some lamports but below rent-exempt
    account.data = vec![]; // No data allocated
    account.owner = SYSTEM_PROGRAM_ID; // Still system-owned
}

fn calculate_base_offset(extended_mint: bool, with_rent: bool, topup: bool) -> u8 {
    match (extended_mint, with_rent, topup) {
        (false, false, false) => 10, // create_base
        (false, true, false) => 20,  // create_rent
        (false, false, true) => 30,  // create_topup
        (true, false, false) => 40,  // create_ext
        (true, true, false) => 50,   // create_ext_rent
        (true, false, true) => 60,   // create_ext_topup
        _ => 70,                     // fallback
    }
}

fn calculate_bump_base_offset(extended_mint: bool, with_rent: bool) -> u8 {
    match (extended_mint, with_rent) {
        (false, false) => 90, // create_with_bump_base
        (false, true) => 95,  // create_with_bump_rent
        (true, false) => 100, // create_with_bump_ext
        (true, true) => 105,  // create_with_bump_ext_rent
    }
}

fn configure_bencher<'a>(
    mollusk: Mollusk,
    _name: &'a str,
    must_pass: bool,
    out_dir: &'a str,
) -> MolluskComputeUnitBencher<'a> {
    let mut bencher = MolluskComputeUnitBencher::new(mollusk).out_dir(out_dir);

    if must_pass {
        bencher = bencher.must_pass(true);
    }

    bencher
}

fn execute_benchmark_case<'a>(
    bencher: MolluskComputeUnitBencher<'a>,
    name: &'a str,
    ix: &'a Instruction,
    accounts: &'a [(Pubkey, Account)],
) -> MolluskComputeUnitBencher<'a> {
    bencher.bench((name, ix, accounts))
}

fn run_benchmark_with_validation(
    name: &str,
    ix: &Instruction,
    accounts: &[(Pubkey, Account)],
    program_id: &Pubkey,
    token_program_id: &Pubkey,
    must_pass: bool,
) {
    let cloned_accounts = clone_accounts(accounts);
    let mollusk = fresh_mollusk(program_id, token_program_id);
    let bencher = configure_bencher(mollusk, name, must_pass, "../target/benches");
    let mut bencher = execute_benchmark_case(bencher, name, ix, &cloned_accounts);
    bencher.execute();
}

fn create_standard_program_accounts(token_program_id: &Pubkey) -> Vec<(Pubkey, Account)> {
    vec![
        (
            SYSTEM_PROGRAM_ID,
            AccountBuilder::executable_program(NATIVE_LOADER_ID),
        ),
        (
            *token_program_id,
            AccountBuilder::executable_program(LOADER_V3),
        ),
    ]
}

fn generate_test_case_name(base: &str, extended: bool, with_rent: bool, topup: bool) -> String {
    let mut name = base.to_string();
    if extended {
        name.push_str("_ext");
    }
    if with_rent {
        name.push_str("_rent");
    }
    if topup {
        name.push_str("_topup");
    }
    name
}
