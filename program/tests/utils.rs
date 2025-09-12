//! Test utilities for migrated mollusk tests
#![cfg(any(test, feature = "test-utils"))]

use {
    mollusk_svm::{
        program::loader_keys::LOADER_V3, result::ProgramResult, Mollusk, MolluskContext,
    },
    solana_program::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
        sysvar,
    },
    solana_sdk::{account::Account, signature::Keypair, signer::Signer},
    solana_system_interface::instruction as system_instruction,
    solana_system_interface::program as system_program,
    spl_associated_token_account_interface::address::get_associated_token_address_with_program_id,
    std::vec::Vec,
    std::{collections::HashMap, path::Path},
};

// Standard SPL token account size (fixed for all SPL token accounts)
const TOKEN_ACCOUNT_SIZE: usize = 165;
// Standard mint account size (base size without extensions)
const MINT_ACCOUNT_SIZE: usize = 82;

// Native loader program ID (used across both test suites)
const NATIVE_LOADER_ID: Pubkey = Pubkey::new_from_array([
    5, 135, 132, 191, 20, 139, 164, 40, 47, 176, 18, 87, 72, 136, 169, 241, 83, 160, 125, 173, 247,
    101, 192, 69, 92, 154, 151, 3, 128, 0, 0, 0,
]);

/// Setup mollusk with ATA and token programs for testing
pub fn setup_mollusk_with_programs(token_program_id: &Pubkey) -> Mollusk {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");

    // 1) Load ATA by name from target deploy dir
    {
        let sbf_out = workspace_root.join("target/deploy");
        std::env::set_var("SBF_OUT_DIR", &sbf_out);
        std::env::set_var("BPF_OUT_DIR", sbf_out);
    }
    let ata_program_id = spl_associated_token_account::id();
    let mut mollusk = Mollusk::new(&ata_program_id, "spl_associated_token_account");

    // 2) Load the selected token program by name from programs/ dir
    {
        let sbf_out = workspace_root.join("programs");
        std::env::set_var("SBF_OUT_DIR", &sbf_out);
        std::env::set_var("BPF_OUT_DIR", sbf_out);
    }
    if *token_program_id == spl_token_2022_interface::id() {
        mollusk.add_program(token_program_id, "spl_token_2022", &LOADER_V3);
    } else {
        // Pinocchio token program
        mollusk.add_program(token_program_id, "pinocchio_token_program", &LOADER_V3);
    }

    mollusk
}

pub mod test_util_exports {
    #![allow(dead_code)]
    use super::*;

    /// Setup a MolluskContext with ATA and token programs for testing
    pub fn setup_context_with_programs(
        token_program_id: &Pubkey,
    ) -> MolluskContext<HashMap<Pubkey, Account>> {
        let mollusk = setup_mollusk_with_programs(token_program_id);
        mollusk.with_context(HashMap::new())
    }

    /// Ensure a system-owned account exists in the context store with the given lamports
    pub fn ctx_ensure_system_account_exists(
        context: &MolluskContext<HashMap<Pubkey, Account>>,
        address: Pubkey,
        lamports: u64,
    ) {
        if context.account_store.borrow().get(&address).is_none() {
            context
                .account_store
                .borrow_mut()
                .insert(address, Account::new(lamports, 0, &system_program::id()));
        }
    }

    /// Ensure multiple system accounts exist in the context store with the provided lamports
    pub fn ctx_ensure_system_accounts_with_lamports(
        context: &MolluskContext<HashMap<Pubkey, Account>>,
        entries: &[(Pubkey, u64)],
    ) {
        for (address, lamports) in entries.iter().copied() {
            ctx_ensure_system_account_exists(context, address, lamports);
        }
    }

