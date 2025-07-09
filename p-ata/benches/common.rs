use {
    mollusk_svm::{program::loader_keys::LOADER_V3, Mollusk},
    solana_account::Account,
    solana_instruction,
    solana_pubkey::Pubkey,
    solana_sysvar::rent,
    spl_token_2022::extension::ExtensionType,
    spl_token_interface::state::Transmutable,
    std::env,
};

pub mod account_templates;
pub mod constants;

use account_templates::*;
use constants::{account_sizes::*, lamports::*};

// ================================ CONSTANTS ================================

pub const SYSTEM_PROGRAM_ID: Pubkey = Pubkey::new_from_array([0u8; 32]);
pub const NATIVE_LOADER_ID: Pubkey = Pubkey::new_from_array([
    5, 135, 132, 191, 20, 139, 164, 40, 47, 176, 18, 87, 72, 136, 169, 241, 83, 160, 125, 173, 247,
    101, 192, 69, 92, 154, 151, 3, 128, 0, 0, 0,
]);

// ============================= ACCOUNT BUILDERS =============================

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

    pub fn token_account_data(mint: &Pubkey, owner: &Pubkey, amount: u64) -> Vec<u8> {
        #[cfg(feature = "full-debug-logs")]
        println!(
            "🔧 Creating token account data | Mint: {} | Owner: {}",
            mint.to_string()[0..8].to_string(),
            owner.to_string()[0..8].to_string()
        );

        build_token_account_data_core(
            mint.as_ref().try_into().expect("Pubkey is 32 bytes"),
            owner.as_ref().try_into().expect("Pubkey is 32 bytes"),
            amount,
        )
        .to_vec()
    }

    pub fn mint_data(decimals: u8) -> Vec<u8> {
        build_mint_data_core(decimals).to_vec()
    }

    pub fn extended_mint_data(decimals: u8) -> Vec<u8> {
        let required_len =
            ExtensionType::try_calculate_account_len::<spl_token_2022::state::Mint>(&[
                ExtensionType::ImmutableOwner,
            ])
            .expect("calc len");

        let mut data = Self::mint_data(decimals);
        data.resize(required_len, 0u8);

        let cursor = MINT_ACCOUNT_SIZE;
        let immutable_owner_header = [7u8, 0u8, 0u8, 0u8];
        data[cursor..cursor + 4].copy_from_slice(&immutable_owner_header);

        data
    }

    pub fn multisig_data(m: u8, signer_pubkeys: &[Pubkey]) -> Vec<u8> {
        let byte_refs: Vec<&[u8; 32]> = signer_pubkeys
            .iter()
            .map(|pk| pk.as_ref().try_into().expect("Pubkey is 32 bytes"))
            .collect();
        build_multisig_data_core(m, &byte_refs)
    }

    pub fn system_account(lamports: u64) -> Account {
        Account::new(lamports, 0, &SYSTEM_PROGRAM_ID)
    }

    pub fn executable_program(owner: Pubkey) -> Account {
        Account {
            lamports: 0,
            data: Vec::new(),
            owner,
            executable: true,
            rent_epoch: 0,
        }
    }

    pub fn token_account(
        mint: &Pubkey,
        owner: &Pubkey,
        amount: u64,
        token_program_id: &Pubkey,
    ) -> Account {
        Account {
            lamports: TOKEN_ACCOUNT_RENT_EXEMPT,
            data: Self::token_account_data(mint, owner, amount),
            owner: *token_program_id,
            executable: false,
            rent_epoch: 0,
        }
    }

    pub fn mint_account(decimals: u8, token_program_id: &Pubkey, extended: bool) -> Account {
        Account {
            lamports: ONE_SOL,
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

    pub fn token_2022_mint_account(decimals: u8, token_program_id: &Pubkey) -> Account {
        Account {
            lamports: ONE_SOL,
            data: Self::token_2022_mint_data(decimals),
            owner: *token_program_id,
            executable: false,
            rent_epoch: 0,
        }
    }

    pub fn token_2022_mint_data(decimals: u8) -> Vec<u8> {
        let mut data = [0u8; MINT_ACCOUNT_SIZE];
        let mint_authority = structured_pk(
            &AtaVariant::SplAta,
            TestBankId::Benchmarks,
            123,
            AccountTypeId::Mint,
        );

        data[0..4].copy_from_slice(&1u32.to_le_bytes());
        data[4..36].copy_from_slice(mint_authority.as_ref());
        data[44] = decimals;
        data[45] = 1;
        data[46..50].copy_from_slice(&0u32.to_le_bytes());

        data.to_vec()
    }
}

// ========================== STRUCTURED ADDRESS ALLOCATION ==========================

/// Test bank identifier  
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestBankId {
    Benchmarks = 0,
    Failures = 1,
}

/// Account type identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum AccountTypeId {
    Payer = 0,
    Mint = 1,
    Wallet = 2,
    Ata = 3,
    SystemProgram = 4,
    TokenProgram = 5,
    RentSysvar = 6,
    OwnerMint = 7,
    NestedMint = 8,
    OwnerAta = 9,
    NestedAta = 10,
    Signer1 = 11,
    Signer2 = 12,
    Signer3 = 13,
}

