#![cfg(feature = "test-bpf")]

use {
    mollusk_svm::{program::loader_keys::LOADER_V4, Mollusk},
    mollusk_svm_bencher::MolluskComputeUnitBencher,
    solana_logger,
    solana_sdk::{
        account::Account,
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
        system_program,
        sysvar,
    },
    spl_token_interface::{
        program::ID as TOKEN_PROGRAM_ID_BYTES,
        state::{account::Account as TokenAccount, mint::Mint, Transmutable},
    },
};

/// Build a zero-rent `Rent` sysvar account with correctly sized data buffer.
fn rent_sysvar_account() -> Account {
    Account {
        lamports: 0,
        data: Vec::new(), // Rent sysvar data not inspected in program logic
        owner: sysvar::rent::id(),
        executable: false,
        rent_epoch: 0,
    }
}

/// Build raw token Account data with the supplied mint / owner / amount.
fn build_token_account_data(mint: &Pubkey, owner: &Pubkey, amount: u64) -> Vec<u8> {
    let mut data = vec![0u8; TokenAccount::LEN];

    // Offsets based on token Account layout (see interface/src/state/account.rs)
    // mint:   0..32
    data[0..32].copy_from_slice(mint.as_ref());
    // owner:  32..64
    data[32..64].copy_from_slice(owner.as_ref());
    // amount: 64..72 (u64 LE)
    data[64..72].copy_from_slice(&amount.to_le_bytes());
    // state enum byte after delegate COption (32+32+8+36 = 108)
    data[108] = 1; // Initialized

    data
}

/// Build mint data with given decimals and marked initialized.
fn build_mint_data(decimals: u8) -> Vec<u8> {
    let mut data = vec![0u8; Mint::LEN];
    // decimals offset: COption<Pubkey>(36) + supply(8) = 44
    data[44] = decimals;
    data[45] = 1; // is_initialized = true
    data
}

