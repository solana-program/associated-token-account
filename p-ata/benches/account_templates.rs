#![allow(dead_code)]
//! Account templates for benchmark tests

use {solana_account::Account, solana_pubkey::Pubkey, solana_sysvar::rent};

use crate::{constants::lamports::*, AccountBuilder, NATIVE_LOADER_ID, SYSTEM_PROGRAM_ID};

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
            mint: (
                mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            system_program: (
                SYSTEM_PROGRAM_ID,
                AccountBuilder::executable_program(NATIVE_LOADER_ID),
            ),
            token_program: (
                *token_program_id,
                AccountBuilder::executable_program(mollusk_svm::program::loader_keys::LOADER_V3),
            ),
        }
    }

    /// Configure the ATA as an existing token account
    ///
    /// Used for CreateIdempotent tests where the ATA already exists
    pub fn with_existing_ata(
        mut self,
        mint: &Pubkey,
        wallet: &Pubkey,
        token_program_id: &Pubkey,
    ) -> Self {
        self.ata.1 = AccountBuilder::token_account(mint, wallet, 0, token_program_id);
        self
    }

    /// Configure the ATA as a topup account (has some lamports but not rent-exempt)
    ///
    /// Used for create-account-prefunded tests
    pub fn with_topup_ata(mut self) -> Self {
        self.ata.1.lamports = 1_000_000; // Below rent-exempt threshold
        self.ata.1.data = vec![]; // No data allocated yet
        self.ata.1.owner = SYSTEM_PROGRAM_ID; // Still system-owned
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

    /// Use Token-2022 mint instead of standard mint
    ///
    /// Used for Token-2022 specific tests
    pub fn with_token_2022_mint(mut self, decimals: u8) -> Self {
        self.mint.1 = AccountBuilder::token_2022_mint_account(decimals, &self.token_program.0);
        self
    }

    /// Use extended mint format (with ImmutableOwner extension)
    ///
    /// Used for tests that require extended mint accounts
    pub fn with_extended_mint(mut self, decimals: u8, token_program_id: &Pubkey) -> Self {
        self.mint.1 = AccountBuilder::mint_account(decimals, token_program_id, true);
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
            nested_mint: (
                nested_mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            dest_ata: (
                dest_ata,
                AccountBuilder::token_account(&nested_mint, &wallet, 0, token_program_id),
            ),
            owner_ata: (
                owner_ata,
                AccountBuilder::token_account(&owner_mint, &wallet, 0, token_program_id),
            ),
            owner_mint: (
                owner_mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
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
        // Replace wallet with multisig account
        self.wallet.1 = Account {
            lamports: ONE_SOL,
            data: AccountBuilder::multisig_data(threshold, &signers),
            owner: self.token_program.0,
            executable: false,
            rent_epoch: 0,
        };

        // Add signer accounts
        for signer in &signers {
            self.multisig_signers
                .push((*signer, AccountBuilder::system_account(ONE_SOL)));
        }

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
        accounts: &mut Vec<(Pubkey, Account)>,
        target_address: Pubkey,
        wrong_owner: Pubkey,
    ) {
        if let Some(pos) = accounts
            .iter()
            .position(|(addr, _)| *addr == target_address)
        {
            accounts[pos].1.owner = wrong_owner;
        }
    }

    /// Set account balance to insufficient amount (for failure tests)
    pub fn set_insufficient_balance(
        accounts: &mut Vec<(Pubkey, Account)>,
        target_address: Pubkey,
        balance: u64,
    ) {
        if let Some(pos) = accounts
            .iter()
            .position(|(addr, _)| *addr == target_address)
        {
            accounts[pos].1.lamports = balance;
        }
    }

    /// Replace account with wrong address (for failure tests)
    pub fn replace_account_address(
        accounts: &mut Vec<(Pubkey, Account)>,
        old_address: Pubkey,
        new_address: Pubkey,
    ) -> bool {
        if let Some(pos) = accounts.iter().position(|(addr, _)| *addr == old_address) {
            let account = accounts[pos].1.clone();
            accounts[pos] = (new_address, account);
            true
        } else {
            false
        }
    }

    /// Set account data to wrong size (for failure tests)
    pub fn set_wrong_data_size(
        accounts: &mut Vec<(Pubkey, Account)>,
        target_address: Pubkey,
        size: usize,
    ) {
        if let Some(pos) = accounts
            .iter()
            .position(|(addr, _)| *addr == target_address)
        {
            accounts[pos].1.data = vec![0u8; size];
        }
    }
}
