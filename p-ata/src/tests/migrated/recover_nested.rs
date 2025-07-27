//! Migrated test for recover_nested functionality using mollusk and pinocchio

use {
    crate::tests::test_utils::{
        create_mollusk_mint_data, create_mollusk_token_account_data, setup_mollusk_with_programs,
        NATIVE_LOADER_ID,
    },
    mollusk_svm::{program::loader_keys::LOADER_V3, result::Check, Mollusk},
    solana_instruction::{AccountMeta, Instruction},
    solana_program::program_error::ProgramError,
    solana_pubkey::Pubkey,
    solana_sdk::{account::Account, signature::Keypair, signer::Signer, system_program, sysvar},
    spl_associated_token_account_client::address::get_associated_token_address_with_program_id,
    std::vec::Vec,
};

/// Creates mint account data with specified decimals
fn create_mint_data(decimals: u8) -> Vec<u8> {
    const MINT_ACCOUNT_SIZE: usize = 82;
    let mut data = [0u8; MINT_ACCOUNT_SIZE];
    data[0..4].copy_from_slice(&1u32.to_le_bytes()); // state = 1 (Initialized)
    data[44] = decimals;
    data[45] = 1; // is_initialized = 1
    data.to_vec()
}

/// Helper to create token account data with specified properties
fn create_token_account_data(mint: &Pubkey, owner: &Pubkey, amount: u64) -> Vec<u8> {
    const TOKEN_ACCOUNT_SIZE: usize = 165; // SPL Token account size (no extensions)
    let mut data = [0u8; TOKEN_ACCOUNT_SIZE];

    // mint
    data[0..32].copy_from_slice(mint.as_ref());
    // owner
    data[32..64].copy_from_slice(owner.as_ref());
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

    data.to_vec()
}

/// Create base accounts needed for all tests
fn create_base_accounts(
    payer: &Keypair,
    wallet: &Pubkey,
    token_program: &Pubkey,
) -> Vec<(Pubkey, Account)> {
    [
        (
            payer.pubkey(),
            Account::new(1_000_000_000, 0, &system_program::id()),
        ),
        (*wallet, Account::new(0, 0, &system_program::id())),
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
            *token_program,
            Account {
                lamports: 0,
                data: Vec::new(),
                owner: LOADER_V3,
                executable: true,
                rent_epoch: 0,
            },
        ),
        (sysvar::rent::id(), Account::new(1009200, 17, &sysvar::id())),
    ]
    .to_vec()
}

/// Common test variables setup
struct TestContext {
    ata_program_id: Pubkey,
    token_program_id: Pubkey,
    wallet: Keypair,
    mint: Pubkey,
    payer: Keypair,
}

impl TestContext {
    fn new(token_program_id: Pubkey) -> Self {
        Self {
            ata_program_id: spl_associated_token_account::id(),
            token_program_id,
            wallet: Keypair::new(),
            mint: Pubkey::new_unique(),
            payer: Keypair::new(),
        }
    }

    fn new_with_different_mints(token_program_id: Pubkey) -> (Self, Pubkey) {
        let mut ctx = Self::new(token_program_id);
        let nested_mint = Pubkey::new_unique();
        (ctx, nested_mint)
    }
}

/// Build recover nested instruction
fn build_recover_nested_instruction(
    ata_program_id: Pubkey,
    wallet: Pubkey,
    owner_mint: Pubkey,
    nested_mint: Pubkey,
    token_program: Pubkey,
) -> Instruction {
    let owner_ata =
        get_associated_token_address_with_program_id(&wallet, &owner_mint, &token_program);
    let destination_ata =
        get_associated_token_address_with_program_id(&wallet, &nested_mint, &token_program);
    let nested_ata =
        get_associated_token_address_with_program_id(&owner_ata, &nested_mint, &token_program);

    let accounts = [
        AccountMeta::new(nested_ata, false),
        AccountMeta::new_readonly(nested_mint, false),
        AccountMeta::new(destination_ata, false),
        AccountMeta::new_readonly(owner_ata, false),
        AccountMeta::new_readonly(owner_mint, false),
        AccountMeta::new(wallet, true),
        AccountMeta::new_readonly(token_program, false),
    ];

    Instruction {
        program_id: ata_program_id,
        accounts: accounts.to_vec(),
        data: [2u8].to_vec(), // discriminator 2 (RecoverNested)
    }
}