    /// Create standard base accounts needed for mollusk tests
    pub fn create_mollusk_base_accounts(payer: &Keypair) -> Vec<(Pubkey, Account)> {
        [
            (
                payer.pubkey(),
                Account::new(10_000_000_000, 0, &system_program::id()),
            ),
            (
                system_program::id(),
                Account {
                    lamports: 0,
                    data: Vec::new(),
                    owner: NATIVE_LOADER_ID,
                    executable: true,
                    rent_epoch: 0,
                },
            ),
            {
                use solana_sdk::rent::Rent;
                let rent = Rent::default();

                (
                    sysvar::rent::id(),
                    Account {
                        lamports: 0,
                        data: create_rent_data(
                            rent.lamports_per_byte_year,
                            rent.exemption_threshold,
                            rent.burn_percent,
                        ),
                        owner: sysvar::id(),
                        executable: false,
                        rent_epoch: 0,
                    },
                )
            },
        ]
        .into()
    }

    /// Create standard base accounts with token program
    pub fn create_mollusk_base_accounts_with_token(
        payer: &Keypair,
        token_program_id: &Pubkey,
    ) -> Vec<(Pubkey, Account)> {
        let mut accounts = create_mollusk_base_accounts(payer);

        accounts.push((
            *token_program_id,
            mollusk_svm::program::create_program_account_loader_v3(token_program_id),
        ));

        accounts
    }

    /// The type of ATA creation instruction to build.
    #[derive(Debug)]
    pub enum CreateAtaInstructionType {
        /// The standard `Create` instruction, which can optionally include a bump seed and account length.
        Create {
            bump: Option<u8>,
            account_len: Option<u16>,
        },
        #[allow(dead_code, reason = "Some tests construct only a subset of variants")]
        /// The `CreateIdempotent` instruction, which can optionally include a bump seed.
        CreateIdempotent { bump: Option<u8> },
    }

    /// Encodes the instruction data payload for ATA creation-related instructions.
    pub fn encode_create_ata_instruction_data(
        instruction_type: &CreateAtaInstructionType,
    ) -> Vec<u8> {
        match instruction_type {
            CreateAtaInstructionType::Create { bump, account_len } => {
                let mut data = vec![0]; // Discriminator for Create
                if let Some(b) = bump {
                    data.push(*b);
                    if let Some(len) = account_len {
                        data.extend_from_slice(&len.to_le_bytes());
                    }
                }
                data
            }
            CreateAtaInstructionType::CreateIdempotent { bump } => {
                let mut data = vec![1]; // Discriminator for CreateIdempotent
                if let Some(b) = bump {
                    data.push(*b);
                }
                data
            }
        }
    }

    /// Build a create associated token account instruction with a given discriminator
    pub fn build_create_ata_instruction(
        ata_program_id: Pubkey,
        payer: Pubkey,
        ata_address: Pubkey,
        wallet: Pubkey,
        mint: Pubkey,
        token_program: Pubkey,
        instruction_type: CreateAtaInstructionType,
    ) -> Instruction {
        Instruction {
            program_id: ata_program_id,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new(ata_address, false),
                AccountMeta::new_readonly(wallet, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new_readonly(token_program, false),
                AccountMeta::new_readonly(sysvar::rent::id(), false),
            ],
            data: encode_create_ata_instruction_data(&instruction_type),
        }
    }

    /// Create token account data for mollusk testing
    pub fn create_token_account_data(mint: &[u8; 32], owner: &[u8; 32], amount: u64) -> Vec<u8> {
        let mut data = vec![0u8; TOKEN_ACCOUNT_SIZE];

        // mint
        data[0..32].copy_from_slice(mint);
        // owner
        data[32..64].copy_from_slice(owner);
        // amount
        data[64..72].copy_from_slice(&amount.to_le_bytes());
        // delegate option = 0 (none)
        data[72] = 0;
        // state = 1 (initialized)
        data[108] = 1;
        // is_native option = 0 (none)
        data[109] = 0;
        // delegated_amount = 0
        data[110..118].copy_from_slice(&0u64.to_le_bytes());
        // close_authority option = 0 (none)
        data[118] = 0;

        data
    }

    /// Create mint account data for mollusk testing
    pub fn create_mollusk_mint_data(decimals: u8) -> Vec<u8> {
        let mut data = vec![0u8; MINT_ACCOUNT_SIZE];
        data[0..4].copy_from_slice(&1u32.to_le_bytes()); // state = 1 (Initialized)
        data[44] = decimals;
        data[45] = 1; // is_initialized = 1
        data
    }

