/// Debug logging macro that only compiles under the full-debug-logs feature
#[macro_export]
macro_rules! debug_log {
    ($($arg:tt)*) => {
        #[cfg(feature = "full-debug-logs")]
        std::println!($($arg)*);
    };
}

use pinocchio::pubkey::Pubkey;
use solana_pubkey::Pubkey as SolanaPubkey;

pub struct AllProgramIds {
    pub spl_ata_program_id: Pubkey,
    pub pata_prefunded_program_id: Pubkey,
    pub pata_legacy_program_id: Pubkey,
    pub token_program_id: Pubkey,
    pub token_2022_program_id: Pubkey,
}

#[derive(Debug, Clone)]
pub struct AtaImplementation {
    pub name: &'static str,
    pub program_id: SolanaPubkey,
    pub binary_name: &'static str,
    #[allow(dead_code)]
    pub variant: AtaVariant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtaVariant {
    PAtaLegacy,    // P-ATA without create-prefunded-account
    PAtaPrefunded, // P-ATA with create-prefunded-account
    SplAta,        // Original SPL ATA
}

pub struct AllAtaImplementations {
    pub spl_impl: AtaImplementation,
    pub pata_prefunded_impl: AtaImplementation,
    pub pata_legacy_impl: AtaImplementation,
}

impl AllAtaImplementations {
    pub fn iter(&self) -> impl Iterator<Item = &AtaImplementation> {
        [
            &self.spl_impl,
            &self.pata_prefunded_impl,
            &self.pata_legacy_impl,
        ]
        .into_iter()
    }
}

impl AtaImplementation {
    pub fn all() -> AllAtaImplementations {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let program_ids = load_program_ids(manifest_dir);

        AllAtaImplementations {
            spl_impl: Self::spl_ata(SolanaPubkey::from(program_ids.spl_ata_program_id)),
            pata_prefunded_impl: Self::p_ata_prefunded(SolanaPubkey::from(
                program_ids.pata_prefunded_program_id,
            )),
            pata_legacy_impl: Self::p_ata_legacy(SolanaPubkey::from(
                program_ids.pata_legacy_program_id,
            )),
        }
    }

    pub fn p_ata_legacy(program_id: SolanaPubkey) -> Self {
        Self {
            name: "p-ata-legacy",
            program_id,
            binary_name: "pinocchio_ata_program",
            variant: AtaVariant::PAtaLegacy,
        }
    }

    pub fn p_ata_prefunded(program_id: SolanaPubkey) -> Self {
        Self {
            name: "p-ata-prefunded",
            program_id,
            binary_name: "pinocchio_ata_program_prefunded",
            variant: AtaVariant::PAtaPrefunded,
        }
    }

    pub fn spl_ata(program_id: SolanaPubkey) -> Self {
        Self {
            name: "spl-ata",
            program_id,
            binary_name: "spl_associated_token_account",
            variant: AtaVariant::SplAta,
        }
    }
}

use std::format;
#[cfg(any(test, feature = "std"))]
use std::{vec, vec::Vec};

pub mod shared_constants {

    use solana_pubkey::Pubkey as SolanaPubkey;
    use {pinocchio::pubkey::Pubkey, pinocchio_pubkey::pubkey};

    /// Standard SPL token account size (fixed for all SPL token accounts)
    pub const TOKEN_ACCOUNT_SIZE: usize = 165;
    /// Standard mint account size (base size without extensions)
    pub const MINT_ACCOUNT_SIZE: usize = 82;
    /// Multisig account size
    pub const MULTISIG_ACCOUNT_SIZE: usize = 355;

    /// Standard lamport amounts for testing
    pub const ONE_SOL: u64 = 1_000_000_000;
    pub const TOKEN_ACCOUNT_RENT_EXEMPT: u64 = 2_000_000;
    pub const MINT_ACCOUNT_RENT_EXEMPT: u64 = 2_000_000;
    pub const EXTENDED_MINT_ACCOUNT_RENT_EXEMPT: u64 = 3_000_000;

    /// Native loader program ID (used across both test suites)
    pub const NATIVE_LOADER_ID: SolanaPubkey = SolanaPubkey::new_from_array([
        5, 135, 132, 191, 20, 139, 164, 40, 47, 176, 18, 87, 72, 136, 169, 241, 83, 160, 125, 173,
        247, 101, 192, 69, 92, 154, 151, 3, 128, 0, 0, 0,
    ]);

