use {
    mollusk_svm::Mollusk,
    mollusk_svm_bencher::MolluskComputeUnitBencher,
    mollusk_svm_programs_token::{token, token2022},
    solana_account::Account,
    solana_address::Address,
    solana_instruction::{AccountMeta, Instruction},
    solana_program_option::COption,
    solana_program_pack::Pack,
    solana_system_interface::program as system_program,
    spl_associated_token_account_interface::{
        address::get_associated_token_address_with_program_id,
        instruction::{
            create_associated_token_account, create_associated_token_account_idempotent,
        },
        program::id as ata_program_id,
    },
    spl_token_interface::state::{Account as TokenAccount, AccountState, Mint},
    std::path::PathBuf,
};

fn with_rent_account(
    mut instruction: Instruction,
    mut accounts: Vec<(Address, Account)>,
    rent_sysvar: &(Address, Account),
) -> (Instruction, Vec<(Address, Account)>) {
    instruction
        .accounts
        .push(AccountMeta::new_readonly(rent_sysvar.0, false));
    accounts.push(rent_sysvar.clone());
    (instruction, accounts)
}

fn token_account(program_id: &Address, mint: Address, owner: Address, amount: u64) -> Account {
    let account = TokenAccount {
        mint,
        owner,
        amount,
        delegate: COption::None,
        state: AccountState::Initialized,
        is_native: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    };

    if program_id == &spl_token_interface::id() {
        token::create_account_for_token_account(account)
    } else {
        token2022::create_account_for_token_account(account)
    }
}

fn recover_nested_case(
    wallet: Address,
    owner_mint: Address,
    nested_mint: Address,
    owner_token_program_id: Address,
    nested_token_program_id: Address,
    spl_token_account: &(Address, Account),
    t22_account: &(Address, Account),
) -> (Instruction, Vec<(Address, Account)>) {
    let mint_data = Mint {
        mint_authority: COption::Some(Address::new_from_array([200; 32])),
        supply: 1_000_000,
        decimals: 6,
        is_initialized: true,
        freeze_authority: COption::None,
    };
    let owner_ata =
        get_associated_token_address_with_program_id(&wallet, &owner_mint, &owner_token_program_id);
    let dest_ata = get_associated_token_address_with_program_id(
        &wallet,
        &nested_mint,
        &nested_token_program_id,
    );
    let nested_ata = get_associated_token_address_with_program_id(
        &owner_ata,
        &nested_mint,
        &nested_token_program_id,
    );

    let mut accounts = vec![
        AccountMeta::new(nested_ata, false),
        AccountMeta::new_readonly(nested_mint, false),
        AccountMeta::new(dest_ata, false),
        AccountMeta::new_readonly(owner_ata, false),
        AccountMeta::new_readonly(owner_mint, false),
        AccountMeta::new(wallet, true),
        AccountMeta::new_readonly(owner_token_program_id, false),
    ];
    if owner_token_program_id != nested_token_program_id {
        accounts.push(AccountMeta::new_readonly(nested_token_program_id, false));
    }
    let ix = Instruction {
        program_id: ata_program_id(),
        accounts,
        data: vec![2u8],
    };

    let mut accs = vec![
        (
            nested_ata,
            token_account(&nested_token_program_id, nested_mint, owner_ata, 100),
        ),
        (
            nested_mint,
            if nested_token_program_id == spl_token_interface::id() {
                token::create_account_for_mint(mint_data)
            } else {
                token2022::create_account_for_mint(mint_data)
            },
        ),
        (
            dest_ata,
            token_account(&nested_token_program_id, nested_mint, wallet, 0),
        ),
        (
            owner_ata,
            token_account(&owner_token_program_id, owner_mint, wallet, 0),
        ),
        (
            owner_mint,
            if owner_token_program_id == spl_token_interface::id() {
                token::create_account_for_mint(mint_data)
            } else {
                token2022::create_account_for_mint(mint_data)
            },
        ),
        (wallet, Account::new(1_000_000, 0, &system_program::id())),
        if owner_token_program_id == spl_token_interface::id() {
            spl_token_account.clone()
        } else {
            t22_account.clone()
        },
    ];

    if owner_token_program_id != nested_token_program_id {
        accs.push(if nested_token_program_id == spl_token_interface::id() {
            spl_token_account.clone()
        } else {
            t22_account.clone()
        });
    }

    (ix, accs)
}