    /// Create rent sysvar data for testing
    pub fn create_rent_data(
        lamports_per_byte_year: u64,
        exemption_threshold: f64,
        burn_percent: u8,
    ) -> Vec<u8> {
        lamports_per_byte_year
            .to_le_bytes()
            .into_iter()
            .chain(exemption_threshold.to_le_bytes())
            .chain([burn_percent])
            .collect()
    }

    /// Helper function to update account data in accounts vector after instruction execution
    fn update_account_from_result(
        mollusk: &Mollusk,
        instruction: &Instruction,
        accounts: &mut [(Pubkey, Account)],
        target_pubkey: Pubkey,
    ) {
        if let Some((_, acct)) = mollusk
            .process_instruction(instruction, accounts)
            .resulting_accounts
            .into_iter()
            .find(|(pk, _)| *pk == target_pubkey)
        {
            if let Some((_, a)) = accounts.iter_mut().find(|(pk, _)| *pk == target_pubkey) {
                *a = acct;
            }
        }
    }

    /// Ensures a given `address` exists as a system account with the specified `lamports`.
    /// If an account for `address` already exists, it is left unchanged.
    pub fn ensure_system_account_exists(
        accounts: &mut Vec<(Pubkey, Account)>,
        address: Pubkey,
        lamports: u64,
    ) {
        if !accounts.iter().any(|(pubkey, _)| *pubkey == address) {
            accounts.push((address, Account::new(lamports, 0, &system_program::id())));
        }
    }

    /// Ensures multiple system accounts exist with the provided lamport values.
    /// Each tuple is `(pubkey, lamports)`.
    pub fn ensure_system_accounts_with_lamports(
        accounts: &mut Vec<(Pubkey, Account)>,
        entries: &[(Pubkey, u64)],
    ) {
        for (address, lamports) in entries.iter().copied() {
            ensure_system_account_exists(accounts, address, lamports);
        }
    }

    /// Processes an instruction with Mollusk and merges resulting account updates back into `accounts`.
    /// Returns the `ProgramResult` from the execution for assertions.
    pub fn process_and_merge_instruction(
        mollusk: &Mollusk,
        instruction: &Instruction,
        accounts: &mut Vec<(Pubkey, Account)>,
    ) -> ProgramResult {
        let result = mollusk.process_instruction(instruction, accounts);

        for (updated_pubkey, updated_account) in result.resulting_accounts.into_iter() {
            if let Some((_, existing_account)) =
                accounts.iter_mut().find(|(pk, _)| *pk == updated_pubkey)
            {
                *existing_account = updated_account;
            } else {
                accounts.push((updated_pubkey, updated_account));
            }
        }

        result.program_result
    }

    /// Process and validate an instruction, then merge resulting account updates into `accounts`.
    /// Returns the full `InstructionResult` for optional further inspection.
    pub fn process_and_validate_then_merge(
        mollusk: &Mollusk,
        instruction: &Instruction,
        accounts: &mut Vec<(Pubkey, Account)>,
        checks: &[mollusk_svm::result::Check],
    ) -> mollusk_svm::result::InstructionResult {
        let result = mollusk.process_and_validate_instruction(instruction, accounts, checks);
        merge_resulting_accounts(accounts, &result);
        result
    }

    /// Merge resulting accounts from a Mollusk `InstructionResult` back into the in-memory `accounts` list.
    pub fn merge_resulting_accounts(
        accounts: &mut Vec<(Pubkey, Account)>,
        result: &mollusk_svm::result::InstructionResult,
    ) {
        for (updated_pubkey, updated_account) in result.resulting_accounts.clone().into_iter() {
            if let Some((_, existing_account)) =
                accounts.iter_mut().find(|(pk, _)| *pk == updated_pubkey)
            {
                *existing_account = updated_account;
            } else {
                accounts.push((updated_pubkey, updated_account));
            }
        }
    }

