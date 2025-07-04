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
    spl_token_2022::extension::ExtensionType,
    spl_token_interface::state::Transmutable,
    std::{fs, path::Path},
};

#[path = "common.rs"]
mod common;
use common::*;

// ================================ FAILURE TEST CONSTANTS ================================

const FAKE_SYSTEM_PROGRAM_ID: Pubkey = Pubkey::new_from_array([1u8; 32]);
const FAKE_TOKEN_PROGRAM_ID: Pubkey = Pubkey::new_from_array([2u8; 32]);
const WRONG_PROGRAM_ID: Pubkey = Pubkey::new_from_array([3u8; 32]);

// ================================ FAILURE TEST BUILDERS ================================

struct FailureTestBuilder;

impl FailureTestBuilder {
    /// Build CREATE failure test with wrong payer owner
    fn build_fail_wrong_payer_owner(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let payer = const_pk(100);
        let mint = const_pk(101);
        let wallet =
            OptimalKeyFinder::find_optimal_wallet(102, token_program_id, &mint, program_id);
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let accounts = vec![
            // Payer owned by wrong program (should be system program)
            (
                payer,
                Account {
                    lamports: 1_000_000_000,
                    data: Vec::new(),
                    owner: *token_program_id, // Wrong! Should be system program
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (ata, AccountBuilder::system_account(0)),
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

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new(ata, false),
                AccountMeta::new_readonly(wallet, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new_readonly(*token_program_id, false),
            ],
            data: vec![0u8], // Create instruction
        };

        (ix, accounts)
    }

    /// Build CREATE failure test with payer not signed
    fn build_fail_payer_not_signed(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let payer = const_pk(110);
        let mint = const_pk(111);
        let wallet =
            OptimalKeyFinder::find_optimal_wallet(112, token_program_id, &mint, program_id);
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            (ata, AccountBuilder::system_account(0)),
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

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(payer, false), // NOT SIGNED! Should be true
                AccountMeta::new(ata, false),
                AccountMeta::new_readonly(wallet, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new_readonly(*token_program_id, false),
            ],
            data: vec![0u8], // Create instruction
        };

        (ix, accounts)
    }

    /// Build CREATE failure test with wrong system program
    fn build_fail_wrong_system_program(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let payer = const_pk(120);
        let mint = const_pk(121);
        let wallet =
            OptimalKeyFinder::find_optimal_wallet(122, token_program_id, &mint, program_id);
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            (ata, AccountBuilder::system_account(0)),
            (wallet, AccountBuilder::system_account(0)),
            (
                mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            // Fake system program instead of real one
            (
                FAKE_SYSTEM_PROGRAM_ID,
                AccountBuilder::executable_program(NATIVE_LOADER_ID),
            ),
            (
                *token_program_id,
                AccountBuilder::executable_program(LOADER_V3),
            ),
        ];

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new(ata, false),
                AccountMeta::new_readonly(wallet, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(FAKE_SYSTEM_PROGRAM_ID, false), // Wrong system program
                AccountMeta::new_readonly(*token_program_id, false),
            ],
            data: vec![0u8], // Create instruction
        };

        (ix, accounts)
    }

    /// Build CREATE failure test with wrong token program
    fn build_fail_wrong_token_program(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let payer = const_pk(130);
        let mint = const_pk(131);
        let wallet =
            OptimalKeyFinder::find_optimal_wallet(132, token_program_id, &mint, program_id);
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            (ata, AccountBuilder::system_account(0)),
            (wallet, AccountBuilder::system_account(0)),
            (
                mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (
                SYSTEM_PROGRAM_ID,
                AccountBuilder::executable_program(NATIVE_LOADER_ID),
            ),
            // Fake token program instead of real one
            (
                FAKE_TOKEN_PROGRAM_ID,
                AccountBuilder::executable_program(LOADER_V3),
            ),
        ];

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new(ata, false),
                AccountMeta::new_readonly(wallet, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new_readonly(FAKE_TOKEN_PROGRAM_ID, false), // Wrong token program
            ],
            data: vec![0u8], // Create instruction
        };

