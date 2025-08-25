mod utils;

use {
    solana_pubkey::Pubkey,
    solana_sdk::{
        account::Account,
        instruction::{AccountMeta, InstructionError},
        signature::Signer,
        signer::keypair::Keypair,
    },
    solana_system_interface::instruction as system_instruction,
    spl_associated_token_account_interface::{
        address::get_associated_token_address_with_program_id, instruction,
    },
    spl_token_2022_interface::{
        extension::{ExtensionType, StateWithExtensionsOwned},
        state::{Account as TokenAccount, Mint},
    },
    utils::*,
};

fn create_mint_mollusk(
    mollusk: &mollusk_svm::Mollusk,
    accounts: &mut Vec<(Pubkey, Account)>,
    payer: &Keypair,
    program_id: &Pubkey,
) -> (Pubkey, Keypair) {
    let mint_account = Keypair::new();
    let mint_authority = Keypair::new();
    accounts.extend([
        (
            mint_account.pubkey(),
            Account::new(0, 0, &solana_program::system_program::id()),
        ),
        (
            mint_authority.pubkey(),
            account_builder::AccountBuilder::system_account(1_000_000),
        ),
    ]);
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
    let instruction = build_create_ata_instruction(
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
    assert!(result.program_result.is_ok());
    if let Some((_, account)) = result
        .resulting_accounts
        .into_iter()
        .find(|(pubkey, _)| *pubkey == ata_address)
    {
        accounts.push((ata_address, account));
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
    expected_error: Option<InstructionError>,
) {
    let initial_lamports = accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == nested_associated_token_address)
        .map(|(_, account)| account.lamports)
        .unwrap_or(TOKEN_ACCOUNT_RENT_EXEMPT);

    // mint to nested account
    let amount = 100;
    let mint_to_ix = spl_token_2022::instruction::mint_to(
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
    let result = mollusk.process_instruction(&recover_instruction, accounts);
    if let Some(expected_error) = expected_error {
        assert_eq!(result.program_result.unwrap_err(), expected_error.into());
    } else {
        assert!(result.program_result.is_ok());
        let destination_account = result
            .resulting_accounts
            .iter()
            .find(|(pubkey, _)| *pubkey == destination_token_address)
            .expect("Destination account should exist")
            .1
            .clone();
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
        assert_eq!(wallet_account.lamports, initial_lamports);
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

    let recover_instruction =
        instruction::recover_nested(&wallet.pubkey(), &mint, &mint, program_id);
    try_recover_nested_mollusk(
        &mollusk,
        &mut accounts,
        program_id,
        mint,
        &mint_authority,
        nested_associated_token_address,
        owner_associated_token_address,
        &wallet,
        recover_instruction,
        None,
    );
}

#[test]
fn success_same_mint_2022() {
    check_same_mint_mollusk(&spl_token_2022::id());
}

#[test]
fn success_same_mint() {
    check_same_mint_mollusk(&spl_token::id());
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
    check_different_mints_mollusk(&spl_token::id());
}

#[test]
fn success_different_mints_2022() {
    check_different_mints_mollusk(&spl_token_2022::id());
}

// Error test cases using mollusk
#[test]
fn fail_missing_wallet_signature_2022() {
    let mollusk = setup_mollusk_with_programs(&spl_token_2022::id());
    let payer = Keypair::new();
    let wallet = Keypair::new();
    let mut accounts = create_mollusk_base_accounts_with_token(&payer, &spl_token_2022::id());
    accounts.push((
        wallet.pubkey(),
        account_builder::AccountBuilder::system_account(1_000_000),
    ));
    let (mint, mint_authority) =
        create_mint_mollusk(&mollusk, &mut accounts, &payer, &spl_token_2022::id());
    let owner_ata = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet.pubkey(),
        &mint,
        &spl_token_2022::id(),
    );
    let nested_ata = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &owner_ata,
        &mint,
        &spl_token_2022::id(),
    );

    let mut recover =
        instruction::recover_nested(&wallet.pubkey(), &mint, &mint, &spl_token_2022::id());
    recover.accounts[5] = AccountMeta::new(wallet.pubkey(), false); // Remove signature requirement
    try_recover_nested_mollusk(
        &mollusk,
        &mut accounts,
        &spl_token_2022::id(),
        mint,
        &mint_authority,
        nested_ata,
        owner_ata,
        &wallet,
        recover,
        Some(InstructionError::MissingRequiredSignature),
    );
}

#[test]
fn fail_missing_wallet_signature() {
    let mollusk = setup_mollusk_with_programs(&spl_token::id());
    let payer = Keypair::new();
    let wallet = Keypair::new();
    let mut accounts = create_mollusk_base_accounts_with_token(&payer, &spl_token::id());
    accounts.push((
        wallet.pubkey(),
        account_builder::AccountBuilder::system_account(1_000_000),
    ));
    let (mint, mint_authority) =
        create_mint_mollusk(&mollusk, &mut accounts, &payer, &spl_token::id());
    let owner_ata = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet.pubkey(),
        &mint,
        &spl_token::id(),
    );
    let nested_ata = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &owner_ata,
        &mint,
        &spl_token::id(),
    );

    let mut recover = instruction::recover_nested(&wallet.pubkey(), &mint, &mint, &spl_token::id());
    recover.accounts[5] = AccountMeta::new(wallet.pubkey(), false);
    try_recover_nested_mollusk(
        &mollusk,
        &mut accounts,
        &spl_token::id(),
        mint,
        &mint_authority,
        nested_ata,
        owner_ata,
        &wallet,
        recover,
        Some(InstructionError::MissingRequiredSignature),
    );
}