/// Build recover nested instruction with modified accounts (for error testing)
fn build_recover_nested_instruction_modified<F>(
    ata_program_id: Pubkey,
    wallet: Pubkey,
    owner_mint: Pubkey,
    nested_mint: Pubkey,
    token_program: Pubkey,
    modification: F,
) -> Instruction
where
    F: FnOnce(&mut Vec<AccountMeta>),
{
    let owner_ata =
        get_associated_token_address_with_program_id(&wallet, &owner_mint, &token_program);
    let destination_ata =
        get_associated_token_address_with_program_id(&wallet, &nested_mint, &token_program);
    let nested_ata =
        get_associated_token_address_with_program_id(&owner_ata, &nested_mint, &token_program);

    let mut accounts = [
        AccountMeta::new(nested_ata, false),
        AccountMeta::new_readonly(nested_mint, false),
        AccountMeta::new(destination_ata, false),
        AccountMeta::new_readonly(owner_ata, false),
        AccountMeta::new_readonly(owner_mint, false),
        AccountMeta::new(wallet, true),
        AccountMeta::new_readonly(token_program, false),
    ]
    .to_vec();

    modification(&mut accounts);

    Instruction {
        program_id: ata_program_id,
        accounts,
        data: [2u8].to_vec(),
    }
}

/// Setup complete test scenario with real token program accounts
fn setup_recover_test_scenario(
    mollusk: &Mollusk,
    ata_program_id: &Pubkey,
    token_program_id: &Pubkey,
    payer: &Keypair,
    wallet: &Pubkey,
    owner_mint: &Pubkey,
    nested_mint: &Pubkey,
    create_destination: bool,
    amount: u64,
) -> Vec<(Pubkey, Account)> {
    let mut accounts = create_base_accounts(payer, wallet, token_program_id);

    // Add the ATA program
    accounts.push((
        *ata_program_id,
        Account {
            lamports: 0,
            data: Vec::new(),
            owner: LOADER_V3,
            executable: true,
            rent_epoch: 0,
        },
    ));

    // Add owner mint
    accounts.push((
        *owner_mint,
        Account {
            lamports: 1_461_600,
            data: create_mint_data(0),
            owner: *token_program_id,
            executable: false,
            rent_epoch: 0,
        },
    ));

    // Add nested mint if different
    if owner_mint != nested_mint {
        accounts.push((
            *nested_mint,
            Account {
                lamports: 1_461_600,
                data: create_mint_data(0),
                owner: *token_program_id,
                executable: false,
                rent_epoch: 0,
            },
        ));
    }

    // Create owner ATA with real token account structure
    let owner_ata =
        get_associated_token_address_with_program_id(wallet, owner_mint, token_program_id);
    accounts.push((
        owner_ata,
        Account {
            lamports: 2_039_280,
            data: create_token_account_data(owner_mint, wallet, 0),
            owner: *token_program_id,
            executable: false,
            rent_epoch: 0,
        },
    ));

    // Create nested ATA with real token account structure
    let nested_ata =
        get_associated_token_address_with_program_id(&owner_ata, nested_mint, token_program_id);
    accounts.push((
        nested_ata,
        Account {
            lamports: 2_039_280,
            data: create_token_account_data(nested_mint, &owner_ata, amount),
            owner: *token_program_id,
            executable: false,
            rent_epoch: 0,
        },
    ));

    // Create destination ATA if needed
    if create_destination {
        let destination_ata =
            get_associated_token_address_with_program_id(wallet, nested_mint, token_program_id);
        accounts.push((
            destination_ata,
            Account {
                lamports: 2_039_280,
                data: create_token_account_data(nested_mint, wallet, 0),
                owner: *token_program_id,
                executable: false,
                rent_epoch: 0,
            },
        ));
    }

    accounts
}

/// Helper function to run success test for same mint scenario
fn run_success_same_mint_test(token_program_id: Pubkey) {
    let ctx = TestContext::new(token_program_id);
    let mollusk = setup_mollusk_with_programs(&ctx.token_program_id);

    let instruction = build_recover_nested_instruction(
        ctx.ata_program_id,
        ctx.wallet.pubkey(),
        ctx.mint,
        ctx.mint, // same mint
        ctx.token_program_id,
    );

    let accounts = setup_recover_test_scenario(
        &mollusk,
        &ctx.ata_program_id,
        &ctx.token_program_id,
        &ctx.payer,
        &ctx.wallet.pubkey(),
        &ctx.mint,
        &ctx.mint,
        true, // create destination
        100,  // amount
    );

    mollusk.process_and_validate_instruction(&instruction, &accounts, &[Check::success()]);
}