fn main() {
    // Disable noisy logs in output.
    let _ = solana_logger::setup_with("");

    // Tell Mollusk where to locate the compiled SBF program ELF so it can be loaded.
    // Resolve relative to the project root (CARGO_MANIFEST_DIR).
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    std::env::set_var(
        "SBF_OUT_DIR",
        format!("{}/target/sbpf-solana-solana/release", manifest_dir),
    );

    // Program id & Mollusk harness – assumes compiled .so is at target/deploy/pinocchio_ata_program.so
    let program_id = Pubkey::new_unique();

    // Token program id as Pubkey (convert from interface constant bytes)
    let token_program_id = Pubkey::new_from_array(TOKEN_PROGRAM_ID_BYTES);

    /* ------------------------------- CREATE -------------------------------- */
    let payer = Pubkey::new_unique();
    let wallet = Pubkey::new_unique();
    let mint = Pubkey::new_unique();

    // Derived Associated Token Account (wallet + mint)
    let (ata, _bump) = Pubkey::find_program_address(
        &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
        &program_id,
    );

    // Account list (see processor::process_create docs)
    let accounts_create = vec![
        // payer
        (
            payer,
            Account::new(1_000_000_000, 0, &system_program::id()),
        ),
        // ata (PDA, uninitialized)
        (ata, Account::new(0, 0, &system_program::id())),
        // wallet
        (wallet, Account::new(0, 0, &system_program::id())),
        // mint
        (
            mint,
            Account {
                lamports: 1_000_000_000,
                data: build_mint_data(0),
                owner: token_program_id,
                executable: false,
                rent_epoch: 0,
            },
        ),
        // system program (dummy)
        (
            system_program::id(),
            Account::new(0, 0, &system_program::id()),
        ),
        // token program (marked executable true so invoke succeeds)
        (
            token_program_id,
            Account {
                lamports: 0,
                data: Vec::new(),
                owner: token_program_id,
                executable: true,
                rent_epoch: 0,
            },
        ),
        // rent sysvar
        (sysvar::rent::id(), rent_sysvar_account()),
    ];

    let create_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer, true),
            AccountMeta::new(ata, false),
            AccountMeta::new_readonly(wallet, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new_readonly(token_program_id, false),
            AccountMeta::new_readonly(sysvar::rent::id(), false),
        ],
        data: vec![], // 0 => Create
    };

    /* ------------------------------ RECOVER ------------------------------- */
    let owner_mint = Pubkey::new_unique();
    let nested_mint = Pubkey::new_unique();

    let (owner_ata, owner_bump) = Pubkey::find_program_address(
        &[wallet.as_ref(), token_program_id.as_ref(), owner_mint.as_ref()],
        &program_id,
    );
    let (nested_ata, _nested_bump) = Pubkey::find_program_address(
        &[owner_ata.as_ref(), token_program_id.as_ref(), nested_mint.as_ref()],
        &program_id,
    );
    let (dest_ata, _dest_bump) = Pubkey::find_program_address(
        &[wallet.as_ref(), token_program_id.as_ref(), nested_mint.as_ref()],
        &program_id,
    );

    let accounts_recover = vec![
        // nested_ata – holds tokens owned by owner_ata
        (
            nested_ata,
            Account {
                lamports: 1_000_000_000,
                data: build_token_account_data(&nested_mint, &owner_ata, 100),
                owner: token_program_id,
                executable: false,
                rent_epoch: 0,
            },
        ),
        // nested_mint
        (
            nested_mint,
            Account {
                lamports: 1_000_000_000,
                data: build_mint_data(0),
                owner: token_program_id,
                executable: false,
                rent_epoch: 0,
            },
        ),
        // dest_ata – wallet's ATA for nested_mint (starts empty)
        (
            dest_ata,
            Account {
                lamports: 1_000_000_000,
                data: build_token_account_data(&nested_mint, &wallet, 0),
                owner: token_program_id,
                executable: false,
                rent_epoch: 0,
            },
        ),
        // owner_ata – wallet's ATA for owner_mint (owner of nested_ata)
        (
            owner_ata,
            Account {
                lamports: 1_000_000_000,
                data: build_token_account_data(&owner_mint, &wallet, 0),
                owner: token_program_id,
                executable: false,
                rent_epoch: 0,
            },
        ),
        // owner_mint
        (
            owner_mint,
            Account {
                lamports: 1_000_000_000,
                data: build_mint_data(0),
                owner: token_program_id,
                executable: false,
                rent_epoch: 0,
            },
        ),
        // wallet (signer)
        (wallet, Account::new(1_000_000_000, 0, &system_program::id())),
        // token program (executable)
        (
            token_program_id,
            Account {
                lamports: 0,
                data: Vec::new(),
                owner: token_program_id,
                executable: true,
                rent_epoch: 0,
            },
        ),
    ];

    let recover_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(nested_ata, false),
            AccountMeta::new_readonly(nested_mint, false),
            AccountMeta::new(dest_ata, false),
            AccountMeta::new(owner_ata, false),
            AccountMeta::new_readonly(owner_mint, false),
            AccountMeta::new_readonly(wallet, true),
            AccountMeta::new_readonly(token_program_id, false),
        ],
        data: vec![2u8], // 2 => RecoverNested
    };

    /* ------------------------------ BENCH -------------------------------- */
    // Start with a Mollusk instance that already contains the common builtin programs
    let mut mollusk = Mollusk::default();
    // Add our program under test (p-ata)
    mollusk.add_program(&program_id, "pinocchio_ata_program", &LOADER_V4);
    // Add the compiled Pinocchio token program so CPIs execute successfully.
    mollusk.add_program(&token_program_id, "pinocchio_token", &LOADER_V4);

    MolluskComputeUnitBencher::new(mollusk)
        .bench(("create", &create_ix, &accounts_create[..]))
        .bench(("recover", &recover_ix, &accounts_recover[..]))
        .must_pass(true)
        .out_dir("../target/benches")
        .execute();

    // Prevent "function never used" warnings for the bumps (they're needed for seed calc correctness)
    let _ = owner_bump;
} 