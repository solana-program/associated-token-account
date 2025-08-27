//! Test utilities for migrated mollusk tests
//! Adapted from p-ata/migrated/test_utils.rs and test_helpers.rs

use {
    mollusk_svm::{program::loader_keys::LOADER_V3, Mollusk},
    solana_program::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
        sysvar,
    },
    solana_sdk::{account::Account, signature::Keypair, signer::Signer},
    solana_system_interface::instruction as system_instruction,
    solana_system_interface::program as system_program,
    std::path::Path,
    std::vec::Vec,
};

/// Standard SPL token account size (fixed for all SPL token accounts)
pub const TOKEN_ACCOUNT_SIZE: usize = 165;
/// Standard mint account size (base size without extensions)
pub const MINT_ACCOUNT_SIZE: usize = 82;

/// Standard lamport amounts for testing
pub const TOKEN_ACCOUNT_RENT_EXEMPT: u64 = 2_074_080;
pub const MINT_ACCOUNT_RENT_EXEMPT: u64 = 2_000_000;

/// Native loader program ID (used across both test suites)
pub const NATIVE_LOADER_ID: Pubkey = Pubkey::new_from_array([
    5, 135, 132, 191, 20, 139, 164, 40, 47, 176, 18, 87, 72, 136, 169, 241, 83, 160, 125, 173, 247,
    101, 192, 69, 92, 154, 151, 3, 128, 0, 0, 0,
]);

/// Setup mollusk with ATA and token programs for testing
pub fn setup_mollusk_with_programs(token_program_id: &Pubkey) -> Mollusk {
    let mut mollusk = Mollusk::default();

    let ata_program_id = spl_associated_token_account::id();
    // TODO: to run p-ata, replace here with pinocchio_ata_program
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    let ata_candidates = [
        workspace_root.join("target/deploy/spl_associated_token_account"),
        workspace_root.join("target/sbf/deploy/spl_associated_token_account"),
    ];
    let ata_program_path = ata_candidates
        .iter()
        .find(|p| p.with_extension("so").exists())
        .unwrap_or_else(|| {
            panic!(
                "ATA program .so file not found. Tried paths: {:?}. Run 'cargo build-sbf' to build the program.",
                ata_candidates.iter().map(|p| p.with_extension("so")).collect::<Vec<_>>()
            )
        })
        .to_string_lossy()
        .into_owned();
    mollusk.add_program(&ata_program_id, &ata_program_path, &LOADER_V3);

    // Load appropriate token program
    let program_candidates = if *token_program_id == spl_token_2022_interface::id() {
        [
            workspace_root.join("p-ata/programs/token-2022/target/deploy/spl_token_2022"),
            workspace_root.join("p-ata/programs/token-2022/target/sbf/deploy/spl_token_2022"),
        ]
    } else {
        [
            workspace_root.join("p-ata/programs/token/target/deploy/pinocchio_token_program"),
            workspace_root.join("p-ata/programs/token/target/sbf/deploy/pinocchio_token_program"),
        ]
    };
    let program_path = program_candidates
        .iter()
        .find(|p| p.with_extension("so").exists())
        .unwrap_or_else(|| {
            panic!(
                "Token program .so file not found for {:?}. Tried paths: {:?}. Run 'cargo build-sbf' to build the program.",
                token_program_id,
                program_candidates.iter().map(|p| p.with_extension("so")).collect::<Vec<_>>()
            )
        })
        .to_string_lossy()
        .into_owned();
    mollusk.add_program(token_program_id, &program_path, &LOADER_V3);

    mollusk
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
        Account {
            lamports: 0,
            data: Vec::new(),
            owner: LOADER_V3,
            executable: true,
            rent_epoch: 0,
        },
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
    /// The `CreateIdempotent` instruction, which can optionally include a bump seed.
    CreateIdempotent { bump: Option<u8> },
}

/// Encodes the instruction data payload for ATA creation-related instructions.
pub fn encode_create_ata_instruction_data(instruction_type: &CreateAtaInstructionType) -> Vec<u8> {
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

/// Ensures the derived ATA address exists as a system account in the accounts list
/// This is required by Mollusk since the ATA program expects to write to this address
/// The program will convert this system account into a token account during execution
pub fn ensure_ata_system_account_exists(
    accounts: &mut Vec<(Pubkey, Account)>,
    ata_address: Pubkey,
) {
    // Check if ATA account already exists in the accounts list
    if !accounts.iter().any(|(pubkey, _)| *pubkey == ata_address) {
        // Add system account at the derived ATA address (program expects system ownership initially)
        accounts.push((ata_address, Account::new(0, 0, &system_program::id())));
    }
}

/// Build a create ATA instruction and ensure the derived ATA address exists as a system account
/// This only adds a system account if NO account exists at the ATA address
/// If an account already exists (regardless of owner), it is preserved unchanged
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
    ensure_ata_system_account_exists(accounts, ata_address);

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

        pub fn system_account(lamports: u64) -> Account {
            Account {
                lamports,
                data: Vec::new(),
                owner: solana_system_interface::program::id(),
                executable: false,
                rent_epoch: 0,
            }
        }

        pub fn executable_program(loader: Pubkey) -> Account {
            Account {
                lamports: 0,
                data: Vec::new(),
                owner: loader,
                executable: true,
                rent_epoch: 0,
            }
        }

        pub fn mint(decimals: u8, _mint_authority: &Pubkey) -> Account {
            let data = create_mollusk_mint_data(decimals);
            let rent = solana_sdk::rent::Rent::default();
            Account {
                lamports: rent.minimum_balance(data.len()),
                data,
                owner: spl_token_interface::id().into(),
                executable: false,
                rent_epoch: 0,
            }
        }

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
