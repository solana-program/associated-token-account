mod utils;

use {
    mollusk_svm::result::Check,
    solana_program::program_pack::Pack,
    solana_pubkey::Pubkey,
    solana_sdk::{
        account::Account, instruction::AccountMeta, program_error::ProgramError, signature::Signer,
        signer::keypair::Keypair,
    },
    spl_associated_token_account_interface::{
        address::get_associated_token_address_with_program_id, instruction,
    },
    spl_token_2022_interface::{
        extension::StateWithExtensionsOwned, state::Account as TokenAccount,
    },
    utils::*,
};

/// Ensure all accounts required by a recover_nested instruction are provided to Mollusk
fn create_mint_mollusk(
    mollusk: &mollusk_svm::Mollusk,
    accounts: &mut Vec<(Pubkey, Account)>,
    payer: &Keypair,
    program_id: &Pubkey,
) -> (Pubkey, Keypair) {
    let mint_account = Keypair::new();
    let mint_authority = Keypair::new();
    accounts.extend([(
        mint_authority.pubkey(),
        account_builder::AccountBuilder::system_account(1_000_000),
    )]);
    let mint_accounts = create_test_mint(
        mollusk,
        &mint_account,
        &mint_authority,
        payer,
        program_id,
        0,
    );
    accounts.extend(mint_accounts.into_iter().filter(|(pubkey, _)| {
        *pubkey == mint_account.pubkey() || *pubkey == mint_authority.pubkey()
    }));
    (mint_account.pubkey(), mint_authority)
}

#[allow(clippy::too_many_arguments)]
fn try_recover_nested_mollusk(
    mollusk: &mollusk_svm::Mollusk,
    accounts: &mut Vec<(Pubkey, Account)>,
    program_id: &Pubkey,
    nested_mint: Pubkey,
    nested_mint_authority: &Keypair,
    nested_associated_token_address: Pubkey,
    destination_token_address: Pubkey,
    wallet: &Keypair,
    recover_instruction: solana_program::instruction::Instruction,
    expected_error: Option<ProgramError>,
) {
    let initial_lamports = accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == wallet.pubkey())
        .map(|(_, account)| account.lamports)
        .expect("Wallet account should exist in initial accounts");

    // mint to nested account
    let amount = 100;
    let mint_to_ix = if *program_id == spl_token_interface::id() {
        spl_token_interface::instruction::mint_to(
            program_id,
            &nested_mint,
            &nested_associated_token_address,
            &nested_mint_authority.pubkey(),
            &[],
            amount,
        )
        .unwrap()
    } else if *program_id == spl_token_2022_interface::id() {
        spl_token_2022_interface::instruction::mint_to(
            program_id,
            &nested_mint,
            &nested_associated_token_address,
            &nested_mint_authority.pubkey(),
            &[],
            amount,
        )
        .unwrap()
    } else {
        panic!("Unsupported token program id: {}", program_id);
    };
    process_and_validate_then_merge(mollusk, &mint_to_ix, accounts, &[Check::success()]);

    // transfer / close nested account
    if let Some(expected_error) = expected_error {
        mollusk.process_and_validate_instruction(
            &recover_instruction,
            accounts,
            &[Check::err(expected_error)],
        );
    } else {
        process_and_validate_then_merge(
            mollusk,
            &recover_instruction,
            accounts,
            &[Check::success()],
        );
        let destination_account = get_account(accounts, destination_token_address);

        // Calculate rent for assertions
        let rent = solana_sdk::rent::Rent::default();

        // Assert destination ATA is properly set up as a token account
        mollusk_svm::result::InstructionResult {
            resulting_accounts: vec![(destination_token_address, destination_account.clone())],
            ..Default::default()
        }
        .run_checks::<mollusk_svm::Mollusk>(
            &[Check::account(&destination_token_address)
                .owner(program_id)
                .rent_exempt()
                .build()],
            &mollusk_svm::result::config::Config {
                panic: true,
                verbose: true,
            },
            mollusk,
        );

        let destination_state =
            StateWithExtensionsOwned::<TokenAccount>::unpack(destination_account.data).unwrap();
        assert_eq!(destination_state.base.amount, amount);
        let wallet_account = get_account(accounts, wallet.pubkey());

        // Calculate the rent for the nested ATA that gets closed
        let ata_space = if *program_id == spl_token_2022_interface::id() {
            spl_token_2022_interface::extension::ExtensionType::try_calculate_account_len::<
                spl_token_2022_interface::state::Account,
            >(&[spl_token_2022_interface::extension::ExtensionType::ImmutableOwner])
            .expect("failed to calculate Token-2022 account length")
        } else {
            spl_token_interface::state::Account::LEN
        };
        let nested_ata_rent = rent.minimum_balance(ata_space);
        // CORRECT calculation based on actual behavior:
        // The recover operation:
        // 1. Closes nested ATA (wallet receives nested_ata_rent)
        // 2. The destination ATA already exists as a token account (no cost to wallet)
        // Net effect: wallet_final = wallet_initial + nested_ata_rent

        let expected_final_lamports = initial_lamports.saturating_add(nested_ata_rent);

        assert_eq!(wallet_account.lamports, expected_final_lamports);
    }
}