    /// Returns a cloned `Account` for the given `pubkey` from the in-memory `accounts` list.
    /// Panics with a clear message if the account is not present.
    pub fn get_account(accounts: &[(Pubkey, Account)], pubkey: Pubkey) -> Account {
        accounts
            .iter()
            .find(|(pk, _)| *pk == pubkey)
            .expect("account not found")
            .1
            .clone()
    }

    /// Creates an associated token account via the ATA program and merges account updates.
    /// Returns `(associated_token_address, program_result)`.
    #[allow(dead_code)]
    pub fn create_associated_token_account_mollusk(
        mollusk: &Mollusk,
        accounts: &mut Vec<(Pubkey, Account)>,
        payer: &Keypair,
        owner: &Pubkey,
        mint: &Pubkey,
        token_program: &Pubkey,
    ) -> (Pubkey, ProgramResult) {
        let ata_address = get_associated_token_address_with_program_id(owner, mint, token_program);

        // Ensure the provided owner (wallet) account exists in the accounts list for Mollusk metas
        ensure_system_account_exists(accounts, *owner, 0);

        // Ensure placeholder system account for ATA address
        ensure_system_account_exists(accounts, ata_address, 0);

        let instruction = build_create_ata_instruction(
            spl_associated_token_account::id(),
            payer.pubkey(),
            ata_address,
            *owner,
            *mint,
            *token_program,
            CreateAtaInstructionType::Create {
                bump: None,
                account_len: None,
            },
        );

        let mollusk_result = process_and_merge_instruction(mollusk, &instruction, accounts);
        (ata_address, mollusk_result)
    }

    /// Legacy variant: creates ATA using empty instruction data (deprecated path).
    #[allow(dead_code)]
    pub fn create_associated_token_account_legacy_mollusk(
        mollusk: &Mollusk,
        accounts: &mut Vec<(Pubkey, Account)>,
        payer: &Keypair,
        owner: &Pubkey,
        mint: &Pubkey,
        token_program: &Pubkey,
    ) -> (Pubkey, ProgramResult) {
        let ata_address = get_associated_token_address_with_program_id(owner, mint, token_program);

        ensure_system_account_exists(accounts, *owner, 0);
        ensure_system_account_exists(accounts, ata_address, 0);

        let mut instruction = build_create_ata_instruction(
            spl_associated_token_account::id(),
            payer.pubkey(),
            ata_address,
            *owner,
            *mint,
            *token_program,
            CreateAtaInstructionType::Create {
                bump: None,
                account_len: None,
            },
        );
        instruction.data = Vec::new();

        let mollusk_result = process_and_merge_instruction(mollusk, &instruction, accounts);
        (ata_address, mollusk_result)
    }

    /// Ensures all recover-nested derived ATAs exist as system accounts: owner ATA, destination ATA, nested ATA.
    #[allow(dead_code)]
    pub fn ensure_recover_nested_accounts(
        accounts: &mut Vec<(Pubkey, Account)>,
        wallet_address: &Pubkey,
        nested_token_mint_address: &Pubkey,
        owner_token_mint_address: &Pubkey,
        token_program: &Pubkey,
    ) {
        let owner_ata = get_associated_token_address_with_program_id(
            wallet_address,
            owner_token_mint_address,
            token_program,
        );
        let destination_ata = get_associated_token_address_with_program_id(
            wallet_address,
            nested_token_mint_address,
            token_program,
        );
        let nested_ata = get_associated_token_address_with_program_id(
            &owner_ata,
            nested_token_mint_address,
            token_program,
        );
        for ata in [owner_ata, destination_ata, nested_ata] {
            ensure_system_account_exists(accounts, ata, 0);
        }
    }

    /// Ensures executable program and sysvar accounts are present.
    /// Provide any mix of program IDs (e.g., token-2022, token classic, ATA, system) and it will add executable accounts.
    /// Always ensures rent sysvar is present.
    #[allow(dead_code)]
    pub fn ensure_program_accounts_present(
        accounts: &mut Vec<(Pubkey, Account)>,
        program_ids: &[Pubkey],
    ) {
        for pid in program_ids.iter().copied() {
            if !accounts.iter().any(|(pk, _)| *pk == pid) {
                accounts.push((
                    pid,
                    mollusk_svm::program::create_program_account_loader_v3(&pid),
                ));
            }
        }
        if !accounts.iter().any(|(pk, _)| *pk == sysvar::rent::id()) {
            accounts.push((
                sysvar::rent::id(),
                account_builder::AccountBuilder::rent_sysvar(),
            ));
        }
    }

