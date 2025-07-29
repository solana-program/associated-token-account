use {
    pinocchio::pubkey::Pubkey,
    pinocchio_pubkey::pubkey,
    spl_token_2022::{
        extension::{
            default_account_state::DefaultAccountState, group_pointer::GroupPointer,
            interest_bearing_mint::InterestBearingConfig, metadata_pointer::MetadataPointer,
            mint_close_authority::MintCloseAuthority, non_transferable::NonTransferable,
            pausable::PausableConfig, permanent_delegate::PermanentDelegate,
            transfer_fee::TransferFeeConfig, transfer_hook::TransferHook, ExtensionType,
            PodStateWithExtensionsMut,
        },
        pod::PodMint,
    },
    spl_token_group_interface::state::{TokenGroup, TokenGroupMember},
    spl_token_interface::state::{
        account::Account as TokenAccount, multisig::Multisig, Transmutable,
    },
    spl_token_metadata_interface::state::TokenMetadata,
};

use std::{string::String, vec, vec::Vec};

pub const SPL_TOKEN_PROGRAM_ID: Pubkey = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");

// Shared constants for mollusk testing
pub const NATIVE_LOADER_ID: SolanaPubkey = SolanaPubkey::new_from_array([
    5, 135, 132, 191, 20, 139, 164, 40, 47, 176, 18, 87, 72, 136, 169, 241, 83, 160, 125, 173, 247,
    101, 192, 69, 92, 154, 151, 3, 128, 0, 0, 0,
]);

/// Matches the pinocchio Account struct.
/// Account fields are private, so this struct allows more readable
/// use of them in tests.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AccountLayout {
    pub borrow_state: u8,
    pub is_signer: u8,
    pub is_writable: u8,
    pub executable: u8,
    pub resize_delta: i32,
    pub key: Pubkey,
    pub owner: Pubkey,
    pub lamports: u64,
    pub data_len: u64,
}

// ---- Shared Mollusk Test Utilities ----

#[cfg(test)]
use {
    mollusk_svm::{program::loader_keys::LOADER_V3, result::Check, Mollusk},
    solana_instruction::{AccountMeta, Instruction},
    solana_pubkey::Pubkey as SolanaPubkey,
    solana_sdk::{account::Account, signature::Keypair, signer::Signer, sysvar},
    solana_system_interface::{instruction as system_instruction, program as system_program},
};

/// Common mollusk setup with ATA program and token program
#[cfg(test)]
pub fn setup_mollusk_with_programs(token_program_id: &SolanaPubkey) -> Mollusk {
    let ata_program_id = spl_associated_token_account::id();
    let mut mollusk = Mollusk::default();

    mollusk.add_program(
        &ata_program_id,
        "target/deploy/pinocchio_ata_program",
        &LOADER_V3,
    );

    let program_path = if *token_program_id == spl_token_2022::id() {
        "programs/token-2022/target/deploy/spl_token_2022"
    } else {
        "programs/token/target/deploy/pinocchio_token_program"
    };

    mollusk.add_program(token_program_id, program_path, &LOADER_V3);
    mollusk
}

/// Create standard base accounts needed for mollusk tests
#[cfg(test)]
pub fn create_mollusk_base_accounts(payer: &Keypair) -> Vec<(SolanaPubkey, Account)> {
    [
        (
            payer.pubkey(),
            Account::new(10_000_000_000, 0, &system_program::id()),
        ),
        (
            system_program::id(),
            Account {
                lamports: 0,
                data: Vec::new(),
                owner: NATIVE_LOADER_ID,
                executable: true,
                rent_epoch: 0,
            },
        ),
        // Properly initialize the Rent sysvar with realistic parameters instead of an all-zero placeholder.
        {
            // Use the same default rent values that Mollusk exposes so tests use the exact same
            // parameters as the program logic. This prevents mismatches when calculating the
            // minimum balance required for rent-exemption.
            use solana_sdk::rent::Rent;

            let rent = Rent::default();
            let rent_data = create_rent_data(
                rent.lamports_per_byte_year,
                rent.exemption_threshold,
                rent.burn_percent,
            );

            (
                sysvar::rent::id(),
                Account {
                    lamports: 0,
                    data: rent_data,
                    owner: sysvar::id(),
                    executable: false,
                    rent_epoch: 0,
                },
            )
        },
    ]
    .to_vec()
}