fn check_same_mint_mollusk(program_id: &Pubkey) {
    let mollusk = setup_mollusk_with_programs(program_id);
    let payer = Keypair::new();
    let wallet = Keypair::new();
    let mut accounts = create_mollusk_base_accounts_with_token(&payer, program_id);
    ensure_system_accounts_with_lamports(&mut accounts, &[(wallet.pubkey(), 1_000_000)]);

    let (mint, mint_authority) = create_mint_mollusk(&mollusk, &mut accounts, &payer, program_id);
    let (owner_associated_token_address, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet.pubkey(),
        &mint,
        program_id,
    );
    let (nested_associated_token_address, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &owner_associated_token_address,
        &mint,
        program_id,
    );
    // Create destination ATA (wallet's ATA for the mint) - this is the same as owner_associated_token_address for same mint case
    let destination_token_address = owner_associated_token_address;

    let recover_instruction =
        instruction::recover_nested(&wallet.pubkey(), &mint, &mint, program_id);
    try_recover_nested_mollusk(
        &mollusk,
        &mut accounts,
        program_id,
        mint,
        &mint_authority,
        nested_associated_token_address,
        destination_token_address,
        &wallet,
        recover_instruction,
        None,
    );
}

#[test]
fn success_same_mint_2022() {
    check_same_mint_mollusk(&spl_token_2022_interface::id());
}

#[test]
fn success_same_mint() {
    check_same_mint_mollusk(&spl_token_interface::id());
}

fn check_different_mints_mollusk(program_id: &Pubkey) {
    let mollusk = setup_mollusk_with_programs(program_id);
    let payer = Keypair::new();
    let wallet = Keypair::new();
    let mut accounts = create_mollusk_base_accounts_with_token(&payer, program_id);
    ensure_system_accounts_with_lamports(&mut accounts, &[(wallet.pubkey(), 1_000_000)]);

    let (owner_mint, _owner_mint_authority) =
        create_mint_mollusk(&mollusk, &mut accounts, &payer, program_id);
    let (nested_mint, nested_mint_authority) =
        create_mint_mollusk(&mollusk, &mut accounts, &payer, program_id);
    let (owner_associated_token_address, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet.pubkey(),
        &owner_mint,
        program_id,
    );
    let (nested_associated_token_address, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &owner_associated_token_address,
        &nested_mint,
        program_id,
    );
    let (destination_token_address, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet.pubkey(),
        &nested_mint,
        program_id,
    );

    let recover_instruction =
        instruction::recover_nested(&wallet.pubkey(), &owner_mint, &nested_mint, program_id);
    try_recover_nested_mollusk(
        &mollusk,
        &mut accounts,
        program_id,
        nested_mint,
        &nested_mint_authority,
        nested_associated_token_address,
        destination_token_address,
        &wallet,
        recover_instruction,
        None,
    );
}

#[test]
fn success_different_mints() {
    check_different_mints_mollusk(&spl_token_interface::id());
}

#[test]
fn success_different_mints_2022() {
    check_different_mints_mollusk(&spl_token_2022_interface::id());
}