    /// Convenience: build and process a mint_to, merging updates, and assert success.
    #[allow(dead_code)]
    pub fn mint_to_and_merge(
        mollusk: &Mollusk,
        accounts: &mut Vec<(Pubkey, Account)>,
        token_program: &Pubkey,
        mint: &Pubkey,
        destination: &Pubkey,
        authority: &Pubkey,
        amount: u64,
    ) -> ProgramResult {
        let ix = spl_token_2022_interface::instruction::mint_to(
            token_program,
            mint,
            destination,
            authority,
            &[],
            amount,
        )
        .unwrap();
        process_and_merge_instruction(mollusk, &ix, accounts)
    }

    /// Build a create ATA instruction and ensure the derived ATA address exists as a system account
    /// This only adds a system account if NO account exists at the ATA address
    /// If an account already exists (regardless of owner), it is preserved unchanged
    #[allow(dead_code, reason = "exported for benching consumers")]
    #[allow(clippy::too_many_arguments)]
    pub fn build_create_ata_instruction_with_system_account(
        accounts: &mut Vec<(Pubkey, Account)>,
        ata_program_id: Pubkey,
        payer: Pubkey,
        ata_address: Pubkey,
        wallet: Pubkey,
        mint: Pubkey,
        token_program: Pubkey,
        instruction_type: CreateAtaInstructionType,
    ) -> Instruction {
        // Ensure the derived ATA address exists as a system account (as the program expects)
        ensure_system_account_exists(accounts, ata_address, 0);

        // Build the instruction
        build_create_ata_instruction(
            ata_program_id,
            payer,
            ata_address,
            wallet,
            mint,
            token_program,
            instruction_type,
        )
    }

    /// Creates and initializes a mint account with the given parameters.
    /// Returns a vector of accounts including the initialized mint and all necessary
    /// base accounts for testing.
    pub fn create_test_mint(
        mollusk: &Mollusk,
        mint_account: &Keypair,
        mint_authority: &Keypair,
        payer: &Keypair,
        token_program: &Pubkey,
        decimals: u8,
    ) -> Vec<(Pubkey, Account)> {
        let mint_space = MINT_ACCOUNT_SIZE as u64;
        let rent_lamports = 1_461_600u64;

        let create_mint_ix = system_instruction::create_account(
            &payer.pubkey(),
            &mint_account.pubkey(),
            rent_lamports,
            mint_space,
            token_program,
        );

        let mut accounts = create_mollusk_base_accounts_with_token(payer, token_program);

        accounts.push((
            mint_account.pubkey(),
            Account::new(0, 0, &system_program::id()),
        ));
        accounts.push((
            mint_authority.pubkey(),
            Account::new(1_000_000, 0, &system_program::id()),
        ));

        // Create the mint account on-chain.
        mollusk.process_and_validate_instruction(
            &create_mint_ix,
            &accounts,
            &[mollusk_svm::result::Check::success()],
        );
        let init_mint_ix = if *token_program == spl_token_2022_interface::id() {
            spl_token_2022_interface::instruction::initialize_mint(
                token_program,
                &mint_account.pubkey(),
                &mint_authority.pubkey(),
                Some(&mint_authority.pubkey()),
                decimals,
            )
            .unwrap()
        } else {
            spl_token_interface::instruction::initialize_mint(
                token_program,
                &mint_account.pubkey(),
                &mint_authority.pubkey(),
                Some(&mint_authority.pubkey()),
                decimals,
            )
            .unwrap()
        };

        // Refresh the mint account data after creation.
        update_account_from_result(
            mollusk,
            &create_mint_ix,
            &mut accounts,
            mint_account.pubkey(),
        );

        mollusk.process_and_validate_instruction(
            &init_mint_ix,
            &accounts,
            &[mollusk_svm::result::Check::success()],
        );

        // Final refresh so callers see the initialized state.
        update_account_from_result(mollusk, &init_mint_ix, &mut accounts, mint_account.pubkey());

        accounts
    }

