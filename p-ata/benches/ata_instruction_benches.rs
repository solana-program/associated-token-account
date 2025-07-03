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

// ================================ CONSTANTS ================================

const SYSTEM_PROGRAM_ID: Pubkey = Pubkey::new_from_array([0u8; 32]);
const NATIVE_LOADER_ID: Pubkey = Pubkey::new_from_array([
    5, 135, 132, 191, 20, 139, 164, 40, 47, 176, 18, 87, 72, 136, 169, 241, 83, 160, 125, 173, 247,
    101, 192, 69, 92, 154, 151, 3, 128, 0, 0, 0,
]);

// =============================== UTILITIES =================================

/// Helper to create deterministic pubkeys (32 identical bytes)
fn const_pk(byte: u8) -> Pubkey {
    Pubkey::new_from_array([byte; 32])
}

/// Clone accounts vector for benchmark isolation
fn clone_accounts(src: &[(Pubkey, Account)]) -> Vec<(Pubkey, Account)> {
    src.iter().map(|(k, v)| (*k, v.clone())).collect()
}

/// Build mint data core structure
#[inline(always)]
fn build_mint_data_core(decimals: u8) -> [u8; 82] {
    let mut data = [0u8; 82]; // Mint::LEN

    // mint_authority: COption<Pubkey> (36 bytes: 4 tag + 32 pubkey)
    data[0..4].copy_from_slice(&1u32.to_le_bytes()); // COption tag = Some
    data[4..36].fill(0); // All-zeros pubkey (valid but no authority)

    // supply: u64 (8 bytes) - stays as 0

    // decimals: u8 (1 byte)
    data[44] = decimals;

    // is_initialized: bool (1 byte)
    data[45] = 1; // true

    // freeze_authority: COption<Pubkey> (36 bytes: 4 tag + 32 pubkey)
    data[46..50].copy_from_slice(&0u32.to_le_bytes()); // COption tag = None
    // Remaining 32 bytes already 0

    data
}

/// Build token account data core structure
#[inline(always)]
fn build_token_account_data_core(mint: &[u8; 32], owner: &[u8; 32], amount: u64) -> [u8; 165] {
    let mut data = [0u8; 165]; // TokenAccount::LEN
    data[0..32].copy_from_slice(mint); // mint
    data[32..64].copy_from_slice(owner); // owner
    data[64..72].copy_from_slice(&amount.to_le_bytes()); // amount
    data[108] = 1; // state = Initialized
    data
}

/// Build TLV extension header
#[inline(always)]
fn build_tlv_extension(extension_type: u16, data_len: u16) -> [u8; 4] {
    let mut header = [0u8; 4];
    header[0..2].copy_from_slice(&extension_type.to_le_bytes());
    header[2..4].copy_from_slice(&data_len.to_le_bytes());
    header
}

/// Build multisig account data
#[inline(always)]
fn build_multisig_data_core(m: u8, signer_pubkeys: &[&[u8; 32]]) -> Vec<u8> {
    use spl_token_interface::state::multisig::{Multisig, MAX_SIGNERS};

    assert!(
        m as usize <= signer_pubkeys.len(),
        "m cannot exceed number of provided signers"
    );
    assert!(m >= 1, "m must be at least 1");
    assert!(
        signer_pubkeys.len() <= MAX_SIGNERS as usize,
        "too many signers provided"
    );

    let mut data = vec![0u8; Multisig::LEN];
    data[0] = m; // m threshold
    data[1] = signer_pubkeys.len() as u8; // n signers
    data[2] = 1; // is_initialized

    for (i, pk) in signer_pubkeys.iter().enumerate() {
        let offset = 3 + i * 32;
        data[offset..offset + 32].copy_from_slice(*pk);
    }
    data
}

/// Create a fresh Mollusk instance with required programs
fn fresh_mollusk(program_id: &Pubkey, token_program_id: &Pubkey) -> Mollusk {
    let mut mollusk = Mollusk::default();
    mollusk.add_program(program_id, "pinocchio_ata_program", &LOADER_V3);
    mollusk.add_program(
        &Pubkey::from(spl_token_interface::program::ID),
        "pinocchio_token_program",
        &LOADER_V3,
    );
    mollusk.add_program(token_program_id, "pinocchio_token_program", &LOADER_V3);

    // Add Token-2022 program with the actual Token-2022 binary
    let token_2022_id = Pubkey::new_from_array(pinocchio_pubkey::pubkey!(
        "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
    ));
    mollusk.add_program(&token_2022_id, "spl_token_2022", &LOADER_V3);

    mollusk
}