// Error test cases using mollusk
#[test]
fn fail_missing_wallet_signature_2022() {
    let mollusk = setup_mollusk_with_programs(&spl_token_2022_interface::id());
    let payer = Keypair::new();
    let wallet = Keypair::new();
    let mut accounts =
        create_mollusk_base_accounts_with_token(&payer, &spl_token_2022_interface::id());
    ensure_system_accounts_with_lamports(&mut accounts, &[(wallet.pubkey(), 1_000_000)]);
    let (mint, mint_authority) = create_mint_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &spl_token_2022_interface::id(),
    );
    let (owner_ata, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet.pubkey(),
        &mint,
        &spl_token_2022_interface::id(),
    );
    let (nested_ata, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &owner_ata,
        &mint,
        &spl_token_2022_interface::id(),
    );

    let mut recover = instruction::recover_nested(
        &wallet.pubkey(),
        &mint,
        &mint,
        &spl_token_2022_interface::id(),
    );
    recover.accounts[5] = AccountMeta::new(wallet.pubkey(), false); // Remove signature requirement
    try_recover_nested_mollusk(
        &mollusk,
        &mut accounts,
        &spl_token_2022_interface::id(),
        mint,
        &mint_authority,
        nested_ata,
        owner_ata,
        &wallet,
        recover,
        Some(ProgramError::MissingRequiredSignature),
    );
}

#[test]
fn fail_missing_wallet_signature() {
    let mollusk = setup_mollusk_with_programs(&spl_token_interface::id());
    let payer = Keypair::new();
    let wallet = Keypair::new();
    let mut accounts = create_mollusk_base_accounts_with_token(&payer, &spl_token_interface::id());
    ensure_system_accounts_with_lamports(&mut accounts, &[(wallet.pubkey(), 1_000_000)]);
    let (mint, mint_authority) =
        create_mint_mollusk(&mollusk, &mut accounts, &payer, &spl_token_interface::id());
    let (owner_ata, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet.pubkey(),
        &mint,
        &spl_token_interface::id(),
    );
    let (nested_ata, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &owner_ata,
        &mint,
        &spl_token_interface::id(),
    );

    let mut recover =
        instruction::recover_nested(&wallet.pubkey(), &mint, &mint, &spl_token_interface::id());
    recover.accounts[5] = AccountMeta::new(wallet.pubkey(), false);
    try_recover_nested_mollusk(
        &mollusk,
        &mut accounts,
        &spl_token_interface::id(),
        mint,
        &mint_authority,
        nested_ata,
        owner_ata,
        &wallet,
        recover,
        Some(ProgramError::MissingRequiredSignature),
    );
}

#[test]
fn fail_wrong_signer_2022() {
    let mollusk = setup_mollusk_with_programs(&spl_token_2022_interface::id());
    let payer = Keypair::new();
    let wallet = Keypair::new();
    let wrong_wallet = Keypair::new();
    let mut accounts =
        create_mollusk_base_accounts_with_token(&payer, &spl_token_2022_interface::id());
    ensure_system_accounts_with_lamports(
        &mut accounts,
        &[
            (wallet.pubkey(), 1_000_000),
            (wrong_wallet.pubkey(), 1_000_000),
        ],
    );
    let (mint, mint_authority) = create_mint_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &spl_token_2022_interface::id(),
    );
    let (owner_ata, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet.pubkey(),
        &mint,
        &spl_token_2022_interface::id(),
    );
    let (nested_ata, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &owner_ata,
        &mint,
        &spl_token_2022_interface::id(),
    );

    // Ensure all accounts required by recover_nested are provided to Mollusk
    ensure_recover_nested_accounts(
        &mut accounts,
        &wrong_wallet.pubkey(),
        &mint,
        &mint,
        &spl_token_2022_interface::id(),
    );

    let recover_instruction = instruction::recover_nested(
        &wrong_wallet.pubkey(),
        &mint,
        &mint,
        &spl_token_2022_interface::id(),
    ); // Wrong signer
    try_recover_nested_mollusk(
        &mollusk,
        &mut accounts,
        &spl_token_2022_interface::id(),
        mint,
        &mint_authority,
        nested_ata,
        owner_ata,
        &wrong_wallet,
        recover_instruction,
        Some(ProgramError::IllegalOwner),
    );
}

#[test]
fn fail_wrong_signer() {
    let mollusk = setup_mollusk_with_programs(&spl_token_interface::id());
    let payer = Keypair::new();
    let wallet = Keypair::new();
    let wrong_wallet = Keypair::new();
    let mut accounts = create_mollusk_base_accounts_with_token(&payer, &spl_token_interface::id());
    ensure_system_accounts_with_lamports(
        &mut accounts,
        &[
            (wallet.pubkey(), 1_000_000),
            (wrong_wallet.pubkey(), 1_000_000),
        ],
    );
    let (mint, mint_authority) =
        create_mint_mollusk(&mollusk, &mut accounts, &payer, &spl_token_interface::id());
    let (owner_ata, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet.pubkey(),
        &mint,
        &spl_token_interface::id(),
    );
    let (nested_ata, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &owner_ata,
        &mint,
        &spl_token_interface::id(),
    );

    // Ensure all accounts required by recover_nested are provided to Mollusk
    ensure_recover_nested_accounts(
        &mut accounts,
        &wrong_wallet.pubkey(),
        &mint,
        &mint,
        &spl_token_interface::id(),
    );

    let recover_instruction = instruction::recover_nested(
        &wrong_wallet.pubkey(),
        &mint,
        &mint,
        &spl_token_interface::id(),
    ); // Wrong signer
    try_recover_nested_mollusk(
        &mollusk,
        &mut accounts,
        &spl_token_interface::id(),
        mint,
        &mint_authority,
        nested_ata,
        owner_ata,
        &wrong_wallet,
        recover_instruction,
        Some(ProgramError::IllegalOwner),
    );
}