/// Convert AtaVariant to byte value
fn variant_to_byte(variant: &AtaVariant) -> u8 {
    match variant {
        AtaVariant::PAtaLegacy => 1, // Changed from 0 to avoid system program ID
        AtaVariant::PAtaPrefunded => 2, // Changed from 1 to 2
        AtaVariant::SplAta => 3,     // Changed from 2 to 3
    }
}

/// Generate a structured pubkey from 4-byte coordinate system
/// [variant, test_bank, test_number, account_type].
/// Avoids some issues with test cross-contamination by using predictable
/// but different keys for different tests.
pub fn structured_pk(
    variant: &AtaVariant,
    test_bank: TestBankId,
    test_number: u8,
    account_type: AccountTypeId,
) -> Pubkey {
    // For proper byte-for-byte comparison between implementations,
    // use consistent addresses for wallet/owner and mint accounts
    let effective_variant = match account_type {
        AccountTypeId::Wallet
        | AccountTypeId::Mint
        | AccountTypeId::OwnerMint
        | AccountTypeId::NestedMint => &AtaVariant::SplAta, // Always use Original for consistency
        _ => variant, // Use actual variant for other account types (Payer, ATA addresses, etc.)
    };

    let mut bytes = [0u8; 32];
    bytes[0] = variant_to_byte(effective_variant);
    bytes[1] = test_bank as u8;
    bytes[2] = test_number;
    bytes[3] = account_type as u8;

    Pubkey::new_from_array(bytes)
}

/// Generate multiple structured pubkeys at once.
/// Avoids some issues with test cross-contamination by using predictable
/// but different keys for different tests.
#[allow(dead_code)]
pub fn structured_pk_multi<const N: usize>(
    variant: &AtaVariant,
    test_bank: TestBankId,
    test_number: u8,
    account_types: [AccountTypeId; N],
) -> [Pubkey; N] {
    account_types.map(|account_type| structured_pk(variant, test_bank, test_number, account_type))
}

