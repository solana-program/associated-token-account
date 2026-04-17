use {
    mollusk_svm::result::Check,
    solana_instruction::AccountMeta,
    solana_program_error::ProgramError,
    solana_program_pack::Pack,
    solana_pubkey::Pubkey,
    solana_rent::Rent,
    spl_associated_token_account_interface::address::get_associated_token_address_with_program_id,
    spl_associated_token_account_mollusk_harness::{
        AtaTestHarness, CreateAtaInstructionType, build_create_ata_instruction,
        token_2022_immutable_owner_account_len, token_2022_immutable_owner_rent_exempt_balance,
        token_account_rent_exempt_balance,
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
            rent_sysvar_via_account: false,
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
                rent_sysvar_via_account: false,
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

fn instruction_type(idempotent: bool, rent_sysvar_via_account: bool) -> CreateAtaInstructionType {
    if idempotent {
        CreateAtaInstructionType::CreateIdempotent {
            bump: None,
            rent_sysvar_via_account,
        }
    } else {
        CreateAtaInstructionType::Create {
            bump: None,
            account_len: None,
            rent_sysvar_via_account,
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
        instruction_type(idempotent, false),
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
    let mut harness = AtaTestHarness::new(&token_program_id)
        .with_wallet(1_000_000)
        .with_raw_mint(
            token_program_id,
            Rent::default().minimum_balance(Mint::LEN),
            vec![0; Mint::LEN - 1],
        );
    let instruction = harness.build_create_ata_instruction(instruction_type(idempotent, false));

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

    let instruction = harness.build_create_ata_instruction(instruction_type(idempotent, false));

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[Check::err(ProgramError::IncorrectProgramId)],
    );
}

#[test_matrix(
    [spl_token_interface::id(), spl_token_2022_interface::id()],
    [true, false],
    [true, false]
)]
fn create_accepts_fresh_account(
    token_program_id: Pubkey,
    idempotent: bool,
    rent_sysvar_via_account: bool,
) {
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
        instruction_type(idempotent, rent_sysvar_via_account),
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
    [true, false],
    [true, false]
)]
fn create_accepts_prefunded_account_below_rent_exempt_minimum(
    token_program_id: Pubkey,
    idempotent: bool,
    rent_sysvar_via_account: bool,
) {
    let harness = AtaTestHarness::new(&token_program_id).with_wallet_and_mint(1_000_000, 6);
    let wallet = harness.wallet.unwrap();
    let mint = harness.mint.unwrap();
    let ata_address =
        get_associated_token_address_with_program_id(&wallet, &mint, &token_program_id);

    let insufficient_lamports = if token_program_id == spl_token_2022_interface::id() {
        token_2022_immutable_owner_rent_exempt_balance()
    } else {
        token_account_rent_exempt_balance()
    }
    .saturating_sub(1);
    harness.ensure_account_exists_with_lamports(ata_address, insufficient_lamports);

    let instruction = build_create_ata_instruction(
        spl_associated_token_account_interface::program::id(),
        harness.payer,
        ata_address,
        wallet,
        mint,
        token_program_id,
        instruction_type(idempotent, rent_sysvar_via_account),
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
    [true, false],
    [true, false]
)]
fn create_accepts_prefunded_account_above_rent_exempt_minimum(
    token_program_id: Pubkey,
    idempotent: bool,
    rent_sysvar_via_account: bool,
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
        instruction_type(idempotent, rent_sysvar_via_account),
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
    [true, false],
    [true, false]
)]
fn create_fails_cpi_with_invalid_system_program_account(
    token_program_id: Pubkey,
    idempotent: bool,
    rent_sysvar_via_account: bool,
) {
    let mut harness = AtaTestHarness::new(&token_program_id).with_wallet_and_mint(1_000_000, 6);
    let bogus_system_program = Pubkey::new_unique();
    harness.ensure_account_exists_with_lamports(bogus_system_program, 1_000_000);

    let mut instruction =
        harness.build_create_ata_instruction(instruction_type(idempotent, rent_sysvar_via_account));
    instruction.accounts[4] = AccountMeta::new_readonly(bogus_system_program, false);

    // The runtime returns `NotEnoughAccountKeys` when the CPI target (system program) is
    // missing from the transaction's account list.
    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[Check::err(ProgramError::NotEnoughAccountKeys)],
    );
}