#[test]
fn fail_wrong_signer_2022() {
    let mollusk = setup_mollusk_with_programs(&spl_token_2022::id());
    let payer = Keypair::new();
    let wallet = Keypair::new();
    let wrong_wallet = Keypair::new();
    let mut accounts = create_mollusk_base_accounts_with_token(&payer, &spl_token_2022::id());
    accounts.extend([(wallet.pubkey(), account_builder::AccountBuilder::system_account(1_000_000)), (wrong_wallet.pubkey(), account_builder::AccountBuilder::system_account(1_000_000))]);
    let (mint, mint_authority) = create_mint_mollusk(&mollusk, &mut accounts, &payer, &spl_token_2022::id());
    let owner_ata = create_associated_token_account_mollusk(&mollusk, &mut accounts, &payer, &wallet.pubkey(), &mint, &spl_token_2022::id());
    let nested_ata = create_associated_token_account_mollusk(&mollusk, &mut accounts, &payer, &owner_ata, &mint, &spl_token_2022::id());
    
    let recover_instruction = instruction::recover_nested(&wrong_wallet.pubkey(), &mint, &mint, &spl_token_2022::id()); // Wrong signer
    try_recover_nested_mollusk(&mollusk, &mut accounts, &spl_token_2022::id(), mint, &mint_authority, nested_ata, owner_ata, &wrong_wallet, recover_instruction, Some(InstructionError::IllegalOwner));
}

#[test]
fn fail_wrong_signer() {
    let mollusk = setup_mollusk_with_programs(&spl_token::id());
    let payer = Keypair::new();
    let wallet = Keypair::new();
    let wrong_wallet = Keypair::new();
    let mut accounts = create_mollusk_base_accounts_with_token(&payer, &spl_token::id());
    accounts.extend([(wallet.pubkey(), account_builder::AccountBuilder::system_account(1_000_000)), (wrong_wallet.pubkey(), account_builder::AccountBuilder::system_account(1_000_000))]);
    let (mint, mint_authority) = create_mint_mollusk(&mollusk, &mut accounts, &payer, &spl_token::id());
    let owner_ata = create_associated_token_account_mollusk(&mollusk, &mut accounts, &payer, &wallet.pubkey(), &mint, &spl_token::id());
    let nested_ata = create_associated_token_account_mollusk(&mollusk, &mut accounts, &payer, &owner_ata, &mint, &spl_token::id());
    
    let recover_instruction = instruction::recover_nested(&wrong_wallet.pubkey(), &mint, &mint, &spl_token::id()); // Wrong signer
    try_recover_nested_mollusk(&mollusk, &mut accounts, &spl_token::id(), mint, &mint_authority, nested_ata, owner_ata, &wrong_wallet, recover_instruction, Some(InstructionError::IllegalOwner));
}

