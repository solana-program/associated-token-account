#![cfg_attr(feature = "std", allow(dead_code, unused_imports))]
//! Account templates for benchmark tests

use {
    pinocchio_ata_program::debug_log,
    pinocchio_ata_program::test_utils::{
        account_builder::AccountBuilder,
        shared_constants::{NATIVE_LOADER_ID, ONE_SOL},
    },
    solana_account::Account,
    solana_pubkey::Pubkey,
    solana_sysvar::rent,
    std::{vec, vec::Vec},
};

#[cfg(feature = "full-debug-logs")]
use std::println;

/// Standard account set for most ATA benchmark tests
///
/// Contains the 6 core accounts needed for basic ATA operations:
/// - Payer (funds the operation)
/// - ATA (the associated token account being created/modified)
/// - Wallet (owner of the ATA)
/// - Mint (token mint the ATA is associated with)
/// - System Program (for account creation)
/// - Token Program (for token operations)
pub struct StandardAccountSet {
    pub payer: (Pubkey, Account),
    pub ata: (Pubkey, Account),
    pub wallet: (Pubkey, Account),
    pub mint: (Pubkey, Account),
    pub system_program: (Pubkey, Account),
    pub token_program: (Pubkey, Account),
}

impl StandardAccountSet {
    /// Create a new standard account set for basic ATA operations
    ///
    /// # Arguments
    /// * `payer` - The account that will fund the operation
    /// * `ata` - The associated token account address
    /// * `wallet` - The wallet that will own the ATA
    /// * `mint` - The token mint for the ATA
    /// * `token_program_id` - The token program ID
    pub fn new(
        payer: Pubkey,
        ata: Pubkey,
        wallet: Pubkey,
        mint: Pubkey,
        token_program_id: &Pubkey,
    ) -> Self {
        Self {
            payer: (payer, AccountBuilder::system_account(ONE_SOL)),
            ata: (ata, AccountBuilder::system_account(0)), // Will be created by instruction
            wallet: (wallet, AccountBuilder::system_account(0)),
            mint: (mint, AccountBuilder::mint(0, token_program_id)),
            system_program: (
                solana_system_interface::program::id(),
                AccountBuilder::executable_program(NATIVE_LOADER_ID),
            ),
            token_program: (
                *token_program_id,
                AccountBuilder::executable_program(mollusk_svm::program::loader_keys::LOADER_V3),
            ),
        }
    }

    /// Configure the ATA as an existing token account.
    ///
    /// Used for CreateIdempotent tests where the ATA already exists.
    ///
    /// # Panics
    /// Panics if the ATA has already been initialized – i.e. when its owner is no longer the
    /// system program or its data buffer is non-empty – which would indicate that another
    /// mutator has been applied out of order.
    pub fn with_existing_ata(
        mut self,
        mint: &Pubkey,
        wallet: &Pubkey,
        token_program_id: &Pubkey,
    ) -> Self {
        // Protect against accidental re-initialisation when helpers are chained in the wrong order.
        assert_eq!(
            self.ata.1.owner,
            solana_system_interface::program::id(),
            "with_existing_ata() called after ATA owner was already set – check builder call order"
        );
        assert!(
            self.ata.1.data.is_empty(),
            "with_existing_ata() expects ATA data to be empty"
        );
        self.ata.1 = AccountBuilder::token_account(mint, wallet, 0, token_program_id);
        self
    }

    /// Configure the ATA as a top-up account (has some lamports but not rent-exempt).
    ///
    /// Used for create-prefunded-account tests.
    ///
    /// # Panics
    /// Panics if the ATA has already been initialized or given a non-zero balance.
    pub fn with_topup_ata(mut self) -> Self {
        assert_eq!(
            self.ata.1.owner,
            solana_system_interface::program::id(),
            "with_topup_ata() called after ATA owner was already set – check builder call order"
        );
        assert_eq!(
            self.ata.1.lamports, 0,
            "with_topup_ata() expects ATA lamports to be zero before top-up"
        );
        self.ata.1.lamports = 1_000_000; // Below rent-exempt threshold
        self.ata.1.data = vec![]; // No data allocated yet
        self.ata.1.owner = solana_system_interface::program::id(); // Still system-owned
        self
    }