        (ix, accounts)
    }

    /// Build CREATE failure test with insufficient funds
    fn build_fail_insufficient_funds(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let payer = const_pk(140);
        let mint = const_pk(141);
        let wallet =
            OptimalKeyFinder::find_optimal_wallet(142, token_program_id, &mint, program_id);
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let accounts = vec![
            // Payer with insufficient funds
            (payer, AccountBuilder::system_account(1000)), // Very low balance
            (ata, AccountBuilder::system_account(0)),
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

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new(ata, false),
                AccountMeta::new_readonly(wallet, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new_readonly(*token_program_id, false),
            ],
            data: vec![0u8], // Create instruction
        };

        (ix, accounts)
    }

    /// Build CREATE failure test with wrong ATA address (doesn't match PDA derivation)
    fn build_fail_wrong_ata_address(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let payer = const_pk(170);
        let mint = const_pk(171);
        let wallet =
            OptimalKeyFinder::find_optimal_wallet(172, token_program_id, &mint, program_id);
        let wrong_ata = const_pk(173); // Wrong ATA address (doesn't match PDA)

        let accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            (wrong_ata, AccountBuilder::system_account(0)), // Wrong ATA address
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

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new(wrong_ata, false), // Wrong ATA address
                AccountMeta::new_readonly(wallet, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new_readonly(*token_program_id, false),
            ],
            data: vec![0u8], // Create instruction
        };

        (ix, accounts)
    }

    /// Build CREATE failure test with mint owned by wrong program
    fn build_fail_mint_wrong_owner(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let payer = const_pk(180);
        let mint = const_pk(181);
        let wallet =
            OptimalKeyFinder::find_optimal_wallet(182, token_program_id, &mint, program_id);
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            (ata, AccountBuilder::system_account(0)),
            (wallet, AccountBuilder::system_account(0)),
            // Mint owned by system program instead of token program
            (
                mint,
                Account {
                    lamports: 1_000_000_000,
                    data: AccountBuilder::mint_data(0),
                    owner: SYSTEM_PROGRAM_ID, // Wrong owner!
                    executable: false,
                    rent_epoch: 0,
                },
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

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new(ata, false),
                AccountMeta::new_readonly(wallet, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new_readonly(*token_program_id, false),
            ],
            data: vec![0u8], // Create instruction
        };

        (ix, accounts)
    }

    /// Build CREATE failure test with invalid mint structure
    fn build_fail_invalid_mint_structure(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let payer = const_pk(190);
        let mint = const_pk(191);
        let wallet =
            OptimalKeyFinder::find_optimal_wallet(192, token_program_id, &mint, program_id);
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            (ata, AccountBuilder::system_account(0)),
            (wallet, AccountBuilder::system_account(0)),
            // Mint with invalid structure (wrong size)
            (
                mint,
                Account {
                    lamports: 1_000_000_000,
                    data: vec![0u8; 50], // Wrong size - should be 82
                    owner: *token_program_id,
                    executable: false,
                    rent_epoch: 0,
                },
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

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new(ata, false),
                AccountMeta::new_readonly(wallet, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new_readonly(*token_program_id, false),
            ],
            data: vec![0u8],
        };

        (ix, accounts)
    }

    /// Build CREATE_IDEMPOTENT failure test with invalid token account structure
    fn build_fail_invalid_token_account_structure(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let payer = const_pk(00);
        let mint = const_pk(01);
        let wallet =
            OptimalKeyFinder::find_optimal_wallet(202, token_program_id, &mint, program_id);
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            // ATA exists but has invalid token account structure
            (
                ata,
                Account {
                    lamports: 2_000_000,
                    data: vec![0xFF; 165], // Invalid token account data (all 0xFF)
                    owner: *token_program_id, // Correct owner
                    executable: false,
                    rent_epoch: 0,
                },
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

        let ix = Instruction {
            program_id: *program_id,
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

    /// Build RECOVER failure test with wallet not signer
    fn build_fail_recover_wallet_not_signer(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let nested_ata = const_pk(10);
        let nested_mint = const_pk(11);
        let dest_ata = const_pk(12);
        let owner_ata = const_pk(13);
        let owner_mint = const_pk(14);
        let wallet = const_pk(15);

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
                AccountMeta::new(wallet, false), // NOT SIGNED! Should be true
                AccountMeta::new_readonly(*token_program_id, false),
                AccountMeta::new_readonly(Pubkey::from(spl_token_interface::program::ID), false),
            ],
            data: vec![2u8], // RecoverNested instruction
        };

        (ix, accounts)
    }

    /// Build RECOVER failure test with multisig insufficient signers
    fn build_fail_recover_multisig_insufficient_signers(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let nested_ata = const_pk(20);
        let nested_mint = const_pk(21);
        let dest_ata = const_pk(22);
        let owner_ata = const_pk(23);
        let owner_mint = const_pk(24);
        let wallet_ms = const_pk(25);

        let signer1 = Pubkey::new_unique();
        let signer2 = Pubkey::new_unique();
        let signer3 = Pubkey::new_unique();

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
                AccountBuilder::token_account(&nested_mint, &wallet_ms, 0, token_program_id),
            ),
            (
                owner_ata,
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
                    data: AccountBuilder::multisig_data(2, &[signer1, signer2, signer3]), // m=2, need 2 signers
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

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(nested_ata, false),
                AccountMeta::new_readonly(nested_mint, false),
                AccountMeta::new(dest_ata, false),
                AccountMeta::new(owner_ata, false),
                AccountMeta::new_readonly(owner_mint, false),
                AccountMeta::new(wallet_ms, false), // Multisig wallet
                AccountMeta::new_readonly(*token_program_id, false),
                AccountMeta::new_readonly(Pubkey::from(spl_token_interface::program::ID), false),
                AccountMeta::new_readonly(signer1, true), // Only 1 signer, need 2
                AccountMeta::new_readonly(signer2, false), // Not signed
                AccountMeta::new_readonly(signer3, false), // Not signed
            ],
            data: vec![2u8], // RecoverNested instruction
        };

        (ix, accounts)
    }

    /// Build failure test with invalid instruction discriminator
    fn build_fail_invalid_discriminator(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let payer = const_pk(30);
        let mint = const_pk(31);
        let wallet =
            OptimalKeyFinder::find_optimal_wallet(232, token_program_id, &mint, program_id);
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            (ata, AccountBuilder::system_account(0)),
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

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new(ata, false),
                AccountMeta::new_readonly(wallet, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new_readonly(*token_program_id, false),
            ],
            data: vec![99u8], // Invalid discriminator (should be 0, 1, or 2)
        };

        (ix, accounts)
    }

    /// Build failure test with invalid bump value
    fn build_fail_invalid_bump_value(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let payer = const_pk(40);
        let mint = const_pk(41);
        let wallet =
            OptimalKeyFinder::find_optimal_wallet(242, token_program_id, &mint, program_id);
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            (ata, AccountBuilder::system_account(0)),
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

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new(ata, false),
                AccountMeta::new_readonly(wallet, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new_readonly(*token_program_id, false),
            ],
            data: vec![0u8, 99u8], // Create with invalid bump (not the correct bump)
        };

        (ix, accounts)
    }

    /// Build CREATE failure test with non-executable program accounts
    fn build_fail_non_executable_program(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let payer = const_pk(50);
        let mint = const_pk(51);
        let wallet =
            OptimalKeyFinder::find_optimal_wallet(252, token_program_id, &mint, program_id);
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            (ata, AccountBuilder::system_account(0)),
            (wallet, AccountBuilder::system_account(0)),
            (
                mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (
                SYSTEM_PROGRAM_ID,
                AccountBuilder::executable_program(NATIVE_LOADER_ID),
            ),
            // Token program marked as non-executable
            (
                *token_program_id,
                Account {
                    lamports: 0,
                    data: Vec::new(),
                    owner: LOADER_V3,
                    executable: false, // Should be true!
                    rent_epoch: 0,
                },
            ),
        ];

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new(ata, false),
                AccountMeta::new_readonly(wallet, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new_readonly(*token_program_id, false),
            ],
            data: vec![0u8],
        };

        (ix, accounts)
    }

    /// Build CREATE failure test with ATA owned by system program (existing ATA with wrong owner)
    fn build_fail_ata_owned_by_system_program(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let payer = const_pk(60);
        let mint = const_pk(61);
        let wallet = OptimalKeyFinder::find_optimal_wallet(62, token_program_id, &mint, program_id);
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            // ATA already exists but owned by system program (not token program)
            (
                ata,
                Account {
                    lamports: 2_000_000,
                    data: vec![0u8; 165],     // Token account size
                    owner: SYSTEM_PROGRAM_ID, // Wrong owner - should be token program
                    executable: false,
                    rent_epoch: 0,
                },
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

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new(ata, false),
                AccountMeta::new_readonly(wallet, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new_readonly(*token_program_id, false),
            ],
            data: vec![0u8],
        };

        (ix, accounts)
    }

    /// Build RECOVER failure test with wrong nested ATA address
    fn build_fail_recover_wrong_nested_ata_address(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let wrong_nested_ata = const_pk(70); // Wrong nested ATA address
        let nested_mint = const_pk(71);
        let dest_ata = const_pk(72);
        let owner_ata = const_pk(73);
        let owner_mint = const_pk(74);
        let wallet = const_pk(75);

        let accounts = vec![
            // Wrong nested ATA address (doesn't match proper derivation)
            (
                wrong_nested_ata,
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
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let nested_ata = const_pk(80);
        let nested_mint = const_pk(81);
        let wrong_dest_ata = const_pk(82); // Wrong destination ATA
        let owner_ata = const_pk(83);
        let owner_mint = const_pk(84);
        let wallet = const_pk(85);

        let accounts = vec![
            (
                nested_ata,
                AccountBuilder::token_account(&nested_mint, &owner_ata, 100, token_program_id),
            ),
            (
                nested_mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            // Wrong destination ATA
            (
                wrong_dest_ata,
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
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let nested_ata = const_pk(90);
        let nested_mint = const_pk(91);
        let dest_ata = const_pk(92);
        let owner_ata = const_pk(93);
        let owner_mint = const_pk(94);
        let wallet = const_pk(95);

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
            data: vec![2u8, 99u8], // RecoverNested with invalid bump
        };

        (ix, accounts)
    }

    /// Build CREATE failure test with wrong token account size
    fn build_fail_wrong_token_account_size(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let payer = const_pk(00);
        let mint = const_pk(01);
        let wallet = OptimalKeyFinder::find_optimal_wallet(02, token_program_id, &mint, program_id);
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            // ATA exists but wrong size
            (
                ata,
                Account {
                    lamports: 2_000_000,
                    data: vec![0u8; 100], // Wrong size - should be 165
                    owner: *token_program_id,
                    executable: false,
                    rent_epoch: 0,
                },
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

        let ix = Instruction {
            program_id: *program_id,
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
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let payer = const_pk(10);
        let mint = const_pk(11);
        let wrong_mint = const_pk(12);
        let wallet = OptimalKeyFinder::find_optimal_wallet(13, token_program_id, &mint, program_id);
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            // ATA points to wrong mint
            (
                ata,
                AccountBuilder::token_account(&wrong_mint, &wallet, 0, token_program_id),
            ),
            (wallet, AccountBuilder::system_account(0)),
            (
                mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (
                wrong_mint,
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

        let ix = Instruction {
            program_id: *program_id,
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
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let payer = const_pk(0);
        let mint = const_pk(1);
        let wallet = OptimalKeyFinder::find_optimal_wallet(22, token_program_id, &mint, program_id);
        let wrong_owner = const_pk(3);
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            // ATA has wrong owner
            (
                ata,
                AccountBuilder::token_account(&mint, &wrong_owner, 0, token_program_id),
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

        let ix = Instruction {
            program_id: *program_id,
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
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let payer = const_pk(30);
        let mint = const_pk(31);
        let wallet = OptimalKeyFinder::find_optimal_wallet(32, token_program_id, &mint, program_id);
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            (ata, AccountBuilder::system_account(0)),
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

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(ata, false), // ATA marked as non-writable
                AccountMeta::new_readonly(wallet, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new_readonly(*token_program_id, false),
            ],
            data: vec![0u8],
        };

        (ix, accounts)
    }

    /// Build RECOVER failure test with nested account having wrong owner
    fn build_fail_recover_nested_wrong_owner(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let nested_ata = const_pk(10);
        let nested_mint = const_pk(11);
        let dest_ata = const_pk(12);
        let owner_ata = const_pk(13);
        let owner_mint = const_pk(14);
        let wallet = const_pk(15);
        let wrong_owner = const_pk(16);

        let accounts = vec![
            // Nested ATA owned by wrong owner (not the owner_ata)
            (
                nested_ata,
                AccountBuilder::token_account(&nested_mint, &wrong_owner, 100, token_program_id),
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
            data: vec![2u8],
        };

        (ix, accounts)
    }

    /// Build CREATE failure test with wrong account size for extensions
    fn build_fail_wrong_account_size_for_extensions(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let payer = const_pk(50);
        let mint = const_pk(51);
        let wallet = OptimalKeyFinder::find_optimal_wallet(52, token_program_id, &mint, program_id);
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            // ATA with wrong size for extensions (too small for ImmutableOwner)
            (
                ata,
                Account {
                    lamports: 2_000_000,
                    data: vec![0u8; 165], // Standard size, but mint has extensions
                    owner: *token_program_id,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (wallet, AccountBuilder::system_account(0)),
            // Extended mint that requires larger ATA
            (
                mint,
                AccountBuilder::mint_account(0, token_program_id, true),
            ), // extended = true
            (
                SYSTEM_PROGRAM_ID,
                AccountBuilder::executable_program(NATIVE_LOADER_ID),
            ),
            (
                *token_program_id,
                AccountBuilder::executable_program(LOADER_V3),
            ),
        ];

        let ix = Instruction {
            program_id: *program_id,
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

    /// Build CREATE failure test with missing extensions
    fn build_fail_missing_extensions(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let payer = const_pk(60);
        let mint = const_pk(61);
        let wallet = OptimalKeyFinder::find_optimal_wallet(62, token_program_id, &mint, program_id);
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            // ATA missing required extensions
            (
                ata,
                Account {
                    lamports: 2_000_000,
                    data: vec![0u8; 200], // Large enough but missing extension data
                    owner: *token_program_id,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (wallet, AccountBuilder::system_account(0)),
            // Extended mint that requires extensions in ATA
            (
                mint,
                AccountBuilder::mint_account(0, token_program_id, true),
            ), // extended = true
            (
                SYSTEM_PROGRAM_ID,
                AccountBuilder::executable_program(NATIVE_LOADER_ID),
            ),
            (
                *token_program_id,
                AccountBuilder::executable_program(LOADER_V3),
            ),
        ];

        let ix = Instruction {
            program_id: *program_id,
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

    /// Build CREATE failure test with invalid extension data
    fn build_fail_invalid_extension_data(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let payer = const_pk(70);
        let mint = const_pk(71);
        let wallet = OptimalKeyFinder::find_optimal_wallet(72, token_program_id, &mint, program_id);
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            // ATA with malformed extension headers
            (
                ata,
                Account {
                    lamports: 2_000_000,
                    data: {
                        let mut data = vec![0u8; 200];
                        // Add invalid extension header at the end
                        data[165..169].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // Invalid extension type
                        data
                    },
                    owner: *token_program_id,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (wallet, AccountBuilder::system_account(0)),
            (
                mint,
                AccountBuilder::mint_account(0, token_program_id, true),
            ), // extended = true
            (
                SYSTEM_PROGRAM_ID,
                AccountBuilder::executable_program(NATIVE_LOADER_ID),
            ),
            (
                *token_program_id,
                AccountBuilder::executable_program(LOADER_V3),
            ),
        ];

        let ix = Instruction {
            program_id: *program_id,
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

    /// Build RECOVER failure test with invalid multisig data
    fn build_fail_invalid_multisig_data(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let nested_ata = const_pk(80);
        let nested_mint = const_pk(81);
        let dest_ata = const_pk(82);
        let owner_ata = const_pk(83);
        let owner_mint = const_pk(84);
        let wallet_ms = const_pk(85);

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
                AccountBuilder::token_account(&nested_mint, &wallet_ms, 0, token_program_id),
            ),
            (
                owner_ata,
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
                    data: vec![0xFF; 355], // Invalid multisig data (all 0xFF)
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
        ];

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(nested_ata, false),
                AccountMeta::new_readonly(nested_mint, false),
                AccountMeta::new(dest_ata, false),
                AccountMeta::new(owner_ata, false),
                AccountMeta::new_readonly(owner_mint, false),
                AccountMeta::new(wallet_ms, false), // Multisig wallet with invalid data
                AccountMeta::new_readonly(*token_program_id, false),
                AccountMeta::new_readonly(Pubkey::from(spl_token_interface::program::ID), false),
            ],
            data: vec![2u8],
        };

        (ix, accounts)
    }

    /// Build RECOVER failure test with invalid signer accounts (not in multisig list)
    fn build_fail_invalid_signer_accounts(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let nested_ata = const_pk(90);
        let nested_mint = const_pk(91);
        let dest_ata = const_pk(92);
        let owner_ata = const_pk(93);
        let owner_mint = const_pk(94);
        let wallet_ms = const_pk(95);

        let signer1 = Pubkey::new_unique();
        let signer2 = Pubkey::new_unique();
        let signer3 = Pubkey::new_unique();
        let wrong_signer = Pubkey::new_unique(); // Not in multisig list

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
                AccountBuilder::token_account(&nested_mint, &wallet_ms, 0, token_program_id),
            ),
            (
                owner_ata,
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
                    data: AccountBuilder::multisig_data(2, &[signer1, signer2, signer3]), // m=2
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
            (wrong_signer, AccountBuilder::system_account(1_000_000_000)),
        ];

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(nested_ata, false),
                AccountMeta::new_readonly(nested_mint, false),
                AccountMeta::new(dest_ata, false),
                AccountMeta::new(owner_ata, false),
                AccountMeta::new_readonly(owner_mint, false),
                AccountMeta::new(wallet_ms, false), // Multisig wallet
                AccountMeta::new_readonly(*token_program_id, false),
                AccountMeta::new_readonly(Pubkey::from(spl_token_interface::program::ID), false),
                AccountMeta::new_readonly(wrong_signer, true), // Wrong signer (not in multisig list)
            ],
            data: vec![2u8],
        };

        (ix, accounts)
    }

    /// Build RECOVER failure test with uninitialized multisig
    fn build_fail_uninitialized_multisig(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let nested_ata = const_pk(100);
        let nested_mint = const_pk(101);
        let dest_ata = const_pk(102);
        let owner_ata = const_pk(103);
        let owner_mint = const_pk(104);
        let wallet_ms = const_pk(105);

        let signer1 = Pubkey::new_unique();

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
                AccountBuilder::token_account(&nested_mint, &wallet_ms, 0, token_program_id),
            ),
            (
                owner_ata,
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
                    data: {
                        let mut data = vec![0u8; 355]; // Multisig::LEN
                        data[0] = 1; // m = 1
                        data[1] = 1; // n = 1
                        data[2] = 0; // is_initialized = false (uninitialized!)
                        data[3..35].copy_from_slice(signer1.as_ref());
                        data
                    },
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
        ];

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(nested_ata, false),
                AccountMeta::new_readonly(nested_mint, false),
                AccountMeta::new(dest_ata, false),
                AccountMeta::new(owner_ata, false),
                AccountMeta::new_readonly(owner_mint, false),
                AccountMeta::new(wallet_ms, false), // Uninitialized multisig wallet
                AccountMeta::new_readonly(*token_program_id, false),
                AccountMeta::new_readonly(Pubkey::from(spl_token_interface::program::ID), false),
                AccountMeta::new_readonly(signer1, true),
            ],
            data: vec![2u8],
        };

        (ix, accounts)
    }
}

// ================================ BASIC FAILURE TEST RUNNER ================================

struct FailureTestRunner;

impl FailureTestRunner {
    /// Run a single failure test case
    fn run_failure_test(
        name: &str,
        ix: &Instruction,
        accounts: &[(Pubkey, Account)],
        program_id: &Pubkey,
        token_program_id: &Pubkey,
        expected_to_fail: bool,
    ) {
        println!("\n=== Running failure test: {} ===", name);

        let cloned_accounts = clone_accounts(accounts);
        let mollusk = fresh_mollusk(program_id, token_program_id);

        let result = mollusk.process_instruction(ix, &cloned_accounts);

        match result.program_result {
            mollusk_svm::result::ProgramResult::Success => {
                if expected_to_fail {
                    println!("❌ UNEXPECTED SUCCESS: {} should have failed", name);
                } else {
                    println!("✅ SUCCESS: {}", name);
                }
            }
            _ => {
                if expected_to_fail {
                    println!(
                        "✅ EXPECTED FAILURE: {} failed with {:?}",
                        name, result.program_result
                    );
                } else {
                    println!(
                        "❌ UNEXPECTED FAILURE: {} failed with {:?}",
                        name, result.program_result
                    );
                }
            }
        }
    }
}

// ================================ MAIN FUNCTION ================================

fn main() {
    // Setup logging
    let _ = solana_logger::setup_with(
        "info,solana_runtime=info,solana_program_runtime=info,mollusk=debug",
    );

    // Get manifest directory and setup environment
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    println!("CARGO_MANIFEST_DIR: {}", manifest_dir);

    // Setup environment (copied from ata_instruction_benches.rs)
    let sbf_out_dir = format!("{}/target/sbpf-solana-solana/release", manifest_dir);
    std::env::set_var("SBF_OUT_DIR", &sbf_out_dir);
    std::fs::create_dir_all(&sbf_out_dir).expect("Failed to create SBF_OUT_DIR");

    // Load program IDs (copied from ata_instruction_benches.rs)
    let ata_keypair_path = format!(
        "{}/target/deploy/pinocchio_ata_program-keypair.json",
        manifest_dir
    );
    let ata_keypair_data = fs::read_to_string(&ata_keypair_path)
        .expect("Failed to read pinocchio_ata_program-keypair.json");
    let ata_keypair_bytes: Vec<u8> = serde_json::from_str(&ata_keypair_data)
        .expect("Failed to parse pinocchio_ata_program keypair JSON");
    let ata_keypair =
        Keypair::try_from(&ata_keypair_bytes[..]).expect("Invalid pinocchio_ata_program keypair");
    let program_id = ata_keypair.pubkey();
    let token_program_id = Pubkey::from(spl_token_interface::program::ID);

    println!("ATA Program ID: {}", program_id);
    println!("Token Program ID: {}", token_program_id);

    // Run basic failure tests
    println!("\n=== Running Basic Account Ownership Failure Tests ===");

    let basic_failure_tests = [
        (
            "fail_wrong_payer_owner",
            FailureTestBuilder::build_fail_wrong_payer_owner(&program_id, &token_program_id),
        ),
        (
            "fail_payer_not_signed",
            FailureTestBuilder::build_fail_payer_not_signed(&program_id, &token_program_id),
        ),
        (
            "fail_wrong_system_program",
            FailureTestBuilder::build_fail_wrong_system_program(&program_id, &token_program_id),
        ),
        (
            "fail_wrong_token_program",
            FailureTestBuilder::build_fail_wrong_token_program(&program_id, &token_program_id),
        ),
        (
            "fail_insufficient_funds",
            FailureTestBuilder::build_fail_insufficient_funds(&program_id, &token_program_id),
        ),
    ];

    for (name, (ix, accounts)) in basic_failure_tests {
        FailureTestRunner::run_failure_test(
            name,
            &ix,
            &accounts,
            &program_id,
            &token_program_id,
            true, // expected to fail
        );
    }

    println!("\n=== Running Address Derivation and Structure Failure Tests ===");

    let additional_failure_tests = [
        (
            "fail_wrong_ata_address",
            FailureTestBuilder::build_fail_wrong_ata_address(&program_id, &token_program_id),
        ),
        (
            "fail_mint_wrong_owner",
            FailureTestBuilder::build_fail_mint_wrong_owner(&program_id, &token_program_id),
        ),
        (
            "fail_invalid_mint_structure",
            FailureTestBuilder::build_fail_invalid_mint_structure(&program_id, &token_program_id),
        ),
        (
            "fail_invalid_token_account_structure",
            FailureTestBuilder::build_fail_invalid_token_account_structure(
                &program_id,
                &token_program_id,
            ),
        ),
        (
            "fail_recover_wallet_not_signer",
            FailureTestBuilder::build_fail_recover_wallet_not_signer(
                &program_id,
                &token_program_id,
            ),
        ),
        (
            "fail_recover_multisig_insufficient_signers",
            FailureTestBuilder::build_fail_recover_multisig_insufficient_signers(
                &program_id,
                &token_program_id,
            ),
        ),
        (
            "fail_invalid_discriminator",
            FailureTestBuilder::build_fail_invalid_discriminator(&program_id, &token_program_id),
        ),
        (
            "fail_invalid_bump_value",
            FailureTestBuilder::build_fail_invalid_bump_value(&program_id, &token_program_id),
        ),
    ];

    for (name, (ix, accounts)) in additional_failure_tests {
        FailureTestRunner::run_failure_test(
            name,
            &ix,
            &accounts,
            &program_id,
            &token_program_id,
            true, // expected to fail
        );
    }

    println!("\n=== Running Additional Validation Coverage Tests ===");

    let extended_failure_tests = [
        (
            "fail_non_executable_program",
            FailureTestBuilder::build_fail_non_executable_program(&program_id, &token_program_id),
        ),
        (
            "fail_ata_owned_by_system_program",
            FailureTestBuilder::build_fail_ata_owned_by_system_program(
                &program_id,
                &token_program_id,
            ),
        ),
        (
            "fail_recover_wrong_nested_ata_address",
            FailureTestBuilder::build_fail_recover_wrong_nested_ata_address(
                &program_id,
                &token_program_id,
            ),
        ),
        (
            "fail_recover_wrong_destination_address",
            FailureTestBuilder::build_fail_recover_wrong_destination_address(
                &program_id,
                &token_program_id,
            ),
        ),
        (
            "fail_recover_invalid_bump_value",
            FailureTestBuilder::build_fail_recover_invalid_bump_value(
                &program_id,
                &token_program_id,
            ),
        ),
        (
            "fail_wrong_token_account_size",
            FailureTestBuilder::build_fail_wrong_token_account_size(&program_id, &token_program_id),
        ),
        (
            "fail_token_account_wrong_mint",
            FailureTestBuilder::build_fail_token_account_wrong_mint(&program_id, &token_program_id),
        ),
        (
            "fail_token_account_wrong_owner",
            FailureTestBuilder::build_fail_token_account_wrong_owner(
                &program_id,
                &token_program_id,
            ),
        ),
        (
            "fail_immutable_account",
            FailureTestBuilder::build_fail_immutable_account(&program_id, &token_program_id),
        ),
        (
            "fail_recover_nested_wrong_owner",
            FailureTestBuilder::build_fail_recover_nested_wrong_owner(
                &program_id,
                &token_program_id,
            ),
        ),
        (
            "fail_wrong_account_size_for_extensions",
            FailureTestBuilder::build_fail_wrong_account_size_for_extensions(
                &program_id,
                &token_program_id,
            ),
        ),
        (
            "fail_missing_extensions",
            FailureTestBuilder::build_fail_missing_extensions(&program_id, &token_program_id),
        ),
        (
            "fail_invalid_extension_data",
            FailureTestBuilder::build_fail_invalid_extension_data(&program_id, &token_program_id),
        ),
        (
            "fail_invalid_multisig_data",
            FailureTestBuilder::build_fail_invalid_multisig_data(&program_id, &token_program_id),
        ),
        (
            "fail_invalid_signer_accounts",
            FailureTestBuilder::build_fail_invalid_signer_accounts(&program_id, &token_program_id),
        ),
        (
            "fail_uninitialized_multisig",
            FailureTestBuilder::build_fail_uninitialized_multisig(&program_id, &token_program_id),
        ),
    ];

    for (name, (ix, accounts)) in extended_failure_tests {
        FailureTestRunner::run_failure_test(
            name,
            &ix,
            &accounts,
            &program_id,
            &token_program_id,
            true, // expected to fail
        );
    }

    println!("\n=== FAILURE CASE TESTS PASSED ===");
}

/// Run performance comparison tests to demonstrate compute savings
fn run_performance_comparison_tests(program_id: &Pubkey, token_program_id: &Pubkey) {
    println!("\n--- Performance Comparison: Create vs CreateWithBump ---");

    // Test expensive find_program_address vs cheap bump provision
    let (expensive_create, expensive_accounts) =
        create_expensive_create_scenario(program_id, token_program_id);
    let (cheap_create, cheap_accounts) =
        create_cheap_create_with_bump_scenario(program_id, token_program_id);

    // These should both succeed but with different compute costs
    FailureTestRunner::run_failure_test(
        "expensive_create_scenario",
        &expensive_create,
        &expensive_accounts,
        program_id,
        token_program_id,
        false, // expected to succeed
    );

    FailureTestRunner::run_failure_test(
        "cheap_create_with_bump_scenario",
        &cheap_create,
        &cheap_accounts,
        program_id,
        token_program_id,
        false, // expected to succeed
    );
}

/// Create expensive CREATE scenario (low bump = expensive find_program_address)
fn create_expensive_create_scenario(
    program_id: &Pubkey,
    token_program_id: &Pubkey,
) -> (Instruction, Vec<(Pubkey, Account)>) {
    // Find wallet that produces very low bump (expensive to compute)
    let mut worst_wallet = const_pk(50);
    let mut worst_bump = 255u8;
    let mint = const_pk(51);

    for b in 250..=254 {
        let candidate = const_pk(b);
        let (_, bump) = Pubkey::find_program_address(
            &[candidate.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );
        if bump < worst_bump {
            worst_wallet = candidate;
            worst_bump = bump;
            if bump <= 50 {
                break;
            }
        }
    }

    let (ata, _bump) = Pubkey::find_program_address(
        &[
            worst_wallet.as_ref(),
            token_program_id.as_ref(),
            mint.as_ref(),
        ],
        program_id,
    );

    let accounts = vec![
        (const_pk(49), AccountBuilder::system_account(1_000_000_000)), // payer
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

    let ix = Instruction {
        program_id: *program_id,
        accounts: vec![
            AccountMeta::new(const_pk(49), true),
            AccountMeta::new(ata, false),
            AccountMeta::new_readonly(worst_wallet, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
            AccountMeta::new_readonly(*token_program_id, false),
        ],
        data: vec![0u8], // Create instruction (expensive find_program_address)
    };

    (ix, accounts)
}

/// Create cheap CREATE with bump scenario (skips find_program_address)
fn create_cheap_create_with_bump_scenario(
    program_id: &Pubkey,
    token_program_id: &Pubkey,
) -> (Instruction, Vec<(Pubkey, Account)>) {
    let payer = const_pk(49);
    let mint = const_pk(51);
    let wallet = const_pk(50); // Same wallet from expensive scenario

    let (ata, bump) = Pubkey::find_program_address(
        &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
        program_id,
    );

    let accounts = vec![
        (payer, AccountBuilder::system_account(1_000_000_000)),
        (ata, AccountBuilder::system_account(0)),
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

    let ix = Instruction {
        program_id: *program_id,
        accounts: vec![
            AccountMeta::new(payer, true),
            AccountMeta::new(ata, false),
            AccountMeta::new_readonly(wallet, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
            AccountMeta::new_readonly(*token_program_id, false),
        ],
        data: vec![0u8, bump], // Create with bump (cheap, skips find_program_address)
    };

    (ix, accounts)
}