/// Create standard base accounts with token program
#[cfg(test)]
pub fn create_mollusk_base_accounts_with_token(
    payer: &Keypair,
    token_program_id: &SolanaPubkey,
) -> Vec<(SolanaPubkey, Account)> {
    let mut accounts = create_mollusk_base_accounts(payer);

    accounts.push((
        *token_program_id,
        Account {
            lamports: 0,
            data: Vec::new(),
            owner: LOADER_V3,
            executable: true,
            rent_epoch: 0,
        },
    ));

    accounts
}

/// Create standard base accounts with token program and wallet
#[cfg(test)]
pub fn create_mollusk_base_accounts_with_token_and_wallet(
    payer: &Keypair,
    wallet: &SolanaPubkey,
    token_program_id: &SolanaPubkey,
) -> Vec<(SolanaPubkey, Account)> {
    // Start with the standard base accounts (payer, system program, rent sysvar, token program)
    let mut accounts = create_mollusk_base_accounts_with_token(payer, token_program_id);

    // Add the wallet account with zero lamports, owned by the system program. This is
    // frequently required by tests that reference the wallet but previously had to push it
    // manually.
    accounts.push((*wallet, Account::new(0, 0, &system_program::id())));

    accounts
}

/// The type of ATA creation instruction to build.
pub enum CreateAtaInstructionType {
    /// The standard `Create` instruction, which can optionally include a bump seed and account length.
    Create {
        bump: Option<u8>,
        account_len: Option<u16>,
    },
    /// The `CreateIdempotent` instruction.
    CreateIdempotent,
}

/// Build a create associated token account instruction with a given discriminator
#[cfg(test)]
pub fn build_create_ata_instruction(
    ata_program_id: SolanaPubkey,
    payer: SolanaPubkey,
    ata_address: SolanaPubkey,
    wallet: SolanaPubkey,
    mint: SolanaPubkey,
    token_program: SolanaPubkey,
    instruction_type: CreateAtaInstructionType,
) -> Instruction {
    let accounts = [
        AccountMeta::new(payer, true),
        AccountMeta::new(ata_address, false),
        AccountMeta::new_readonly(wallet, false),
        AccountMeta::new_readonly(mint, false),
        AccountMeta::new_readonly(system_program::id(), false),
        AccountMeta::new_readonly(token_program, false),
        AccountMeta::new_readonly(sysvar::rent::id(), false),
    ];

    let data = match instruction_type {
        CreateAtaInstructionType::Create { bump, account_len } => {
            let mut data = vec![0]; // Discriminator for Create
            if let Some(b) = bump {
                data.push(b);
                if let Some(len) = account_len {
                    data.extend_from_slice(&len.to_le_bytes());
                }
            }
            data
        }
        CreateAtaInstructionType::CreateIdempotent => vec![1],
    };

    Instruction {
        program_id: ata_program_id,
        accounts: accounts.to_vec(),
        data,
    }
}

