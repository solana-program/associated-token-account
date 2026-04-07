use {
    mollusk_svm::Mollusk,
    mollusk_svm_bencher::MolluskComputeUnitBencher,
    mollusk_svm_programs_token::{token, token2022},
    solana_account::Account,
    solana_program_option::COption,
    solana_program_pack::Pack,
    solana_pubkey::Pubkey,
    solana_system_interface::program as system_program,
    spl_associated_token_account_interface::{
        address::get_associated_token_address_with_program_id,
        instruction::{
            create_associated_token_account, create_associated_token_account_idempotent,
            recover_nested,
        },
        program::id as ata_program_id,
    },
    spl_token_interface::state::{Account as TokenAccount, AccountState, Mint},
};

fn main() {
    solana_logger::setup_with("");

    let mut mollusk = Mollusk::new(
        &ata_program_id(),
        "pinocchio_associated_token_account_program",
    );
    token::add_program(&mut mollusk);
    token2022::add_program(&mut mollusk);

    let payer = Pubkey::new_unique();
    let mint_authority = Pubkey::new_unique();
    let payer_account = Account::new(10_000_000_000, 0, &system_program::id());

    let system_account = mollusk_svm::program::keyed_account_for_system_program();
    let spl_token_account = token::keyed_account();
    let t22_account = token2022::keyed_account();

    let mint_data = Mint {
        mint_authority: COption::Some(mint_authority),
        supply: 1_000_000,
        decimals: 6,
        is_initialized: true,
        freeze_authority: COption::None,
    };

    let token_mint = Pubkey::new_unique();
    let token_mint_account = token::create_account_for_mint(mint_data);
    let t22_mint = Pubkey::new_unique();
    let t22_mint_account = token2022::create_account_for_mint(mint_data);

    // Bench 1: create (spl-token)
    let wallet1 = Pubkey::new_unique();
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

    // Bench 2: create (token-2022)
    let wallet2 = Pubkey::new_unique();
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

    // Bench 3: create_idempotent (new, spl-token)
    let wallet3 = Pubkey::new_unique();
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

    // Bench 4: create_idempotent (new, token-2022)
    let wallet3b = Pubkey::new_unique();
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

    // Bench 5: create_idempotent (existing, spl-token)
    let wallet4 = Pubkey::new_unique();
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
    let wallet4b = Pubkey::new_unique();
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
    let wallet5 = Pubkey::new_unique();
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

    // Bench 8: create (prefunded, token-2022)
    let wallet5b = Pubkey::new_unique();
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

    // Bench 9: recover_nested
    let recover_wallet = Pubkey::new_unique();
    let owner_mint = Pubkey::new_unique();
    let nested_mint = Pubkey::new_unique();

    let owner_mint_account = token::create_account_for_mint(mint_data);
    let nested_mint_account = token::create_account_for_mint(mint_data);

    let owner_ata = get_associated_token_address_with_program_id(
        &recover_wallet,
        &owner_mint,
        &spl_token_interface::id(),
    );
    let dest_ata = get_associated_token_address_with_program_id(
        &recover_wallet,
        &nested_mint,
        &spl_token_interface::id(),
    );
    let nested_ata = get_associated_token_address_with_program_id(
        &owner_ata,
        &nested_mint,
        &spl_token_interface::id(),
    );

    let owner_ata_account = token::create_account_for_token_account(TokenAccount {
        mint: owner_mint,
        owner: recover_wallet,
        amount: 0,
        delegate: COption::None,
        state: AccountState::Initialized,
        is_native: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    });
    let dest_ata_account = token::create_account_for_token_account(TokenAccount {
        mint: nested_mint,
        owner: recover_wallet,
        amount: 0,
        delegate: COption::None,
        state: AccountState::Initialized,
        is_native: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    });
    let nested_ata_account = token::create_account_for_token_account(TokenAccount {
        mint: nested_mint,
        owner: owner_ata,
        amount: 100,
        delegate: COption::None,
        state: AccountState::Initialized,
        is_native: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    });

    let ix6 = recover_nested(
        &recover_wallet,
        &owner_mint,
        &nested_mint,
        &spl_token_interface::id(),
    );
    let accs6 = vec![
        (nested_ata, nested_ata_account),
        (nested_mint, nested_mint_account),
        (dest_ata, dest_ata_account),
        (owner_ata, owner_ata_account),
        (owner_mint, owner_mint_account),
        (
            recover_wallet,
            Account::new(1_000_000, 0, &system_program::id()),
        ),
        spl_token_account.clone(),
    ];

    MolluskComputeUnitBencher::new(mollusk)
        .bench(("create (spl-token)", &ix1, &accs1))
        .bench(("create (token-2022)", &ix2, &accs2))
        .bench(("create_idempotent (new, spl-token)", &ix3, &accs3))
        .bench(("create_idempotent (new, token-2022)", &ix3b, &accs3b))
        .bench(("create_idempotent (existing, spl-token)", &ix4, &accs4))
        .bench(("create_idempotent (existing, token-2022)", &ix4b, &accs4b))
        .bench(("create (prefunded, spl-token)", &ix5, &accs5))
        .bench(("create (prefunded, token-2022)", &ix5b, &accs5b))
        .bench(("recover_nested", &ix6, &accs6))
        .must_pass(true)
        .execute();
}
