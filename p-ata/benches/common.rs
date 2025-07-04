use {
    mollusk_svm::{program::loader_keys::LOADER_V3, Mollusk},
    solana_account::Account,
    solana_pubkey::Pubkey,
    solana_sysvar::rent,
    spl_token_2022::extension::ExtensionType,
    spl_token_interface::state::Transmutable,
};

// ================================ CONSTANTS ================================

pub const SYSTEM_PROGRAM_ID: Pubkey = Pubkey::new_from_array([0u8; 32]);
pub const NATIVE_LOADER_ID: Pubkey = Pubkey::new_from_array([
    5, 135, 132, 191, 20, 139, 164, 40, 47, 176, 18, 87, 72, 136, 169, 241, 83, 160, 125, 173, 247,
    101, 192, 69, 92, 154, 151, 3, 128, 0, 0, 0,
]);

// ============================= ACCOUNT BUILDERS =============================

pub struct AccountBuilder;

impl AccountBuilder {
    /// Build a zero-rent `Rent` sysvar account with correctly sized data buffer
    pub fn rent_sysvar() -> Account {
        Account {
            lamports: 1,
            data: vec![1u8; 17], // Minimal rent sysvar data
            owner: rent::id(),
            executable: false,
            rent_epoch: 0,
        }
    }

    /// Build raw token Account data with the supplied mint / owner / amount
    pub fn token_account_data(mint: &Pubkey, owner: &Pubkey, amount: u64) -> Vec<u8> {
        build_token_account_data_core(
            mint.as_ref().try_into().expect("Pubkey is 32 bytes"),
            owner.as_ref().try_into().expect("Pubkey is 32 bytes"),
            amount,
        )
        .to_vec()
    }

    /// Build mint data with given decimals and marked initialized
    pub fn mint_data(decimals: u8) -> Vec<u8> {
        build_mint_data_core(decimals).to_vec()
    }

    /// Build extended mint data with ImmutableOwner extension
    pub fn extended_mint_data(decimals: u8) -> Vec<u8> {
        let required_len =
            ExtensionType::try_calculate_account_len::<spl_token_2022::state::Mint>(&[
                ExtensionType::ImmutableOwner,
            ])
            .expect("calc len");

        let mut data = Self::mint_data(decimals);
        data.resize(required_len, 0u8);

        // Add TLV entries at correct offset (base len = 82)
        let mut cursor = 82;
        // ImmutableOwner header
        let immutable_owner_header = build_tlv_extension(ExtensionType::ImmutableOwner as u16, 0);
        data[cursor..cursor + 4].copy_from_slice(&immutable_owner_header);
        cursor += 4;
        // Sentinel header
        data[cursor..cursor + 4].copy_from_slice(&0u32.to_le_bytes());

        data
    }

    /// Build Multisig account data with given signer public keys and threshold `m`
    pub fn multisig_data(m: u8, signer_pubkeys: &[Pubkey]) -> Vec<u8> {
        let byte_refs: Vec<&[u8; 32]> = signer_pubkeys
            .iter()
            .map(|pk| pk.as_ref().try_into().expect("Pubkey is 32 bytes"))
            .collect();
        build_multisig_data_core(m, &byte_refs)
    }

    /// Create a basic system account
    pub fn system_account(lamports: u64) -> Account {
        Account::new(lamports, 0, &SYSTEM_PROGRAM_ID)
    }

    /// Create an executable program account
    pub fn executable_program(owner: Pubkey) -> Account {
        Account {
            lamports: 0,
            data: Vec::new(),
            owner,
            executable: true,
            rent_epoch: 0,
        }
    }

    /// Create a token account with specified parameters
    pub fn token_account(
        mint: &Pubkey,
        owner: &Pubkey,
        amount: u64,
        token_program_id: &Pubkey,
    ) -> Account {
        Account {
            lamports: 2_000_000, // rent-exempt
            data: Self::token_account_data(mint, owner, amount),
            owner: *token_program_id,
            executable: false,
            rent_epoch: 0,
        }
    }

    /// Create a mint account
    pub fn mint_account(decimals: u8, token_program_id: &Pubkey, extended: bool) -> Account {
        Account {
            lamports: 1_000_000_000,
            data: if extended {
                Self::extended_mint_data(decimals)
            } else {
                Self::mint_data(decimals)
            },
            owner: *token_program_id,
            executable: false,
            rent_epoch: 0,
        }
    }
}

// =========================== OPTIMAL KEY FINDERS ==========================

pub struct OptimalKeyFinder;

impl OptimalKeyFinder {
    /// Find a wallet pubkey that yields the maximum bump (255) for its ATA
    pub fn find_optimal_wallet(
        start_byte: u8,
        token_program_id: &Pubkey,
        mint: &Pubkey,
        program_id: &Pubkey,
    ) -> Pubkey {
        let mut wallet = const_pk(start_byte);
        let mut best_bump = 0u8;

        for b in start_byte..=255 {
            let candidate = const_pk(b);
            let (_, bump) = Pubkey::find_program_address(
                &[candidate.as_ref(), token_program_id.as_ref(), mint.as_ref()],
                program_id,
            );
            if bump > best_bump {
                wallet = candidate;
                best_bump = bump;
                if bump == 255 {
                    break;
                }
            }
        }
        wallet
    }

