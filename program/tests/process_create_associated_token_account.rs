use {
    ata_mollusk_harness::{
        build_create_ata_instruction, token_2022_immutable_owner_rent_exempt_balance,
        AtaTestHarness, CreateAtaInstructionType,
    },
    mollusk_svm::result::Check,
    solana_instruction::AccountMeta,
    solana_program_error::ProgramError,
    solana_pubkey::Pubkey,
    solana_sysvar as sysvar,
    spl_associated_token_account_interface::address::get_associated_token_address_with_program_id,
};

#[test]
fn test_associated_token_address() {
    let mut harness =
        AtaTestHarness::new(&spl_token_2022_interface::id()).with_wallet_and_mint(1_000_000, 6);
    harness.create_ata(CreateAtaInstructionType::default());
}

#[test]
fn test_create_with_fewer_lamports() {
    let harness =
        AtaTestHarness::new(&spl_token_2022_interface::id()).with_wallet_and_mint(1_000_000, 6);

    let wallet = harness.wallet.unwrap();
    let mint = harness.mint.unwrap();
    let ata_address = get_associated_token_address_with_program_id(
        &wallet,
        &mint,
        &spl_token_2022_interface::id(),
    );

    let insufficient_lamports = 890880;
    harness.ensure_account_exists_with_lamports(ata_address, insufficient_lamports);

    let instruction = build_create_ata_instruction(
        spl_associated_token_account_interface::program::id(),
        harness.payer,
        ata_address,
        wallet,
        mint,
        spl_token_2022_interface::id(),
        CreateAtaInstructionType::default(),
    );

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[
            Check::success(),
            Check::account(&ata_address)
                .lamports(token_2022_immutable_owner_rent_exempt_balance())
                .owner(&spl_token_2022_interface::id())
                .build(),
        ],
    );
}

#[test]
fn test_create_with_excess_lamports() {
    let harness =
        AtaTestHarness::new(&spl_token_2022_interface::id()).with_wallet_and_mint(1_000_000, 6);

    let wallet = harness.wallet.unwrap();
    let mint = harness.mint.unwrap();
    let ata_address = get_associated_token_address_with_program_id(
        &wallet,
        &mint,
        &spl_token_2022_interface::id(),
    );

    let excess_lamports = token_2022_immutable_owner_rent_exempt_balance() + 1;
    harness.ensure_account_exists_with_lamports(ata_address, excess_lamports);

    let instruction = build_create_ata_instruction(
        spl_associated_token_account_interface::program::id(),
        harness.payer,
        ata_address,
        wallet,
        mint,
        spl_token_2022_interface::id(),
        CreateAtaInstructionType::default(),
    );

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[
            Check::success(),
            Check::account(&ata_address)
                .lamports(excess_lamports)
                .owner(&spl_token_2022_interface::id())
                .build(),
        ],
    );
}

#[test]
fn test_create_account_mismatch() {
    let harness =
        AtaTestHarness::new(&spl_token_2022_interface::id()).with_wallet_and_mint(1_000_000, 6);

    let wallet = harness.wallet.unwrap();
    let mint = harness.mint.unwrap();
    let ata_address = get_associated_token_address_with_program_id(
        &wallet,
        &mint,
        &spl_token_2022_interface::id(),
    );

    for account_idx in [1, 2, 3] {
        let mut instruction = build_create_ata_instruction(
            spl_associated_token_account_interface::program::id(),
            harness.payer,
            ata_address,
            wallet,
            mint,
            spl_token_2022_interface::id(),
            CreateAtaInstructionType::default(),
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

#[test]
fn test_create_associated_token_account_using_legacy_implicit_instruction() {
    let mut harness =
        AtaTestHarness::new(&spl_token_2022_interface::id()).with_wallet_and_mint(1_000_000, 6);

    harness.create_and_check_ata_with_custom_instruction(
        CreateAtaInstructionType::default(),
        |instruction| {
            instruction.data = vec![];
            instruction
                .accounts
                .push(AccountMeta::new_readonly(sysvar::rent::id(), false));
        },
    );
}