/// Helper function to run success test for different mints scenario
fn run_success_different_mints_test(token_program_id: Pubkey) {
    let (ctx, nested_mint) = TestContext::new_with_different_mints(token_program_id);
    let mollusk = setup_mollusk_with_programs(&ctx.token_program_id);

    let instruction = build_recover_nested_instruction(
        ctx.ata_program_id,
        ctx.wallet.pubkey(),
        ctx.mint,
        nested_mint,
        ctx.token_program_id,
    );

    let accounts = setup_recover_test_scenario(
        &mollusk,
        &ctx.ata_program_id,
        &ctx.token_program_id,
        &ctx.payer,
        &ctx.wallet.pubkey(),
        &ctx.mint,
        &nested_mint,
        true, // create destination
        100,  // amount
    );

    mollusk.process_and_validate_instruction(&instruction, &accounts, &[Check::success()]);
}

/// Helper function to run missing wallet signature test
fn run_missing_wallet_signature_test(token_program_id: Pubkey) {
    let ctx = TestContext::new(token_program_id);
    let mollusk = setup_mollusk_with_programs(&ctx.token_program_id);

    let instruction = build_recover_nested_instruction_modified(
        ctx.ata_program_id,
        ctx.wallet.pubkey(),
        ctx.mint,
        ctx.mint,
        ctx.token_program_id,
        |accounts| {
            // Make wallet account not a signer
            accounts[5] = AccountMeta::new(ctx.wallet.pubkey(), false);
        },
    );

    let accounts = setup_recover_test_scenario(
        &mollusk,
        &ctx.ata_program_id,
        &ctx.token_program_id,
        &ctx.payer,
        &ctx.wallet.pubkey(),
        &ctx.mint,
        &ctx.mint,
        true,
        100,
    );

    mollusk.process_and_validate_instruction(
        &instruction,
        &accounts,
        &[Check::err(ProgramError::MissingRequiredSignature)],
    );
}

/// Helper function to run wrong signer test
fn run_wrong_signer_test(token_program_id: Pubkey) {
    let ctx = TestContext::new(token_program_id);
    let wrong_wallet = Keypair::new();
    let mollusk = setup_mollusk_with_programs(&ctx.token_program_id);

    let instruction = build_recover_nested_instruction(
        ctx.ata_program_id,
        wrong_wallet.pubkey(), // wrong signer
        ctx.mint,
        ctx.mint,
        ctx.token_program_id,
    );

    // Setup accounts for the CORRECT wallet only
    let mut accounts = setup_recover_test_scenario(
        &mollusk,
        &ctx.ata_program_id,
        &ctx.token_program_id,
        &ctx.payer,
        &ctx.wallet.pubkey(), // accounts exist for correct wallet
        &ctx.mint,
        &ctx.mint,
        true,
        100,
    );

    // Add the missing accounts that the instruction will try to access (for wrong_wallet) as uninitialized
    let wrong_owner_ata = get_associated_token_address_with_program_id(
        &wrong_wallet.pubkey(),
        &ctx.mint,
        &ctx.token_program_id,
    );
    let wrong_destination_ata = get_associated_token_address_with_program_id(
        &wrong_wallet.pubkey(),
        &ctx.mint,
        &ctx.token_program_id,
    );
    let wrong_nested_ata = get_associated_token_address_with_program_id(
        &wrong_owner_ata,
        &ctx.mint,
        &ctx.token_program_id,
    );

    accounts.extend([
        (wrong_owner_ata, Account::new(0, 0, &system_program::id())),
        (
            wrong_destination_ata,
            Account::new(0, 0, &system_program::id()),
        ),
        (wrong_nested_ata, Account::new(0, 0, &system_program::id())),
        (
            wrong_wallet.pubkey(),
            Account::new(0, 0, &system_program::id()),
        ),
    ]);

    mollusk.process_and_validate_instruction(
        &instruction,
        &accounts,
        &[Check::err(ProgramError::InvalidAccountData)],
    );
}

