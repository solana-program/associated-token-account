use {
    mollusk_svm::result::Check,
    solana_instruction::AccountMeta,
    solana_program_error::ProgramError,
    solana_pubkey::Pubkey,
    spl_associated_token_account_interface::address::get_associated_token_address_with_program_id,
    spl_associated_token_account_mollusk_harness::{
        build_create_ata_instruction, AtaTestHarness, CreateAtaInstructionType,
    },
    test_case::test_case,
};

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn create_rejects_too_few_accounts(token_program_id: Pubkey) {
    let harness = AtaTestHarness::new(&token_program_id).with_wallet_and_mint(1_000_000, 6);
    let wallet = harness.wallet.unwrap();
    let mint = harness.mint.unwrap();
    let ata_address =
        get_associated_token_address_with_program_id(&wallet, &mint, &token_program_id);

    let mut instruction = build_create_ata_instruction(
        spl_associated_token_account_interface::program::id(),
        harness.payer,
        ata_address,
        wallet,
        mint,
        token_program_id,
        CreateAtaInstructionType::Create {
            bump: None,
            account_len: None,
        },
    );
    instruction.accounts.truncate(5);

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[Check::err(ProgramError::NotEnoughAccountKeys)],
    );
}

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn create_account_mismatch(token_program_id: Pubkey) {
    let harness = AtaTestHarness::new(&token_program_id).with_wallet_and_mint(1_000_000, 6);

    let wallet = harness.wallet.unwrap();
    let mint = harness.mint.unwrap();
    let ata_address =
        get_associated_token_address_with_program_id(&wallet, &mint, &token_program_id);

    for account_idx in [1, 2, 3, 5] {
        let mut instruction = build_create_ata_instruction(
            spl_associated_token_account_interface::program::id(),
            harness.payer,
            ata_address,
            wallet,
            mint,
            token_program_id,
            CreateAtaInstructionType::Create {
                bump: None,
                account_len: None,
            },
        );

        instruction.accounts[account_idx] = if account_idx == 1 {
            AccountMeta::new(Pubkey::default(), false)
        } else {
            AccountMeta::new_readonly(Pubkey::default(), false)
        };

        harness.ctx.process_and_validate_instruction(
            &instruction,
            &[Check::err(ProgramError::InvalidSeeds)],
        );
    }
}