/// Create valid token account data for mollusk testing (solana SDK compatible)
#[cfg(test)]
pub fn create_mollusk_token_account_data(
    mint: &SolanaPubkey,
    owner: &SolanaPubkey,
    amount: u64,
) -> Vec<u8> {
    const TOKEN_ACCOUNT_SIZE: usize = 165; // SPL Token account size (no extensions)
    let mut data = [0u8; TOKEN_ACCOUNT_SIZE];

    // mint
    data[0..32].copy_from_slice(mint.as_ref());
    // owner
    data[32..64].copy_from_slice(owner.as_ref());
    // amount
    data[64..72].copy_from_slice(&amount.to_le_bytes());
    // delegate option = 0 (none)
    data[72] = 0;
    // state = 1 (initialized)
    data[108] = 1;
    // is_native option = 0 (none)
    data[109] = 0;
    // delegated_amount = 0
    data[110..118].copy_from_slice(&0u64.to_le_bytes());
    // close_authority option = 0 (none)
    data[118] = 0;

    data.to_vec()
}

/// Create mint account data for mollusk testing
#[cfg(test)]
pub fn create_mollusk_mint_data(decimals: u8) -> Vec<u8> {
    const MINT_ACCOUNT_SIZE: usize = 82;
    let mut data = [0u8; MINT_ACCOUNT_SIZE];
    data[0..4].copy_from_slice(&1u32.to_le_bytes()); // state = 1 (Initialized)
    data[44] = decimals;
    data[45] = 1; // is_initialized = 1
    data.to_vec()
}

/// Create valid token account data for testing
pub fn create_token_account_data(mint: &Pubkey, owner: &Pubkey, amount: u64) -> Vec<u8> {
    let mollusk_mint = SolanaPubkey::new_from_array(*mint);
    let mollusk_owner = SolanaPubkey::new_from_array(*owner);
    create_mollusk_token_account_data(&mollusk_mint, &mollusk_owner, amount)
}

/// Create valid multisig data for testing
pub fn create_multisig_data(m: u8, n: u8, signers: &[Pubkey]) -> Vec<u8> {
    let mut data = vec![0u8; Multisig::LEN];
    data[0] = m;
    data[1] = n;
    data[2] = 1; // initialized

    // Add signers (starting at byte 12, each signer is 32 bytes)
    for (i, signer) in signers.iter().take(n as usize).enumerate() {
        let start = 12 + (i * 32);
        let end = start + 32;
        data[start..end].copy_from_slice(signer.as_ref());
    }

    data
}

/// Create rent sysvar data for testing
pub fn create_rent_data(
    lamports_per_byte_year: u64,
    exemption_threshold: f64,
    burn_percent: u8,
) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&lamports_per_byte_year.to_le_bytes());
    data.extend_from_slice(&exemption_threshold.to_le_bytes());
    data.push(burn_percent);
    data
}

/// Test helper to verify token account structure
pub fn validate_token_account_structure(
    data: &[u8],
    expected_mint: &Pubkey,
    expected_owner: &Pubkey,
) -> bool {
    if data.len() < TokenAccount::LEN {
        return false;
    }

    // Check mint
    if &data[0..32] != expected_mint.as_ref() {
        return false;
    }

    // Check owner
    if &data[32..64] != expected_owner.as_ref() {
        return false;
    }

    // Check initialized state
    data[108] != 0
}

/// Calculate the rent-exempt lamports required
pub fn calculate_account_rent(len: usize) -> u64 {
    // Mollusk embeds a `Rent` sysvar instance that mirrors Solana’s
    // runtime parameters, so we reuse it rather than hard-coding the
    // constant.
    mollusk_svm::Mollusk::default()
        .sysvars
        .rent
        .minimum_balance(len)
}

#[cfg(test)]
mod tests {
    use crate::processor::is_spl_token_program;

    use super::*;

    #[test]
    fn test_validate_token_account_structure() {
        let mint = Pubkey::from([1u8; 32]);
        let owner = Pubkey::from([2u8; 32]);
        let data = create_token_account_data(&mint, &owner, 1000);

        assert!(validate_token_account_structure(&data, &mint, &owner));

        let wrong_mint = Pubkey::from([99u8; 32]);
        assert!(!validate_token_account_structure(
            &data,
            &wrong_mint,
            &owner
        ));
    }