// ============================= ACCOUNT BUILDERS =============================

struct AccountBuilder;

impl AccountBuilder {
    /// Build a zero-rent `Rent` sysvar account with correctly sized data buffer
    fn rent_sysvar() -> Account {
        Account {
            lamports: 1,
            data: vec![1u8; 17], // Minimal rent sysvar data
            owner: rent::id(),
            executable: false,
            rent_epoch: 0,
        }
    }

    /// Build raw token Account data with the supplied mint / owner / amount
    fn token_account_data(mint: &Pubkey, owner: &Pubkey, amount: u64) -> Vec<u8> {
        build_token_account_data_core(
            mint.as_ref().try_into().expect("Pubkey is 32 bytes"),
            owner.as_ref().try_into().expect("Pubkey is 32 bytes"),
            amount,
        ).to_vec()
    }

    /// Build mint data with given decimals and marked initialized
    fn mint_data(decimals: u8) -> Vec<u8> {
        build_mint_data_core(decimals).to_vec()
    }

    /// Build extended mint data with ImmutableOwner extension
    fn extended_mint_data(decimals: u8) -> Vec<u8> {
        let required_len =
            ExtensionType::try_calculate_account_len::<spl_token_2022::state::Mint>(&[
                ExtensionType::ImmutableOwner,
            ])
            .expect("calc len");

        let mut data = Self::mint_data(decimals);
        data.resize(required_len, 0u8);

        // Add TLV entries at correct offset (base len = 82)
        let mut cursor = 82;
        // ImmutableOwner header
        let immutable_owner_header = build_tlv_extension(ExtensionType::ImmutableOwner as u16, 0);
        data[cursor..cursor + 4].copy_from_slice(&immutable_owner_header);
        cursor += 4;
        // Sentinel header
        data[cursor..cursor + 4].copy_from_slice(&0u32.to_le_bytes());

        data
    }

    /// Build Multisig account data with given signer public keys and threshold `m`
    fn multisig_data(m: u8, signer_pubkeys: &[Pubkey]) -> Vec<u8> {
        let byte_refs: Vec<&[u8; 32]> = signer_pubkeys
            .iter()
            .map(|pk| pk.as_ref().try_into().expect("Pubkey is 32 bytes"))
            .collect();
        build_multisig_data_core(m, &byte_refs)
    }

    /// Create a basic system account
    fn system_account(lamports: u64) -> Account {
        Account::new(lamports, 0, &SYSTEM_PROGRAM_ID)
    }

    /// Create an executable program account
    fn executable_program(owner: Pubkey) -> Account {
        Account {
            lamports: 0,
            data: Vec::new(),
            owner,
            executable: true,
            rent_epoch: 0,
        }
    }

    /// Create a token account with specified parameters
    fn token_account(
        mint: &Pubkey,
        owner: &Pubkey,
        amount: u64,
        token_program_id: &Pubkey,
    ) -> Account {
        Account {
            lamports: 2_000_000, // rent-exempt
            data: Self::token_account_data(mint, owner, amount),
            owner: *token_program_id,
            executable: false,
            rent_epoch: 0,
        }
    }

    /// Create a mint account
    fn mint_account(decimals: u8, token_program_id: &Pubkey, extended: bool) -> Account {
        Account {
            lamports: 1_000_000_000,
            data: if extended {
                Self::extended_mint_data(decimals)
            } else {
                Self::mint_data(decimals)
            },
            owner: *token_program_id,
            executable: false,
            rent_epoch: 0,
        }
    }
}

// =========================== OPTIMAL KEY FINDERS ==========================

struct OptimalKeyFinder;