/// Helper function to run not nested test
fn run_not_nested_test(token_program_id: Pubkey) {
    let ctx = TestContext::new(token_program_id);
    let wrong_wallet = Pubkey::new_unique();
    let mollusk = setup_mollusk_with_programs(&ctx.token_program_id);

    let instruction = build_recover_nested_instruction(
        ctx.ata_program_id,
        ctx.wallet.pubkey(),
        ctx.mint,
        ctx.mint,
        ctx.token_program_id,
    );

    // Set up accounts where the nested account is NOT actually nested (not owned by owner_ata)
    let mut accounts =
        create_base_accounts(&ctx.payer, &ctx.wallet.pubkey(), &ctx.token_program_id);

    // Add mint
    accounts.push((
        ctx.mint,
        Account {
            lamports: 1_461_600,
            data: create_mint_data(0),
            owner: ctx.token_program_id,
            executable: false,
            rent_epoch: 0,
        },
    ));

    // Add owner ATA (correctly owned by wallet)
    let owner_ata = get_associated_token_address_with_program_id(
        &ctx.wallet.pubkey(),
        &ctx.mint,
        &ctx.token_program_id,
    );
    accounts.push((
        owner_ata,
        Account {
            lamports: 2_039_280,
            data: create_token_account_data(&ctx.mint, &ctx.wallet.pubkey(), 0),
            owner: ctx.token_program_id,
            executable: false,
            rent_epoch: 0,
        },
    ));

    // The nested ATA is owned by wrong_wallet, not owner_ata (making it not actually nested)
    let nested_ata =
        get_associated_token_address_with_program_id(&owner_ata, &ctx.mint, &ctx.token_program_id);
    accounts.push((
        nested_ata,
        Account {
            lamports: 2_039_280,
            data: create_token_account_data(&ctx.mint, &wrong_wallet, 100), // owned by wrong_wallet, not owner_ata
            owner: ctx.token_program_id,
            executable: false,
            rent_epoch: 0,
        },
    ));

    // Add destination ATA
    let destination_ata = get_associated_token_address_with_program_id(
        &ctx.wallet.pubkey(),
        &ctx.mint,
        &ctx.token_program_id,
    );
    accounts.push((
        destination_ata,
        Account {
            lamports: 2_039_280,
            data: create_token_account_data(&ctx.mint, &ctx.wallet.pubkey(), 0),
            owner: ctx.token_program_id,
            executable: false,
            rent_epoch: 0,
        },
    ));

    mollusk.process_and_validate_instruction(
        &instruction,
        &accounts,
        &[Check::err(ProgramError::Custom(4))], // TokenError::OwnerMismatch
    );
}

/// Helper function to run wrong address derivation owner test
fn run_wrong_address_derivation_owner_test(token_program_id: Pubkey) {
    let ctx = TestContext::new(token_program_id);
    let wrong_wallet = Pubkey::new_unique();
    let mollusk = setup_mollusk_with_programs(&ctx.token_program_id);

    let wrong_owner_ata = get_associated_token_address_with_program_id(
        &wrong_wallet,
        &ctx.mint,
        &ctx.token_program_id,
    );

    let instruction = build_recover_nested_instruction_modified(
        ctx.ata_program_id,
        ctx.wallet.pubkey(),
        ctx.mint,
        ctx.mint,
        ctx.token_program_id,
        |accounts| {
            // Replace owner_ata (account[3]) with wrong derivation
            accounts[3] = AccountMeta::new_readonly(wrong_owner_ata, false);
        },
    );

    let mut accounts = setup_recover_test_scenario(
        &mollusk,
        &ctx.ata_program_id,
        &ctx.token_program_id,
        &ctx.payer,
        &ctx.wallet.pubkey(),
        &ctx.mint,
        &ctx.mint,
        true,
        100,
    );

    // Add the wrong owner ATA account
    accounts.push((
        wrong_owner_ata,
        Account {
            lamports: 2_039_280,
            data: create_token_account_data(&ctx.mint, &wrong_wallet, 0),
            owner: ctx.token_program_id,
            executable: false,
            rent_epoch: 0,
        },
    ));

    mollusk.process_and_validate_instruction(
        &instruction,
        &accounts,
        &[Check::err(ProgramError::InvalidSeeds)],
    );
}

// CONSOLIDATED TESTS - Each test now runs for both token programs

#[test]
fn success_same_mint() {
    run_success_same_mint_test(spl_token::id());
}

#[test]
fn success_same_mint_2022() {
    run_success_same_mint_test(spl_token_2022::id());
}

#[test]
fn success_different_mints() {
    run_success_different_mints_test(spl_token::id());
}

#[test]
fn success_different_mints_2022() {
    run_success_different_mints_test(spl_token_2022::id());
}