/// Find a pubkey that gives the same lowest bump across multiple ATA program IDs
///
/// This function finds a pubkey that produces the lowest common bump value across all
/// provided ATA program IDs.
///
/// Avoids some issues with test cross-contamination by using predictable
/// but different keys for different tests.
///
/// # Arguments
/// * `variant` - The ATA variant to use for base key generation
/// * `test_bank` - The test bank ID for structuring
/// * `test_number` - The test number for structuring  
/// * `account_type` - The account type for structuring
/// * `ata_program_ids` - Slice of ATA program IDs to find common bump for
/// * `token_program_id` - The token program ID for PDA derivation
/// * `mint` - The mint pubkey for PDA derivation
///
/// # Returns
/// A pubkey that gives the same lowest possible bump across all provided program IDs.
pub fn structured_pk_with_optimal_common_bump(
    variant: &AtaVariant,
    test_bank: TestBankId,
    test_number: u8,
    account_type: AccountTypeId,
    ata_program_ids: &[Pubkey], // can be 1; otherwise, finds pk with COMMON optimal bump
    token_program_id: &Pubkey,
    mint: &Pubkey,
) -> Pubkey {
    // Handle empty array case
    if ata_program_ids.is_empty() {
        return structured_pk(variant, test_bank, test_number, account_type);
    }

    // Start with the base structured key
    let base_key = structured_pk(variant, test_bank, test_number, account_type);
    let mut key_bytes = base_key.to_bytes();

    // Try different variations until we find one with optimal bump (255) for all program IDs
    for modifier in 0u32..10000 {
        // Modify the last 4 bytes with the modifier
        let modifier_bytes = modifier.to_le_bytes();
        key_bytes[28..32].copy_from_slice(&modifier_bytes);

        let test_key = Pubkey::new_from_array(key_bytes);

        // Check if this key produces bump 255 for all program IDs
        let all_optimal = ata_program_ids.iter().all(|program_id| {
            let (_, bump) = Pubkey::find_program_address(
                &[test_key.as_ref(), token_program_id.as_ref(), mint.as_ref()],
                program_id,
            );
            bump == 255
        });

        if all_optimal {
            return test_key;
        }
    }

    // If we couldn't find a common optimal bump, return base key
    base_key
}

pub fn build_multisig_data_core(m: u8, signer_pubkeys: &[&[u8; 32]]) -> Vec<u8> {
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
    data[0] = m;
    data[1] = signer_pubkeys.len() as u8;
    data[2] = 1;

    for (i, pk) in signer_pubkeys.iter().enumerate() {
        let offset = 3 + i * 32;
        data[offset..offset + 32].copy_from_slice(*pk);
    }
    data
}

#[inline(always)]
fn build_mint_data_core(decimals: u8) -> [u8; MINT_ACCOUNT_SIZE] {
    let mut data = [0u8; MINT_ACCOUNT_SIZE];
    data[0..4].copy_from_slice(&0u32.to_le_bytes());
    data[44] = decimals;
    data[45] = 1;
    data[46..50].copy_from_slice(&0u32.to_le_bytes());

    data
}

#[inline(always)]
fn build_token_account_data_core(
    mint: &[u8; 32],
    owner: &[u8; 32],
    amount: u64,
) -> [u8; TOKEN_ACCOUNT_SIZE] {
    let mut data = [0u8; TOKEN_ACCOUNT_SIZE];
    data[0..32].copy_from_slice(mint);
    data[32..64].copy_from_slice(owner);
    data[64..72].copy_from_slice(&amount.to_le_bytes());
    data[108] = 1;
    data
}

// ========================== SHARED BENCHMARK SETUP ============================

pub struct BenchmarkSetup;

pub struct AllProgramIds {
    pub spl_ata_program_id: Pubkey,
    pub pata_prefunded_program_id: Pubkey,
    pub pata_legacy_program_id: Pubkey,
    pub token_program_id: Pubkey,
    pub token_2022_program_id: Pubkey,
}

impl BenchmarkSetup {
    /// Setup SBF output directory and copy required files
    pub fn setup_sbf_environment(manifest_dir: &str) -> String {
        use std::path::Path;

        // Use the standard deploy directory where p-ata program is built
        let deploy_dir = format!("{}/target/deploy", manifest_dir);
        println!("Setting SBF_OUT_DIR to: {}", deploy_dir);
        std::env::set_var("SBF_OUT_DIR", &deploy_dir);

        // Ensure the deploy directory exists
        std::fs::create_dir_all(&deploy_dir).expect("Failed to create deploy directory");

        // Create symbolic links to programs in their actual locations
        let symlinks = [
            (
                "spl_associated_token_account.so",
                "../target/deploy/spl_associated_token_account.so",
            ),
            (
                "pinocchio_token_program.so",
                "programs/token/target/deploy/pinocchio_token_program.so",
            ),
            (
                "spl_token_2022.so",
                "programs/token-2022/target/deploy/spl_token_2022.so",
            ),
        ];

        for (filename, target_path) in &symlinks {
            let link_path = Path::new(&deploy_dir).join(filename);
            let full_target_path = Path::new(manifest_dir).join(target_path);

            if full_target_path.exists() && !link_path.exists() {
                println!("Creating symlink {} -> {}", filename, target_path);
                #[cfg(unix)]
                {
                    std::os::unix::fs::symlink(&full_target_path, &link_path).unwrap_or_else(|e| {
                        panic!("Failed to create symlink for {}: {}", filename, e)
                    });
                }
                #[cfg(windows)]
                {
                    std::os::windows::fs::symlink_file(&full_target_path, &link_path)
                        .unwrap_or_else(|e| {
                            panic!("Failed to create symlink for {}: {}", filename, e)
                        });
                }
            }
        }

        deploy_dir
    }