    /// Find mint that gives optimal bump for nested ATA
    pub fn find_optimal_nested_mint(
        start_byte: u8,
        owner_ata: &Pubkey,
        token_program_id: &Pubkey,
        program_id: &Pubkey,
    ) -> Pubkey {
        let mut nested_mint = const_pk(start_byte);
        let mut best_bump = 0u8;

        for b in start_byte..=255 {
            let candidate = const_pk(b);
            let (_, bump) = Pubkey::find_program_address(
                &[
                    owner_ata.as_ref(),
                    token_program_id.as_ref(),
                    candidate.as_ref(),
                ],
                program_id,
            );
            if bump > best_bump {
                nested_mint = candidate;
                best_bump = bump;
                if bump == 255 {
                    break;
                }
            }
        }
        nested_mint
    }
}

// =============================== UTILITIES =================================

/// Helper to create deterministic pubkeys (32 identical bytes)
pub fn const_pk(byte: u8) -> Pubkey {
    Pubkey::new_from_array([byte; 32])
}

/// Clone accounts vector for benchmark isolation
pub fn clone_accounts(src: &[(Pubkey, Account)]) -> Vec<(Pubkey, Account)> {
    src.iter().map(|(k, v)| (*k, v.clone())).collect()
}

/// Create a fresh Mollusk instance with required programs
pub fn fresh_mollusk(program_id: &Pubkey, token_program_id: &Pubkey) -> Mollusk {
    let mut mollusk = Mollusk::default();
    mollusk.add_program(program_id, "pinocchio_ata_program", &LOADER_V3);
    mollusk.add_program(
        &Pubkey::from(spl_token_interface::program::ID),
        "pinocchio_token_program",
        &LOADER_V3,
    );
    mollusk.add_program(token_program_id, "pinocchio_token_program", &LOADER_V3);

    // Add Token-2022 program with the actual Token-2022 binary
    let token_2022_id = Pubkey::new_from_array(pinocchio_pubkey::pubkey!(
        "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
    ));
    mollusk.add_program(&token_2022_id, "spl_token_2022", &LOADER_V3);

    mollusk
}

pub fn build_instruction_data(discriminator: u8, additional_data: &[u8]) -> Vec<u8> {
    let mut data = vec![discriminator];
    data.extend_from_slice(additional_data);
    data
}

/// Build multisig account data
pub fn build_multisig_data_core(m: u8, signer_pubkeys: &[&[u8; 32]]) -> Vec<u8> {
    use spl_token_interface::state::multisig::{Multisig, MAX_SIGNERS};

    assert!(
        m as usize <= signer_pubkeys.len(),
        "m cannot exceed number of provided signers"
    );
    assert!(m >= 1, "m must be at least 1");
    assert!(
        signer_pubkeys.len() <= MAX_SIGNERS as usize,
        "too many signers provided"
    );

    let mut data = vec![0u8; Multisig::LEN];
    data[0] = m; // m threshold
    data[1] = signer_pubkeys.len() as u8; // n signers
    data[2] = 1; // is_initialized

    for (i, pk) in signer_pubkeys.iter().enumerate() {
        let offset = 3 + i * 32;
        data[offset..offset + 32].copy_from_slice(*pk);
    }
    data
}

/// Build mint data core structure
#[inline(always)]
fn build_mint_data_core(decimals: u8) -> [u8; 82] {
    let mut data = [0u8; 82]; // Mint::LEN

    // mint_authority: COption<Pubkey> (36 bytes: 4 tag + 32 pubkey)
    data[0..4].copy_from_slice(&1u32.to_le_bytes()); // COption tag = Some
    data[4..36].fill(0); // All-zeros pubkey (valid but no authority)

    // supply: u64 (8 bytes) - stays as 0

    // decimals: u8 (1 byte)
    data[44] = decimals;

    // is_initialized: bool (1 byte)
    data[45] = 1; // true

    // freeze_authority: COption<Pubkey> (36 bytes: 4 tag + 32 pubkey)
    data[46..50].copy_from_slice(&0u32.to_le_bytes()); // COption tag = None
                                                       // Remaining 32 bytes already 0

    data
}

/// Build token account data core structure
#[inline(always)]
fn build_token_account_data_core(mint: &[u8; 32], owner: &[u8; 32], amount: u64) -> [u8; 165] {
    let mut data = [0u8; 165]; // TokenAccount::LEN
    data[0..32].copy_from_slice(mint); // mint
    data[32..64].copy_from_slice(owner); // owner
    data[64..72].copy_from_slice(&amount.to_le_bytes()); // amount
    data[108] = 1; // state = Initialized
    data
}

/// Build TLV extension header
#[inline(always)]
fn build_tlv_extension(extension_type: u16, data_len: u16) -> [u8; 4] {
    let mut header = [0u8; 4];
    header[0..2].copy_from_slice(&extension_type.to_le_bytes());
    header[2..4].copy_from_slice(&data_len.to_le_bytes());
    header
}
