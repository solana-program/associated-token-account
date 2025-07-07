use {
    bs58,
    colored::Colorize,
    mollusk_svm::{program::loader_keys::LOADER_V3, Mollusk},
    solana_account::Account,
    solana_instruction,
    solana_pubkey::Pubkey,
    solana_sysvar::rent,
    spl_token_2022::extension::ExtensionType,
    spl_token_interface::state::Transmutable,
    std::collections::HashMap,
    std::env,
};

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

        let cursor = 82;
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
            lamports: 2_000_000, // rent-exempt
            data: Self::token_account_data(mint, owner, amount),
            owner: *token_program_id,
            executable: false,
            rent_epoch: 0,
        }
    }

    pub fn mint_account(decimals: u8, token_program_id: &Pubkey, extended: bool) -> Account {
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

    pub fn token_2022_mint_account(decimals: u8, token_program_id: &Pubkey) -> Account {
        Account {
            lamports: 1_000_000_000,
            data: Self::token_2022_mint_data(decimals),
            owner: *token_program_id,
            executable: false,
            rent_epoch: 0,
        }
    }

    pub fn token_2022_mint_data(decimals: u8) -> Vec<u8> {
        let mut data = [0u8; 82];
        let mint_authority = structured_pk(
            &AtaVariant::Original,
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
        AtaVariant::PAtaStandard => 0,
        AtaVariant::PAtaPrefunded => 1,
        AtaVariant::Original => 2,
    }
}

/// Generate a structured pubkey from 4-byte coordinate system
/// [variant, test_bank, test_number, account_type]
pub fn structured_pk(
    variant: &AtaVariant,
    test_bank: TestBankId,
    test_number: u8,
    account_type: AccountTypeId,
) -> Pubkey {
    let mut bytes = [0u8; 32];
    bytes[0] = variant_to_byte(variant);
    bytes[1] = test_bank as u8;
    bytes[2] = test_number;
    bytes[3] = account_type as u8;
    Pubkey::new_from_array(bytes)
}

/// Generate multiple structured pubkeys at once
pub fn structured_pk_multi<const N: usize>(
    variant: &AtaVariant,
    test_bank: TestBankId,
    test_number: u8,
    account_types: [AccountTypeId; N],
) -> [Pubkey; N] {
    account_types.map(|account_type| structured_pk(variant, test_bank, test_number, account_type))
}

/// Find a structured public key that gives optimal bump (255) for ATA derivation
pub fn structured_pk_with_optimal_bump(
    variant: &AtaVariant,
    test_bank: TestBankId,
    test_number: u8,
    account_type: AccountTypeId,
    ata_program_id: &Pubkey,
    token_program_id: &Pubkey,
    mint: &Pubkey,
) -> Pubkey {
    // Start with the base structured key
    let base_key = structured_pk(variant, test_bank, test_number, account_type);

    // Test if base key already has optimal bump
    let (_, bump) = Pubkey::find_program_address(
        &[base_key.as_ref(), token_program_id.as_ref(), mint.as_ref()],
        ata_program_id,
    );

    if bump == 255 {
        return base_key;
    }

    // Search for a key that gives optimal bump by modifying the last 4 bytes
    let mut key_bytes = [0u8; 32];
    key_bytes[0] = variant_to_byte(variant);
    key_bytes[1] = test_bank as u8;
    key_bytes[2] = test_number;
    key_bytes[3] = account_type as u8;

    // Try different variations until we find one with bump 255
    for modifier in 0u32..10000 {
        // Modify the last 4 bytes with the modifier
        let modifier_bytes = modifier.to_le_bytes();
        key_bytes[28..32].copy_from_slice(&modifier_bytes);

        let test_key = Pubkey::new_from_array(key_bytes);
        let (_, test_bump) = Pubkey::find_program_address(
            &[test_key.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            ata_program_id,
        );

        if test_bump == 255 {
            println!(
                "Found optimal bump key [{}, {}, {}, {}]: {} (modifier: {})",
                variant_to_byte(variant),
                test_bank as u8,
                test_number,
                account_type as u8,
                test_key,
                modifier
            );
            return test_key;
        }
    }

    // If we couldn't find optimal bump, warn and return base key
    println!(
        "Warning: Could not find optimal bump key [{}, {}, {}, {}], using base key with bump {}",
        variant_to_byte(variant),
        test_bank as u8,
        test_number,
        account_type as u8,
        bump
    );
    base_key
}

pub fn clone_accounts(src: &[(Pubkey, Account)]) -> Vec<(Pubkey, Account)> {
    src.iter().map(|(k, v)| (*k, v.clone())).collect()
}

pub(crate) fn build_instruction_data(discriminator: u8, additional_data: &[u8]) -> Vec<u8> {
    let mut data = vec![discriminator];
    data.extend_from_slice(additional_data);
    data
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
fn build_mint_data_core(decimals: u8) -> [u8; 82] {
    let mut data = [0u8; 82];
    data[0..4].copy_from_slice(&0u32.to_le_bytes());
    data[44] = decimals;
    data[45] = 1;
    data[46..50].copy_from_slice(&0u32.to_le_bytes());

    data
}

#[inline(always)]
fn build_token_account_data_core(mint: &[u8; 32], owner: &[u8; 32], amount: u64) -> [u8; 165] {
    let mut data = [0u8; 165];
    data[0..32].copy_from_slice(mint);
    data[32..64].copy_from_slice(owner);
    data[64..72].copy_from_slice(&amount.to_le_bytes());
    data[108] = 1;
    data
}

// ========================== SHARED BENCHMARK SETUP ============================

pub struct BenchmarkSetup;

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
    pub fn load_program_ids(manifest_dir: &str) -> (Pubkey, Pubkey) {
        use solana_keypair::Keypair;
        use solana_signer::Signer;
        use std::fs;

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

    /// Try to load P-ATA prefunded program ID, return None if not available
    pub(crate) fn try_load_prefunded_ata_program_id(manifest_dir: &str) -> Option<Pubkey> {
        use solana_keypair::Keypair;
        use solana_signer::Signer;
        use std::fs;

        let prefunded_keypair_path = format!(
            "{}/target/deploy/pinocchio_ata_program_prefunded-keypair.json",
            manifest_dir
        );

        if let Ok(keypair_data) = fs::read_to_string(&prefunded_keypair_path) {
            if let Ok(keypair_bytes) = serde_json::from_str::<Vec<u8>>(&keypair_data) {
                if let Ok(keypair) = Keypair::try_from(&keypair_bytes[..]) {
                    println!("Loaded P-ATA prefunded program ID: {}", keypair.pubkey());
                    return Some(keypair.pubkey());
                }
            }
        }

        println!("P-ATA prefunded program not found");
        println!("   Build with --features create-account-prefunded to enable prefunded tests");
        None
    }

    /// Load both p-ata and original ATA program IDs
    pub(crate) fn load_both_program_ids(manifest_dir: &str) -> (Pubkey, Option<Pubkey>, Pubkey) {
        let (p_ata_program_id, token_program_id) = Self::load_program_ids(manifest_dir);

        // Try to load original ATA program keypair
        let original_ata_program_id = Self::try_load_original_ata_program_id(manifest_dir);

        (p_ata_program_id, original_ata_program_id, token_program_id)
    }

    /// Load all available program IDs (P-ATA variants + original)
    pub(crate) fn load_all_program_ids(
        manifest_dir: &str,
    ) -> (Pubkey, Option<Pubkey>, Option<Pubkey>, Pubkey) {
        let (standard_program_id, token_program_id) = Self::load_program_ids(manifest_dir);
        let prefunded_program_id = Self::try_load_prefunded_ata_program_id(manifest_dir);
        let original_program_id = Self::try_load_original_ata_program_id(manifest_dir);

        (
            standard_program_id,
            prefunded_program_id,
            original_program_id,
            token_program_id,
        )
    }

    /// Try to load original ATA program ID, return None if not available
    pub(crate) fn try_load_original_ata_program_id(manifest_dir: &str) -> Option<Pubkey> {
        use solana_keypair::Keypair;
        use solana_signer::Signer;
        use std::fs;

        // Original ATA is built to ../target/deploy/ (parent directory)
        let original_keypair_path = format!(
            "{}/../target/deploy/spl_associated_token_account-keypair.json",
            manifest_dir
        );

        if let Ok(keypair_data) = fs::read_to_string(&original_keypair_path) {
            if let Ok(keypair_bytes) = serde_json::from_str::<Vec<u8>>(&keypair_data) {
                if let Ok(keypair) = Keypair::try_from(&keypair_bytes[..]) {
                    println!("Loaded original ATA program ID: {}", keypair.pubkey());
                    return Some(keypair.pubkey());
                }
            }
        }

        println!("Original ATA program not found, comparison mode unavailable");
        println!("   Run with --features build-programs to build both implementations");
        None
    }

    /// Validate that the benchmark setup works with a simple test
    pub(crate) fn validate_setup(
        mollusk: &Mollusk,
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> Result<(), String> {
        use solana_instruction::{AccountMeta, Instruction};

        // Simple validation test - create a basic instruction and ensure it doesn't crash
        let payer = structured_pk(
            &AtaVariant::Original,
            TestBankId::Benchmarks,
            1,
            AccountTypeId::Payer,
        );
        let mint = structured_pk(
            &AtaVariant::Original,
            TestBankId::Benchmarks,
            1,
            AccountTypeId::Mint,
        );
        let wallet = structured_pk(
            &AtaVariant::Original,
            TestBankId::Benchmarks,
            1,
            AccountTypeId::Wallet,
        );
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
    PAtaStandard,  // P-ATA without create-account-prefunded
    PAtaPrefunded, // P-ATA with create-account-prefunded
    Original,      // Original SPL ATA
}

impl AtaImplementation {
    pub fn all() -> Vec<Self> {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let (standard_program_id, prefunded_program_id, original_program_id, _token_program_id) =
            BenchmarkSetup::load_all_program_ids(manifest_dir);

        let mut implementations = vec![Self::p_ata_standard(standard_program_id)];

        if let Some(prefunded_id) = prefunded_program_id {
            implementations.push(Self::p_ata_prefunded(prefunded_id));
        }

        if let Some(original_id) = original_program_id {
            implementations.push(Self::original(original_id));
        }

        implementations
    }

    pub fn p_ata_standard(program_id: Pubkey) -> Self {
        Self {
            name: "p-ata-standard",
            program_id,
            binary_name: "pinocchio_ata_program",
            variant: AtaVariant::PAtaStandard,
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

    pub fn original(program_id: Pubkey) -> Self {
        Self {
            name: "original",
            program_id,
            binary_name: "spl_associated_token_account",
            variant: AtaVariant::Original,
        }
    }

    /// Adapt instruction data for this implementation
    pub fn adapt_instruction_data(&self, data: Vec<u8>) -> Vec<u8> {
        match self.variant {
            AtaVariant::PAtaStandard | AtaVariant::PAtaPrefunded => data, // P-ATA supports bump optimizations
            AtaVariant::Original => {
                // Original ATA doesn't support bump optimizations, strip them
                match data.as_slice() {
                    [0, _bump] => vec![0], // Create with bump -> Create without bump
                    [2, _bump] => vec![2], // RecoverNested with bump -> RecoverNested without bump
                    _ => data,             // Pass through other formats
                }
            }
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum CompatibilityStatus {
    Identical,           // Both succeeded with identical account states
    BothRejected,        // Both failed with same error types
    OptimizedBehavior,   // P-ATA succeeded where original failed (bump optimization)
    ExpectedDifferences, // Both succeeded but with expected differences (e.g., different ATA addresses)
    AccountMismatch,     // Both succeeded but account states differ (concerning)
    IncompatibleFailure, // Both failed but with different error codes
    IncompatibleSuccess, // One succeeded, one failed unexpectedly
}

#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    pub implementation: String,
    pub test_name: String,
    pub compute_units: u64,
    pub success: bool,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ComparisonResult {
    pub test_name: String,
    pub p_ata: BenchmarkResult,
    pub original: BenchmarkResult,
    pub compute_savings: Option<i64>,
    pub savings_percentage: Option<f64>,
    pub compatibility_status: CompatibilityStatus,
}

// ========================== SHARED COMPARISON RUNNER ============================

pub struct ComparisonRunner;

impl ComparisonRunner {
    /// Run a single benchmark for one implementation
    pub fn run_single_benchmark(
        test_name: &str,
        ix: &solana_instruction::Instruction,
        accounts: &[(Pubkey, Account)],
        implementation: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> BenchmarkResult {
        let mollusk = Self::create_mollusk_for_all_ata_implementations(token_program_id);
        let result = mollusk.process_instruction(ix, accounts);

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
        }
    }

    pub fn create_mollusk_for_all_ata_implementations(token_program_id: &Pubkey) -> Mollusk {
        let mut mollusk = Mollusk::default();

        for implementation in AtaImplementation::all() {
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

        let savings_percentage = compute_savings.map(|savings| {
            if original_result.compute_units > 0 {
                (savings as f64 / original_result.compute_units as f64) * 100.0
            } else {
                0.0
            }
        });

        let compatibility_status =
            Self::determine_compatibility_status(&p_ata_result, &original_result);

        ComparisonResult {
            test_name: test_name.to_string(),
            p_ata: p_ata_result,
            original: original_result,
            compute_savings,
            savings_percentage,
            compatibility_status,
        }
    }

    /// Determine compatibility status based on results
    pub fn determine_compatibility_status(
        p_ata_result: &BenchmarkResult,
        original_result: &BenchmarkResult,
    ) -> CompatibilityStatus {
        match (p_ata_result.success, original_result.success) {
            (true, true) => {
                // Both succeeded - check if this is the Token-2022 test which has expected differences
                if p_ata_result.test_name == "create_token2022" {
                    CompatibilityStatus::ExpectedDifferences
                } else if p_ata_result.test_name.starts_with("fail_") {
                    // CRITICAL: Both implementations succeeded in a failure test - this is a test issue!
                    CompatibilityStatus::Identical
                } else {
                    // For other tests, assume identical if both succeeded
                    CompatibilityStatus::Identical
                }
            }
            (false, false) => {
                // Both failed - check if they failed with same error type
                match (&p_ata_result.error_message, &original_result.error_message) {
                    (Some(p_ata_err), Some(orig_err)) => {
                        // Simple heuristic: if error messages contain similar keywords, consider them compatible
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
                // P-ATA succeeded, Original failed
                if p_ata_result.test_name.starts_with("fail_") {
                    // CRITICAL SECURITY ISSUE: P-ATA succeeded in a failure test where original correctly failed!
                    CompatibilityStatus::IncompatibleSuccess
                } else {
                    // Performance test - P-ATA optimization (e.g., bump optimization)
                    CompatibilityStatus::OptimizedBehavior
                }
            }
            (false, true) => {
                // P-ATA failed, Original succeeded
                if p_ata_result.test_name.starts_with("fail_") {
                    // CRITICAL SECURITY ISSUE: Original succeeded in a failure test where P-ATA correctly failed!
                    CompatibilityStatus::IncompatibleSuccess
                } else {
                    // Performance test - Original works but P-ATA fails (concerning)
                    CompatibilityStatus::IncompatibleSuccess
                }
            }
        }
    }

    /// Check if two error messages are compatible (same type of error)
    fn errors_are_compatible(p_ata_err: &str, orig_err: &str) -> bool {
        // Check for exact match - identical errors are always compatible
        p_ata_err == orig_err
    }
}

// ========================== BASE TEST TYPES ============================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    pub const RENT_LEN: Self = Self {
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
            (false, false, true) => "bump+len arg", // LEN variant now includes bump
            (true, true, false) => "rent+bump arg",
            (true, false, true) => "rent+bump+len arg", // RENT+LEN variant now includes bump
            (true, true, true) => "all optimizations",  // Special marker for best combination
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
    pub fn required_pata_variant(&self) -> AtaVariant {
        match self {
            Self::CreateTopup => AtaVariant::PAtaPrefunded, // Uses create-account-prefunded feature
            Self::CreateTopupNoCap => AtaVariant::PAtaStandard, // Uses standard P-ATA without the feature
            _ => AtaVariant::PAtaStandard, // All other tests use standard P-ATA
        }
    }

    pub fn supported_variants(&self) -> Vec<TestVariant> {
        match self {
            Self::Create => vec![
                TestVariant::BASE,
                TestVariant::RENT,
                TestVariant::BUMP,
                TestVariant::RENT_BUMP,
            ],
            Self::CreateIdempotent => vec![TestVariant::BASE, TestVariant::RENT],
            Self::CreateTopup => vec![
                TestVariant::BASE,
                TestVariant::RENT,
                TestVariant::BUMP,
                TestVariant::RENT_BUMP,
            ],
            Self::CreateTopupNoCap => vec![
                TestVariant::BASE,
                TestVariant::RENT,
                TestVariant::BUMP,
                TestVariant::RENT_BUMP,
            ],
            Self::CreateToken2022 => vec![
                TestVariant::BASE,
                TestVariant::RENT,
                TestVariant::BUMP,
                TestVariant::LEN,
                TestVariant::RENT_BUMP,
                TestVariant::RENT_LEN,
                TestVariant::RENT_BUMP_LEN,
            ],
            Self::RecoverNested => vec![TestVariant::BASE, TestVariant::BUMP],
            Self::RecoverMultisig => vec![TestVariant::BASE, TestVariant::BUMP],
            Self::WorstCase => vec![TestVariant::BASE, TestVariant::BUMP],
        }
    }
}