    /// Add rent sysvar to the account set
    ///
    /// Used when tests specify rent_arg = true
    pub fn with_rent_sysvar(self) -> StandardAccountSetWithRent {
        StandardAccountSetWithRent {
            base: self,
            rent_sysvar: (rent::id(), AccountBuilder::rent_sysvar()),
        }
    }

    /// Update the mint to use Token-2022 specific layout
    pub fn with_token_2022_mint(mut self, decimals: u8) -> Self {
        self.mint.1 = AccountBuilder::extended_mint(decimals, &self.token_program.0);
        self
    }

    /// Update the mint to use extended mint with multiple extensions
    pub fn with_extended_mint(mut self, decimals: u8) -> Self {
        self.mint.1 =
            AccountBuilder::extended_mint_with_extensions(decimals, &self.token_program.0);
        self
    }

    /// Set custom payer balance (for failure tests)
    ///
    /// Used for insufficient funds tests
    pub fn with_payer_balance(mut self, balance: u64) -> Self {
        self.payer.1.lamports = balance;
        self
    }

    /// Convert to vector format expected by benchmark functions
    pub fn to_vec(self) -> Vec<(Pubkey, Account)> {
        vec![
            self.payer,
            self.ata,
            self.wallet,
            self.mint,
            self.system_program,
            self.token_program,
        ]
    }
}

/// Extended account set that includes rent sysvar
///
/// Used when tests need the rent sysvar account
pub struct StandardAccountSetWithRent {
    base: StandardAccountSet,
    rent_sysvar: (Pubkey, Account),
}

impl StandardAccountSetWithRent {
    /// Convert to vector format with rent sysvar included
    pub fn to_vec(self) -> Vec<(Pubkey, Account)> {
        let mut accounts = self.base.to_vec();
        accounts.push(self.rent_sysvar);
        accounts
    }
}

/// Account set for recovery operations (RecoverNested and RecoverMultisig)
///
/// Contains the 8+ accounts needed for recovery operations:
/// - Nested ATA (source account with tokens to recover)
/// - Nested Mint (mint of the tokens being recovered)
/// - Destination ATA (where tokens will be recovered to)
/// - Owner ATA (intermediate owner account)
/// - Owner Mint (mint of the owner tokens)
/// - Wallet (ultimate owner)
/// - Token Program
/// - SPL Token Interface Program
/// - Optional: Multisig signers
pub struct RecoverAccountSet {
    pub nested_ata: (Pubkey, Account),
    pub nested_mint: (Pubkey, Account),
    pub dest_ata: (Pubkey, Account),
    pub owner_ata: (Pubkey, Account),
    pub owner_mint: (Pubkey, Account),
    pub wallet: (Pubkey, Account),
    pub token_program: (Pubkey, Account),
    pub spl_token_interface: (Pubkey, Account),
    pub multisig_signers: Vec<(Pubkey, Account)>,
}

