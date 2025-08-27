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
fn ensure_recover_nested_accounts(
    accounts: &mut Vec<(Pubkey, Account)>,
    wallet_address: &Pubkey,
    nested_token_mint_address: &Pubkey,
    owner_token_mint_address: &Pubkey,
    token_program: &Pubkey,
) {
    // The recover_nested instruction derives these addresses internally:
    // 1. owner_associated_account_address (wallet's ATA for owner mint)
    let owner_ata = get_associated_token_address_with_program_id(
        wallet_address,
        owner_token_mint_address,
        token_program,
    );

    // 2. destination_associated_account_address (wallet's ATA for nested mint)
    let destination_ata = get_associated_token_address_with_program_id(
        wallet_address,
        nested_token_mint_address,
        token_program,
    );

    // 3. nested_associated_account_address (owner_ata's ATA for nested mint)
    let nested_ata = get_associated_token_address_with_program_id(
        &owner_ata,
        nested_token_mint_address,
        token_program,
    );

    // Ensure all derived addresses are present as system accounts
    for ata_address in [owner_ata, destination_ata, nested_ata] {
        if !accounts.iter().any(|(pubkey, _)| *pubkey == ata_address) {
            accounts.push((
                ata_address,
                account_builder::AccountBuilder::system_account(0),
            ));
        }
    }
}

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

fn create_associated_token_account_mollusk(
    mollusk: &mollusk_svm::Mollusk,
    accounts: &mut Vec<(Pubkey, Account)>,
    payer: &Keypair,
    owner: &Pubkey,
    mint: &Pubkey,
    program_id: &Pubkey,
) -> Pubkey {
    let ata_address = get_associated_token_address_with_program_id(owner, mint, program_id);

    // Ensure the provided owner (wallet) account exists in the accounts list.
    // Mollusk requires every AccountMeta in the instruction to have a backing Account entry.
    if !accounts.iter().any(|(pubkey, _)| *pubkey == *owner) {
        accounts.push((*owner, account_builder::AccountBuilder::system_account(0)));
    }

    let instruction = build_create_ata_instruction_with_system_account(
        accounts,
        spl_associated_token_account::id(),
        payer.pubkey(),
        ata_address,
        *owner,
        *mint,
        *program_id,
        CreateAtaInstructionType::Create {
            bump: None,
            account_len: None,
        },
    );
    let result = mollusk.process_instruction(&instruction, accounts);
    if !result.program_result.is_ok() {
        panic!("ATA creation failed: {:?}", result.program_result);
    }
    if let Some((_, account)) = result
        .resulting_accounts
        .into_iter()
        .find(|(pubkey, _)| *pubkey == ata_address)
    {
        // Update the existing account in the accounts list instead of pushing a duplicate
        if let Some((_, existing_account)) = accounts
            .iter_mut()
            .find(|(pubkey, _)| *pubkey == ata_address)
        {
            *existing_account = account;
        }
    }
    ata_address
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
    let mint_to_ix = spl_token_2022_interface::instruction::mint_to(
        program_id,
        &nested_mint,
        &nested_associated_token_address,
        &nested_mint_authority.pubkey(),
        &[],
        amount,
    )
    .unwrap();
    let result = mollusk.process_instruction(&mint_to_ix, accounts);
    assert!(result.program_result.is_ok());
    if let Some((_, account)) = result
        .resulting_accounts
        .into_iter()
        .find(|(pubkey, _)| *pubkey == nested_associated_token_address)
    {
        if let Some((_, existing_account)) = accounts
            .iter_mut()
            .find(|(pubkey, _)| *pubkey == nested_associated_token_address)
        {
            *existing_account = account;
        }
    }

    // transfer / close nested account
    if let Some(expected_error) = expected_error {
        mollusk.process_and_validate_instruction(
            &recover_instruction,
            accounts,
            &[Check::err(expected_error)],
        );
    } else {
        let result = mollusk.process_instruction(&recover_instruction, accounts);
        assert!(result.program_result.is_ok());
        let destination_account = result
            .resulting_accounts
            .iter()
            .find(|(pubkey, _)| *pubkey == destination_token_address)
            .expect("Destination account should exist")
            .1
            .clone();

        // Calculate rent for assertions
        let rent = solana_sdk::rent::Rent::default();

        // Assert destination ATA is properly set up as a token account
        assert_eq!(destination_account.owner, *program_id);
        let destination_ata_rent = rent.minimum_balance(destination_account.data.len());
        assert!(
            destination_account.lamports >= destination_ata_rent,
            "Destination ATA should be rent-exempt: {} >= {}",
            destination_account.lamports,
            destination_ata_rent
        );

        let destination_state =
            StateWithExtensionsOwned::<TokenAccount>::unpack(destination_account.data).unwrap();
        assert_eq!(destination_state.base.amount, amount);
        let wallet_account = result
            .resulting_accounts
            .iter()
            .find(|(pubkey, _)| *pubkey == wallet.pubkey())
            .expect("Wallet account should exist")
            .1
            .clone();

        // Calculate the rent for the nested ATA that gets closed
        let ata_space = if *program_id == spl_token_2022_interface::id() {
            // spl-token-2022 accounts get the ImmutableOwner extension, which adds 5 bytes
            // (2 bytes extension type + 2 bytes length + 1 byte data)
            spl_token_2022_interface::state::Account::LEN + 5
        } else {
            spl_token_interface::state::Account::LEN
        };
        let nested_ata_rent = rent.minimum_balance(ata_space);
        // CORRECT calculation based on actual behavior:
        // The recover operation:
        // 1. Closes nested ATA (wallet receives nested_ata_rent)
        // 2. The destination ATA already exists as a token account (no cost to wallet)
        // Net effect: wallet_final = wallet_initial + nested_ata_rent

        let expected_final_lamports = initial_lamports + nested_ata_rent;

        assert_eq!(wallet_account.lamports, expected_final_lamports);
    }
}