#[test]
fn fail_not_nested_2022() {
    let mollusk = setup_mollusk_with_programs(&spl_token_2022_interface::id());
    let payer = Keypair::new();
    let wallet = Keypair::new();
    let wrong_wallet = Pubkey::new_unique();
    let mut accounts =
        create_mollusk_base_accounts_with_token(&payer, &spl_token_2022_interface::id());
    ensure_system_accounts_with_lamports(
        &mut accounts,
        &[(wallet.pubkey(), 1_000_000), (wrong_wallet, 1_000_000)],
    );
    let (mint, mint_authority) = create_mint_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &spl_token_2022_interface::id(),
    );
    let (owner_ata, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet.pubkey(),
        &mint,
        &spl_token_2022_interface::id(),
    );
    let (nested_ata, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wrong_wallet,
        &mint,
        &spl_token_2022_interface::id(),
    ); // Not nested under owner_ata

    // Ensure all accounts required by recover_nested are provided to Mollusk
    ensure_recover_nested_accounts(
        &mut accounts,
        &wallet.pubkey(),
        &mint,
        &mint,
        &spl_token_2022_interface::id(),
    );

    let recover_instruction = instruction::recover_nested(
        &wallet.pubkey(),
        &mint,
        &mint,
        &spl_token_2022_interface::id(),
    );
    try_recover_nested_mollusk(
        &mollusk,
        &mut accounts,
        &spl_token_2022_interface::id(),
        mint,
        &mint_authority,
        nested_ata,
        owner_ata,
        &wallet,
        recover_instruction,
        Some(ProgramError::IllegalOwner),
    );
}

#[test]
fn fail_not_nested() {
    let mollusk = setup_mollusk_with_programs(&spl_token_interface::id());
    let payer = Keypair::new();
    let wallet = Keypair::new();
    let wrong_wallet = Pubkey::new_unique();
    let mut accounts = create_mollusk_base_accounts_with_token(&payer, &spl_token_interface::id());
    ensure_system_accounts_with_lamports(
        &mut accounts,
        &[(wallet.pubkey(), 1_000_000), (wrong_wallet, 1_000_000)],
    );
    let (mint, mint_authority) =
        create_mint_mollusk(&mollusk, &mut accounts, &payer, &spl_token_interface::id());
    let (owner_ata, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet.pubkey(),
        &mint,
        &spl_token_interface::id(),
    );
    let (nested_ata, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wrong_wallet,
        &mint,
        &spl_token_interface::id(),
    ); // Not nested under owner_ata

    // Ensure all accounts required by recover_nested are provided to Mollusk
    ensure_recover_nested_accounts(
        &mut accounts,
        &wallet.pubkey(),
        &mint,
        &mint,
        &spl_token_interface::id(),
    );

    let recover_instruction =
        instruction::recover_nested(&wallet.pubkey(), &mint, &mint, &spl_token_interface::id());
    try_recover_nested_mollusk(
        &mollusk,
        &mut accounts,
        &spl_token_interface::id(),
        mint,
        &mint_authority,
        nested_ata,
        owner_ata,
        &wallet,
        recover_instruction,
        Some(ProgramError::IllegalOwner),
    );
}
#[test]
fn fail_wrong_address_derivation_owner_2022() {
    let mollusk = setup_mollusk_with_programs(&spl_token_2022_interface::id());
    let payer = Keypair::new();
    let wallet = Keypair::new();
    let mut accounts =
        create_mollusk_base_accounts_with_token(&payer, &spl_token_2022_interface::id());
    ensure_system_accounts_with_lamports(&mut accounts, &[(wallet.pubkey(), 1_000_000)]);
    let (mint, mint_authority) = create_mint_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &spl_token_2022_interface::id(),
    );
    let (owner_ata, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet.pubkey(),
        &mint,
        &spl_token_2022_interface::id(),
    );
    let (nested_ata, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &owner_ata,
        &mint,
        &spl_token_2022_interface::id(),
    );

    // Ensure all accounts required by recover_nested are provided to Mollusk
    ensure_recover_nested_accounts(
        &mut accounts,
        &wallet.pubkey(),
        &mint,
        &mint,
        &spl_token_2022_interface::id(),
    );

    let mut recover_instruction = instruction::recover_nested(
        &wallet.pubkey(),
        &mint,
        &mint,
        &spl_token_2022_interface::id(),
    );
    let wrong_owner_address = Pubkey::new_unique();
    recover_instruction.accounts[3] = AccountMeta::new_readonly(wrong_owner_address, false); // Wrong owner address

    // Ensure the wrong owner address is also provided as a system account
    accounts.push((
        wrong_owner_address,
        account_builder::AccountBuilder::system_account(0),
    ));

    try_recover_nested_mollusk(
        &mollusk,
        &mut accounts,
        &spl_token_2022_interface::id(),
        mint,
        &mint_authority,
        nested_ata,
        wrong_owner_address,
        &wallet,
        recover_instruction,
        Some(ProgramError::InvalidSeeds),
    );
}