#[test]
fn fail_missing_wallet_signature() {
    run_missing_wallet_signature_test(spl_token::id());
}

#[test]
fn fail_missing_wallet_signature_2022() {
    run_missing_wallet_signature_test(spl_token_2022::id());
}

#[test]
fn fail_wrong_signer() {
    run_wrong_signer_test(spl_token::id());
}

#[test]
fn fail_wrong_signer_2022() {
    run_wrong_signer_test(spl_token_2022::id());
}

#[test]
fn fail_not_nested() {
    run_not_nested_test(spl_token::id());
}

#[test]
fn fail_not_nested_2022() {
    run_not_nested_test(spl_token_2022::id());
}

#[test]
fn fail_wrong_address_derivation_owner() {
    run_wrong_address_derivation_owner_test(spl_token::id());
}

#[test]
fn fail_wrong_address_derivation_owner_2022() {
    run_wrong_address_derivation_owner_test(spl_token_2022::id());
}

// UNIQUE TESTS - these don't have duplicates or need special handling

/// Verification test that demonstrates proper token program account structure
#[test]
fn test_real_token_account_creation() {
    let ctx = TestContext::new(spl_token::id());
    let mollusk = setup_mollusk_with_programs(&ctx.token_program_id);

    // Use the improved setup that creates real token accounts
    let accounts = setup_recover_test_scenario(
        &mollusk,
        &ctx.ata_program_id,
        &ctx.token_program_id,
        &ctx.payer,
        &ctx.wallet.pubkey(),
        &ctx.mint,
        &ctx.mint,
        true,
        0, // no tokens initially
    );

    // Verify that real accounts were created properly
    let mint_account = accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == ctx.mint)
        .map(|(_, account)| account)
        .expect("Mint should exist");

    // Verify mint is properly initialized (state = 1 in first 4 bytes)
    assert_eq!(mint_account.owner, ctx.token_program_id);
    assert_eq!(mint_account.data.len(), 82);
    assert_eq!(
        u32::from_le_bytes([
            mint_account.data[0],
            mint_account.data[1],
            mint_account.data[2],
            mint_account.data[3]
        ]),
        1
    );

    // Verify owner ATA exists and is properly initialized
    let owner_ata = get_associated_token_address_with_program_id(
        &ctx.wallet.pubkey(),
        &ctx.mint,
        &ctx.token_program_id,
    );
    let owner_ata_account = accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == owner_ata)
        .map(|(_, account)| account)
        .expect("Owner ATA should exist");

    assert_eq!(owner_ata_account.owner, ctx.token_program_id);
    assert_eq!(owner_ata_account.data.len(), 165);
    // Verify token account state = 1 (initialized) at byte 108
    assert_eq!(owner_ata_account.data[108], 1);
}

#[test]
fn fail_owner_account_does_not_exist() {
    let ctx = TestContext::new(spl_token_2022::id());
    let mollusk = setup_mollusk_with_programs(&ctx.token_program_id);

    let instruction = build_recover_nested_instruction(
        ctx.ata_program_id,
        ctx.wallet.pubkey(),
        ctx.mint,
        ctx.mint,
        ctx.token_program_id,
    );

    // Create accounts manually, excluding the owner ATA
    let mut accounts =
        create_base_accounts(&ctx.payer, &ctx.wallet.pubkey(), &ctx.token_program_id);

    // Add mint
    accounts.push((
        ctx.mint,
        Account {
            lamports: 1_461_600,
            data: create_mint_data(0),
            owner: ctx.token_program_id,
            executable: false,
            rent_epoch: 0,
        },
    ));

    // Add owner ATA as uninitialized account to simulate "does not exist"
    let owner_ata = get_associated_token_address_with_program_id(
        &ctx.wallet.pubkey(),
        &ctx.mint,
        &ctx.token_program_id,
    );
    accounts.push((owner_ata, Account::new(0, 0, &system_program::id())));

    // Add nested ATA
    let nested_ata =
        get_associated_token_address_with_program_id(&owner_ata, &ctx.mint, &ctx.token_program_id);
    accounts.push((
        nested_ata,
        Account {
            lamports: 2_039_280,
            data: create_token_account_data(&ctx.mint, &owner_ata, 100),
            owner: ctx.token_program_id,
            executable: false,
            rent_epoch: 0,
        },
    ));

    // Add destination ATA
    let destination_ata = get_associated_token_address_with_program_id(
        &ctx.wallet.pubkey(),
        &ctx.mint,
        &ctx.token_program_id,
    );
    accounts.push((
        destination_ata,
        Account {
            lamports: 2_039_280,
            data: create_token_account_data(&ctx.mint, &ctx.wallet.pubkey(), 0),
            owner: ctx.token_program_id,
            executable: false,
            rent_epoch: 0,
        },
    ));

    mollusk.process_and_validate_instruction(
        &instruction,
        &accounts,
        &[Check::err(ProgramError::InvalidAccountData)],
    );
}

