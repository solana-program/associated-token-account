use {
    mollusk_svm::result::Check,
    solana_instruction::AccountMeta,
    solana_program_error::ProgramError,
    solana_program_pack::Pack,
    solana_pubkey::Pubkey,
    solana_rent::Rent,
    solana_system_interface::program as system_program,
    spl_associated_token_account_interface::address::get_associated_token_address_with_program_id,
    spl_associated_token_account_mollusk_harness::{
        build_create_ata_instruction, token_2022_immutable_owner_account_len,
        token_2022_immutable_owner_rent_exempt_balance, token_account_rent_exempt_balance,
        AccountBuilder, AtaTestHarness, CreateAtaInstructionType,
    },
    spl_token_interface::state::Mint,
    test_case::{test_case, test_matrix},
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
fn create_rejects_mismatch_derivation(token_program_id: Pubkey) {
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

fn instruction_type(idempotent: bool) -> CreateAtaInstructionType {
    if idempotent {
        CreateAtaInstructionType::CreateIdempotent { bump: None }
    } else {
        CreateAtaInstructionType::Create {
            bump: None,
            account_len: None,
        }
    }
}

#[test_matrix(
    [spl_token_interface::id(), spl_token_2022_interface::id()],
    [true, false]
)]
fn create_rejects_wrong_token_program_account_after_passing_seed_check(
    token_program_id: Pubkey,
    idempotent: bool,
) {
    let harness = AtaTestHarness::new(&token_program_id).with_wallet_and_mint(1_000_000, 6);
    let wallet = harness.wallet.unwrap();
    let mint = harness.mint.unwrap();
    let bogus_token_program = Pubkey::new_unique();
    let ata_address =
        get_associated_token_address_with_program_id(&wallet, &mint, &bogus_token_program);

    harness.ensure_account_exists_with_lamports(bogus_token_program, 1_000_000);

    let instruction = build_create_ata_instruction(
        spl_associated_token_account_interface::program::id(),
        harness.payer,
        ata_address,
        wallet,
        mint,
        bogus_token_program,
        instruction_type(idempotent),
    );

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[Check::err(ProgramError::IncorrectProgramId)],
    );
}

#[test_matrix(
    [spl_token_interface::id(), spl_token_2022_interface::id()],
    [true, false]
)]
fn create_rejects_invalid_mint_data(token_program_id: Pubkey, idempotent: bool) {
    let mut harness = AtaTestHarness::new(&token_program_id).with_wallet(1_000_000);
    let mint = Pubkey::new_unique();
    harness.ensure_account_exists_with_lamports(mint, Rent::default().minimum_balance(Mint::LEN));
    {
        let mut store = harness.ctx.account_store.borrow_mut();
        let mint_account = store.get_mut(&mint).unwrap();
        mint_account.owner = token_program_id;
        mint_account.data = vec![0; Mint::LEN - 1];
    }
    harness.mint = Some(mint);

    let instruction = harness.build_create_ata_instruction(instruction_type(idempotent));

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[Check::err(ProgramError::Custom(
            spl_token_2022_interface::error::TokenError::InvalidMint as u32,
        ))],
    );
}

#[test_matrix(
    [spl_token_interface::id(), spl_token_2022_interface::id()],
    [true, false]
)]
fn create_rejects_prefunded_initialized_system_account(token_program_id: Pubkey, idempotent: bool) {
    let harness = AtaTestHarness::new(&token_program_id).with_wallet_and_mint(1_000_000, 6);
    let wallet = harness.wallet.unwrap();
    let mint = harness.mint.unwrap();
    let ata_address =
        get_associated_token_address_with_program_id(&wallet, &mint, &token_program_id);

    let mut initialized_system_account =
        AccountBuilder::token_account(&mint, &wallet, 0, &token_program_id);
    initialized_system_account.owner = system_program::id();
    harness
        .ctx
        .account_store
        .borrow_mut()
        .insert(ata_address, initialized_system_account);

    let instruction = build_create_ata_instruction(
        spl_associated_token_account_interface::program::id(),
        harness.payer,
        ata_address,
        wallet,
        mint,
        token_program_id,
        instruction_type(idempotent),
    );

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[Check::err(ProgramError::Custom(
            spl_token_2022_interface::error::TokenError::NotRentExempt as u32,
        ))],
    );
}

#[test_matrix(
    [spl_token_interface::id(), spl_token_2022_interface::id()],
    [true, false]
)]
fn create_rejects_mint_not_owned_by_token_program(token_program_id: Pubkey, idempotent: bool) {
    let mut harness = AtaTestHarness::new(&token_program_id).with_wallet_and_mint(1_000_000, 6);
    let mint = harness.mint.unwrap();
    harness
        .ctx
        .account_store
        .borrow_mut()
        .get_mut(&mint)
        .unwrap()
        .owner = Pubkey::new_unique();

    let instruction = harness.build_create_ata_instruction(instruction_type(idempotent));

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[Check::err(ProgramError::IncorrectProgramId)],
    );
}