#[test]
fn fail_not_nested_2022() {
    let mollusk = setup_mollusk_with_programs(&spl_token_2022::id());
    let payer = Keypair::new();
    let wallet = Keypair::new();
    let wrong_wallet = Pubkey::new_unique();
    let mut accounts = create_mollusk_base_accounts_with_token(&payer, &spl_token_2022::id());
    accounts.extend([(wallet.pubkey(), account_builder::AccountBuilder::system_account(1_000_000)), (wrong_wallet, account_builder::AccountBuilder::system_account(1_000_000))]);
    let (mint, mint_authority) = create_mint_mollusk(&mollusk, &mut accounts, &payer, &spl_token_2022::id());
    let owner_ata = create_associated_token_account_mollusk(&mollusk, &mut accounts, &payer, &wallet.pubkey(), &mint, &spl_token_2022::id());
    let nested_ata = create_associated_token_account_mollusk(&mollusk, &mut accounts, &payer, &wrong_wallet, &mint, &spl_token_2022::id()); // Not nested under owner_ata
    
    let recover_instruction = instruction::recover_nested(&wallet.pubkey(), &mint, &mint, &spl_token_2022::id());
    try_recover_nested_mollusk(&mollusk, &mut accounts, &spl_token_2022::id(), mint, &mint_authority, nested_ata, owner_ata, &wallet, recover_instruction, Some(InstructionError::IllegalOwner));
}

#[test]
fn fail_not_nested() {
    let mollusk = setup_mollusk_with_programs(&spl_token::id());
    let payer = Keypair::new();
    let wallet = Keypair::new();
    let wrong_wallet = Pubkey::new_unique();
    let mut accounts = create_mollusk_base_accounts_with_token(&payer, &spl_token::id());
    accounts.extend([(wallet.pubkey(), account_builder::AccountBuilder::system_account(1_000_000)), (wrong_wallet, account_builder::AccountBuilder::system_account(1_000_000))]);
    let (mint, mint_authority) = create_mint_mollusk(&mollusk, &mut accounts, &payer, &spl_token::id());
    let owner_ata = create_associated_token_account_mollusk(&mollusk, &mut accounts, &payer, &wallet.pubkey(), &mint, &spl_token::id());
    let nested_ata = create_associated_token_account_mollusk(&mollusk, &mut accounts, &payer, &wrong_wallet, &mint, &spl_token::id()); // Not nested under owner_ata
    
    let recover_instruction = instruction::recover_nested(&wallet.pubkey(), &mint, &mint, &spl_token::id());
    try_recover_nested_mollusk(&mollusk, &mut accounts, &spl_token::id(), mint, &mint_authority, nested_ata, owner_ata, &wallet, recover_instruction, Some(InstructionError::IllegalOwner));
}
#[test]
fn fail_wrong_address_derivation_owner_2022() {
    let mollusk = setup_mollusk_with_programs(&spl_token_2022::id());
    let payer = Keypair::new();
    let wallet = Keypair::new();
    let mut accounts = create_mollusk_base_accounts_with_token(&payer, &spl_token_2022::id());
    accounts.push((wallet.pubkey(), account_builder::AccountBuilder::system_account(1_000_000)));
    let (mint, mint_authority) = create_mint_mollusk(&mollusk, &mut accounts, &payer, &spl_token_2022::id());
    let owner_ata = create_associated_token_account_mollusk(&mollusk, &mut accounts, &payer, &wallet.pubkey(), &mint, &spl_token_2022::id());
    let nested_ata = create_associated_token_account_mollusk(&mollusk, &mut accounts, &payer, &owner_ata, &mint, &spl_token_2022::id());
    
    let mut recover_instruction = instruction::recover_nested(&wallet.pubkey(), &mint, &mint, &spl_token_2022::id());
    recover_instruction.accounts[3] = AccountMeta::new(Pubkey::new_unique(), false); // Wrong owner address
    try_recover_nested_mollusk(&mollusk, &mut accounts, &spl_token_2022::id(), mint, &mint_authority, nested_ata, Pubkey::new_unique(), &wallet, recover_instruction, Some(InstructionError::InvalidSeeds));
}

#[test]
fn fail_wrong_address_derivation_owner() {
    let mollusk = setup_mollusk_with_programs(&spl_token::id());
    let payer = Keypair::new();
    let wallet = Keypair::new();
    let mut accounts = create_mollusk_base_accounts_with_token(&payer, &spl_token::id());
    accounts.push((wallet.pubkey(), account_builder::AccountBuilder::system_account(1_000_000)));
    let (mint, mint_authority) = create_mint_mollusk(&mollusk, &mut accounts, &payer, &spl_token::id());
    let owner_ata = create_associated_token_account_mollusk(&mollusk, &mut accounts, &payer, &wallet.pubkey(), &mint, &spl_token::id());
    let nested_ata = create_associated_token_account_mollusk(&mollusk, &mut accounts, &payer, &owner_ata, &mint, &spl_token::id());
    
    let mut recover_instruction = instruction::recover_nested(&wallet.pubkey(), &mint, &mint, &spl_token::id());
    recover_instruction.accounts[3] = AccountMeta::new(Pubkey::new_unique(), false); // Wrong owner address
    try_recover_nested_mollusk(&mollusk, &mut accounts, &spl_token::id(), mint, &mint_authority, nested_ata, Pubkey::new_unique(), &wallet, recover_instruction, Some(InstructionError::InvalidSeeds));
}