impl RecoverAccountSet {
    /// Create a new recovery account set
    ///
    /// # Arguments
    /// * `nested_ata` - The nested ATA containing tokens to recover
    /// * `nested_mint` - The mint of the tokens being recovered
    /// * `dest_ata` - The destination ATA for recovered tokens
    /// * `owner_ata` - The intermediate owner ATA
    /// * `owner_mint` - The mint of the owner tokens
    /// * `wallet` - The ultimate owner wallet
    /// * `token_program_id` - The token program ID
    /// * `token_amount` - Amount of tokens in the nested ATA
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        nested_ata: Pubkey,
        nested_mint: Pubkey,
        dest_ata: Pubkey,
        owner_ata: Pubkey,
        owner_mint: Pubkey,
        wallet: Pubkey,
        token_program_id: &Pubkey,
        token_amount: u64,
    ) -> Self {
        Self {
            nested_ata: (
                nested_ata,
                AccountBuilder::token_account(
                    &nested_mint,
                    &owner_ata,
                    token_amount,
                    token_program_id,
                ),
            ),
            nested_mint: (nested_mint, AccountBuilder::mint(0, token_program_id)),
            dest_ata: (
                dest_ata,
                AccountBuilder::token_account(&nested_mint, &wallet, 0, token_program_id),
            ),
            owner_ata: (
                owner_ata,
                AccountBuilder::token_account(&owner_mint, &wallet, 0, token_program_id),
            ),
            owner_mint: (owner_mint, AccountBuilder::mint(0, token_program_id)),
            wallet: (wallet, AccountBuilder::system_account(ONE_SOL)),
            token_program: (
                *token_program_id,
                AccountBuilder::executable_program(mollusk_svm::program::loader_keys::LOADER_V3),
            ),
            spl_token_interface: (
                Pubkey::from(spl_token_interface::program::ID),
                AccountBuilder::executable_program(mollusk_svm::program::loader_keys::LOADER_V3),
            ),
            multisig_signers: vec![],
        }
    }

    /// Configure wallet as multisig account with signers
    ///
    /// Used for RecoverMultisig tests
    pub fn with_multisig(mut self, threshold: u8, signers: Vec<Pubkey>) -> Self {
        debug_log!(
            "🔍 [DEBUG] Setting up multisig with threshold: {}, signers: {}",
            threshold,
            signers.len()
        );
        for (_i, _signer) in signers.iter().enumerate() {
            debug_log!("    Signer {}: {}", _i, _signer);
        }

        // Replace wallet with multisig account
        self.wallet.1 = Account {
            lamports: ONE_SOL,
            data: {
                let byte_refs: Vec<&[u8; 32]> = signers
                    .iter()
                    .take(signers.len())
                    .map(|pk| pk.as_ref().try_into().expect("Pubkey is 32 bytes"))
                    .collect();
                pinocchio_ata_program::test_utils::unified_builders::create_multisig_data_unified(
                    threshold, &byte_refs,
                )
            },
            owner: self.token_program.0,
            executable: false,
            rent_epoch: 0,
        };

        // Add signer accounts
        for signer in &signers {
            self.multisig_signers
                .push((*signer, AccountBuilder::system_account(ONE_SOL)));
        }

        debug_log!("    Multisig wallet address: {}", self.wallet.0);
        debug_log!("    Added {} signer accounts", self.multisig_signers.len());

        self
    }

    /// Convert to vector format expected by benchmark functions
    pub fn to_vec(self) -> Vec<(Pubkey, Account)> {
        let mut accounts = vec![
            self.nested_ata,
            self.nested_mint,
            self.dest_ata,
            self.owner_ata,
            self.owner_mint,
            self.wallet,
            self.token_program,
            self.spl_token_interface,
        ];

        // Add multisig signers if present
        accounts.extend(self.multisig_signers);

        accounts
    }
}

/// Builder for failure test scenarios
///
/// Provides helpers to modify account sets for specific failure modes
pub struct FailureAccountBuilder;

impl FailureAccountBuilder {
    /// Set account owner to wrong program (for failure tests)
    pub fn set_wrong_owner(
        accounts: &mut [(Pubkey, Account)],
        target_address: Pubkey,
        wrong_owner: Pubkey,
    ) {
        if let Some(pos) = accounts
            .iter()
            .position(|(address, _)| *address == target_address)
        {
            accounts[pos].1.owner = wrong_owner;
        }
    }