    /// SPL Token program ID (pinocchio format)
    pub const SPL_TOKEN_PROGRAM_ID: Pubkey = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
}

pub mod unified_builders {
    use super::shared_constants::*;
    use std::{vec, vec::Vec};

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

    pub fn create_mint_data(decimals: u8) -> Vec<u8> {
        let mut data = vec![0u8; MINT_ACCOUNT_SIZE];
        data[0..4].copy_from_slice(&1u32.to_le_bytes()); // state = 1 (Initialized)
        data[44] = decimals;
        data[45] = 1; // is_initialized = 1
        data
    }

    /// Create multisig account data
    pub fn create_multisig_data_unified(m: u8, signer_pubkeys: &[&[u8; 32]]) -> Vec<u8> {
        use spl_token_interface::state::multisig::{Multisig, MAX_SIGNERS};
        use spl_token_interface::state::Transmutable;

        assert!(
            m as usize <= signer_pubkeys.len(),
            "m cannot exceed number of provided signers"
        );
        assert!(m >= 1, "m must be at least 1");
        assert!(
            signer_pubkeys.len() <= MAX_SIGNERS as usize,
            "too many signers provided"
        );

        // Create data buffer with the exact size expected by spl_token_interface
        let mut data = vec![0u8; Multisig::LEN];

        // Set the multisig fields manually to match the exact struct layout
        data[0] = m; // m: u8
        data[1] = signer_pubkeys.len() as u8; // n: u8
        data[2] = 1; // is_initialized: u8 (1 = true)
                     // According to the on-chain `Multisig` layout the signer array starts
                     // immediately after the three-byte header (m, n, is_initialized) with *no* padding.

        // Copy each signer into place right after the 3-byte header.
        for (i, pk_bytes) in signer_pubkeys.iter().enumerate() {
            let offset = 3 + i * 32; // Each `Pubkey` is 32 bytes
            data[offset..offset + 32].copy_from_slice(*pk_bytes);
        }

        data
    }