fn check_same_mint_mollusk(program_id: &Pubkey) {
    let mollusk = setup_mollusk_with_programs(program_id);
    let payer = Keypair::new();
    let wallet = Keypair::new();
    let mut accounts = create_mollusk_base_accounts_with_token(&payer, program_id);
    accounts.push((
        wallet.pubkey(),
        account_builder::AccountBuilder::system_account(1_000_000),
    ));

    let (mint, mint_authority) = create_mint_mollusk(&mollusk, &mut accounts, &payer, program_id);
    let owner_associated_token_address = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet.pubkey(),
        &mint,
        program_id,
    );
    let nested_associated_token_address = create_associated_token_account_mollusk(
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
    accounts.push((
        wallet.pubkey(),
        account_builder::AccountBuilder::system_account(1_000_000),
    ));

    let (owner_mint, _owner_mint_authority) =
        create_mint_mollusk(&mollusk, &mut accounts, &payer, program_id);
    let (nested_mint, nested_mint_authority) =
        create_mint_mollusk(&mollusk, &mut accounts, &payer, program_id);
    let owner_associated_token_address = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet.pubkey(),
        &owner_mint,
        program_id,
    );
    let nested_associated_token_address = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &owner_associated_token_address,
        &nested_mint,
        program_id,
    );
    let destination_token_address = create_associated_token_account_mollusk(
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
    accounts.push((
        wallet.pubkey(),
        account_builder::AccountBuilder::system_account(1_000_000),
    ));
    let (mint, mint_authority) = create_mint_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &spl_token_2022_interface::id(),
    );
    let owner_ata = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet.pubkey(),
        &mint,
        &spl_token_2022_interface::id(),
    );
    let nested_ata = create_associated_token_account_mollusk(
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
    accounts.push((
        wallet.pubkey(),
        account_builder::AccountBuilder::system_account(1_000_000),
    ));
    let (mint, mint_authority) =
        create_mint_mollusk(&mollusk, &mut accounts, &payer, &spl_token_interface::id());
    let owner_ata = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet.pubkey(),
        &mint,
        &spl_token_interface::id(),
    );
    let nested_ata = create_associated_token_account_mollusk(
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
    accounts.extend([
        (
            wallet.pubkey(),
            account_builder::AccountBuilder::system_account(1_000_000),
        ),
        (
            wrong_wallet.pubkey(),
            account_builder::AccountBuilder::system_account(1_000_000),
        ),
    ]);
    let (mint, mint_authority) = create_mint_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &spl_token_2022_interface::id(),
    );
    let owner_ata = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet.pubkey(),
        &mint,
        &spl_token_2022_interface::id(),
    );
    let nested_ata = create_associated_token_account_mollusk(
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
    accounts.extend([
        (
            wallet.pubkey(),
            account_builder::AccountBuilder::system_account(1_000_000),
        ),
        (
            wrong_wallet.pubkey(),
            account_builder::AccountBuilder::system_account(1_000_000),
        ),
    ]);
    let (mint, mint_authority) =
        create_mint_mollusk(&mollusk, &mut accounts, &payer, &spl_token_interface::id());
    let owner_ata = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet.pubkey(),
        &mint,
        &spl_token_interface::id(),
    );
    let nested_ata = create_associated_token_account_mollusk(
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
    accounts.extend([
        (
            wallet.pubkey(),
            account_builder::AccountBuilder::system_account(1_000_000),
        ),
        (
            wrong_wallet,
            account_builder::AccountBuilder::system_account(1_000_000),
        ),
    ]);
    let (mint, mint_authority) = create_mint_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &spl_token_2022_interface::id(),
    );
    let owner_ata = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet.pubkey(),
        &mint,
        &spl_token_2022_interface::id(),
    );
    let nested_ata = create_associated_token_account_mollusk(
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
    accounts.extend([
        (
            wallet.pubkey(),
            account_builder::AccountBuilder::system_account(1_000_000),
        ),
        (
            wrong_wallet,
            account_builder::AccountBuilder::system_account(1_000_000),
        ),
    ]);
    let (mint, mint_authority) =
        create_mint_mollusk(&mollusk, &mut accounts, &payer, &spl_token_interface::id());
    let owner_ata = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet.pubkey(),
        &mint,
        &spl_token_interface::id(),
    );
    let nested_ata = create_associated_token_account_mollusk(
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
    accounts.push((
        wallet.pubkey(),
        account_builder::AccountBuilder::system_account(1_000_000),
    ));
    let (mint, mint_authority) = create_mint_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &spl_token_2022_interface::id(),
    );
    let owner_ata = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet.pubkey(),
        &mint,
        &spl_token_2022_interface::id(),
    );
    let nested_ata = create_associated_token_account_mollusk(
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
    recover_instruction.accounts[3] = AccountMeta::new(wrong_owner_address, false); // Wrong owner address

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
    accounts.push((
        wallet.pubkey(),
        account_builder::AccountBuilder::system_account(1_000_000),
    ));
    let (mint, mint_authority) =
        create_mint_mollusk(&mollusk, &mut accounts, &payer, &spl_token_interface::id());
    let owner_ata = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet.pubkey(),
        &mint,
        &spl_token_interface::id(),
    );
    let nested_ata = create_associated_token_account_mollusk(
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
    recover_instruction.accounts[3] = AccountMeta::new(wrong_owner_address, false); // Wrong owner address

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
    accounts.push((
        wallet.pubkey(),
        account_builder::AccountBuilder::system_account(1_000_000),
    ));

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
    let nested_ata = create_associated_token_account_mollusk(
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

    // Ensure the token program is provided as an account
    if !accounts
        .iter()
        .any(|(pubkey, _)| *pubkey == spl_token_2022_interface::id())
    {
        accounts.push((
            spl_token_2022_interface::id(),
            account_builder::AccountBuilder::executable_program(spl_token_2022_interface::id()),
        ));
    }

    // Ensure the ATA program is provided as an account
    if !accounts
        .iter()
        .any(|(pubkey, _)| *pubkey == spl_associated_token_account::id())
    {
        accounts.push((
            spl_associated_token_account::id(),
            account_builder::AccountBuilder::executable_program(spl_associated_token_account::id()),
        ));
    }

    // Ensure the system program is provided as an account
    let system_program_id = solana_program::pubkey!("11111111111111111111111111111111");
    if !accounts
        .iter()
        .any(|(pubkey, _)| *pubkey == system_program_id)
    {
        accounts.push((
            system_program_id,
            account_builder::AccountBuilder::executable_program(system_program_id),
        ));
    }

    // Ensure the rent sysvar is provided as an account
    let rent_sysvar_id = solana_program::pubkey!("SysvarRent111111111111111111111111111111111");
    if !accounts.iter().any(|(pubkey, _)| *pubkey == rent_sysvar_id) {
        accounts.push((
            rent_sysvar_id,
            account_builder::AccountBuilder::system_account(0),
        ));
    }

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
    accounts.push((
        wallet.pubkey(),
        account_builder::AccountBuilder::system_account(1_000_000),
    ));
    let (mint, mint_authority) = create_mint_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &spl_token_2022_interface::id(),
    );
    let owner_ata = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet.pubkey(),
        &mint,
        &spl_token_2022_interface::id(),
    );
    let nested_ata = create_associated_token_account_mollusk(
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

    // The instruction also needs the token program itself as an account
    accounts.push((
        spl_token_interface::id(),
        account_builder::AccountBuilder::executable_program(spl_token_interface::id()),
    ));

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
    accounts.extend([
        (
            wallet.pubkey(),
            account_builder::AccountBuilder::system_account(1_000_000),
        ),
        (
            wrong_wallet,
            account_builder::AccountBuilder::system_account(1_000_000),
        ),
    ]);
    let (mint, mint_authority) = create_mint_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &spl_token_2022_interface::id(),
    );
    let owner_ata = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet.pubkey(),
        &mint,
        &spl_token_2022_interface::id(),
    );
    let nested_ata = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &owner_ata,
        &mint,
        &spl_token_2022_interface::id(),
    );
    let wrong_destination_ata = create_associated_token_account_mollusk(
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