fn main() {
    solana_logger::setup_with("");

    let mut mollusk = Mollusk::new(
        &ata_program_id(),
        "pinocchio_associated_token_account_program",
    );
    token::add_program(&mut mollusk);

    // Load Token-2022 with batch instruction support
    let t22_elf_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../program/tests/fixtures/spl_token_2022.so");
    let t22_elf = mollusk_svm::file::read_file(t22_elf_path);
    mollusk.add_program_with_loader_and_elf(
        &spl_token_2022_interface::id(),
        &mollusk_svm::program::loader_keys::LOADER_V3,
        &t22_elf,
    );

    let payer = Address::new_unique();
    let mint_authority = Address::new_unique();
    let payer_account = Account::new(10_000_000_000, 0, &system_program::id());

    let system_account = mollusk_svm::program::keyed_account_for_system_program();
    let rent_sysvar = mollusk.sysvars.keyed_account_for_rent_sysvar();
    let spl_token_account = token::keyed_account();
    let t22_account = token2022::keyed_account();

    let mint_data = Mint {
        mint_authority: COption::Some(mint_authority),
        supply: 1_000_000,
        decimals: 6,
        is_initialized: true,
        freeze_authority: COption::None,
    };

    let token_mint = Address::new_unique();
    let token_mint_account = token::create_account_for_mint(mint_data);
    let t22_mint = Address::new_unique();
    let t22_mint_account = token2022::create_account_for_mint(mint_data);

    // Bench 1: create (spl-token)
    let wallet1 = Address::new_unique();
    let ata1 = get_associated_token_address_with_program_id(
        &wallet1,
        &token_mint,
        &spl_token_interface::id(),
    );
    let ix1 =
        create_associated_token_account(&payer, &wallet1, &token_mint, &spl_token_interface::id());
    let accs1 = vec![
        (payer, payer_account.clone()),
        (ata1, Account::default()),
        (wallet1, Account::new(1_000_000, 0, &system_program::id())),
        (token_mint, token_mint_account.clone()),
        system_account.clone(),
        spl_token_account.clone(),
    ];
    let (ix1_rent, accs1_rent) = with_rent_account(ix1.clone(), accs1.clone(), &rent_sysvar);

    // Bench 2: create (token-2022)
    let wallet2 = Address::new_unique();
    let ata2 = get_associated_token_address_with_program_id(
        &wallet2,
        &t22_mint,
        &spl_token_2022_interface::id(),
    );
    let ix2 = create_associated_token_account(
        &payer,
        &wallet2,
        &t22_mint,
        &spl_token_2022_interface::id(),
    );
    let accs2 = vec![
        (payer, payer_account.clone()),
        (ata2, Account::default()),
        (wallet2, Account::new(1_000_000, 0, &system_program::id())),
        (t22_mint, t22_mint_account.clone()),
        system_account.clone(),
        t22_account.clone(),
    ];
    let (ix2_rent, accs2_rent) = with_rent_account(ix2.clone(), accs2.clone(), &rent_sysvar);

    // Bench 3: create_idempotent (new, spl-token)
    let wallet3 = Address::new_unique();
    let ata3 = get_associated_token_address_with_program_id(
        &wallet3,
        &token_mint,
        &spl_token_interface::id(),
    );
    let ix3 = create_associated_token_account_idempotent(
        &payer,
        &wallet3,
        &token_mint,
        &spl_token_interface::id(),
    );
    let accs3 = vec![
        (payer, payer_account.clone()),
        (ata3, Account::default()),
        (wallet3, Account::new(1_000_000, 0, &system_program::id())),
        (token_mint, token_mint_account.clone()),
        system_account.clone(),
        spl_token_account.clone(),
    ];
    let (ix3_rent, accs3_rent) = with_rent_account(ix3.clone(), accs3.clone(), &rent_sysvar);

    // Bench 4: create_idempotent (new, token-2022)
    let wallet3b = Address::new_unique();
    let ata3b = get_associated_token_address_with_program_id(
        &wallet3b,
        &t22_mint,
        &spl_token_2022_interface::id(),
    );
    let ix3b = create_associated_token_account_idempotent(
        &payer,
        &wallet3b,
        &t22_mint,
        &spl_token_2022_interface::id(),
    );
    let accs3b = vec![
        (payer, payer_account.clone()),
        (ata3b, Account::default()),
        (wallet3b, Account::new(1_000_000, 0, &system_program::id())),
        (t22_mint, t22_mint_account.clone()),
        system_account.clone(),
        t22_account.clone(),
    ];
    let (ix3b_rent, accs3b_rent) = with_rent_account(ix3b.clone(), accs3b.clone(), &rent_sysvar);

    // Bench 5: create_idempotent (existing, spl-token)
    let wallet4 = Address::new_unique();
    let ata4 = get_associated_token_address_with_program_id(
        &wallet4,
        &token_mint,
        &spl_token_interface::id(),
    );
    let existing_ata = token::create_account_for_token_account(TokenAccount {
        mint: token_mint,
        owner: wallet4,
        amount: 0,
        delegate: COption::None,
        state: AccountState::Initialized,
        is_native: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    });
    let ix4 = create_associated_token_account_idempotent(
        &payer,
        &wallet4,
        &token_mint,
        &spl_token_interface::id(),
    );
    let accs4 = vec![
        (payer, payer_account.clone()),
        (ata4, existing_ata),
        (wallet4, Account::new(1_000_000, 0, &system_program::id())),
        (token_mint, token_mint_account.clone()),
        system_account.clone(),
        spl_token_account.clone(),
    ];

    // Bench 6: create_idempotent (existing, token-2022)
    let wallet4b = Address::new_unique();
    let ata4b = get_associated_token_address_with_program_id(
        &wallet4b,
        &t22_mint,
        &spl_token_2022_interface::id(),
    );
    let existing_ata_t22 = token2022::create_account_for_token_account(TokenAccount {
        mint: t22_mint,
        owner: wallet4b,
        amount: 0,
        delegate: COption::None,
        state: AccountState::Initialized,
        is_native: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    });
    let ix4b = create_associated_token_account_idempotent(
        &payer,
        &wallet4b,
        &t22_mint,
        &spl_token_2022_interface::id(),
    );
    let accs4b = vec![
        (payer, payer_account.clone()),
        (ata4b, existing_ata_t22),
        (wallet4b, Account::new(1_000_000, 0, &system_program::id())),
        (t22_mint, t22_mint_account.clone()),
        system_account.clone(),
        t22_account.clone(),
    ];

    // Bench 7: create (prefunded, spl-token)
    let wallet5 = Address::new_unique();
    let ata5 = get_associated_token_address_with_program_id(
        &wallet5,
        &token_mint,
        &spl_token_interface::id(),
    );
    let spl_prefund_lamports = solana_rent::Rent::default()
        .minimum_balance(TokenAccount::LEN)
        .saturating_sub(1);
    let ix5 =
        create_associated_token_account(&payer, &wallet5, &token_mint, &spl_token_interface::id());
    let accs5 = vec![
        (payer, payer_account.clone()),
        (
            ata5,
            Account::new(spl_prefund_lamports, 0, &system_program::id()),
        ),
        (wallet5, Account::new(1_000_000, 0, &system_program::id())),
        (token_mint, token_mint_account.clone()),
        system_account.clone(),
        spl_token_account.clone(),
    ];
    let (ix5_rent, accs5_rent) = with_rent_account(ix5.clone(), accs5.clone(), &rent_sysvar);

    // Bench 8: create (prefunded, token-2022)
    let wallet5b = Address::new_unique();
    let ata5b = get_associated_token_address_with_program_id(
        &wallet5b,
        &t22_mint,
        &spl_token_2022_interface::id(),
    );
    let t22_prefund_lamports = solana_rent::Rent::default()
        .minimum_balance(
            spl_token_2022_interface::extension::ExtensionType::try_calculate_account_len::<
                spl_token_2022_interface::state::Account,
            >(&[spl_token_2022_interface::extension::ExtensionType::ImmutableOwner])
            .unwrap(),
        )
        .saturating_sub(1);
    let ix5b = create_associated_token_account(
        &payer,
        &wallet5b,
        &t22_mint,
        &spl_token_2022_interface::id(),
    );
    let accs5b = vec![
        (payer, payer_account.clone()),
        (
            ata5b,
            Account::new(t22_prefund_lamports, 0, &system_program::id()),
        ),
        (wallet5b, Account::new(1_000_000, 0, &system_program::id())),
        (t22_mint, t22_mint_account.clone()),
        system_account.clone(),
        t22_account.clone(),
    ];
    let (ix5b_rent, accs5b_rent) = with_rent_account(ix5b.clone(), accs5b.clone(), &rent_sysvar);

    // Benches 11-14: recover_nested
    let (ix6, accs6) = recover_nested_case(
        Address::new_from_array([1; 32]),
        Address::new_from_array([2; 32]),
        Address::new_from_array([3; 32]),
        spl_token_interface::id(),
        spl_token_interface::id(),
        &spl_token_account,
        &t22_account,
    );
    let (ix6b, accs6b) = recover_nested_case(
        Address::new_from_array([10; 32]),
        Address::new_from_array([11; 32]),
        Address::new_from_array([12; 32]),
        spl_token_2022_interface::id(),
        spl_token_2022_interface::id(),
        &spl_token_account,
        &t22_account,
    );
    let (ix6c, accs6c) = recover_nested_case(
        Address::new_from_array([4; 32]),
        Address::new_from_array([5; 32]),
        Address::new_from_array([6; 32]),
        spl_token_interface::id(),
        spl_token_2022_interface::id(),
        &spl_token_account,
        &t22_account,
    );
    let (ix6d, accs6d) = recover_nested_case(
        Address::new_from_array([7; 32]),
        Address::new_from_array([8; 32]),
        Address::new_from_array([9; 32]),
        spl_token_2022_interface::id(),
        spl_token_interface::id(),
        &spl_token_account,
        &t22_account,
    );

    MolluskComputeUnitBencher::new(mollusk)
        .bench(("create (spl-token)", &ix1, &accs1))
        .bench((
            "create (spl-token, w/ rent account)",
            &ix1_rent,
            &accs1_rent,
        ))
        .bench(("create (token-2022)", &ix2, &accs2))
        .bench((
            "create (token-2022, w/ rent account)",
            &ix2_rent,
            &accs2_rent,
        ))
        .bench(("create_idempotent (new, spl-token)", &ix3, &accs3))
        .bench((
            "create_idempotent (new, spl-token, w/ rent account)",
            &ix3_rent,
            &accs3_rent,
        ))
        .bench(("create_idempotent (new, token-2022)", &ix3b, &accs3b))
        .bench((
            "create_idempotent (new, token-2022, w/ rent account)",
            &ix3b_rent,
            &accs3b_rent,
        ))
        .bench(("create_idempotent (existing, spl-token)", &ix4, &accs4))
        .bench(("create_idempotent (existing, token-2022)", &ix4b, &accs4b))
        .bench(("create (prefunded, spl-token)", &ix5, &accs5))
        .bench((
            "create (prefunded, spl-token, w/ rent account)",
            &ix5_rent,
            &accs5_rent,
        ))
        .bench(("create (prefunded, token-2022)", &ix5b, &accs5b))
        .bench((
            "create (prefunded, token-2022, w/ rent account)",
            &ix5b_rent,
            &accs5b_rent,
        ))
        .bench((
            "recover_nested (owner=spl-token, nested=spl-token)",
            &ix6,
            &accs6,
        ))
        .bench((
            "recover_nested (owner=token-2022, nested=token-2022)",
            &ix6b,
            &accs6b,
        ))
        .bench((
            "recover_nested (owner=spl-token, nested=token-2022)",
            &ix6c,
            &accs6c,
        ))
        .bench((
            "recover_nested (owner=token-2022, nested=spl-token)",
            &ix6d,
            &accs6d,
        ))
        .must_pass(true)
        .execute();
}