    /// Load program keypairs and return program IDs
    pub fn load_program_ids(manifest_dir: &str) -> AllProgramIds {
        use solana_keypair::Keypair;
        use solana_signer::Signer;
        use std::fs;

        let programs_to_load: Vec<(&str, &str)> = vec![
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

        let mut program_ids: AllProgramIds = AllProgramIds {
            spl_ata_program_id: Pubkey::default(),
            pata_prefunded_program_id: Pubkey::default(),
            pata_legacy_program_id: Pubkey::default(),
            token_program_id: Pubkey::default(),
            token_2022_program_id: Pubkey::default(),
        };

        for (keypair_path, program_name) in programs_to_load {
            let keypair_path = format!("{}/{}", manifest_dir, keypair_path);
            let keypair_data = fs::read_to_string(&keypair_path)
                .expect(&format!("Failed to read {}", keypair_path));
            let keypair_bytes: Vec<u8> = serde_json::from_str(&keypair_data).expect(&format!(
                "Failed to parse keypair JSON for {}",
                keypair_path
            ));
            let keypair = Keypair::try_from(&keypair_bytes[..])
                .expect(&format!("Invalid keypair for {}", keypair_path));
            let program_id = keypair.pubkey();
            // println!("Loaded {} program ID: {}", program_name, program_id);
            match program_name {
                "pinocchio_ata_program" => program_ids.pata_legacy_program_id = program_id,
                "pinocchio_ata_program_prefunded" => {
                    program_ids.pata_prefunded_program_id = program_id
                }
                "spl_associated_token_account" => program_ids.spl_ata_program_id = program_id,
                "spl_token_2022" => program_ids.token_2022_program_id = program_id,
                "pinocchio_token_program" => program_ids.token_program_id = program_id,
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

    #[allow(dead_code)]
    /// Validate that the benchmark setup works with a simple test
    pub(crate) fn validate_setup(
        mollusk: &Mollusk,
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> Result<(), String> {
        use solana_instruction::{AccountMeta, Instruction};

        // Simple validation test - create a basic instruction and ensure it doesn't crash
        let payer = structured_pk(
            &AtaVariant::SplAta,
            TestBankId::Benchmarks,
            1,
            AccountTypeId::Payer,
        );
        let mint = structured_pk(
            &AtaVariant::SplAta,
            TestBankId::Benchmarks,
            1,
            AccountTypeId::Mint,
        );
        let wallet = structured_pk(
            &AtaVariant::SplAta,
            TestBankId::Benchmarks,
            1,
            AccountTypeId::Wallet,
        );
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let accounts = StandardAccountSet::new(payer, ata, wallet, mint, token_program_id).to_vec();

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

        let result = mollusk.process_instruction(&ix, &accounts);

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

// ========================== SHARED COMPARISON FRAMEWORK ============================

#[derive(Debug, Clone)]
pub struct AtaImplementation {
    pub name: &'static str,
    pub program_id: Pubkey,
    pub binary_name: &'static str,
    pub variant: AtaVariant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtaVariant {
    PAtaLegacy,    // P-ATA without create-account-prefunded
    PAtaPrefunded, // P-ATA with create-account-prefunded
    SplAta,        // Original SPL ATA
}

pub struct AllAtaImplementations {
    pub spl_impl: AtaImplementation,
    pub pata_prefunded_impl: AtaImplementation,
    pub pata_legacy_impl: AtaImplementation,
}

impl AllAtaImplementations {
    pub fn iter(&self) -> impl Iterator<Item = &AtaImplementation> {
        vec![
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
        let program_ids = BenchmarkSetup::load_program_ids(manifest_dir);

        AllAtaImplementations {
            spl_impl: Self::spl_ata(program_ids.spl_ata_program_id),
            pata_prefunded_impl: Self::p_ata_prefunded(program_ids.pata_prefunded_program_id),
            pata_legacy_impl: Self::p_ata_legacy(program_ids.pata_legacy_program_id),
        }
    }

    pub fn p_ata_legacy(program_id: Pubkey) -> Self {
        Self {
            name: "p-ata-legacy",
            program_id,
            binary_name: "pinocchio_ata_program",
            variant: AtaVariant::PAtaLegacy,
        }
    }

    pub(crate) fn p_ata_prefunded(program_id: Pubkey) -> Self {
        Self {
            name: "p-ata-prefunded",
            program_id,
            binary_name: "pinocchio_ata_program_prefunded",
            variant: AtaVariant::PAtaPrefunded,
        }
    }

    pub fn spl_ata(program_id: Pubkey) -> Self {
        Self {
            name: "spl-ata",
            program_id,
            binary_name: "spl_associated_token_account",
            variant: AtaVariant::SplAta,
        }
    }

    /// Adapt instruction data for this implementation
    pub fn adapt_instruction_data(&self, data: Vec<u8>) -> Vec<u8> {
        match self.variant {
            AtaVariant::PAtaLegacy | AtaVariant::PAtaPrefunded => data, // P-ATA supports bump optimizations
            AtaVariant::SplAta => {
                // SPL ATA doesn't support bump optimizations, strip them
                match data.as_slice() {
                    [0, _bump] => vec![0],
                    [2, _bump] => vec![2],
                    _ => data,
                }
            }
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, PartialEq, Clone)]
pub enum CompatibilityStatus {
    /// Both implementations succeeded and produced byte-for-byte identical results.
    ///
    /// **GUARANTEES:**
    /// - Both instructions succeeded
    /// - All **writable accounts** (including ATA accounts) are byte-for-byte identical:
    ///   - `data`: Complete binary equality
    ///   - `lamports`: Exact same balance  
    ///   - `owner`: Same program owner
    /// - Read-only accounts are not compared (they shouldn't change)
    ///
    /// **IMPLEMENTATION NOTES:**
    /// - Mint and owner addresses are intentionally kept consistent between P-ATA and SPL ATA
    ///   tests to enable true byte-for-byte comparison of ATA accounts
    /// - SysvarRent differences are handled separately and don't affect this status
    ///
    /// **DOES NOT GUARANTEE:**
    /// - Identical compute unit consumption (tracked separately)
    /// - Identical instruction data in the case of new p-ATA optimizations)
    /// - Read-only account equality (not relevant for result validation)
    Identical,
    BothRejected,        // Both failed with same error types
    OptimizedBehavior,   // P-ATA succeeded where original failed (bump optimization)
    AccountMismatch,     // Both succeeded but account states differ (concerning)
    IncompatibleFailure, // Both failed but with different error codes
    IncompatibleSuccess, // One succeeded, one failed unexpectedly
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    pub implementation: String,
    pub test_name: String,
    pub compute_units: u64,
    pub success: bool,
    pub error_message: Option<String>,
    pub captured_output: String, // Capture mollusk debug output
}

#[derive(Debug, Clone)]
pub struct ComparisonResult {
    pub test_name: String,
    pub p_ata: BenchmarkResult,
    pub spl_ata: BenchmarkResult,
    pub compute_savings: Option<i64>,
    pub compatibility_status: CompatibilityStatus,
}

// ========================== SHARED COMPARISON RUNNER ============================

pub struct BenchmarkRunner;

impl BenchmarkRunner {
    /// Run a single benchmark for one implementation
    pub fn run_single_benchmark(
        test_name: &str,
        ix: &solana_instruction::Instruction,
        accounts: &[(Pubkey, Account)],
        implementation: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> BenchmarkResult {
        let mollusk = Self::create_mollusk_for_all_ata_implementations(token_program_id);

        // First run with quiet logging unless full-debug-logs feature is enabled
        #[cfg(not(feature = "full-debug-logs"))]
        let result = mollusk.process_instruction(ix, accounts);

        #[cfg(feature = "full-debug-logs")]
        let result = {
            // Enable debug logging for full-debug-logs feature
            let _original_rust_log =
                std::env::var("RUST_LOG").unwrap_or_else(|_| "error".to_string());
            std::env::set_var("RUST_LOG", "debug");
            let _ = solana_logger::setup_with(
                "debug,solana_runtime=debug,solana_program_runtime=debug,mollusk=debug",
            );

            let result = mollusk.process_instruction(ix, accounts);

            // Restore original logging
            std::env::set_var("RUST_LOG", &_original_rust_log);
            let _ = solana_logger::setup_with(
                "error,solana_runtime=error,solana_program_runtime=error,mollusk=error",
            );

            result
        };

        let success = matches!(
            result.program_result,
            mollusk_svm::result::ProgramResult::Success
        );
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
            captured_output: String::new(), // Will be populated if we need to re-run with debug
        }
    }

    /// Run a benchmark with verbose debug logging enabled - used for problematic results
    pub fn run_single_benchmark_with_debug(
        test_name: &str,
        ix: &solana_instruction::Instruction,
        accounts: &[(Pubkey, Account)],
        implementation: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> BenchmarkResult {
        // Temporarily enable debug logging
        let original_rust_log = std::env::var("RUST_LOG").unwrap_or_else(|_| "error".to_string());
        std::env::set_var("RUST_LOG", "debug");

        let _ = solana_logger::setup_with(
            "debug,solana_runtime=debug,solana_program_runtime=debug,mollusk=debug",
        );

        let mollusk = Self::create_mollusk_for_all_ata_implementations(token_program_id);

        // Capture output during execution
        let captured_output =
            Self::capture_output_during_execution(|| mollusk.process_instruction(ix, accounts));

        let (result, output) = captured_output;

        // Restore quiet logging unless full-debug-logs feature is enabled
        #[cfg(not(feature = "full-debug-logs"))]
        {
            std::env::set_var("RUST_LOG", &original_rust_log);
            let _ = solana_logger::setup_with(
                "error,solana_runtime=error,solana_program_runtime=error,mollusk=error",
            );
        }

        let success = matches!(
            result.program_result,
            mollusk_svm::result::ProgramResult::Success
        );
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
            captured_output: output,
        }
    }

    /// Capture stdout/stderr output during function execution
    fn capture_output_during_execution<F, R>(f: F) -> (R, String)
    where
        F: FnOnce() -> R,
    {
        use std::sync::{Arc, Mutex};

        let captured = Arc::new(Mutex::new(Vec::new()));
        let captured_clone = captured.clone();

        let result = f();

        let captured_text = if let Ok(buffer) = captured_clone.lock() {
            String::from_utf8_lossy(&buffer).to_string()
        } else {
            String::new()
        };

        (result, captured_text)
    }

    pub fn create_mollusk_for_all_ata_implementations(token_program_id: &Pubkey) -> Mollusk {
        let mut mollusk = Mollusk::default();

        for implementation in AtaImplementation::all().iter() {
            mollusk.add_program(
                &implementation.program_id,
                implementation.binary_name,
                &LOADER_V3,
            );
        }

        mollusk.add_program(token_program_id, "pinocchio_token_program", &LOADER_V3);

        let token_2022_id = Pubkey::new_from_array(pinocchio_pubkey::pubkey!(
            "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
        ));
        mollusk.add_program(&token_2022_id, "spl_token_2022", &LOADER_V3);

        mollusk
    }

    /// Create comparison result with compatibility checking
    pub fn create_comparison_result(
        test_name: &str,
        p_ata_result: BenchmarkResult,
        original_result: BenchmarkResult,
    ) -> ComparisonResult {
        let compute_savings = if p_ata_result.success && original_result.success {
            Some(original_result.compute_units as i64 - p_ata_result.compute_units as i64)
        } else {
            None
        };

        let compatibility_status =
            Self::determine_compatibility_status(&p_ata_result, &original_result);

        ComparisonResult {
            test_name: test_name.to_string(),
            p_ata: p_ata_result,
            spl_ata: original_result,
            compute_savings,
            compatibility_status,
        }
    }

    /// Determine compatibility status based on results
    pub fn determine_compatibility_status(
        p_ata_result: &BenchmarkResult,
        original_result: &BenchmarkResult,
    ) -> CompatibilityStatus {
        // Check if this is a P-ATA-only test (N/A for original)
        if let Some(ref error_msg) = original_result.error_message {
            if error_msg.contains("N/A - Test not applicable to original ATA") {
                return CompatibilityStatus::OptimizedBehavior; // P-ATA-only feature
            }
        }

        match (p_ata_result.success, original_result.success) {
            (true, true) => CompatibilityStatus::Identical,
            (false, false) => {
                // Both failed - check if they failed with same error type
                match (&p_ata_result.error_message, &original_result.error_message) {
                    (Some(p_ata_err), Some(orig_err)) => {
                        if Self::errors_are_compatible(p_ata_err, orig_err) {
                            CompatibilityStatus::BothRejected
                        } else {
                            CompatibilityStatus::IncompatibleFailure
                        }
                    }
                    _ => CompatibilityStatus::IncompatibleFailure,
                }
            }
            (true, false) => {
                if p_ata_result.test_name.starts_with("fail_") {
                    CompatibilityStatus::IncompatibleSuccess
                } else {
                    CompatibilityStatus::OptimizedBehavior
                }
            }
            (false, true) => CompatibilityStatus::IncompatibleSuccess,
        }
    }

    /// Check if two error messages are compatible (same type of error)
    fn errors_are_compatible(p_ata_err: &str, orig_err: &str) -> bool {
        p_ata_err == orig_err
    }

    /// Print individual comparison result
    #[allow(dead_code)]
    pub fn print_comparison_result(result: &ComparisonResult) {
        println!("\n--- {} ---", result.test_name);

        // Compute unit comparison
        println!(
            "  P-ATA:    {:>8} CUs | {}",
            result.p_ata.compute_units,
            if result.p_ata.success {
                "Success"
            } else {
                "Failed"
            }
        );
        println!(
            "  Original: {:>8} CUs | {}",
            result.spl_ata.compute_units,
            if result.spl_ata.success {
                "Success"
            } else {
                "Failed"
            }
        );

        // Savings analysis (mainly relevant for successful tests)
        if let Some(savings) = result.compute_savings {
            if savings > 0 {
                println!("  Savings: {:>8} CUs ", savings,);
            } else if savings < 0 {
                println!("  Overhead: {:>7} CUs ", -savings,);
            } else {
                println!("  Equal compute usage");
            }
        }

        // Compatibility status
        match result.compatibility_status {
            CompatibilityStatus::Identical => {
                if result.test_name.starts_with("fail_")
                    && result.p_ata.success
                    && result.spl_ata.success
                {
                    println!("  Status: Both succeeded (TEST ISSUE - should fail!)")
                } else {
                    println!("  Status: Identical (both succeeded)")
                }
            }
            CompatibilityStatus::BothRejected => {
                println!("  Status: Both failed (same error type)")
            }
            CompatibilityStatus::OptimizedBehavior => {
                println!("  Status: P-ATA optimization working")
            }
            CompatibilityStatus::AccountMismatch => {
                println!("  Status: Account mismatch (concerning)")
            }
            CompatibilityStatus::IncompatibleFailure => {
                println!("  Status: Different failure modes (concerning)")
            }
            CompatibilityStatus::IncompatibleSuccess => {
                if result.test_name.starts_with("fail_") {
                    // Check which implementation actually succeeded
                    if result.p_ata.success && !result.spl_ata.success {
                        println!(
                            "  Status: 🚨 CRITICAL SECURITY ISSUE - P-ATA bypassed validation!"
                        )
                    } else if !result.p_ata.success && result.spl_ata.success {
                        println!("  Status: 🚨 CRITICAL SECURITY ISSUE - Original ATA bypassed validation!")
                    } else {
                        println!("  Status: 🚨 CRITICAL SECURITY ISSUE - Validation mismatch!")
                    }
                } else {
                    println!("  Status: Incompatible success/failure (concerning)")
                }
            }
        }

        // Show error details if needed
        if !result.p_ata.success {
            if let Some(ref error) = result.p_ata.error_message {
                println!("  P-ATA Error: {}", error);
            }
        }
        if !result.spl_ata.success {
            if let Some(ref error) = result.spl_ata.error_message {
                println!("  Original Error: {}", error);
            }
        }
    }
}

// ========================== BASE TEST TYPES ============================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum BaseTestType {
    Create,
    CreateIdempotent,
    CreateTopup,
    CreateTopupNoCap,
    CreateToken2022,
    RecoverNested,
    RecoverMultisig,
    WorstCase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TestVariant {
    pub rent_arg: bool,
    pub bump_arg: bool,
    pub len_arg: bool,
}

#[allow(dead_code)]
impl TestVariant {
    pub const BASE: Self = Self {
        rent_arg: false,
        bump_arg: false,
        len_arg: false,
    };
    pub const RENT: Self = Self {
        rent_arg: true,
        bump_arg: false,
        len_arg: false,
    };
    pub const BUMP: Self = Self {
        rent_arg: false,
        bump_arg: true,
        len_arg: false,
    };
    pub const LEN: Self = Self {
        rent_arg: false,
        bump_arg: false,
        len_arg: true,
    };
    pub const RENT_BUMP: Self = Self {
        rent_arg: true,
        bump_arg: true,
        len_arg: false,
    };
    pub const BUMP_LEN: Self = Self {
        rent_arg: true,
        bump_arg: false,
        len_arg: true,
    };
    pub const RENT_BUMP_LEN: Self = Self {
        rent_arg: true,
        bump_arg: true,
        len_arg: true,
    };

    pub fn column_name(&self) -> &'static str {
        match (self.rent_arg, self.bump_arg, self.len_arg) {
            (false, false, false) => "p-ata",
            (true, false, false) => "rent arg",
            (false, true, false) => "bump arg",
            (false, false, true) => "bump+len arg", // len cannot be passed without bump
            (true, true, false) => "rent+bump arg",
            (true, false, true) => "rent+bump+len arg", // len cannot be passed without bump
            (true, true, true) => "all optimizations",
            _ => "unknown",
        }
    }

    pub fn test_suffix(&self) -> String {
        let mut parts = Vec::new();
        if self.rent_arg {
            parts.push("rent");
        }
        if self.bump_arg || self.len_arg {
            parts.push("bump");
        }
        if self.len_arg {
            parts.push("len");
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!("_{}", parts.join("_"))
        }
    }
}

impl BaseTestType {
    #[allow(dead_code)]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::CreateIdempotent => "create_idempotent",
            Self::CreateTopup => "create_topup",
            Self::CreateTopupNoCap => "create_topup_no_cap",
            Self::CreateToken2022 => "create_token2022",
            Self::RecoverNested => "recover_nested",
            Self::RecoverMultisig => "recover_multisig",
            Self::WorstCase => "worst_case",
        }
    }

    /// Returns which P-ATA variant this test should use
    #[allow(dead_code)]
    pub fn required_pata_variant(&self) -> AtaVariant {
        match self {
            Self::CreateTopup => AtaVariant::PAtaPrefunded, // Uses create-account-prefunded feature
            Self::CreateTopupNoCap => AtaVariant::PAtaLegacy, // Uses standard P-ATA without the feature
            _ => AtaVariant::PAtaLegacy,                      // All other tests use standard P-ATA
        }
    }
}