#[test]
fn fail_wrong_address_derivation_owner() {
    let mollusk = setup_mollusk_with_programs(&spl_token_interface::id());
    let payer = Keypair::new();
    let wallet = Keypair::new();
    let mut accounts = create_mollusk_base_accounts_with_token(&payer, &spl_token_interface::id());
    ensure_system_accounts_with_lamports(&mut accounts, &[(wallet.pubkey(), 1_000_000)]);
    let (mint, mint_authority) =
        create_mint_mollusk(&mollusk, &mut accounts, &payer, &spl_token_interface::id());
    let (owner_ata, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet.pubkey(),
        &mint,
        &spl_token_interface::id(),
    );
    let (nested_ata, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &owner_ata,
        &mint,
        &spl_token_interface::id(),
    );

    // Ensure all accounts required by recover_nested are provided to Mollusk
    ensure_recover_nested_accounts(
        &mut accounts,
        &wallet.pubkey(),
        &mint,
        &mint,
        &spl_token_interface::id(),
    );

    let mut recover_instruction =
        instruction::recover_nested(&wallet.pubkey(), &mint, &mint, &spl_token_interface::id());
    let wrong_owner_address = Pubkey::new_unique();
    recover_instruction.accounts[3] = AccountMeta::new_readonly(wrong_owner_address, false); // Wrong owner address

    // Ensure the wrong owner address is also provided as a system account
    accounts.push((
        wrong_owner_address,
        account_builder::AccountBuilder::system_account(0),
    ));

    try_recover_nested_mollusk(
        &mollusk,
        &mut accounts,
        &spl_token_interface::id(),
        mint,
        &mint_authority,
        nested_ata,
        wrong_owner_address,
        &wallet,
        recover_instruction,
        Some(ProgramError::InvalidSeeds),
    );
}

#[test]
fn fail_owner_account_does_not_exist() {
    let mollusk = setup_mollusk_with_programs(&spl_token_2022_interface::id());
    let payer = Keypair::new();
    let wallet = Keypair::new();
    let mut accounts =
        create_mollusk_base_accounts_with_token(&payer, &spl_token_2022_interface::id());
    ensure_system_accounts_with_lamports(&mut accounts, &[(wallet.pubkey(), 1_000_000)]);

    // Add the ATA program as an account (required for create_associated_token_account_mollusk)
    accounts.push((
        spl_associated_token_account::id(),
        account_builder::AccountBuilder::executable_program(spl_associated_token_account::id()),
    ));

    let (mint, mint_authority) = create_mint_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &spl_token_2022_interface::id(),
    );
    // Don't create owner ATA - it should not exist
    let owner_ata_address = get_associated_token_address_with_program_id(
        &wallet.pubkey(),
        &mint,
        &spl_token_2022_interface::id(),
    );
    let (nested_ata, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &owner_ata_address,
        &mint,
        &spl_token_2022_interface::id(),
    );

    // Ensure all accounts required by recover_nested are provided to Mollusk
    ensure_recover_nested_accounts(
        &mut accounts,
        &wallet.pubkey(),
        &mint,
        &mint,
        &spl_token_2022_interface::id(),
    );

    // Ensure the mint authority is provided as an account
    if !accounts
        .iter()
        .any(|(pubkey, _)| *pubkey == mint_authority.pubkey())
    {
        accounts.push((
            mint_authority.pubkey(),
            account_builder::AccountBuilder::system_account(0),
        ));
    }

    // Ensure required program and sysvar accounts are present
    ensure_program_accounts_present(
        &mut accounts,
        &[
            spl_token_2022_interface::id(),
            spl_associated_token_account::id(),
            solana_system_interface::program::id(),
        ],
    );

    let recover_instruction = instruction::recover_nested(
        &wallet.pubkey(),
        &mint,
        &mint,
        &spl_token_2022_interface::id(),
    );
    try_recover_nested_mollusk(
        &mollusk,
        &mut accounts,
        &spl_token_2022_interface::id(),
        mint,
        &mint_authority,
        nested_ata,
        owner_ata_address,
        &wallet,
        recover_instruction,
        Some(ProgramError::IllegalOwner),
    );
}