#[test]
fn fail_wrong_spl_token_program() {
    let ctx = TestContext::new(spl_token_2022::id());
    let wrong_token_program_id = spl_token::id();
    let mut mollusk = setup_mollusk_with_programs(&ctx.token_program_id);

    // Also add the wrong token program
    mollusk.add_program(
        &wrong_token_program_id,
        "programs/token/target/deploy/pinocchio_token_program",
        &LOADER_V3,
    );

    // Build instruction with wrong token program
    let instruction = build_recover_nested_instruction(
        ctx.ata_program_id,
        ctx.wallet.pubkey(),
        ctx.mint,
        ctx.mint,
        wrong_token_program_id, // wrong token program
    );

    // Setup accounts with the CORRECT token program
    let mut accounts = setup_recover_test_scenario(
        &mollusk,
        &ctx.ata_program_id,
        &ctx.token_program_id, // accounts exist for correct token program
        &ctx.payer,
        &ctx.wallet.pubkey(),
        &ctx.mint,
        &ctx.mint,
        true,
        100,
    );

    // Add the missing accounts that the instruction will try to access (using wrong token program) as uninitialized
    let wrong_owner_ata = get_associated_token_address_with_program_id(
        &ctx.wallet.pubkey(),
        &ctx.mint,
        &wrong_token_program_id,
    );
    let wrong_destination_ata = get_associated_token_address_with_program_id(
        &ctx.wallet.pubkey(),
        &ctx.mint,
        &wrong_token_program_id,
    );
    let wrong_nested_ata = get_associated_token_address_with_program_id(
        &wrong_owner_ata,
        &ctx.mint,
        &wrong_token_program_id,
    );

    accounts.extend([
        (wrong_owner_ata, Account::new(0, 0, &system_program::id())),
        (
            wrong_destination_ata,
            Account::new(0, 0, &system_program::id()),
        ),
        (wrong_nested_ata, Account::new(0, 0, &system_program::id())),
        (
            wrong_token_program_id,
            Account {
                lamports: 0,
                data: Vec::new(),
                owner: LOADER_V3,
                executable: true,
                rent_epoch: 0,
            },
        ),
    ]);

    mollusk.process_and_validate_instruction(
        &instruction,
        &accounts,
        &[Check::err(ProgramError::InvalidAccountData)],
    );
}

#[test]
fn fail_destination_not_wallet_ata() {
    let ctx = TestContext::new(spl_token_2022::id());
    let wrong_wallet = Pubkey::new_unique();
    let mollusk = setup_mollusk_with_programs(&ctx.token_program_id);

    let wrong_destination_ata = get_associated_token_address_with_program_id(
        &wrong_wallet,
        &ctx.mint,
        &ctx.token_program_id,
    );

    let instruction = build_recover_nested_instruction_modified(
        ctx.ata_program_id,
        ctx.wallet.pubkey(),
        ctx.mint,
        ctx.mint,
        ctx.token_program_id,
        |accounts| {
            // Replace destination_ata (account[2]) with wrong wallet's ATA
            accounts[2] = AccountMeta::new(wrong_destination_ata, false);
        },
    );

    let mut accounts = setup_recover_test_scenario(
        &mollusk,
        &ctx.ata_program_id,
        &ctx.token_program_id,
        &ctx.payer,
        &ctx.wallet.pubkey(),
        &ctx.mint,
        &ctx.mint,
        true,
        100,
    );

    // Add the wrong destination account
    accounts.push((
        wrong_destination_ata,
        Account {
            lamports: 2_039_280,
            data: create_token_account_data(&ctx.mint, &wrong_wallet, 0),
            owner: ctx.token_program_id,
            executable: false,
            rent_epoch: 0,
        },
    ));

    mollusk.process_and_validate_instruction(
        &instruction,
        &accounts,
        &[Check::err(ProgramError::InvalidSeeds)],
    );
}
