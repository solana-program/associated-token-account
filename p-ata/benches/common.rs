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
    /// Build a zero-rent `Rent` sysvar account with correctly sized data buffer
    pub fn rent_sysvar() -> Account {
        Account {
            lamports: 1,
            data: vec![1u8; 17], // Minimal rent sysvar data
            owner: rent::id(),
            executable: false,
            rent_epoch: 0,
        }
    }

    /// Build raw token Account data with the supplied mint / owner / amount
    pub fn token_account_data(mint: &Pubkey, owner: &Pubkey, amount: u64) -> Vec<u8> {
        build_token_account_data_core(
            mint.as_ref().try_into().expect("Pubkey is 32 bytes"),
            owner.as_ref().try_into().expect("Pubkey is 32 bytes"),
            amount,
        )
        .to_vec()
    }

    /// Build mint data with given decimals and marked initialized
    pub fn mint_data(decimals: u8) -> Vec<u8> {
        build_mint_data_core(decimals).to_vec()
    }

    /// Build extended mint data with ImmutableOwner extension
    pub fn extended_mint_data(decimals: u8) -> Vec<u8> {
        let required_len =
            ExtensionType::try_calculate_account_len::<spl_token_2022::state::Mint>(&[
                ExtensionType::ImmutableOwner,
            ])
            .expect("calc len");

        let mut data = Self::mint_data(decimals);
        data.resize(required_len, 0u8);

        // Add TLV entries at correct offset (base len = 82)
        let cursor = 82;
        let immutable_owner_header = [7u8, 0u8, 0u8, 0u8]; // type=7, length=0 (little-endian)
        data[cursor..cursor + 4].copy_from_slice(&immutable_owner_header);

        data
    }

    /// Build Multisig account data with given signer public keys and threshold `m`
    pub fn multisig_data(m: u8, signer_pubkeys: &[Pubkey]) -> Vec<u8> {
        let byte_refs: Vec<&[u8; 32]> = signer_pubkeys
            .iter()
            .map(|pk| pk.as_ref().try_into().expect("Pubkey is 32 bytes"))
            .collect();
        build_multisig_data_core(m, &byte_refs)
    }

    /// Create a basic system account
    pub fn system_account(lamports: u64) -> Account {
        Account::new(lamports, 0, &SYSTEM_PROGRAM_ID)
    }

    /// Create an executable program account
    pub fn executable_program(owner: Pubkey) -> Account {
        Account {
            lamports: 0,
            data: Vec::new(),
            owner,
            executable: true,
            rent_epoch: 0,
        }
    }

    /// Create a token account with specified parameters
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

    /// Create a mint account
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

    /// Create a Token-2022 specific mint account for testing
    pub fn token_2022_mint_account(decimals: u8, token_program_id: &Pubkey) -> Account {
        Account {
            lamports: 1_000_000_000,
            data: Self::token_2022_mint_data(decimals),
            owner: *token_program_id,
            executable: false,
            rent_epoch: 0,
        }
    }

    /// Build Token-2022 specific mint data (PROPERLY INITIALIZED as Token-2022 expects)
    pub fn token_2022_mint_data(decimals: u8) -> Vec<u8> {
        let mut data = [0u8; 82]; // Mint::LEN

        // Token-2022 requires a valid mint authority (not None)
        let mint_authority = const_pk(123); // Valid deterministic authority

        // mint_authority: COption<Pubkey> (36 bytes: 4 tag + 32 pubkey)
        data[0..4].copy_from_slice(&1u32.to_le_bytes()); // COption tag = Some
        data[4..36].copy_from_slice(mint_authority.as_ref()); // Valid authority

        // supply: u64 (8 bytes) - stays as 0

        // decimals: u8 (1 byte)
        data[44] = decimals;

        // is_initialized: bool (1 byte)
        data[45] = 1; // true - Token-2022 expects initialized mint

        // freeze_authority: COption<Pubkey> (36 bytes: 4 tag + 32 pubkey)
        data[46..50].copy_from_slice(&0u32.to_le_bytes()); // COption tag = None

        data.to_vec()
    }
}

// =========================== OPTIMAL KEY FINDERS ==========================