impl OptimalKeyFinder {
    /// Find a wallet pubkey that yields the maximum bump (255) for its ATA
    fn find_optimal_wallet(
        start_byte: u8,
        token_program_id: &Pubkey,
        mint: &Pubkey,
        program_id: &Pubkey,
    ) -> Pubkey {
        let mut wallet = const_pk(start_byte);
        let mut best_bump = 0u8;

        for b in start_byte..=255 {
            let candidate = const_pk(b);
            let (_, bump) = Pubkey::find_program_address(
                &[candidate.as_ref(), token_program_id.as_ref(), mint.as_ref()],
                program_id,
            );
            if bump > best_bump {
                wallet = candidate;
                best_bump = bump;
                if bump == 255 {
                    break;
                }
            }
        }
        wallet
    }

    /// Find mint that gives optimal bump for nested ATA
    fn find_optimal_nested_mint(
        start_byte: u8,
        owner_ata: &Pubkey,
        token_program_id: &Pubkey,
        program_id: &Pubkey,
    ) -> Pubkey {
        let mut nested_mint = const_pk(start_byte);
        let mut best_bump = 0u8;

        for b in start_byte..=255 {
            let candidate = const_pk(b);
            let (_, bump) = Pubkey::find_program_address(
                &[
                    owner_ata.as_ref(),
                    token_program_id.as_ref(),
                    candidate.as_ref(),
                ],
                program_id,
            );
            if bump > best_bump {
                nested_mint = candidate;
                best_bump = bump;
                if bump == 255 {
                    break;
                }
            }
        }
        nested_mint
    }
}

// ========================== TEST CASE BUILDERS ============================

struct TestCaseBuilder;

impl TestCaseBuilder {
    /// Build CREATE instruction variants
    #[allow(clippy::too_many_arguments)]
    fn build_create(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
        extended_mint: bool,
        with_rent: bool,
        topup: bool,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let base_offset = calculate_base_offset(extended_mint, with_rent, topup);
        let (payer, mint, wallet) = build_base_test_accounts(base_offset, token_program_id, program_id);

        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
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

        let ix = Instruction {
            program_id: *program_id,
            accounts: metas,
            data: build_instruction_data(0, &[]), // Create instruction (discriminator 0 with no bump)
        };

        (ix, accounts)
    }

    /// Build CREATE_IDEMPOTENT instruction (pre-initialized ATA)
    fn build_create_idempotent(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
        with_rent: bool,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let payer = const_pk(1);
        let mint = const_pk(2);

        let wallet = OptimalKeyFinder::find_optimal_wallet(3, token_program_id, &mint, program_id);

        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
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

        let ix = Instruction {
            program_id: *program_id,
            accounts: metas,
            data: build_instruction_data(1, &[]), // CreateIdempotent discriminator
        };

        (ix, accounts)
    }

    /// Build RECOVER instruction for regular wallet
    fn build_recover(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let owner_mint = const_pk(20);

        let wallet =
            OptimalKeyFinder::find_optimal_wallet(30, token_program_id, &owner_mint, program_id);

        let (owner_ata, _) = Pubkey::find_program_address(
            &[
                wallet.as_ref(),
                token_program_id.as_ref(),
                owner_mint.as_ref(),
            ],
            program_id,
        );

        let nested_mint = OptimalKeyFinder::find_optimal_nested_mint(
            40,
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
            data: vec![2u8], // RecoverNested discriminator
        };

        (ix, accounts)
    }