#[test]
fn fail_wrong_spl_token_program() {
    let mollusk = setup_mollusk_with_programs(&spl_token_2022_interface::id());
    let payer = Keypair::new();
    let wallet = Keypair::new();
    let mut accounts =
        create_mollusk_base_accounts_with_token(&payer, &spl_token_2022_interface::id());
    ensure_system_accounts_with_lamports(&mut accounts, &[(wallet.pubkey(), 1_000_000)]);
    let (mint, mint_authority) = create_mint_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &spl_token_2022_interface::id(),
    );
    let (owner_ata, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet.pubkey(),
        &mint,
        &spl_token_2022_interface::id(),
    );
    let (nested_ata, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &owner_ata,
        &mint,
        &spl_token_2022_interface::id(),
    );

    // Ensure all derived ATA accounts are provided to Mollusk
    ensure_recover_nested_accounts(
        &mut accounts,
        &wallet.pubkey(),
        &mint,
        &mint,
        &spl_token_2022_interface::id(),
    );

    // Also ensure accounts for the wrong program ID that the instruction will actually use
    ensure_recover_nested_accounts(
        &mut accounts,
        &wallet.pubkey(),
        &mint,
        &mint,
        &spl_token_interface::id(),
    );

    // Ensure the token program is present as an executable
    ensure_program_accounts_present(&mut accounts, &[spl_token_interface::id()]);

    let recover_instruction =
        instruction::recover_nested(&wallet.pubkey(), &mint, &mint, &spl_token_interface::id()); // Wrong program ID
    try_recover_nested_mollusk(
        &mollusk,
        &mut accounts,
        &spl_token_2022_interface::id(),
        mint,
        &mint_authority,
        nested_ata,
        owner_ata,
        &wallet,
        recover_instruction,
        Some(ProgramError::IllegalOwner),
    );
}

#[test]
fn fail_destination_not_wallet_ata() {
    let mollusk = setup_mollusk_with_programs(&spl_token_2022_interface::id());
    let payer = Keypair::new();
    let wallet = Keypair::new();
    let wrong_wallet = Pubkey::new_unique();
    let mut accounts =
        create_mollusk_base_accounts_with_token(&payer, &spl_token_2022_interface::id());
    ensure_system_accounts_with_lamports(
        &mut accounts,
        &[(wallet.pubkey(), 1_000_000), (wrong_wallet, 1_000_000)],
    );
    let (mint, mint_authority) = create_mint_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &spl_token_2022_interface::id(),
    );
    let (owner_ata, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet.pubkey(),
        &mint,
        &spl_token_2022_interface::id(),
    );
    let (nested_ata, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &owner_ata,
        &mint,
        &spl_token_2022_interface::id(),
    );
    let (wrong_destination_ata, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wrong_wallet,
        &mint,
        &spl_token_2022_interface::id(),
    );

    let mut recover_instruction = instruction::recover_nested(
        &wallet.pubkey(),
        &mint,
        &mint,
        &spl_token_2022_interface::id(),
    );
    recover_instruction.accounts[2] = AccountMeta::new(wrong_destination_ata, false); // Wrong destination
    try_recover_nested_mollusk(
        &mollusk,
        &mut accounts,
        &spl_token_2022_interface::id(),
        mint,
        &mint_authority,
        nested_ata,
        wrong_destination_ata,
        &wallet,
        recover_instruction,
        Some(ProgramError::InvalidSeeds),
    );
}