    pub mod account_builder {
        use {
            super::{create_mollusk_mint_data, create_token_account_data},
            mollusk_svm::Mollusk,
            solana_program::{pubkey::Pubkey, sysvar::rent},
            solana_sdk::account::Account,
            std::vec::Vec,
        };

        pub struct AccountBuilder;

        impl AccountBuilder {
            pub fn rent_sysvar() -> Account {
                let mollusk = Mollusk::default();
                let (_, mollusk_rent_account) = mollusk.sysvars.keyed_account_for_rent_sysvar();

                Account {
                    lamports: mollusk_rent_account.lamports,
                    data: mollusk_rent_account.data,
                    owner: rent::id(),
                    executable: false,
                    rent_epoch: 0,
                }
            }
            #[allow(dead_code, reason = "exported for benchmarking consumers")]
            pub fn system_account(lamports: u64) -> Account {
                Account {
                    lamports,
                    data: Vec::new(),
                    owner: solana_system_interface::program::id(),
                    executable: false,
                    rent_epoch: 0,
                }
            }

            #[allow(dead_code, reason = "exported for benchmarking consumers")]
            pub fn executable_program(loader: Pubkey) -> Account {
                Account {
                    lamports: 0,
                    data: Vec::new(),
                    owner: loader,
                    executable: true,
                    rent_epoch: 0,
                }
            }

            #[allow(dead_code, reason = "exported for benchmarking consumers")]
            pub fn mint(decimals: u8, _mint_authority: &Pubkey) -> Account {
                let data = create_mollusk_mint_data(decimals);
                let rent = solana_sdk::rent::Rent::default();
                Account {
                    lamports: rent.minimum_balance(data.len()),
                    data,
                    owner: spl_token_interface::id(),
                    executable: false,
                    rent_epoch: 0,
                }
            }

            #[allow(dead_code, reason = "exported for benchmarking consumers")]
            pub fn extended_mint(decimals: u8, _mint_authority: &Pubkey) -> Account {
                use solana_program_option::COption;
                use spl_token_2022_interface::{
                    extension::{ExtensionType, PodStateWithExtensionsMut},
                    pod::PodMint,
                };

                // Calculate the minimum size for a Token-2022 mint without extensions
                let required_size = ExtensionType::try_calculate_account_len::<
                    spl_token_2022_interface::state::Mint,
                >(&[])
                .expect("Failed to calculate Token-2022 mint size");

                let mut data = vec![0u8; required_size];

                // Use Token-2022's proper unpacking to initialize the mint
                let mint = PodStateWithExtensionsMut::<PodMint>::unpack_uninitialized(&mut data)
                    .expect("Failed to unpack Token-2022 mint");

                // Initialize base mint fields
                mint.base.mint_authority = COption::None.into();
                mint.base.supply = 0u64.into();
                mint.base.decimals = decimals;
                mint.base.is_initialized = true.into();
                mint.base.freeze_authority = COption::None.into();

                let rent = solana_sdk::rent::Rent::default();
                Account {
                    lamports: rent.minimum_balance(data.len()),
                    data,
                    owner: spl_token_2022_interface::id(),
                    executable: false,
                    rent_epoch: 0,
                }
            }

            pub fn token_account(
                mint: &Pubkey,
                owner: &Pubkey,
                amount: u64,
                token_program: &Pubkey,
            ) -> Account {
                let data = create_token_account_data(&mint.to_bytes(), &owner.to_bytes(), amount);
                let rent = solana_sdk::rent::Rent::default();
                Account {
                    lamports: rent.minimum_balance(data.len()),
                    data,
                    owner: *token_program,
                    executable: false,
                    rent_epoch: 0,
                }
            }
        }
    }
}

// Re-exports removed to avoid unused import warnings. Import from
// `crate::utils::legacy_utils::...` at call sites instead.