    /// Build CREATE instruction with bump optimization
    #[allow(clippy::too_many_arguments)]
    fn build_create_with_bump(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
        extended_mint: bool,
        with_rent: bool,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let base_offset = calculate_bump_base_offset(extended_mint, with_rent);
        let (payer, mint, wallet) = build_base_test_accounts(base_offset, token_program_id, program_id);

        let (ata, bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
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

        let ix = Instruction {
            program_id: *program_id,
            accounts: metas,
            data: build_instruction_data(0, &[bump]), // Create instruction (discriminator 0) with bump
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
}

// ============================ SETUP AND CONFIGURATION =============================

struct BenchmarkSetup;

impl BenchmarkSetup {
    /// Setup SBF output directory and copy required files
    fn setup_sbf_environment(manifest_dir: &str) -> String {
        let sbf_out_dir = format!("{}/target/sbpf-solana-solana/release", manifest_dir);
        println!("Setting SBF_OUT_DIR to: {}", sbf_out_dir);
        std::env::set_var("SBF_OUT_DIR", &sbf_out_dir);

        // Ensure the output directory exists
        std::fs::create_dir_all(&sbf_out_dir).expect("Failed to create SBF_OUT_DIR");

        // Copy pinocchio_token.so from programs/ to SBF_OUT_DIR if needed
        let programs_dir = format!("{}/programs", manifest_dir);
        let token_so_src = Path::new(&programs_dir).join("pinocchio_token_program.so");
        let token_so_dst = Path::new(&sbf_out_dir).join("pinocchio_token_program.so");

        if token_so_src.exists() && !token_so_dst.exists() {
            println!("Copying pinocchio_token_program.so to SBF_OUT_DIR");
            fs::copy(&token_so_src, &token_so_dst)
                .expect("Failed to copy pinocchio_token_program.so to SBF_OUT_DIR");
        }

        // Copy spl_token_2022.so from programs/ to SBF_OUT_DIR if needed
        let token2022_so_src = Path::new(&programs_dir).join("spl_token_2022.so");
        let token2022_so_dst = Path::new(&sbf_out_dir).join("spl_token_2022.so");

        if token2022_so_src.exists() && !token2022_so_dst.exists() {
            println!("Copying spl_token_2022.so to SBF_OUT_DIR");
            fs::copy(&token2022_so_src, &token2022_so_dst)
                .expect("Failed to copy spl_token_2022.so to SBF_OUT_DIR");
        }

        sbf_out_dir
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
        let ata_keypair =
            Keypair::try_from(&ata_keypair_bytes[..]).expect("Invalid pinocchio_ata_program keypair");
        let ata_program_id = ata_keypair.pubkey();

        // Use SPL Token interface ID for token program
        let token_program_id = Pubkey::from(spl_token_interface::program::ID);

        (ata_program_id, token_program_id)
    }

    /// Validate that the benchmark setup works with a simple test
    fn validate_setup(
        mollusk: &Mollusk,
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> Result<(), String> {
        let (test_ix, test_accounts) = TestCaseBuilder::build_create(
            program_id,
            token_program_id,
            false, // not extended
            false, // no rent
            false, // no topup
        );

        let result = mollusk.process_instruction(&test_ix, &test_accounts);

        match result.program_result {
            mollusk_svm::result::ProgramResult::Success => {
                println!("✓ Benchmark setup validation passed");
                Ok(())
            }
            _ => Err(format!(
                "Setup validation failed: {:?}",
                result.program_result
            )),
        }
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

    /// Run all benchmarks
    fn run_all_benchmarks(program_id: &Pubkey, token_program_id: &Pubkey) {
        println!("\n=== Running all benchmarks ===");

        let test_cases = [
            (
                "create_base",
                TestCaseBuilder::build_create(program_id, token_program_id, false, false, false),
            ),
            (
                "create_rent",
                TestCaseBuilder::build_create(program_id, token_program_id, false, true, false),
            ),
            (
                "create_topup",
                TestCaseBuilder::build_create(program_id, token_program_id, false, false, true),
            ),
            (
                "create_idemp",
                TestCaseBuilder::build_create_idempotent(program_id, token_program_id, false),
            ),
            (
                "create_token2022_sim",
                TestCaseBuilder::build_create_token2022_simulation(program_id),
            ),
            (
                "create_with_bump_base",
                TestCaseBuilder::build_create_with_bump(program_id, token_program_id, false, false),
            ),
            (
                "create_with_bump_rent",
                TestCaseBuilder::build_create_with_bump(program_id, token_program_id, false, true),
            ),
            (
                "recover",
                TestCaseBuilder::build_recover(program_id, token_program_id),
            ),
            (
                "recover_multisig",
                TestCaseBuilder::build_recover_multisig(program_id, token_program_id),
            ),
        ];

        for (name, (ix, accounts)) in test_cases {
            Self::run_isolated_benchmark(name, &ix, &accounts, program_id, token_program_id);
        }

        // Run worst-case bump scenario comparison
        Self::run_worst_case_bump_comparison(program_id, token_program_id);
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

    BenchmarkSetup::setup_sbf_environment(manifest_dir);
    let (program_id, token_program_id) = BenchmarkSetup::load_program_ids(manifest_dir);

    println!("ATA Program ID: {}", program_id);
    println!("Token Program ID: {}", token_program_id);

    // Setup Mollusk with required programs
    let mollusk = fresh_mollusk(&program_id, &token_program_id);

    // Validate the setup works
    if let Err(e) = BenchmarkSetup::validate_setup(&mollusk, &program_id, &token_program_id) {
        panic!("Benchmark setup validation failed: {}", e);
    }

    // Run all benchmarks
    BenchmarkRunner::run_all_benchmarks(&program_id, &token_program_id);

    println!("\n✓ All benchmarks completed successfully");
}


/// Build AccountMeta structure
fn build_account_meta(pubkey: &Pubkey, writable: bool, signer: bool) -> AccountMeta {
    AccountMeta {
        pubkey: *pubkey,
        is_writable: writable,
        is_signer: signer,
    }
}

/// Build standard ATA instruction metas
fn build_ata_instruction_metas(
    payer: &Pubkey,
    ata: &Pubkey,
    wallet: &Pubkey,
    mint: &Pubkey,
    system_prog: &Pubkey,
    token_prog: &Pubkey,
) -> Vec<AccountMeta> {
    vec![
        build_account_meta(payer, true, true),      // payer (writable, signer)
        build_account_meta(ata, true, false),       // ata (writable, not signer)
        build_account_meta(wallet, false, false),   // wallet (readonly, not signer)
        build_account_meta(mint, false, false),     // mint (readonly, not signer)
        build_account_meta(system_prog, false, false), // system program (readonly, not signer)
        build_account_meta(token_prog, false, false),  // token program (readonly, not signer)
    ]
}

/// Build instruction data with discriminator
fn build_instruction_data(discriminator: u8, additional_data: &[u8]) -> Vec<u8> {
    let mut data = vec![discriminator];
    data.extend_from_slice(additional_data);
    data
}


/// Build base test accounts
fn build_base_test_accounts(
    base_offset: u8,
    token_program_id: &Pubkey,
    program_id: &Pubkey,
) -> (Pubkey, Pubkey, Pubkey) {
    let payer = const_pk(base_offset);
    let mint = const_pk(base_offset + 1);
    let wallet = OptimalKeyFinder::find_optimal_wallet(
        base_offset + 2,
        token_program_id,
        &mint,
        program_id,
    );
    (payer, mint, wallet)
}

/// Build standard account vector
fn build_standard_account_vec(
    accounts: &[(Pubkey, Account)],
) -> Vec<(Pubkey, Account)> {
    accounts.iter().map(|(k, v)| (*k, v.clone())).collect()
}

/// Modify account for topup scenario
fn modify_account_for_topup(account: &mut Account) {
    account.lamports = 1_000_000; // Some lamports but below rent-exempt
    account.data = vec![]; // No data allocated
    account.owner = SYSTEM_PROGRAM_ID; // Still system-owned
}

/// Calculate base offset for test variants
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

/// Calculate bump offset for bump tests
fn calculate_bump_base_offset(extended_mint: bool, with_rent: bool) -> u8 {
    match (extended_mint, with_rent) {
        (false, false) => 90, // create_with_bump_base
        (false, true) => 95,  // create_with_bump_rent
        (true, false) => 100, // create_with_bump_ext
        (true, true) => 105,  // create_with_bump_ext_rent
    }
}


/// Configure benchmark runner
fn configure_bencher<'a>(
    mollusk: Mollusk,
    _name: &'a str,
    must_pass: bool,
    out_dir: &'a str,
) -> MolluskComputeUnitBencher<'a> {
    let mut bencher = MolluskComputeUnitBencher::new(mollusk)
        .out_dir(out_dir);
    
    if must_pass {
        bencher = bencher.must_pass(true);
    }
    
    bencher
}

/// Execute benchmark case
fn execute_benchmark_case<'a>(
    bencher: MolluskComputeUnitBencher<'a>,
    name: &'a str,
    ix: &'a Instruction,
    accounts: &'a [(Pubkey, Account)],
) -> MolluskComputeUnitBencher<'a> {
    bencher.bench((name, ix, accounts))
}

/// Run benchmark with validation
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

/// Create standard program accounts
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

/// Generate test case name
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