#[test]
fn fail_owner_account_does_not_exist() {
    let mollusk = setup_mollusk_with_programs(&spl_token_2022::id());
    let payer = Keypair::new();
    let wallet = Keypair::new();
    let mut accounts = create_mollusk_base_accounts_with_token(&payer, &spl_token_2022::id());
    accounts.push((wallet.pubkey(), account_builder::AccountBuilder::system_account(1_000_000)));
    let (mint, mint_authority) = create_mint_mollusk(&mollusk, &mut accounts, &payer, &spl_token_2022::id());
    // Don't create owner ATA - it should not exist
    let owner_ata_address = get_associated_token_address_with_program_id(&wallet.pubkey(), &mint, &spl_token_2022::id());
    let nested_ata = create_associated_token_account_mollusk(&mollusk, &mut accounts, &payer, &owner_ata_address, &mint, &spl_token_2022::id());
    
    let recover_instruction = instruction::recover_nested(&wallet.pubkey(), &mint, &mint, &spl_token_2022::id());
    try_recover_nested_mollusk(&mollusk, &mut accounts, &spl_token_2022::id(), mint, &mint_authority, nested_ata, owner_ata_address, &wallet, recover_instruction, Some(InstructionError::IllegalOwner));
}

#[test]
fn fail_wrong_spl_token_program() {
    let mollusk = setup_mollusk_with_programs(&spl_token_2022::id());
    let payer = Keypair::new();
    let wallet = Keypair::new();
    let mut accounts = create_mollusk_base_accounts_with_token(&payer, &spl_token_2022::id());
    accounts.push((wallet.pubkey(), account_builder::AccountBuilder::system_account(1_000_000)));
    let (mint, mint_authority) = create_mint_mollusk(&mollusk, &mut accounts, &payer, &spl_token_2022::id());
    let owner_ata = create_associated_token_account_mollusk(&mollusk, &mut accounts, &payer, &wallet.pubkey(), &mint, &spl_token_2022::id());
    let nested_ata = create_associated_token_account_mollusk(&mollusk, &mut accounts, &payer, &owner_ata, &mint, &spl_token_2022::id());
    
    let recover_instruction = instruction::recover_nested(&wallet.pubkey(), &mint, &mint, &spl_token::id()); // Wrong program ID
    try_recover_nested_mollusk(&mollusk, &mut accounts, &spl_token_2022::id(), mint, &mint_authority, nested_ata, owner_ata, &wallet, recover_instruction, Some(InstructionError::IllegalOwner));
}

#[test]
fn fail_destination_not_wallet_ata() {
    let mollusk = setup_mollusk_with_programs(&spl_token_2022::id());
    let payer = Keypair::new();
    let wallet = Keypair::new();
    let wrong_wallet = Pubkey::new_unique();
    let mut accounts = create_mollusk_base_accounts_with_token(&payer, &spl_token_2022::id());
    accounts.extend([(wallet.pubkey(), account_builder::AccountBuilder::system_account(1_000_000)), (wrong_wallet, account_builder::AccountBuilder::system_account(1_000_000))]);
    let (mint, mint_authority) = create_mint_mollusk(&mollusk, &mut accounts, &payer, &spl_token_2022::id());
    let owner_ata = create_associated_token_account_mollusk(&mollusk, &mut accounts, &payer, &wallet.pubkey(), &mint, &spl_token_2022::id());
    let nested_ata = create_associated_token_account_mollusk(&mollusk, &mut accounts, &payer, &owner_ata, &mint, &spl_token_2022::id());
    let wrong_destination_ata = create_associated_token_account_mollusk(&mollusk, &mut accounts, &payer, &wrong_wallet, &mint, &spl_token_2022::id());
    
    let mut recover_instruction = instruction::recover_nested(&wallet.pubkey(), &mint, &mint, &spl_token_2022::id());
    recover_instruction.accounts[2] = AccountMeta::new(wrong_destination_ata, false); // Wrong destination
    try_recover_nested_mollusk(&mollusk, &mut accounts, &spl_token_2022::id(), mint, &mint_authority, nested_ata, wrong_destination_ata, &wallet, recover_instruction, Some(InstructionError::InvalidSeeds));
}