    #[test]
    fn test_fn_is_spl_token_program() {
        assert!(is_spl_token_program(&SPL_TOKEN_PROGRAM_ID));

        let token_2022_id = pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
        assert!(!is_spl_token_program(&token_2022_id));
    }

    /* -- Tests of Test Helpers -- */

    #[test]
    fn test_create_multisig_data() {
        let signers = vec![
            Pubkey::from([1u8; 32]),
            Pubkey::from([2u8; 32]),
            Pubkey::from([3u8; 32]),
        ];

        let data = create_multisig_data(2, 3, &signers);

        assert_eq!(data.len(), Multisig::LEN);
        assert_eq!(data[0], 2); // m
        assert_eq!(data[1], 3); // n
        assert_eq!(data[2], 1); // initialized

        assert_eq!(&data[12..44], signers[0].as_ref());
    }

    /// Test that the token account data is created correctly
    /// in the test utility function
    #[test]
    fn test_token_account_data_creation() {
        let mint = Pubkey::from([1u8; 32]);
        let owner = Pubkey::from([2u8; 32]);
        let amount = 1000u64;

        let data = create_token_account_data(&mint, &owner, amount);

        // Verify the data structure
        assert_eq!(data.len(), TokenAccount::LEN);
        assert_eq!(&data[0..32], mint.as_ref());
        assert_eq!(&data[32..64], owner.as_ref());

        let stored_amount = u64::from_le_bytes(data[64..72].try_into().unwrap());
        assert_eq!(stored_amount, amount);

        // Verify initialized state
        assert_eq!(data[108], 1);
    }

    /// Test that the rent data is created correctly
    /// in the test utility function
    #[test]
    fn test_rent_data_creation() {
        let lamports_per_byte_year = 1000u64;
        let exemption_threshold = 2.0f64;
        let burn_percent = 50u8;

        let data = create_rent_data(lamports_per_byte_year, exemption_threshold, burn_percent);

        // Verify basic structure (simplified test)
        assert!(!data.is_empty());
        assert_eq!(data.len(), 8 + 8 + 1); // u64 + f64 + u8
    }

    #[test]
    fn test_account_layout_compatibility() {
        unsafe {
            let test_header = AccountLayout {
                borrow_state: 42,
                is_signer: 1,
                is_writable: 1,
                executable: 0,
                resize_delta: 100,
                key: [1u8; 32],
                owner: [2u8; 32],
                lamports: 1000,
                data_len: 256,
            };

            let account_ptr = &test_header as *const AccountLayout;
            let account_ref = &*account_ptr;
            assert_eq!(
                account_ref.borrow_state, 42,
                "borrow_state field should be accessible and match"
            );
            assert_eq!(
                account_ref.data_len, 256,
                "data_len field should be accessible and match"
            );
        }
    }

    #[test]
    fn test_mollusk_utilities() {
        use solana_sdk::signature::Keypair;

        let payer = Keypair::new();
        let token_program = spl_token::id();

        // Test base accounts creation
        let accounts = create_mollusk_base_accounts(&payer);
        assert_eq!(accounts.len(), 3);

        let accounts_with_token = create_mollusk_base_accounts_with_token(&payer, &token_program);
        assert_eq!(accounts_with_token.len(), 4);

        // Test mollusk token account data
        let mint = SolanaPubkey::new_unique();
        let owner = SolanaPubkey::new_unique();
        let data = create_mollusk_token_account_data(&mint, &owner, 1000);
        assert_eq!(data.len(), 165);
        assert_eq!(&data[0..32], mint.as_ref());
        assert_eq!(&data[32..64], owner.as_ref());
        assert_eq!(data[108], 1); // initialized

        // Test mint data
        let mint_data = create_mollusk_mint_data(6);
        assert_eq!(mint_data.len(), 82);
        assert_eq!(mint_data[44], 6); // decimals
        assert_eq!(mint_data[45], 1); // initialized
    }
}