    /// Set account balance to insufficient amount (for failure tests)
    pub fn set_insufficient_balance(
        accounts: &mut [(Pubkey, Account)],
        target_address: Pubkey,
        balance: u64,
    ) {
        if let Some(pos) = accounts
            .iter()
            .position(|(address, _)| *address == target_address)
        {
            accounts[pos].1.lamports = balance;
        }
    }

    /// Replace account with wrong address (for failure tests)
    pub fn replace_account_address(
        accounts: &mut [(Pubkey, Account)],
        old_address: Pubkey,
        new_address: Pubkey,
    ) -> bool {
        if let Some(pos) = accounts
            .iter()
            .position(|(address, _)| *address == old_address)
        {
            let account = accounts[pos].1.clone();
            accounts[pos] = (new_address, account);
            true
        } else {
            false
        }
    }

    /// Set account data to wrong size (for failure tests)
    pub fn set_wrong_data_size(
        accounts: &mut [(Pubkey, Account)],
        target_address: Pubkey,
        size: usize,
    ) {
        if let Some(pos) = accounts
            .iter()
            .position(|(address, _)| *address == target_address)
        {
            accounts[pos].1.data = vec![0u8; size];
        }
    }

    /// Replace account with a token account having wrong mint (for failure tests)
    pub fn set_token_account_wrong_mint(
        accounts: &mut Vec<(Pubkey, Account)>,
        target_address: Pubkey,
        wrong_mint: Pubkey,
        wallet: &Pubkey,
        token_program: &Pubkey,
    ) {
        if let Some(pos) = accounts
            .iter()
            .position(|(address, _)| *address == target_address)
        {
            accounts[pos].1 = AccountBuilder::token_account(&wrong_mint, wallet, 0, token_program);
        }
        // Add the wrong mint account if it doesn't exist
        if !accounts.iter().any(|(address, _)| *address == wrong_mint) {
            accounts.push((wrong_mint, AccountBuilder::mint(0, token_program)));
        }
    }

    /// Replace account with a token account having wrong owner (for failure tests)
    pub fn set_token_account_wrong_owner(
        accounts: &mut [(Pubkey, Account)],
        target_address: Pubkey,
        mint: &Pubkey,
        wrong_owner: &Pubkey,
        token_program: &Pubkey,
    ) {
        if let Some(pos) = accounts
            .iter()
            .position(|(address, _)| *address == target_address)
        {
            accounts[pos].1 = AccountBuilder::token_account(mint, wrong_owner, 0, token_program);
        }
    }

    /// Set account with invalid token account structure (for failure tests)
    pub fn set_invalid_token_account_structure(
        accounts: &mut [(Pubkey, Account)],
        target_address: Pubkey,
        token_program: &Pubkey,
    ) {
        if let Some(pos) = accounts
            .iter()
            .position(|(address, _)| *address == target_address)
        {
            accounts[pos].1.data =
                vec![0xFF; pinocchio_ata_program::test_utils::shared_constants::TOKEN_ACCOUNT_SIZE];
            accounts[pos].1.owner = *token_program;
            accounts[pos].1.lamports = 2_000_000;
        }
    }

    /// Set account with custom data, owner, and lamports (for failure tests)
    pub fn set_custom_account_state(
        accounts: &mut [(Pubkey, Account)],
        target_address: Pubkey,
        data: Vec<u8>,
        owner: Pubkey,
        lamports: u64,
    ) {
        if let Some(pos) = accounts
            .iter()
            .position(|(address, _)| *address == target_address)
        {
            accounts[pos].1.data = data;
            accounts[pos].1.owner = owner;
            accounts[pos].1.lamports = lamports;
        }
    }

    /// Set account with invalid multisig data (for failure tests)
    pub fn set_invalid_multisig_data(
        accounts: &mut [(Pubkey, Account)],
        target_address: Pubkey,
        token_program: &Pubkey,
    ) {
        if let Some(pos) = accounts
            .iter()
            .position(|(address, _)| *address == target_address)
        {
            accounts[pos].1.data = vec![
                    0xFF;
                    pinocchio_ata_program::test_utils::shared_constants::MULTISIG_ACCOUNT_SIZE
                ];
            accounts[pos].1.owner = *token_program;
        }
    }

