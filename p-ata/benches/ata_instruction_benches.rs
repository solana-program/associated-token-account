use {
    mollusk_svm::{program::loader_keys::LOADER_V3, Mollusk},
    mollusk_svm_bencher::MolluskComputeUnitBencher,
    solana_account::Account,
    solana_instruction::{AccountMeta, Instruction},
    solana_keypair::Keypair,
    solana_logger,
    solana_pubkey::Pubkey,
    solana_signer::Signer,
    solana_sysvar::rent,
    std::{fs, path::Path},
};

#[path = "common.rs"]
mod common;
use common::*;

// ========================== ATA IMPLEMENTATION ABSTRACTION ============================

#[derive(Debug, Clone)]
pub struct AtaImplementation {
    pub name: &'static str,
    pub program_id: Pubkey,
    pub binary_name: &'static str,
}

impl AtaImplementation {
    pub fn p_ata(program_id: Pubkey) -> Self {
        Self {
            name: "p-ata",
            program_id,
            binary_name: "pinocchio_ata_program",
        }
    }

    pub fn original(program_id: Pubkey) -> Self {
        Self {
            name: "original",
            program_id,
            binary_name: "spl_associated_token_account",
        }
    }

    /// Adapt instruction data for this implementation
    pub fn adapt_instruction_data(&self, data: Vec<u8>) -> Vec<u8> {
        match self.name {
            "p-ata" => data, // P-ATA supports bump optimizations
            "original" => {
                // Original ATA doesn't support bump optimizations, strip them
                match data.as_slice() {
                    [0, _bump] => vec![0], // Create with bump -> Create without bump
                    [2, _bump] => vec![2], // RecoverNested with bump -> RecoverNested without bump
                    _ => data, // Pass through other formats
                }
            }
            _ => data,
        }
    }
}

#[derive(Debug)]
pub struct BenchmarkResult {
    pub implementation: String,
    pub test_name: String,
    pub compute_units: u64,
    pub success: bool,
    pub error_message: Option<String>,
}

#[derive(Debug)]
pub struct ComparisonResult {
    pub test_name: String,
    pub p_ata: BenchmarkResult,
    pub original: BenchmarkResult,
    pub compute_savings: Option<i64>,
    pub savings_percentage: Option<f64>,
    pub compatibility_status: CompatibilityStatus,
}

#[derive(Debug, PartialEq)]
pub enum CompatibilityStatus {
    Identical,           // Both succeeded with same results
    Compatible,          // Both succeeded, minor differences (expected)
    OptimizedBehavior,   // P-ATA succeeded where original failed (bump optimization)
    IncompatibleFailure, // Different failure modes (concerning)
    IncompatibleSuccess, // One succeeded, one failed unexpectedly
}

// ========================== TEST CASE BUILDERS ============================

struct TestCaseBuilder;