#[cfg(test)]
/// Creates and initializes a mint account with the given parameters.
/// Returns a vector of accounts including the initialized mint and all necessary
/// base accounts for testing.
pub fn create_test_mint(
    mollusk: &Mollusk,
    mint_account: &Keypair,
    mint_authority: &Keypair,
    payer: &Keypair,
    token_program: &SolanaPubkey,
    decimals: u8,
) -> Vec<(SolanaPubkey, Account)> {
    let mint_space = 82u64;
    let rent_lamports = 1_461_600u64;

    let create_mint_ix = system_instruction::create_account(
        &payer.pubkey(),
        &mint_account.pubkey(),
        rent_lamports,
        mint_space,
        token_program,
    );

    let mut accounts = create_mollusk_base_accounts_with_token(payer, token_program);

    accounts.push((
        mint_account.pubkey(),
        Account::new(0, 0, &system_program::id()),
    ));
    accounts.push((
        mint_authority.pubkey(),
        Account::new(1_000_000, 0, &system_program::id()),
    ));

    // Create the mint account on-chain.
    mollusk.process_and_validate_instruction(&create_mint_ix, &accounts, &[Check::success()]);
    let init_mint_ix = spl_token::instruction::initialize_mint(
        token_program,
        &mint_account.pubkey(),
        &mint_authority.pubkey(),
        Some(&mint_authority.pubkey()),
        decimals,
    )
    .unwrap();

    // Refresh the mint account data after creation.
    if let Some((_, acct)) = mollusk
        .process_instruction(&create_mint_ix, &accounts)
        .resulting_accounts
        .into_iter()
        .find(|(pk, _)| *pk == mint_account.pubkey())
    {
        accounts
            .iter_mut()
            .find(|(pk, _)| *pk == mint_account.pubkey())
            .map(|(_, a)| *a = acct);
    }

    mollusk.process_and_validate_instruction(&init_mint_ix, &accounts, &[Check::success()]);

    // Final refresh so callers see the initialized state.
    if let Some((_, acct)) = mollusk
        .process_instruction(&init_mint_ix, &accounts)
        .resulting_accounts
        .into_iter()
        .find(|(pk, _)| *pk == mint_account.pubkey())
    {
        accounts
            .iter_mut()
            .find(|(pk, _)| *pk == mint_account.pubkey())
            .map(|(_, a)| *a = acct);
    }

    accounts
}

/// Create standard ATA test accounts with all required program accounts
#[cfg(test)]
pub fn create_ata_test_accounts(
    payer: &Keypair,
    ata_address: SolanaPubkey,
    wallet: SolanaPubkey,
    mint: SolanaPubkey,
    token_program: SolanaPubkey,
) -> Vec<(SolanaPubkey, Account)> {
    vec![
        (
            payer.pubkey(),
            Account::new(1_000_000_000, 0, &system_program::id()), // Payer with 1 SOL
        ),
        (
            ata_address,
            Account::new(0, 0, &system_program::id()), // ATA account (will be created)
        ),
        (
            wallet,
            Account::new(0, 0, &system_program::id()), // Wallet account
        ),
        (
            mint,
            Account {
                lamports: 1_461_600,
                data: create_mollusk_mint_data(6),
                owner: token_program,
                executable: false,
                rent_epoch: 0,
            },
        ),
        (
            system_program::id(),
            Account {
                lamports: 0,
                data: Vec::new(),
                owner: NATIVE_LOADER_ID,
                executable: true,
                rent_epoch: 0,
            },
        ),
        (
            token_program,
            Account {
                lamports: 0,
                data: Vec::new(),
                owner: LOADER_V3,
                executable: true,
                rent_epoch: 0,
            },
        ),
        (
            sysvar::rent::id(),
            Account::new(1009200, 17, &sysvar::id()), // Rent sysvar
        ),
    ]
}