    /// Create rent sysvar data
    pub fn create_rent_sysvar_data(
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
}

pub use shared_constants::NATIVE_LOADER_ID;

/// Matches the pinocchio Account struct.
/// Account fields are private, so this struct allows more readable
/// use of them in tests.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AccountLayout {
    pub borrow_state: u8,
    pub is_signer: u8,
    pub is_writable: u8,
    pub executable: u8,
    pub resize_delta: i32,
    pub key: Pubkey,
    pub owner: Pubkey,
    pub lamports: u64,
    pub data_len: u64,
}

// ---- Shared Mollusk Test Utilities ----

#[cfg(any(test, feature = "std"))]
use {
    mollusk_svm::{program::loader_keys::LOADER_V3, result::Check, Mollusk},
    solana_instruction::{AccountMeta, Instruction},
    solana_sdk::{account::Account, signature::Keypair, signer::Signer, sysvar},
    solana_system_interface::{instruction as system_instruction, program as system_program},
};

/// Configuration for ATA programs to load in Mollusk
#[cfg(any(test, feature = "std"))]
pub enum MolluskAtaSetup {
    PAtaDropIn,
    /// Load all ATA implementations for comparison (benchmarks)
    AllImplementations,
    /// Load a specific ATA program with custom binary name
    Custom {
        program_id: SolanaPubkey,
        binary_name: &'static str,
    },
}

/// Configuration for token programs to load in Mollusk
#[cfg(any(test, feature = "std"))]
pub enum MolluskTokenSetup {
    /// Load just the specified token program
    Single(SolanaPubkey),
    /// Load the specified token program + Token-2022
    WithToken2022(SolanaPubkey),
}

#[cfg(any(test, feature = "std"))]
pub fn setup_mollusk_unified(
    ata_setup: MolluskAtaSetup,
    token_setup: MolluskTokenSetup,
) -> Mollusk {
    let mut mollusk = Mollusk::default();

    // Setup ATA programs based on configuration
    match ata_setup {
        MolluskAtaSetup::PAtaDropIn => {
            let ata_program_id = spl_associated_token_account::id();
            mollusk.add_program(
                &ata_program_id,
                "target/deploy/pinocchio_ata_program",
                &LOADER_V3,
            );
        }
        MolluskAtaSetup::AllImplementations => {
            // Load all ATA implementations for comparison (benchmarks)
            #[cfg(any(test, feature = "std"))]
            {
                let implementations = AtaImplementation::all();
                for implementation in implementations.iter() {
                    mollusk.add_program(
                        &implementation.program_id,
                        implementation.binary_name,
                        &LOADER_V3,
                    );
                }
            }
        }
        MolluskAtaSetup::Custom {
            program_id,
            binary_name,
        } => {
            // Load a custom ATA program with specified binary
            mollusk.add_program(&program_id, binary_name, &LOADER_V3);
        }
    }

    // Setup token programs based on configuration
    match token_setup {
        MolluskTokenSetup::Single(token_program_id) => {
            let program_path = if token_program_id == spl_token_2022::id() {
                "programs/token-2022/target/deploy/spl_token_2022"
            } else {
                "programs/token/target/deploy/pinocchio_token_program"
            };
            mollusk.add_program(&token_program_id, program_path, &LOADER_V3);
        }
        MolluskTokenSetup::WithToken2022(token_program_id) => {
            // Load the specified token program
            mollusk.add_program(
                &token_program_id,
                "programs/token/target/deploy/pinocchio_token_program",
                &LOADER_V3,
            );

            // Also load Token-2022
            let token_2022_id = spl_token_2022::id();
            mollusk.add_program(
                &token_2022_id,
                "programs/token-2022/target/deploy/spl_token_2022",
                &LOADER_V3,
            );
        }
    }

    mollusk
}

/// Common mollusk setup with ATA program and token program
/// This wraps `setup_mollusk_unified` to load the P-ATA program, appropriate
/// for all tests which are not comparing to SPL ATA.
#[cfg(any(test, feature = "std"))]
pub fn setup_mollusk_with_programs(token_program_id: &SolanaPubkey) -> Mollusk {
    setup_mollusk_unified(
        MolluskAtaSetup::PAtaDropIn,
        MolluskTokenSetup::Single(*token_program_id),
    )
}

/// Load program keypairs and return program IDs
pub fn load_program_ids(manifest_dir: &str) -> AllProgramIds {
    use solana_keypair::Keypair;
    use solana_signer::Signer;
    use std::fs;

    let programs_to_load = [
        (
            "/target/deploy/pinocchio_ata_program-keypair.json",
            "pinocchio_ata_program",
        ),
        (
            "/target/deploy/pinocchio_ata_program_prefunded-keypair.json",
            "pinocchio_ata_program_prefunded",
        ),
        (
            "../target/deploy/spl_associated_token_account-keypair.json",
            "spl_associated_token_account",
        ),
        (
            "/programs/token-2022/target/deploy/spl_token_2022-keypair.json",
            "spl_token_2022",
        ),
        (
            "/programs/token/target/deploy/pinocchio_token_program-keypair.json",
            "pinocchio_token_program",
        ),
    ];

    let mut program_ids = AllProgramIds {
        spl_ata_program_id: Pubkey::default(),
        pata_prefunded_program_id: Pubkey::default(),
        pata_legacy_program_id: Pubkey::default(),
        token_program_id: Pubkey::default(),
        token_2022_program_id: Pubkey::default(),
    };

    for (keypair_path, program_name) in programs_to_load {
        let full_path = format!("{}/{}", manifest_dir, keypair_path);
        let keypair_data = fs::read_to_string(&full_path)
            .unwrap_or_else(|_| panic!("Failed to read {}", full_path));
        let keypair_bytes: Vec<u8> = serde_json::from_str(&keypair_data)
            .unwrap_or_else(|_| panic!("Failed to parse keypair JSON for {}", full_path));
        let keypair = Keypair::try_from(&keypair_bytes[..])
            .unwrap_or_else(|_| panic!("Invalid keypair for {}", full_path));
        let program_id = keypair.pubkey();

        match program_name {
            "pinocchio_ata_program" => program_ids.pata_legacy_program_id = program_id.to_bytes(),
            "pinocchio_ata_program_prefunded" => {
                program_ids.pata_prefunded_program_id = program_id.to_bytes()
            }
            "spl_associated_token_account" => {
                program_ids.spl_ata_program_id = program_id.to_bytes()
            }
            "spl_token_2022" => program_ids.token_2022_program_id = program_id.to_bytes(),
            "pinocchio_token_program" => program_ids.token_program_id = program_id.to_bytes(),
            _ => panic!("Unknown program name: {}", program_name),
        }
    }

    if program_ids.token_program_id == Pubkey::default() {
        panic!("Token program ID not found");
    }
    // Use SPL Token interface ID for p-token program
    program_ids.token_program_id = Pubkey::from(spl_token_interface::program::ID);

    if program_ids.pata_prefunded_program_id == Pubkey::default() {
        panic!("P-ATA prefunded program ID not found");
    }
    if program_ids.pata_legacy_program_id == Pubkey::default() {
        panic!("P-ATA standard program ID not found");
    }
    if program_ids.spl_ata_program_id == Pubkey::default() {
        panic!("SPL ATA program ID not found");
    }
    if program_ids.token_2022_program_id == Pubkey::default() {
        panic!("Token 2022 program ID not found");
    }

    program_ids
}

/// Create standard base accounts needed for mollusk tests
#[cfg(any(test, feature = "std"))]
pub fn create_mollusk_base_accounts(payer: &Keypair) -> Vec<(SolanaPubkey, Account)> {
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
#[cfg(any(test, feature = "std"))]
pub fn create_mollusk_base_accounts_with_token(
    payer: &Keypair,
    token_program_id: &SolanaPubkey,
) -> Vec<(SolanaPubkey, Account)> {
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

/// Create standard base accounts with token program and wallet
#[cfg(any(test, feature = "std"))]
pub fn create_mollusk_base_accounts_with_token_and_wallet(
    payer: &Keypair,
    wallet: &SolanaPubkey,
    token_program_id: &SolanaPubkey,
) -> Vec<(SolanaPubkey, Account)> {
    // Start with the standard base accounts (payer, system program, rent sysvar, token program)
    let mut accounts = create_mollusk_base_accounts_with_token(payer, token_program_id);

    // Add the wallet account with zero lamports, owned by the system program.
    accounts.push((*wallet, Account::new(0, 0, &system_program::id())));

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

#[cfg(any(test, feature = "std"))]
/// Encodes the instruction data payload for ATA creation-related instructions.
/// Extracted for reuse across test and benchmark builders.
/// TODO(refactor): Once all builders use this helper, inline encoding logic above can be removed.
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
#[cfg(any(test, feature = "std"))]
pub fn build_create_ata_instruction(
    ata_program_id: SolanaPubkey,
    payer: SolanaPubkey,
    ata_address: SolanaPubkey,
    wallet: SolanaPubkey,
    mint: SolanaPubkey,
    token_program: SolanaPubkey,
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

/// Create mint account data for mollusk testing
#[cfg(any(test, feature = "std"))]
pub fn create_mollusk_mint_data(decimals: u8) -> Vec<u8> {
    unified_builders::create_mint_data(decimals)
}

/// Create valid multisig data for testing
pub fn create_multisig_data(m: u8, n: u8, signers: &[Pubkey]) -> Vec<u8> {
    unified_builders::create_multisig_data_unified(
        m,
        &signers
            .iter()
            .take(n as usize)
            .map(|pk| pk.as_ref().try_into().expect("Pubkey is 32 bytes"))
            .collect::<Vec<_>>(),
    )
}

/// Create rent sysvar data for testing
pub fn create_rent_data(
    lamports_per_byte_year: u64,
    exemption_threshold: f64,
    burn_percent: u8,
) -> Vec<u8> {
    unified_builders::create_rent_sysvar_data(
        lamports_per_byte_year,
        exemption_threshold,
        burn_percent,
    )
}

/// Test helper to verify token account structure
pub fn validate_token_account_structure(
    data: &[u8],
    expected_mint: &Pubkey,
    expected_owner: &Pubkey,
) -> bool {
    if data.len() < shared_constants::TOKEN_ACCOUNT_SIZE {
        return false;
    }

    // Check mint
    if &data[0..32] != expected_mint.as_ref() {
        return false;
    }

    // Check owner
    if &data[32..64] != expected_owner.as_ref() {
        return false;
    }

    // Check initialized state
    data[108] != 0
}

/// Calculate the rent-exempt lamports required
pub fn calculate_account_rent(len: usize) -> u64 {
    // Mollusk embeds a `Rent` sysvar instance that mirrors Solana’s
    // runtime parameters, so we reuse it rather than hard-coding the
    // constant.
    mollusk_svm::Mollusk::default()
        .sysvars
        .rent
        .minimum_balance(len)
}

#[cfg(any(test, feature = "std"))]
/// Helper function to update account data in accounts vector after instruction execution
fn update_account_from_result(
    mollusk: &Mollusk,
    instruction: &Instruction,
    accounts: &mut [(SolanaPubkey, Account)],
    target_pubkey: SolanaPubkey,
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

#[cfg(any(test, feature = "std"))]
/// Creates and initializes a mint account with the given parameters.
/// Returns a vector of accounts including the initialized mint and all necessary
/// base accounts for testing.
pub fn create_test_mint(
    mollusk: &Mollusk,
    mint_account: &Keypair,
    mint_authority: &Keypair,
    payer: &Keypair,
    token_program: &SolanaPubkey,
    decimals: u8,
) -> Vec<(SolanaPubkey, Account)> {
    let mint_space = shared_constants::MINT_ACCOUNT_SIZE as u64;
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
    mollusk.process_and_validate_instruction(&create_mint_ix, &accounts, &[Check::success()]);
    let init_mint_ix = spl_token::instruction::initialize_mint(
        token_program,
        &mint_account.pubkey(),
        &mint_authority.pubkey(),
        Some(&mint_authority.pubkey()),
        decimals,
    )
    .unwrap();

    // Refresh the mint account data after creation.
    update_account_from_result(
        mollusk,
        &create_mint_ix,
        &mut accounts,
        mint_account.pubkey(),
    );

    mollusk.process_and_validate_instruction(&init_mint_ix, &accounts, &[Check::success()]);

    // Final refresh so callers see the initialized state.
    update_account_from_result(mollusk, &init_mint_ix, &mut accounts, mint_account.pubkey());

    accounts
}

/// Create standard ATA test accounts with all required program accounts
#[cfg(any(test, feature = "std"))]
pub fn create_ata_test_accounts(
    payer: &Keypair,
    ata_address: SolanaPubkey,
    wallet: SolanaPubkey,
    mint: SolanaPubkey,
    token_program: SolanaPubkey,
) -> Vec<(SolanaPubkey, Account)> {
    vec![
        (
            payer.pubkey(),
            Account::new(1_000_000_000, 0, &system_program::id()),
        ), // Payer with 1 SOL
        (ata_address, Account::new(0, 0, &system_program::id())), // ATA account (will be created)
        (wallet, Account::new(0, 0, &system_program::id())),      // Wallet account
        (
            mint,
            Account {
                lamports: shared_constants::MINT_ACCOUNT_RENT_EXEMPT,
                data: create_mollusk_mint_data(6),
                owner: token_program,
                executable: false,
                rent_epoch: 0,
            },
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
        (
            token_program,
            Account {
                lamports: 0,
                data: Vec::new(),
                owner: LOADER_V3,
                executable: true,
                rent_epoch: 0,
            },
        ),
        (sysvar::rent::id(), Account::new(1009200, 17, &sysvar::id())), // Rent sysvar
    ]
}

pub mod account_builder {
    use {
        super::create_mollusk_mint_data,
        crate::test_utils::unified_builders::create_token_account_data, mollusk_svm::Mollusk,
        solana_account::Account, solana_pubkey::Pubkey as SolanaPubkey, solana_sysvar::rent,
        std::vec, std::vec::Vec,
    };

    pub struct AccountBuilder;

    use super::shared_constants::*;

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

        pub fn executable_program(loader: SolanaPubkey) -> Account {
            Account {
                lamports: 0,
                data: Vec::new(),
                owner: loader,
                executable: true,
                rent_epoch: 0,
            }
        }

        pub fn mint(decimals: u8, _mint_authority: &SolanaPubkey) -> Account {
            Account {
                lamports: MINT_ACCOUNT_RENT_EXEMPT,
                data: create_mollusk_mint_data(decimals),
                owner: spl_token_interface::program::id().into(),
                executable: false,
                rent_epoch: 0,
            }
        }

        pub fn extended_mint(decimals: u8, _mint_authority: &SolanaPubkey) -> Account {
            use solana_program_option::COption;
            use spl_token_2022::{
                extension::{ExtensionType, PodStateWithExtensionsMut},
                pod::PodMint,
            };

            // Calculate the minimum size for a Token-2022 mint without extensions
            let required_size =
                ExtensionType::try_calculate_account_len::<spl_token_2022::state::Mint>(&[])
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

            Account {
                lamports: MINT_ACCOUNT_RENT_EXEMPT,
                data,
                owner: spl_token_2022::id(),
                executable: false,
                rent_epoch: 0,
            }
        }

        pub fn extended_mint_with_extensions(
            decimals: u8,
            _mint_authority: &SolanaPubkey,
        ) -> Account {
            use solana_program_option::COption;
            use spl_token_2022::{
                extension::{
                    default_account_state::DefaultAccountState, metadata_pointer::MetadataPointer,
                    non_transferable::NonTransferable, transfer_fee::TransferFeeConfig,
                    transfer_hook::TransferHook, BaseStateWithExtensionsMut, ExtensionType,
                    PodStateWithExtensionsMut,
                },
                pod::PodMint,
                state::AccountState,
            };

            // Extensions that should be included in the extended test
            let mut extension_types = vec![
                ExtensionType::TransferFeeConfig,
                ExtensionType::NonTransferable,
                ExtensionType::TransferHook,
                ExtensionType::DefaultAccountState,
                ExtensionType::MetadataPointer,
            ];
            extension_types.push(ExtensionType::DefaultAccountState); // Mint-only extension
            extension_types.push(ExtensionType::MetadataPointer); // Mint-only extension

            // Calculate the size for a Token-2022 mint with extensions
            let required_size = ExtensionType::try_calculate_account_len::<
                spl_token_2022::state::Mint,
            >(&extension_types)
            .expect("Failed to calculate Token-2022 mint size with extensions");

            let mut data = vec![0; required_size];

            // Use Token-2022's proper unpacking to initialize the mint
            let mut mint = PodStateWithExtensionsMut::<PodMint>::unpack_uninitialized(&mut data)
                .expect("Failed to unpack Token-2022 mint with extensions");

            // Initialize base mint fields
            mint.base.mint_authority = COption::None.into();
            mint.base.supply = 0u64.into();
            mint.base.decimals = decimals;
            mint.base.is_initialized = true.into();
            mint.base.freeze_authority = COption::None.into();

            // Initialize extensions
            // TransferFeeConfig extension
            let transfer_fee_config = mint
                .init_extension::<TransferFeeConfig>(true)
                .expect("Failed to init TransferFeeConfig");
            transfer_fee_config.transfer_fee_config_authority = COption::None.try_into().unwrap();
            transfer_fee_config.withdraw_withheld_authority = COption::None.try_into().unwrap();
            transfer_fee_config.withheld_amount = 0u64.into();

            // Initialize NonTransferable extension
            let _non_transferable = mint
                .init_extension::<NonTransferable>(true)
                .expect("Failed to init NonTransferable");

            // TransferHook extension
            let transfer_hook = mint
                .init_extension::<TransferHook>(true)
                .expect("Failed to init TransferHook");
            transfer_hook.authority = COption::None.try_into().unwrap();
            transfer_hook.program_id = COption::None.try_into().unwrap();

            // Initialize DefaultAccountState extension
            let default_account_state = mint
                .init_extension::<DefaultAccountState>(true)
                .expect("Failed to init DefaultAccountState");
            default_account_state.state = AccountState::Initialized.into();

            // MetadataPointer extension
            let metadata_pointer = mint
                .init_extension::<MetadataPointer>(true)
                .expect("Failed to init MetadataPointer");
            metadata_pointer.authority = COption::None.try_into().unwrap();
            metadata_pointer.metadata_address = COption::None.try_into().unwrap();

            // CRITICAL: Initialize the account type to mark as a proper mint
            mint.init_account_type()
                .expect("Failed to init account type");

            Account {
                lamports: MINT_ACCOUNT_RENT_EXEMPT,
                data,
                owner: spl_token_2022::id(),
                executable: false,
                rent_epoch: 0,
            }
        }

        pub fn token_account(
            mint: &SolanaPubkey,
            owner: &SolanaPubkey,
            amount: u64,
            token_program: &SolanaPubkey,
        ) -> Account {
            Account {
                lamports: TOKEN_ACCOUNT_RENT_EXEMPT,
                data: create_token_account_data(&mint.to_bytes(), &owner.to_bytes(), amount),
                owner: *token_program,
                executable: false,
                rent_epoch: 0,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::unified_builders::create_token_account_data;

    use super::*;

    #[test]
    fn test_validate_token_account_structure() {
        let mint = Pubkey::from([1u8; 32]);
        let owner = Pubkey::from([2u8; 32]);
        let data = create_token_account_data(&mint, &owner, 1000);

        assert!(validate_token_account_structure(&data, &mint, &owner));
        assert!(!validate_token_account_structure(
            &data,
            &Pubkey::from([99u8; 32]),
            &owner
        ));
    }

    /* -- Tests of Test Helpers -- */

    #[test]
    fn test_create_multisig_data() {
        let signers = vec![
            Pubkey::from([1u8; 32]),
            Pubkey::from([2u8; 32]),
            Pubkey::from([3u8; 32]),
        ];
        let data = create_multisig_data(2, 3, &signers);

        assert_eq!(data.len(), shared_constants::MULTISIG_ACCOUNT_SIZE);
        assert_eq!(data[0], 2); // m
        assert_eq!(data[1], 3); // n
        assert_eq!(data[2], 1); // initialized
                                // Signer array starts immediately after the 3-byte header.
        assert_eq!(&data[3..35], signers[0].as_ref());
    }

    /// Test that the token account data is created correctly
    /// in the test utility function
    #[test]
    fn test_token_account_data_creation() {
        let mint = Pubkey::from([1u8; 32]);
        let owner = Pubkey::from([2u8; 32]);
        let amount = 1000u64;

        let data = create_token_account_data(&mint, &owner, amount);

        // Verify the data structure
        assert_eq!(data.len(), shared_constants::TOKEN_ACCOUNT_SIZE);
        assert_eq!(&data[0..32], mint.as_ref());
        assert_eq!(&data[32..64], owner.as_ref());

        let stored_amount = u64::from_le_bytes(data[64..72].try_into().unwrap());
        assert_eq!(stored_amount, amount);

        // Verify initialized state
        assert_eq!(data[108], 1);
    }

    /// Test that the rent data is created correctly
    /// in the test utility function
    #[test]
    fn test_rent_data_creation() {
        let lamports_per_byte_year = 1000u64;
        let exemption_threshold = 2.0f64;
        let burn_percent = 50u8;

        let data = create_rent_data(lamports_per_byte_year, exemption_threshold, burn_percent);

        // Verify basic structure (simplified test)
        assert!(!data.is_empty());
        assert_eq!(data.len(), 8 + 8 + 1); // u64 + f64 + u8
    }

    #[test]
    fn test_account_layout_compatibility() {
        unsafe {
            let test_header = AccountLayout {
                borrow_state: 42,
                is_signer: 1,
                is_writable: 1,
                executable: 0,
                resize_delta: 100,
                key: [1u8; 32],
                owner: [2u8; 32],
                lamports: 1000,
                data_len: 256,
            };

            let account_ptr = &test_header as *const AccountLayout;
            let account_ref = &*account_ptr;
            assert_eq!(
                account_ref.borrow_state, 42,
                "borrow_state field should be accessible and match"
            );
            assert_eq!(
                account_ref.data_len, 256,
                "data_len field should be accessible and match"
            );
        }
    }

    #[test]
    fn test_mollusk_utilities() {
        use solana_sdk::signature::Keypair;

        let payer = Keypair::new();
        let token_program = spl_token::id();

        // Test base accounts creation
        let accounts = create_mollusk_base_accounts(&payer);
        assert_eq!(accounts.len(), 3);

        let accounts_with_token = create_mollusk_base_accounts_with_token(&payer, &token_program);
        assert_eq!(accounts_with_token.len(), 4);

        // Test mollusk token account data
        let mint = SolanaPubkey::new_unique();
        let owner = SolanaPubkey::new_unique();
        let data = create_token_account_data(&mint.to_bytes(), &owner.to_bytes(), 1000);
        assert_eq!(data.len(), shared_constants::TOKEN_ACCOUNT_SIZE);
        assert_eq!(&data[0..32], mint.as_ref());
        assert_eq!(&data[32..64], owner.as_ref());
        assert_eq!(data[108], 1); // initialized

        // Test mint data
        let mint_data = create_mollusk_mint_data(6);
        assert_eq!(mint_data.len(), shared_constants::MINT_ACCOUNT_SIZE);
        assert_eq!(mint_data[44], 6); // decimals
        assert_eq!(mint_data[45], 1); // initialized
    }
}