#[test_matrix(
    [spl_token_interface::id(), spl_token_2022_interface::id()],
    [true, false]
)]
fn create_accepts_fresh_account(token_program_id: Pubkey, idempotent: bool) {
    let harness = AtaTestHarness::new(&token_program_id).with_wallet_and_mint(1_000_000, 6);
    let wallet = harness.wallet.unwrap();
    let mint = harness.mint.unwrap();
    let ata_address =
        get_associated_token_address_with_program_id(&wallet, &mint, &token_program_id);

    let instruction = build_create_ata_instruction(
        spl_associated_token_account_interface::program::id(),
        harness.payer,
        ata_address,
        wallet,
        mint,
        token_program_id,
        instruction_type(idempotent),
    );

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[
            Check::success(),
            Check::account(&ata_address)
                .space(if token_program_id == spl_token_2022_interface::id() {
                    token_2022_immutable_owner_account_len()
                } else {
                    spl_token_interface::state::Account::LEN
                })
                .owner(&token_program_id)
                .lamports(if token_program_id == spl_token_2022_interface::id() {
                    token_2022_immutable_owner_rent_exempt_balance()
                } else {
                    token_account_rent_exempt_balance()
                })
                .build(),
        ],
    );
}

#[test_matrix(
    [spl_token_interface::id(), spl_token_2022_interface::id()],
    [true, false]
)]
fn create_accepts_prefunded_account_below_rent_exempt_minimum(
    token_program_id: Pubkey,
    idempotent: bool,
) {
    let harness = AtaTestHarness::new(&token_program_id).with_wallet_and_mint(1_000_000, 6);
    let wallet = harness.wallet.unwrap();
    let mint = harness.mint.unwrap();
    let ata_address =
        get_associated_token_address_with_program_id(&wallet, &mint, &token_program_id);

    let insufficient_lamports = 890880;
    harness.ensure_account_exists_with_lamports(ata_address, insufficient_lamports);

    let instruction = build_create_ata_instruction(
        spl_associated_token_account_interface::program::id(),
        harness.payer,
        ata_address,
        wallet,
        mint,
        token_program_id,
        instruction_type(idempotent),
    );

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[
            Check::success(),
            Check::account(&ata_address)
                .lamports(if token_program_id == spl_token_2022_interface::id() {
                    token_2022_immutable_owner_rent_exempt_balance()
                } else {
                    token_account_rent_exempt_balance()
                })
                .owner(&token_program_id)
                .build(),
        ],
    );
}

#[test_matrix(
    [spl_token_interface::id(), spl_token_2022_interface::id()],
    [true, false]
)]
fn create_accepts_prefunded_account_above_rent_exempt_minimum(
    token_program_id: Pubkey,
    idempotent: bool,
) {
    let harness = AtaTestHarness::new(&token_program_id).with_wallet_and_mint(1_000_000, 6);
    let wallet = harness.wallet.unwrap();
    let mint = harness.mint.unwrap();
    let ata_address =
        get_associated_token_address_with_program_id(&wallet, &mint, &token_program_id);

    let minimum_lamports = if token_program_id == spl_token_2022_interface::id() {
        token_2022_immutable_owner_rent_exempt_balance()
    } else {
        token_account_rent_exempt_balance()
    };
    let excess_lamports = minimum_lamports.saturating_add(1);
    harness.ensure_account_exists_with_lamports(ata_address, excess_lamports);

    let instruction = build_create_ata_instruction(
        spl_associated_token_account_interface::program::id(),
        harness.payer,
        ata_address,
        wallet,
        mint,
        token_program_id,
        instruction_type(idempotent),
    );

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[
            Check::success(),
            Check::account(&ata_address)
                .lamports(excess_lamports)
                .owner(&token_program_id)
                .build(),
        ],
    );
}

#[test_matrix(
    [spl_token_interface::id(), spl_token_2022_interface::id()],
    [true, false]
)]
fn create_rejects_wrong_system_program_account(token_program_id: Pubkey, idempotent: bool) {
    let mut harness = AtaTestHarness::new(&token_program_id).with_wallet_and_mint(1_000_000, 6);
    let bogus_system_program = Pubkey::new_unique();
    harness.ensure_account_exists_with_lamports(bogus_system_program, 1_000_000);

    let mut instruction = harness.build_create_ata_instruction(instruction_type(idempotent));
    instruction.accounts[4] = AccountMeta::new_readonly(bogus_system_program, false);

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[Check::err(ProgramError::NotEnoughAccountKeys)],
    );
}

//        agent crawl every CPI