pub struct OptimalKeyFinder;

impl OptimalKeyFinder {
    /// Find a wallet pubkey that yields the maximum bump (255) for its ATA
    pub fn find_optimal_wallet(
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
    pub fn find_optimal_nested_mint(
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

// =============================== UTILITIES =================================

/// Helper to create deterministic pubkeys (32 identical bytes)
pub fn const_pk(byte: u8) -> Pubkey {
    Pubkey::new_from_array([byte; 32])
}

/// Clone accounts vector for benchmark isolation
pub fn clone_accounts(src: &[(Pubkey, Account)]) -> Vec<(Pubkey, Account)> {
    src.iter().map(|(k, v)| (*k, v.clone())).collect()
}

/// Create a fresh Mollusk instance with required programs
pub fn fresh_mollusk(program_id: &Pubkey, token_program_id: &Pubkey) -> Mollusk {
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

pub fn build_instruction_data(discriminator: u8, additional_data: &[u8]) -> Vec<u8> {
    let mut data = vec![discriminator];
    data.extend_from_slice(additional_data);
    data
}

/// Build multisig account data
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
    data[0] = m; // m threshold
    data[1] = signer_pubkeys.len() as u8; // n signers
    data[2] = 1; // is_initialized

    for (i, pk) in signer_pubkeys.iter().enumerate() {
        let offset = 3 + i * 32;
        data[offset..offset + 32].copy_from_slice(*pk);
    }
    data
}

/// Build mint data core structure
#[inline(always)]
fn build_mint_data_core(decimals: u8) -> [u8; 82] {
    let mut data = [0u8; 82]; // Mint::LEN

    // mint_authority: COption<Pubkey> (36 bytes: 4 tag + 32 pubkey)
    data[0..4].copy_from_slice(&0u32.to_le_bytes()); // COption tag = None
                                                     // Leave authority bytes as 0 (unused when tag is None)

    // supply: u64 (8 bytes) - stays as 0

    // decimals: u8 (1 byte)
    data[44] = decimals;

    // is_initialized: bool (1 byte)
    data[45] = 1; // true - regular SPL Token expects initialized mints

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

/// Build CREATE instruction for Token-2022 simulation
/// This creates a PROPERLY INITIALIZED Token-2022 mint
pub fn build_create_token2022_simulation(
    program_id: &Pubkey,
) -> (solana_instruction::Instruction, Vec<(Pubkey, Account)>) {
    let token_2022_program_id: Pubkey =
        pinocchio_pubkey::pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb").into();

    let base_offset = 80; // Unique offset to avoid collisions
    let payer = const_pk(base_offset);
    let mint = const_pk(base_offset + 1);
    let mint_authority = const_pk(123); // Must match the authority in token_2022_mint_data

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

    // Create Token-2022 specific mint account (properly initialized)
    let mint_account = AccountBuilder::token_2022_mint_account(0, &token_2022_program_id);

    let accounts = vec![
        (payer, AccountBuilder::system_account(1_000_000_000)),
        (ata, AccountBuilder::system_account(0)),
        (wallet, AccountBuilder::system_account(0)),
        (mint, mint_account),
        (mint_authority, AccountBuilder::system_account(0)), // Add the mint authority account
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
        solana_instruction::AccountMeta::new(payer, true),
        solana_instruction::AccountMeta::new(ata, false),
        solana_instruction::AccountMeta::new_readonly(wallet, false),
        solana_instruction::AccountMeta::new_readonly(mint, false),
        solana_instruction::AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        solana_instruction::AccountMeta::new_readonly(token_2022_program_id, false),
    ];

    let ix = solana_instruction::Instruction {
        program_id: *program_id,
        accounts: metas,
        data: build_instruction_data(0, &[]), // Create instruction (discriminator 0)
    };

    (ix, accounts)
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

    /// Load both p-ata and original ATA program IDs
    pub fn load_both_program_ids(manifest_dir: &str) -> (Pubkey, Option<Pubkey>, Pubkey) {
        let (p_ata_program_id, token_program_id) = Self::load_program_ids(manifest_dir);

        // Try to load original ATA program keypair
        let original_ata_program_id = Self::try_load_original_ata_program_id(manifest_dir);

        (p_ata_program_id, original_ata_program_id, token_program_id)
    }

    /// Try to load original ATA program ID, return None if not available
    pub fn try_load_original_ata_program_id(manifest_dir: &str) -> Option<Pubkey> {
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
    pub fn validate_setup(
        mollusk: &Mollusk,
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> Result<(), String> {
        use solana_instruction::{AccountMeta, Instruction};

        // Simple validation test - create a basic instruction and ensure it doesn't crash
        let payer = const_pk(1);
        let mint = const_pk(2);
        let wallet = const_pk(3);
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
                    _ => data,             // Pass through other formats
                }
            }
            _ => data,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum CompatibilityStatus {
    Identical,           // Both succeeded with identical account states
    BothRejected,        // Both failed with same error types
    OptimizedBehavior,   // P-ATA succeeded where original failed (bump optimization)
    AccountMismatch,     // Both succeeded but account states differ (concerning)
    IncompatibleFailure, // Both failed but with different error codes
    IncompatibleSuccess, // One succeeded, one failed unexpectedly
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
        let mollusk = Self::create_mollusk_for_implementation(implementation, token_program_id);
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

    /// Create appropriate Mollusk instance for implementation
    pub fn create_mollusk_for_implementation(
        implementation: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> Mollusk {
        let mut mollusk = Mollusk::default();

        // Add the ATA program
        mollusk.add_program(
            &implementation.program_id,
            implementation.binary_name,
            &LOADER_V3,
        );

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
                // Both succeeded - for failure tests this shouldn't happen, but if it does, assume identical
                CompatibilityStatus::Identical
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
            (true, false) => CompatibilityStatus::OptimizedBehavior,
            (false, true) => CompatibilityStatus::IncompatibleSuccess,
        }
    }

    /// Check if two error messages are compatible (same type of error)
    fn errors_are_compatible(p_ata_err: &str, orig_err: &str) -> bool {
        let compatible_error_patterns = [
            ("InvalidSeeds", "InvalidSeeds"),
            ("InvalidAccountData", "InvalidAccountData"),
            ("MissingRequiredSignature", "MissingRequiredSignature"),
            ("NotRentExempt", "NotRentExempt"),
            ("InvalidInstructionData", "InvalidInstructionData"),
            ("IncorrectProgramId", "IncorrectProgramId"),
            ("InvalidOwner", "InvalidOwner"),
            ("Uninitialized", "Uninitialized"),
            ("AlreadyInUse", "AlreadyInUse"),
        ];

        for (pattern1, pattern2) in &compatible_error_patterns {
            if (p_ata_err.contains(pattern1) && orig_err.contains(pattern2))
                || (p_ata_err.contains(pattern2) && orig_err.contains(pattern1))
            {
                return true;
            }
        }

        false
    }

    /// Print individual comparison result
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
            result.original.compute_units,
            if result.original.success {
                "Success"
            } else {
                "Failed"
            }
        );

        // Savings analysis (mainly relevant for successful tests)
        if let (Some(savings), Some(percentage)) =
            (result.compute_savings, result.savings_percentage)
        {
            if savings > 0 {
                println!("  Savings: {:>8} CUs ({:.1}%)", savings, percentage);
            } else if savings < 0 {
                println!("  Overhead: {:>7} CUs ({:.1}%)", -savings, -percentage);
            } else {
                println!("  Equal compute usage");
            }
        }

        // Compatibility status
        match result.compatibility_status {
            CompatibilityStatus::Identical => println!("  Status: Identical (both succeeded)"),
            CompatibilityStatus::BothRejected => {
                println!("  Status: Both rejected (same error type)")
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
                println!("  Status: Incompatible success/failure (concerning)")
            }
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
}

// ======================= VERBOSE COMPARISON SUPPORT =======================

/// Structure to hold verbose execution results
pub struct VerboseResults {
    pub instruction_data: Vec<u8>,
    pub original_accounts: Vec<(Pubkey, Account)>,
    pub final_accounts: Vec<(Pubkey, Account)>,
}

/// Verbose comparison utilities
pub struct VerboseComparison;

impl VerboseComparison {
    /// Check if verbose mode is enabled via environment variable
    pub fn is_enabled() -> bool {
        env::var("P_ATA_VERBOSE").is_ok()
    }

    /// Print detailed byte-by-byte comparison
    pub fn print_detailed_comparison(
        comparison: &ComparisonResult,
        p_ata_verbose: &VerboseResults,
        original_verbose: &VerboseResults,
    ) {
        println!("\n🔍 VERBOSE COMPARISON: {}", comparison.test_name);
        println!("==========================================");

        // Compare instruction data
        Self::compare_instruction_data(
            &p_ata_verbose.instruction_data,
            &original_verbose.instruction_data,
        );

        // Compare changed accounts
        Self::compare_account_changes(
            &p_ata_verbose.original_accounts,
            &p_ata_verbose.final_accounts,
            &original_verbose.original_accounts,
            &original_verbose.final_accounts,
        );
    }

    /// Compare instruction data byte-by-byte
    fn compare_instruction_data(p_ata_data: &[u8], original_data: &[u8]) {
        println!("\n📋 INSTRUCTION DATA COMPARISON:");
        println!(
            "P-ATA instruction data:     {}",
            bs58::encode(p_ata_data).into_string()
        );
        println!(
            "Original instruction data:  {}",
            bs58::encode(original_data).into_string()
        );

        if p_ata_data == original_data {
            println!("{}", "✅ Instruction data IDENTICAL".green());
        } else {
            println!("{}", "❌ Instruction data DIFFERS".red());
            Self::print_byte_comparison(p_ata_data, original_data, "Instruction");
        }
    }

    /// Compare account changes between implementations by instruction position
    fn compare_account_changes(
        p_ata_original: &[(Pubkey, Account)],
        p_ata_final: &[(Pubkey, Account)],
        orig_original: &[(Pubkey, Account)],
        orig_final: &[(Pubkey, Account)],
    ) {
        println!("\n📊 ACCOUNT CHANGES COMPARISON:");

        // Map accounts by position in the original account list
        let p_ata_changes_by_pos =
            Self::find_account_changes_by_position(p_ata_original, p_ata_final);
        let orig_changes_by_pos = Self::find_account_changes_by_position(orig_original, orig_final);

        if p_ata_changes_by_pos.is_empty() && orig_changes_by_pos.is_empty() {
            println!("No account changes detected in either implementation.");
            return;
        }

        // Compare accounts by position (role in instruction)
        let max_pos = p_ata_changes_by_pos
            .keys()
            .max()
            .unwrap_or(&0)
            .max(orig_changes_by_pos.keys().max().unwrap_or(&0));

        for pos in 0..=*max_pos {
            if let (Some(p_ata_change), Some(orig_change)) = (
                p_ata_changes_by_pos.get(&pos),
                orig_changes_by_pos.get(&pos),
            ) {
                println!("\n🔄 Account {} Changed:", Self::get_account_role_name(pos));
                println!("   P-ATA Address:    {}", p_ata_change.0);
                println!("   Original Address: {}", orig_change.0);
                Self::compare_account_data(
                    &p_ata_change.1.data,
                    &orig_change.1.data,
                    &format!("Position {}", pos),
                );
            } else if let Some(p_ata_change) = p_ata_changes_by_pos.get(&pos) {
                println!(
                    "\n⚠️  Account {} changed in P-ATA only: {}",
                    Self::get_account_role_name(pos),
                    p_ata_change.0
                );
                Self::print_account_summary(&p_ata_change.1, "P-ATA");
            } else if let Some(orig_change) = orig_changes_by_pos.get(&pos) {
                println!(
                    "\n⚠️  Account {} changed in Original only: {}",
                    Self::get_account_role_name(pos),
                    orig_change.0
                );
                Self::print_account_summary(&orig_change.1, "Original");
            }
        }
    }

    /// Get human-readable name for account position
    fn get_account_role_name(position: usize) -> &'static str {
        match position {
            0 => "0 (Payer)",
            1 => "1 (ATA)",
            2 => "2 (Wallet)",
            3 => "3 (Mint)",
            4 => "4 (System Program)",
            5 => "5 (Token Program)",
            6 => "6 (Rent Sysvar)",
            _ => "Unknown",
        }
    }

    /// Print account summary
    fn print_account_summary(account: &Account, label: &str) {
        println!(
            "   {} account data: {} bytes, {} lamports, owner: {}",
            label,
            account.data.len(),
            account.lamports,
            account.owner
        );
        if !account.data.is_empty() {
            let preview = if account.data.len() > 50 {
                format!("{}...", bs58::encode(&account.data[..50]).into_string())
            } else {
                bs58::encode(&account.data).into_string()
            };
            println!("   Data preview: {}", preview);
        }
    }

    /// Find accounts that changed between original and final states by position
    fn find_account_changes_by_position(
        original: &[(Pubkey, Account)],
        final_accounts: &[(Pubkey, Account)],
    ) -> HashMap<usize, (Pubkey, Account)> {
        let mut changes = HashMap::new();

        for (pos, (pubkey, final_account)) in final_accounts.iter().enumerate() {
            if let Some((_, original_account)) = original.get(pos) {
                if original_account.data != final_account.data
                    || original_account.lamports != final_account.lamports
                    || original_account.owner != final_account.owner
                {
                    changes.insert(pos, (*pubkey, final_account.clone()));
                }
            }
        }

        changes
    }

    /// Find accounts that changed between original and final states (legacy method)
    fn find_account_changes(
        original: &[(Pubkey, Account)],
        final_accounts: &[(Pubkey, Account)],
    ) -> HashMap<Pubkey, Account> {
        let mut changes = HashMap::new();

        for (pubkey, final_account) in final_accounts {
            if let Some((_, original_account)) = original.iter().find(|(pk, _)| pk == pubkey) {
                if original_account.data != final_account.data
                    || original_account.lamports != final_account.lamports
                    || original_account.owner != final_account.owner
                {
                    changes.insert(*pubkey, final_account.clone());
                }
            }
        }

        changes
    }

    /// Compare account data byte-by-byte
    fn compare_account_data(p_ata_data: &[u8], original_data: &[u8], label: &str) {
        // Handle empty data case
        if p_ata_data.is_empty() && original_data.is_empty() {
            println!("   Both accounts have no data (empty)");
            return;
        }

        let p_ata_preview = if p_ata_data.len() > 100 {
            format!(
                "{}... ({} bytes)",
                bs58::encode(&p_ata_data[..100]).into_string(),
                p_ata_data.len()
            )
        } else if p_ata_data.is_empty() {
            "(empty)".to_string()
        } else {
            format!(
                "{} ({} bytes)",
                bs58::encode(p_ata_data).into_string(),
                p_ata_data.len()
            )
        };

        let orig_preview = if original_data.len() > 100 {
            format!(
                "{}... ({} bytes)",
                bs58::encode(&original_data[..100]).into_string(),
                original_data.len()
            )
        } else if original_data.is_empty() {
            "(empty)".to_string()
        } else {
            format!(
                "{} ({} bytes)",
                bs58::encode(original_data).into_string(),
                original_data.len()
            )
        };

        if p_ata_data == original_data {
            println!("   P-ATA data:     {}", p_ata_preview.green());
            println!("   Original data:  {}", orig_preview.green());
            println!("   {}", "✅ Account data IDENTICAL".green());
        } else {
            println!("   P-ATA data:     {}", p_ata_preview.red());
            println!("   Original data:  {}", orig_preview.red());
            println!("   {}", "❌ Account data DIFFERS".red());
            if !p_ata_data.is_empty() || !original_data.is_empty() {
                Self::print_byte_comparison(p_ata_data, original_data, label);
            }
        }
    }

    /// Print byte-by-byte comparison with colored output
    fn print_byte_comparison(data1: &[u8], data2: &[u8], label: &str) {
        println!("\n🔍 Byte-by-byte comparison for {}:", label);

        let b58_1 = bs58::encode(data1).into_string();
        let b58_2 = bs58::encode(data2).into_string();

        let max_len = b58_1.len().max(b58_2.len());
        let chars1: Vec<char> = b58_1.chars().collect();
        let chars2: Vec<char> = b58_2.chars().collect();

        println!(
            "P-ATA:    {}",
            Self::colorize_comparison(&chars1, &chars2, true)
        );
        println!(
            "Original: {}",
            Self::colorize_comparison(&chars2, &chars1, false)
        );

        // Print summary
        let differences = Self::count_differences(&chars1, &chars2);
        if differences > 0 {
            println!(
                "{} differences found in {} characters",
                differences.to_string().red(),
                max_len
            );
        }
    }

    /// Colorize base58 string comparison
    fn colorize_comparison(chars1: &[char], chars2: &[char], _is_first: bool) -> String {
        let mut result = String::new();

        for (i, &ch1) in chars1.iter().enumerate() {
            if let Some(&ch2) = chars2.get(i) {
                if ch1 == ch2 {
                    result.push_str(&ch1.to_string().green().to_string());
                } else {
                    result.push_str(&ch1.to_string().red().to_string());
                }
            } else {
                // This character exists in one string but not the other
                result.push_str(&ch1.to_string().red().to_string());
            }
        }

        result
    }

    /// Count differences between two character arrays
    fn count_differences(chars1: &[char], chars2: &[char]) -> usize {
        let mut differences = 0;
        let max_len = chars1.len().max(chars2.len());

        for i in 0..max_len {
            let ch1 = chars1.get(i);
            let ch2 = chars2.get(i);

            if ch1 != ch2 {
                differences += 1;
            }
        }

        differences
    }
}

impl ComparisonRunner {
    /// Enhanced comparison run with verbose output support
    pub fn run_single_benchmark_verbose(
        test_name: &str,
        ix: &solana_instruction::Instruction,
        accounts: &[(Pubkey, Account)],
        implementation: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (BenchmarkResult, VerboseResults) {
        let mut cloned_accounts = clone_accounts(accounts);
        let mollusk = Self::create_mollusk_for_implementation(implementation, token_program_id);

        // Store the original accounts before execution
        let original_accounts = cloned_accounts.clone();

        let result = mollusk.process_instruction(ix, &cloned_accounts);

        let benchmark_result = BenchmarkResult {
            implementation: implementation.name.to_string(),
            test_name: test_name.to_string(),
            compute_units: result.compute_units_consumed,
            success: matches!(
                result.program_result,
                mollusk_svm::result::ProgramResult::Success
            ),
            error_message: if matches!(
                result.program_result,
                mollusk_svm::result::ProgramResult::Success
            ) {
                None
            } else {
                Some(format!("{:?}", result.program_result))
            },
        };

        // Get the final account states - Mollusk modifies the accounts in place
        // so the resulting_accounts should contain the final states
        let final_accounts = if result.resulting_accounts.is_empty() {
            // Fallback: if resulting_accounts is empty, use the modified cloned_accounts
            cloned_accounts
        } else {
            result.resulting_accounts
        };

        let verbose_results = VerboseResults {
            instruction_data: ix.data.clone(),
            original_accounts,
            final_accounts,
        };

        (benchmark_result, verbose_results)
    }

    /// Run comprehensive comparison with verbose output
    pub fn run_verbose_comparison(
        test_name: &str,
        p_ata_ix: &solana_instruction::Instruction,
        p_ata_accounts: &[(Pubkey, Account)],
        original_ix: &solana_instruction::Instruction,
        original_accounts: &[(Pubkey, Account)],
        p_ata_impl: &AtaImplementation,
        original_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> ComparisonResult {
        let (p_ata_result, p_ata_verbose) = Self::run_single_benchmark_verbose(
            test_name,
            p_ata_ix,
            p_ata_accounts,
            p_ata_impl,
            token_program_id,
        );
        let (original_result, original_verbose) = Self::run_single_benchmark_verbose(
            test_name,
            original_ix,
            original_accounts,
            original_impl,
            token_program_id,
        );

        let comparison_result =
            Self::create_comparison_result(test_name, p_ata_result, original_result);

        // Print verbose output if enabled
        if VerboseComparison::is_enabled() {
            VerboseComparison::print_detailed_comparison(
                &comparison_result,
                &p_ata_verbose,
                &original_verbose,
            );
        }

        comparison_result
    }
}