    /// Add new account to accounts vector (for failure tests)
    pub fn add_account(accounts: &mut Vec<(Pubkey, Account)>, address: Pubkey, account: Account) {
        accounts.push((address, account));
    }
}

/// Helper for instruction modifications in failure tests
pub struct FailureInstructionBuilder;

impl FailureInstructionBuilder {
    /// Set account meta signer status (for failure tests)
    pub fn set_account_signer_status(
        ix: &mut solana_instruction::Instruction,
        target_address: Pubkey,
        is_signer: bool,
    ) {
        if let Some(meta) = ix.accounts.iter_mut().find(|m| m.pubkey == target_address) {
            meta.is_signer = is_signer;
        }
    }

    /// Set account meta writable status (for failure tests)
    pub fn set_account_writable_status(
        ix: &mut solana_instruction::Instruction,
        target_address: Pubkey,
        is_writable: bool,
    ) {
        if let Some(meta) = ix.accounts.iter_mut().find(|m| m.pubkey == target_address) {
            meta.is_writable = is_writable;
        }
    }

    /// Replace account meta address (for failure tests)
    pub fn replace_account_meta_address(
        ix: &mut solana_instruction::Instruction,
        old_address: Pubkey,
        new_address: Pubkey,
    ) {
        if let Some(meta) = ix.accounts.iter_mut().find(|m| m.pubkey == old_address) {
            meta.pubkey = new_address;
        }
    }

    /// Replace account meta by index (for failure tests)
    pub fn replace_account_meta_by_index(
        ix: &mut solana_instruction::Instruction,
        index: usize,
        new_address: Pubkey,
    ) {
        if let Some(meta) = ix.accounts.get_mut(index) {
            meta.pubkey = new_address;
        }
    }

    /// Set account meta signer status by index (for failure tests)
    pub fn set_account_signer_status_by_index(
        ix: &mut solana_instruction::Instruction,
        index: usize,
        is_signer: bool,
    ) {
        if let Some(meta) = ix.accounts.get_mut(index) {
            meta.is_signer = is_signer;
        }
    }

    /// Modify instruction data discriminator (for failure tests)
    pub fn set_discriminator(ix: &mut solana_instruction::Instruction, discriminator: u8) {
        if !ix.data.is_empty() {
            ix.data[0] = discriminator;
        }
    }

    /// Modify instruction data bump value (for failure tests)
    pub fn set_bump_value(ix: &mut solana_instruction::Instruction, bump: u8) {
        if ix.data.len() >= 2 {
            ix.data[1] = bump;
        }
    }

    /// Update both instruction meta and account address (for failure tests)
    pub fn replace_account_everywhere(
        ix: &mut solana_instruction::Instruction,
        accounts: &mut [(Pubkey, Account)],
        old_address: Pubkey,
        new_address: Pubkey,
    ) {
        // Update instruction meta
        Self::replace_account_meta_address(ix, old_address, new_address);

        // Update accounts vector
        FailureAccountBuilder::replace_account_address(accounts, old_address, new_address);
    }

    /// Update both instruction meta and account address by index (for failure tests)
    pub fn replace_account_everywhere_by_index(
        ix: &mut solana_instruction::Instruction,
        accounts: &mut Vec<(Pubkey, Account)>,
        meta_index: usize,
        new_address: Pubkey,
    ) {
        // Get the old address first
        if let Some(meta) = ix.accounts.get(meta_index) {
            let old_address = meta.pubkey;

            // Update instruction meta
            Self::replace_account_meta_by_index(ix, meta_index, new_address);

            // Update accounts vector
            FailureAccountBuilder::replace_account_address(accounts, old_address, new_address);
        }
    }
}