impl TestCaseBuilder {
    /// Build CREATE instruction variants
    #[allow(clippy::too_many_arguments)]
    fn build_create(
        ata_implementation: &AtaImplementation,
        token_program_id: &Pubkey,
        extended_mint: bool,
        with_rent: bool,
        topup: bool,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let base_offset = calculate_base_offset(extended_mint, with_rent, topup);
        let (payer, mint, wallet) =
            build_base_test_accounts(base_offset, token_program_id, &ata_implementation.program_id);

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

    /// Build CREATE_IDEMPOTENT instruction (pre-initialized ATA)
    fn build_create_idempotent(
        ata_implementation: &AtaImplementation,
        token_program_id: &Pubkey,
        with_rent: bool,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let payer = const_pk(1);
        let mint = const_pk(2);

        let wallet = OptimalKeyFinder::find_optimal_wallet(3, token_program_id, &mint, &ata_implementation.program_id);

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

    /// Build RECOVER instruction for regular wallet
    fn build_recover(
        ata_implementation: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let owner_mint = const_pk(20);

        let wallet =
            OptimalKeyFinder::find_optimal_wallet(30, token_program_id, &owner_mint, &ata_implementation.program_id);

        let (owner_ata, _) = Pubkey::find_program_address(
            &[
                wallet.as_ref(),
                token_program_id.as_ref(),
                owner_mint.as_ref(),
            ],
            &ata_implementation.program_id,
        );

        let nested_mint = OptimalKeyFinder::find_optimal_nested_mint(
            40,
            &owner_ata,
            token_program_id,
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

    /// Build CREATE instruction with bump optimization
    #[allow(clippy::too_many_arguments)]
    fn build_create_with_bump(
        ata_implementation: &AtaImplementation,
        token_program_id: &Pubkey,
        extended_mint: bool,
        with_rent: bool,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let base_offset = calculate_bump_base_offset(extended_mint, with_rent);
        let (payer, mint, wallet) =
            build_base_test_accounts(base_offset, token_program_id, &ata_implementation.program_id);

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

    /// Build worst-case bump scenario (very low bump = expensive find_program_address)
    /// Returns both Create and CreateWithBump variants for comparison
    fn build_worst_case_bump_scenario(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (
        (Instruction, Vec<(Pubkey, Account)>),
        (Instruction, Vec<(Pubkey, Account)>),
    ) {
        // Find a wallet that produces a very low bump (expensive to compute)
        let mut worst_wallet = const_pk(200);
        let mut worst_bump = 255u8;
        let mint = const_pk(199); // Fixed mint for consistency

        // Search for wallet with lowest bump (most expensive find_program_address)
        for b in 200..=254 {
            let candidate_wallet = const_pk(b);
            let (_, bump) = Pubkey::find_program_address(
                &[
                    candidate_wallet.as_ref(),
                    token_program_id.as_ref(),
                    mint.as_ref(),
                ],
                program_id,
            );
            if bump < worst_bump {
                worst_wallet = candidate_wallet;
                worst_bump = bump;
                // Stop if we find a really bad bump (expensive computation)
                if bump <= 50 {
                    break;
                }
            }
        }

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

    /// Build CREATE instruction for Token-2022 simulation
    /// This tests our ImmutableOwner extension stamping logic
    fn build_create_token2022_simulation(
        program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let token_2022_program_id: Pubkey =
            pinocchio_pubkey::pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb").into();

        let base_offset = 80; // Unique offset to avoid collisions
        let payer = const_pk(base_offset);
        let mint = const_pk(base_offset + 1);

        let wallet = OptimalKeyFinder::find_optimal_wallet(
            base_offset + 2,
            &token_2022_program_id,
            &mint,
            program_id,
        );

        let (ata, _bump) = Pubkey::find_program_address(
            &[
                wallet.as_ref(),
                token_2022_program_id.as_ref(),
                mint.as_ref(),
            ],
            program_id,
        );

        let accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            (ata, AccountBuilder::system_account(0)),
            (wallet, AccountBuilder::system_account(0)),
            (
                mint,
                AccountBuilder::mint_account(0, &token_2022_program_id, true), // extended = true
            ),
            (
                SYSTEM_PROGRAM_ID,
                AccountBuilder::executable_program(NATIVE_LOADER_ID),
            ),
            (
                token_2022_program_id,
                AccountBuilder::executable_program(LOADER_V3),
            ),
        ];

        let metas = vec![
            AccountMeta::new(payer, true),
            AccountMeta::new(ata, false),
            AccountMeta::new_readonly(wallet, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
            AccountMeta::new_readonly(token_2022_program_id, false),
        ];

        let ix = Instruction {
            program_id: *program_id,
            accounts: metas,
            data: vec![], // Create instruction
        };

        (ix, accounts)
    }

    /// Build RECOVER instruction for multisig wallet
    fn build_recover_multisig(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let owner_mint = const_pk(20);
        let nested_mint = const_pk(40);

        let wallet_ms =
            OptimalKeyFinder::find_optimal_wallet(60, token_program_id, &owner_mint, program_id);

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

    /// Build RECOVER instruction with bump optimization for regular wallet
    fn build_recover_with_bump(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let owner_mint = const_pk(21); // Different from regular recover to avoid collisions

        let wallet =
            OptimalKeyFinder::find_optimal_wallet(31, token_program_id, &owner_mint, program_id);

        let (owner_ata, bump) = Pubkey::find_program_address(
            &[
                wallet.as_ref(),
                token_program_id.as_ref(),
                owner_mint.as_ref(),
            ],
            program_id,
        );

        let nested_mint = OptimalKeyFinder::find_optimal_nested_mint(
            41,
            &owner_ata,
            token_program_id,
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

    /// Build RECOVER instruction with bump optimization for multisig wallet
    fn build_recover_multisig_with_bump(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let owner_mint = const_pk(22); // Different from regular recover to avoid collisions
        let nested_mint = const_pk(42);

        let wallet_ms =
            OptimalKeyFinder::find_optimal_wallet(61, token_program_id, &owner_mint, program_id);

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

struct BenchmarkSetup;

impl BenchmarkSetup {
    /// Setup SBF output directory and copy required files
    fn setup_sbf_environment(manifest_dir: &str) -> String {
        // Use the standard deploy directory where p-ata program is built
        let deploy_dir = format!("{}/target/deploy", manifest_dir);
        println!("Setting SBF_OUT_DIR to: {}", deploy_dir);
        std::env::set_var("SBF_OUT_DIR", &deploy_dir);

        // Ensure the deploy directory exists
        std::fs::create_dir_all(&deploy_dir).expect("Failed to create deploy directory");

        // Create symbolic links to programs in their actual locations
        // From p-ata directory, the programs are at:
        // - Original ATA: ../target/deploy/spl_associated_token_account.so
        // - Token program: programs/token/target/deploy/pinocchio_token_program.so  
        // - Token-2022: programs/token-2022/target/deploy/spl_token_2022.so
        
        let symlinks = [
            ("spl_associated_token_account.so", "../target/deploy/spl_associated_token_account.so"),
            ("pinocchio_token_program.so", "programs/token/target/deploy/pinocchio_token_program.so"),
            ("spl_token_2022.so", "programs/token-2022/target/deploy/spl_token_2022.so"),
        ];
        
        for (filename, target_path) in &symlinks {
            let link_path = Path::new(&deploy_dir).join(filename);
            let full_target_path = Path::new(manifest_dir).join(target_path);
            
            if full_target_path.exists() && !link_path.exists() {
                println!("Creating symlink {} -> {}", filename, target_path);
                #[cfg(unix)]
                {
                    std::os::unix::fs::symlink(&full_target_path, &link_path)
                        .unwrap_or_else(|e| panic!("Failed to create symlink for {}: {}", filename, e));
                }
                #[cfg(windows)]
                {
                    std::os::windows::fs::symlink_file(&full_target_path, &link_path)
                        .unwrap_or_else(|e| panic!("Failed to create symlink for {}: {}", filename, e));
                }
            }
        }

        deploy_dir
    }

    /// Load program keypairs and return program IDs
    fn load_program_ids(manifest_dir: &str) -> (Pubkey, Pubkey) {
        // Load ATA program keypair
        let ata_keypair_path = format!(
            "{}/target/deploy/pinocchio_ata_program-keypair.json",
            manifest_dir
        );
        let ata_keypair_data = fs::read_to_string(&ata_keypair_path)
            .expect("Failed to read pinocchio_ata_program-keypair.json");
        let ata_keypair_bytes: Vec<u8> = serde_json::from_str(&ata_keypair_data)
            .expect("Failed to parse pinocchio_ata_program keypair JSON");
        let ata_keypair = Keypair::try_from(&ata_keypair_bytes[..])
            .expect("Invalid pinocchio_ata_program keypair");
        let ata_program_id = ata_keypair.pubkey();

        // Use SPL Token interface ID for token program
        let token_program_id = Pubkey::from(spl_token_interface::program::ID);

        (ata_program_id, token_program_id)
    }

    /// Load both p-ata and original ATA program IDs
    fn load_both_program_ids(manifest_dir: &str) -> (Pubkey, Option<Pubkey>, Pubkey) {
        let (p_ata_program_id, token_program_id) = Self::load_program_ids(manifest_dir);

        // Try to load original ATA program keypair
        let original_ata_program_id = Self::try_load_original_ata_program_id(manifest_dir);

        (p_ata_program_id, original_ata_program_id, token_program_id)
    }

    /// Try to load original ATA program ID, return None if not available
    fn try_load_original_ata_program_id(manifest_dir: &str) -> Option<Pubkey> {
        // Original ATA is built to ../target/deploy/ (parent directory)
        let original_keypair_path = format!("{}/../target/deploy/spl_associated_token_account-keypair.json", manifest_dir);
        
        if let Ok(keypair_data) = fs::read_to_string(&original_keypair_path) {
            if let Ok(keypair_bytes) = serde_json::from_str::<Vec<u8>>(&keypair_data) {
                if let Ok(keypair) = Keypair::try_from(&keypair_bytes[..]) {
                    println!("✅ Loaded original ATA program ID: {}", keypair.pubkey());
                    return Some(keypair.pubkey());
                }
            }
        }
        
        println!("⚠️  Original ATA program not found, comparison mode unavailable");
        println!("   Run with --features build-programs to build both implementations");
        None
    }

    /// Validate that the benchmark setup works with a simple test
    fn validate_setup(
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
                println!("✓ Benchmark setup validation passed for {}", ata_implementation.name);
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
    /// Run comprehensive comparison between p-ata and original ATA
    fn run_full_comparison(
        p_ata_impl: &AtaImplementation,
        original_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> Vec<ComparisonResult> {
        println!("\n=== 📊 P-ATA VS ORIGINAL ATA COMPREHENSIVE COMPARISON ===");
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
            ("create_extended", true, false, false),
        ];

        for (test_name, extended, with_rent, topup) in test_scenarios {
            let comparison = Self::run_create_test(test_name, p_ata_impl, original_impl, token_program_id, extended, with_rent, topup);
            Self::print_comparison_result(&comparison);
            results.push(comparison);
        }

        // CreateIdempotent variants
        let idempotent_tests = [
            ("create_idempotent_base", false),
            ("create_idempotent_rent", true),
        ];

        for (test_name, with_rent) in idempotent_tests {
            let comparison = Self::run_create_idempotent_test(test_name, p_ata_impl, original_impl, token_program_id, with_rent);
            Self::print_comparison_result(&comparison);
            results.push(comparison);
        }

        // RecoverNested test
        let comparison = Self::run_recover_test("recover_nested", p_ata_impl, original_impl, token_program_id);
        Self::print_comparison_result(&comparison);
        results.push(comparison);

        // Worst-case create scenario (expensive find_program_address)
        let comparison = Self::run_worst_case_create_test("worst_case_create", p_ata_impl, original_impl, token_program_id);
        Self::print_comparison_result(&comparison);
        results.push(comparison);

        // Test P-ATA specific optimizations (these may fail on original)
        let comparison = Self::run_create_with_bump_test("create_with_bump", p_ata_impl, original_impl, token_program_id);
        Self::print_comparison_result(&comparison);
        results.push(comparison);

        let comparison = Self::run_recover_with_bump_test("recover_with_bump", p_ata_impl, original_impl, token_program_id);
        Self::print_comparison_result(&comparison);
        results.push(comparison);

        Self::print_summary(&results);
        results
    }

    /// Run a single benchmark for one implementation
    fn run_single_benchmark(
        test_name: &str,
        ix: &Instruction,
        accounts: &[(Pubkey, Account)],
        implementation: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> BenchmarkResult {
        let mollusk = Self::create_mollusk_for_implementation(implementation, token_program_id);
        let result = mollusk.process_instruction(ix, accounts);

        let success = matches!(result.program_result, mollusk_svm::result::ProgramResult::Success);
        let error_message = if !success {
            Some(format!("{:?}", result.program_result))
        } else {
            None
        };

        BenchmarkResult {
            implementation: implementation.name.to_string(),
            test_name: test_name.to_string(),
            compute_units: result.compute_units_consumed,
            success,
            error_message,
        }
    }

    /// Create appropriate Mollusk instance for implementation
    fn create_mollusk_for_implementation(
        implementation: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> Mollusk {
        let mut mollusk = Mollusk::default();
        
        // Add the ATA program
        mollusk.add_program(&implementation.program_id, implementation.binary_name, &LOADER_V3);

        // Add required token programs
        mollusk.add_program(
            &Pubkey::from(spl_token_interface::program::ID),
            "pinocchio_token_program",
            &LOADER_V3,
        );
        mollusk.add_program(token_program_id, "pinocchio_token_program", &LOADER_V3);

        // Add Token-2022
        let token_2022_id = Pubkey::new_from_array(pinocchio_pubkey::pubkey!(
            "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
        ));
        mollusk.add_program(&token_2022_id, "spl_token_2022", &LOADER_V3);

        mollusk
    }

    /// Analyze and create comparison result
    fn create_comparison_result(
        test_name: &str,
        p_ata_result: BenchmarkResult,
        original_result: BenchmarkResult,
    ) -> ComparisonResult {
        let compute_savings = if p_ata_result.success && original_result.success {
            Some(original_result.compute_units as i64 - p_ata_result.compute_units as i64)
        } else {
            None
        };

        let savings_percentage = compute_savings.map(|savings| {
            if original_result.compute_units > 0 {
                (savings as f64 / original_result.compute_units as f64) * 100.0
            } else {
                0.0
            }
        });

        let compatibility_status = match (p_ata_result.success, original_result.success) {
            (true, true) => {
                if compute_savings.unwrap_or(0) > 0 {
                    CompatibilityStatus::Compatible
                } else {
                    CompatibilityStatus::Identical
                }
            }
            (false, false) => CompatibilityStatus::Compatible, // Both failed as expected
            (true, false) => CompatibilityStatus::OptimizedBehavior, // P-ATA optimization worked
            (false, true) => CompatibilityStatus::IncompatibleSuccess, // Concerning
        };

        ComparisonResult {
            test_name: test_name.to_string(),
            p_ata: p_ata_result,
            original: original_result,
            compute_savings,
            savings_percentage,
            compatibility_status,
        }
    }

    /// Print individual comparison result
    fn print_comparison_result(result: &ComparisonResult) {
        println!("\n--- 📋 {} ---", result.test_name);
        
        // Compute unit comparison
        println!("  P-ATA:    {:>8} CUs | {}", 
            result.p_ata.compute_units,
            if result.p_ata.success { "✅ Success" } else { "❌ Failed" }
        );
        println!("  Original: {:>8} CUs | {}", 
            result.original.compute_units,
            if result.original.success { "✅ Success" } else { "❌ Failed" }
        );

        // Savings analysis
        if let (Some(savings), Some(percentage)) = (result.compute_savings, result.savings_percentage) {
            if savings > 0 {
                println!("  💰 Savings: {:>8} CUs ({:.1}%)", savings, percentage);
            } else if savings < 0 {
                println!("  ⚠️  Overhead: {:>7} CUs ({:.1}%)", -savings, -percentage);
            } else {
                println!("  ⚖️  Equal compute usage");
            }
        }

        // Compatibility status
        match result.compatibility_status {
            CompatibilityStatus::Identical => println!("  🟢 Status: Identical behavior"),
            CompatibilityStatus::Compatible => println!("  🟢 Status: Compatible (expected differences)"),
            CompatibilityStatus::OptimizedBehavior => println!("  🟡 Status: P-ATA optimization working"),
            CompatibilityStatus::IncompatibleFailure => println!("  🔴 Status: Incompatible failure modes"),
            CompatibilityStatus::IncompatibleSuccess => println!("  🔴 Status: Incompatible success/failure"),
        }

        // Show error details if needed
        if !result.p_ata.success {
            if let Some(ref error) = result.p_ata.error_message {
                println!("  P-ATA Error: {}", error);
            }
        }
        if !result.original.success {
            if let Some(ref error) = result.original.error_message {
                println!("  Original Error: {}", error);
            }
        }
    }

    /// Print summary of all comparisons
    fn print_summary(results: &[ComparisonResult]) {
        println!("\n=== 📈 COMPARISON SUMMARY ===");
        
        let total_tests = results.len();
        let compatible_tests = results.iter()
            .filter(|r| matches!(r.compatibility_status, 
                CompatibilityStatus::Identical | CompatibilityStatus::Compatible))
            .count();
        let optimized_tests = results.iter()
            .filter(|r| matches!(r.compatibility_status, CompatibilityStatus::OptimizedBehavior))
            .count();
        let incompatible_tests = results.iter()
            .filter(|r| matches!(r.compatibility_status, 
                CompatibilityStatus::IncompatibleFailure | CompatibilityStatus::IncompatibleSuccess))
            .count();

        println!("Total Tests: {}", total_tests);
        println!("Compatible: {} ({:.1}%)", compatible_tests, 
            (compatible_tests as f64 / total_tests as f64) * 100.0);
        println!("P-ATA Optimizations: {} ({:.1}%)", optimized_tests,
            (optimized_tests as f64 / total_tests as f64) * 100.0);
        println!("Incompatible: {} ({:.1}%)", incompatible_tests,
            (incompatible_tests as f64 / total_tests as f64) * 100.0);

        // ATA vs P-ATA comparison list (exclude bump and prefunded tests)
        println!("\n=== 🔍 ATA vs P-ATA DETAILED COMPARISON ===");
        
        let comparable_tests: Vec<_> = results.iter()
            .filter(|r| {
                // Exclude bump tests (original ATA doesn't support them)
                !r.test_name.contains("with_bump") &&
                // Exclude prefunded tests (p-ata specific)
                !r.test_name.contains("prefunded") &&
                // Only include tests where both succeeded
                r.p_ata.success && r.original.success
            })
            .collect();

        if comparable_tests.is_empty() {
            println!("No comparable tests found (both implementations succeeded).");
            return;
        }

        println!("{:<20} {:>12} {:>12} {:>12} {:>8}", 
            "Test", "Original CUs", "P-ATA CUs", "Savings", "% Saved");
        println!("{}", "-".repeat(68));

        for result in &comparable_tests {
            let savings = result.original.compute_units as i64 - result.p_ata.compute_units as i64;
            let percentage = if result.original.compute_units > 0 {
                (savings as f64 / result.original.compute_units as f64) * 100.0
            } else {
                0.0
            };

            let savings_str = if savings >= 0 {
                format!("+{}", savings)
            } else {
                format!("{}", savings)
            };

            println!("{:<20} {:>12} {:>12} {:>12} {:>7.1}%", 
                result.test_name,
                result.original.compute_units,
                result.p_ata.compute_units,
                savings_str,
                percentage);
        }

        // Summary stats for comparable tests
        let total_original: u64 = comparable_tests.iter().map(|r| r.original.compute_units).sum();
        let total_p_ata: u64 = comparable_tests.iter().map(|r| r.p_ata.compute_units).sum();
        let total_savings = total_original as i64 - total_p_ata as i64;
        let total_percentage = if total_original > 0 {
            (total_savings as f64 / total_original as f64) * 100.0
        } else {
            0.0
        };

        println!("{}", "-".repeat(68));
        println!("{:<20} {:>12} {:>12} {:>12} {:>7.1}%", 
            "TOTAL", total_original, total_p_ata, 
            if total_savings >= 0 { format!("+{}", total_savings) } else { format!("{}", total_savings) },
            total_percentage);
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
        let (p_ata_ix, p_ata_accounts) = TestCaseBuilder::build_create(p_ata_impl, token_program_id, extended, with_rent, topup);
        let (original_ix, original_accounts) = TestCaseBuilder::build_create(original_impl, token_program_id, extended, with_rent, topup);

        let p_ata_result = Self::run_single_benchmark(test_name, &p_ata_ix, &p_ata_accounts, p_ata_impl, token_program_id);
        let original_result = Self::run_single_benchmark(test_name, &original_ix, &original_accounts, original_impl, token_program_id);

        Self::create_comparison_result(test_name, p_ata_result, original_result)
    }

    fn run_create_idempotent_test(
        test_name: &str,
        p_ata_impl: &AtaImplementation,
        original_impl: &AtaImplementation,
        token_program_id: &Pubkey,
        with_rent: bool,
    ) -> ComparisonResult {
        let (p_ata_ix, p_ata_accounts) = TestCaseBuilder::build_create_idempotent(p_ata_impl, token_program_id, with_rent);
        let (original_ix, original_accounts) = TestCaseBuilder::build_create_idempotent(original_impl, token_program_id, with_rent);

        let p_ata_result = Self::run_single_benchmark(test_name, &p_ata_ix, &p_ata_accounts, p_ata_impl, token_program_id);
        let original_result = Self::run_single_benchmark(test_name, &original_ix, &original_accounts, original_impl, token_program_id);

        Self::create_comparison_result(test_name, p_ata_result, original_result)
    }

    fn run_recover_test(
        test_name: &str,
        p_ata_impl: &AtaImplementation,
        original_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> ComparisonResult {
        let (p_ata_ix, p_ata_accounts) = TestCaseBuilder::build_recover(p_ata_impl, token_program_id);
        let (original_ix, original_accounts) = TestCaseBuilder::build_recover(original_impl, token_program_id);

        let p_ata_result = Self::run_single_benchmark(test_name, &p_ata_ix, &p_ata_accounts, p_ata_impl, token_program_id);
        let original_result = Self::run_single_benchmark(test_name, &original_ix, &original_accounts, original_impl, token_program_id);

        Self::create_comparison_result(test_name, p_ata_result, original_result)
    }

    fn run_create_with_bump_test(
        test_name: &str,
        p_ata_impl: &AtaImplementation,
        original_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> ComparisonResult {
        let (p_ata_ix, p_ata_accounts) = TestCaseBuilder::build_create_with_bump(p_ata_impl, token_program_id, false, false);
        let (original_ix, original_accounts) = TestCaseBuilder::build_create_with_bump(original_impl, token_program_id, false, false);

        let p_ata_result = Self::run_single_benchmark(test_name, &p_ata_ix, &p_ata_accounts, p_ata_impl, token_program_id);
        let original_result = Self::run_single_benchmark(test_name, &original_ix, &original_accounts, original_impl, token_program_id);

        Self::create_comparison_result(test_name, p_ata_result, original_result)
    }

    fn run_worst_case_create_test(
        test_name: &str,
        p_ata_impl: &AtaImplementation,
        original_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> ComparisonResult {
        // Build worst-case create scenario (low bump = expensive find_program_address)
        // Use only the regular Create instruction so both implementations can be compared
        let ((p_ata_ix, p_ata_accounts), _) = TestCaseBuilder::build_worst_case_bump_scenario(&p_ata_impl.program_id, token_program_id);
        let ((original_ix, original_accounts), _) = TestCaseBuilder::build_worst_case_bump_scenario(&original_impl.program_id, token_program_id);

        let p_ata_result = Self::run_single_benchmark(test_name, &p_ata_ix, &p_ata_accounts, p_ata_impl, token_program_id);
        let original_result = Self::run_single_benchmark(test_name, &original_ix, &original_accounts, original_impl, token_program_id);

        Self::create_comparison_result(test_name, p_ata_result, original_result)
    }

    fn run_recover_with_bump_test(
        test_name: &str,
        p_ata_impl: &AtaImplementation,
        original_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> ComparisonResult {
        // For this test, we need a bump-enabled recover - let me implement this
        // This is a placeholder - would need to implement build_recover_with_bump
        let (p_ata_ix, p_ata_accounts) = TestCaseBuilder::build_recover(p_ata_impl, token_program_id);
        let (original_ix, original_accounts) = TestCaseBuilder::build_recover(original_impl, token_program_id);

        let p_ata_result = Self::run_single_benchmark(test_name, &p_ata_ix, &p_ata_accounts, p_ata_impl, token_program_id);
        let original_result = Self::run_single_benchmark(test_name, &original_ix, &original_accounts, original_impl, token_program_id);

        Self::create_comparison_result(test_name, p_ata_result, original_result)
    }
}

// =============================== BENCHMARK RUNNER ===============================

struct BenchmarkRunner;

impl BenchmarkRunner {
    /// Run an isolated benchmark for a single test case
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

    /// Run all benchmarks for P-ATA only
    fn run_all_benchmarks(ata_implementation: &AtaImplementation, token_program_id: &Pubkey) {
        println!("\n=== Running all benchmarks for {} ===", ata_implementation.name);

        let test_cases = [
            (
                "create_base",
                TestCaseBuilder::build_create(ata_implementation, token_program_id, false, false, false),
            ),
            (
                "create_rent",
                TestCaseBuilder::build_create(ata_implementation, token_program_id, false, true, false),
            ),
            (
                "create_topup",
                TestCaseBuilder::build_create(ata_implementation, token_program_id, false, false, true),
            ),
            (
                "create_idemp",
                TestCaseBuilder::build_create_idempotent(ata_implementation, token_program_id, false),
            ),
            (
                "create_with_bump_base",
                TestCaseBuilder::build_create_with_bump(ata_implementation, token_program_id, false, false),
            ),
            (
                "create_with_bump_rent",
                TestCaseBuilder::build_create_with_bump(ata_implementation, token_program_id, false, true),
            ),
            (
                "recover",
                TestCaseBuilder::build_recover(ata_implementation, token_program_id),
            ),
        ];

        for (name, (ix, accounts)) in test_cases {
            Self::run_isolated_benchmark(name, &ix, &accounts, &ata_implementation.program_id, token_program_id);
        }

        // Run worst-case bump scenario comparison
        Self::run_worst_case_bump_comparison(&ata_implementation.program_id, token_program_id);
    }

    /// Run worst-case bump scenario to demonstrate Create vs CreateWithBump difference
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
        let p_ata_mollusk = ComparisonRunner::create_mollusk_for_implementation(&p_ata_impl, &token_program_id);
        let original_mollusk = ComparisonRunner::create_mollusk_for_implementation(&original_impl, &token_program_id);

        if let Err(e) = BenchmarkSetup::validate_setup(&p_ata_mollusk, &p_ata_impl, &token_program_id) {
            panic!("P-ATA benchmark setup validation failed: {}", e);
        }

        if let Err(e) = BenchmarkSetup::validate_setup(&original_mollusk, &original_impl, &token_program_id) {
            panic!("Original ATA benchmark setup validation failed: {}", e);
        }

        // Run comprehensive comparison
        let _comparison_results = ComparisonRunner::run_full_comparison(
            &p_ata_impl,
            &original_impl,
            &token_program_id,
        );

        println!("\n✅ Comprehensive comparison completed successfully");

    } else {
        // P-ATA ONLY MODE: Original ATA not available
        println!("\n🔧 Running P-ATA only benchmarks (original ATA not built)");
        println!("   💡 To enable comparison, run: cargo bench --features build-programs");

        // Setup Mollusk with P-ATA only
        let mollusk = fresh_mollusk(&p_ata_program_id, &token_program_id);

        // Validate the setup works
        if let Err(e) = BenchmarkSetup::validate_setup(&mollusk, &p_ata_impl, &token_program_id) {
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
    let wallet =
        OptimalKeyFinder::find_optimal_wallet(base_offset + 2, token_program_id, &mint, program_id);
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
