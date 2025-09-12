//! Test utilities for migrated mollusk tests
#![cfg(any(test, feature = "test-utils"))]

use {
    mollusk_svm::{
        program::loader_keys::LOADER_V3, result::ProgramResult, Mollusk, MolluskContext,
    },
    solana_program::{instruction::Instruction, pubkey::Pubkey, sysvar},
    solana_sdk::{account::Account, signature::Keypair, signer::Signer},
    solana_system_interface::instruction as system_instruction,
    solana_system_interface::program as system_program,
    spl_associated_token_account_interface::address::get_associated_token_address_with_program_id,
    std::vec::Vec,
    std::{collections::HashMap, path::Path},
};

// Standard SPL token account size (fixed for all SPL token accounts)
const TOKEN_ACCOUNT_SIZE: usize = 165;
// Standard mint account size (base size without extensions)
const MINT_ACCOUNT_SIZE: usize = 82;

// Native loader program ID (used across both test suites)
const NATIVE_LOADER_ID: Pubkey = Pubkey::new_from_array([
    5, 135, 132, 191, 20, 139, 164, 40, 47, 176, 18, 87, 72, 136, 169, 241, 83, 160, 125, 173, 247,
    101, 192, 69, 92, 154, 151, 3, 128, 0, 0, 0,
]);

/// Setup mollusk with ATA and token programs for testing
pub fn setup_mollusk_with_programs(token_program_id: &Pubkey) -> Mollusk {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");

    // 1) Load ATA by name from target deploy dir
    {
        let sbf_out = workspace_root.join("target/deploy");
        std::env::set_var("SBF_OUT_DIR", &sbf_out);
        std::env::set_var("BPF_OUT_DIR", sbf_out);
    }
    let ata_program_id = spl_associated_token_account::id();
    let mut mollusk = Mollusk::new(&ata_program_id, "spl_associated_token_account");

    // 2) Load the selected token program by name from programs/ dir
    {
        let sbf_out = workspace_root.join("programs");
        std::env::set_var("SBF_OUT_DIR", &sbf_out);
        std::env::set_var("BPF_OUT_DIR", sbf_out);
    }
    if *token_program_id == spl_token_2022_interface::id() {
        mollusk.add_program(token_program_id, "spl_token_2022", &LOADER_V3);
    } else {
        // Pinocchio token program
        mollusk.add_program(token_program_id, "pinocchio_token_program", &LOADER_V3);
    }

    mollusk
}

pub mod test_util_exports {
    #![allow(dead_code)]
    use {
        super::*, mollusk_svm::result::Check, solana_program::instruction::AccountMeta,
        solana_sdk::program_error::ProgramError,
    };

    /// Ensure a system-owned account exists in the context store with the given lamports
    pub fn ctx_ensure_system_account_exists(
        context: &MolluskContext<HashMap<Pubkey, Account>>,
        address: Pubkey,
        lamports: u64,
    ) {
        if context.account_store.borrow().get(&address).is_none() {
            context
                .account_store
                .borrow_mut()
                .insert(address, Account::new(lamports, 0, &system_program::id()));
        }
    }

    /// Ensure multiple system accounts exist in the context store with the provided lamports
    pub fn ctx_ensure_system_accounts_with_lamports(
        context: &MolluskContext<HashMap<Pubkey, Account>>,
        entries: &[(Pubkey, u64)],
    ) {
        for (address, lamports) in entries.iter().copied() {
            ctx_ensure_system_account_exists(context, address, lamports);
        }
    }

    /// Create standard base accounts needed for mollusk tests
    pub fn create_mollusk_base_accounts(payer: &Keypair) -> Vec<(Pubkey, Account)> {
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
            {
                use solana_sdk::rent::Rent;
                let rent = Rent::default();

                (
                    sysvar::rent::id(),
                    Account {
                        lamports: 0,
                        data: create_rent_data(
                            rent.lamports_per_byte_year,
                            rent.exemption_threshold,
                            rent.burn_percent,
                        ),
                        owner: sysvar::id(),
                        executable: false,
                        rent_epoch: 0,
                    },
                )
            },
        ]
        .into()
    }

    /// Create standard base accounts with token program
    pub fn create_mollusk_base_accounts_with_token(
        payer: &Keypair,
        token_program_id: &Pubkey,
    ) -> Vec<(Pubkey, Account)> {
        let mut accounts = create_mollusk_base_accounts(payer);

        accounts.push((
            *token_program_id,
            mollusk_svm::program::create_program_account_loader_v3(token_program_id),
        ));

        accounts
    }

    /// The type of ATA creation instruction to build.
    #[derive(Debug)]
    pub enum CreateAtaInstructionType {
        /// The standard `Create` instruction, which can optionally include a bump seed and account length.
        Create {
            bump: Option<u8>,
            account_len: Option<u16>,
        },
        #[allow(dead_code, reason = "Some tests construct only a subset of variants")]
        /// The `CreateIdempotent` instruction, which can optionally include a bump seed.
        CreateIdempotent { bump: Option<u8> },
    }

    impl Default for CreateAtaInstructionType {
        fn default() -> Self {
            Self::Create {
                bump: None,
                account_len: None,
            }
        }
    }

    /// Helper functions for common test calculations
    pub mod test_calculations {
        use {
            solana_program::program_pack::Pack,
            solana_sdk::rent::Rent,
            spl_token_2022_interface::{
                extension::ExtensionType, state::Account as Token2022Account,
            },
            spl_token_interface::state::Account as TokenAccount,
        };

        /// Calculate the expected account length for a Token-2022 account with ImmutableOwner extension
        pub fn token_2022_account_len() -> usize {
            ExtensionType::try_calculate_account_len::<Token2022Account>(&[
                ExtensionType::ImmutableOwner,
            ])
            .expect("Failed to calculate Token-2022 account length")
        }

        /// Calculate the expected account length for a standard SPL token account
        pub fn token_account_len() -> usize {
            TokenAccount::LEN
        }

        /// Calculate the rent-exempt balance for a Token-2022 account with ImmutableOwner extension
        pub fn token_2022_account_balance() -> u64 {
            Rent::default().minimum_balance(token_2022_account_len())
        }

        /// Calculate the rent-exempt balance for a standard SPL token account
        pub fn token_account_balance() -> u64 {
            Rent::default().minimum_balance(token_account_len())
        }
    }

    /// Comprehensive test harness for ATA testing scenarios
    pub struct TestHarness {
        pub ctx: MolluskContext<HashMap<Pubkey, Account>>,
        pub token_program_id: Pubkey,
        pub payer: Keypair,
        pub wallet: Option<Keypair>,
        pub mint: Option<Pubkey>,
        pub mint_authority: Option<Keypair>,
        pub ata_address: Option<Pubkey>,
    }

    impl TestHarness {
        /// Create a new test harness with the specified token program
        pub fn new(token_program_id: &Pubkey) -> Self {
            let mollusk = setup_mollusk_with_programs(token_program_id);
            let payer = Keypair::new();

            // Initialize base accounts in context
            let base_accounts = create_mollusk_base_accounts_with_token(&payer, token_program_id);
            let mut accounts = HashMap::new();
            for (pubkey, account) in base_accounts {
                accounts.insert(pubkey, account);
            }

            let ctx = mollusk.with_context(accounts);

            Self {
                ctx,
                token_program_id: *token_program_id,
                payer,
                wallet: None,
                mint: None,
                mint_authority: None,
                ata_address: None,
            }
        }

        /// Add a wallet with the specified lamports
        pub fn with_wallet(mut self, lamports: u64) -> Self {
            let wallet = Keypair::new();
            ctx_ensure_system_accounts_with_lamports(&self.ctx, &[(wallet.pubkey(), lamports)]);
            self.wallet = Some(wallet);
            self
        }

        /// Create and initialize a mint with the specified decimals
        pub fn with_mint(mut self, decimals: u8) -> Self {
            let mint_account = Keypair::new();
            let mint_authority = Keypair::new();

            self.ctx.account_store.borrow_mut().insert(
                mint_authority.pubkey(),
                account_builder::AccountBuilder::system_account(1_000_000),
            );

            let mint_accounts = create_test_mint(
                &self.ctx.mollusk,
                &mint_account,
                &mint_authority,
                &self.payer,
                &self.token_program_id,
                decimals,
            );

            // Merge the mint-related accounts
            for (pubkey, account) in mint_accounts {
                if pubkey == mint_account.pubkey() || pubkey == mint_authority.pubkey() {
                    self.ctx.account_store.borrow_mut().insert(pubkey, account);
                }
            }

            self.mint = Some(mint_account.pubkey());
            self.mint_authority = Some(mint_authority);
            self
        }

        /// Create an ATA for the wallet and mint (requires wallet and mint to be set)
        pub fn with_ata(mut self) -> Self {
            let wallet = self
                .wallet
                .as_ref()
                .expect("Wallet must be set before creating ATA");
            let mint = self.mint.expect("Mint must be set before creating ATA");

            let ata_address = get_associated_token_address_with_program_id(
                &wallet.pubkey(),
                &mint,
                &self.token_program_id,
            );

            // Ensure system account exists for ATA
            ctx_ensure_system_account_exists(&self.ctx, ata_address, 0);

            let instruction = build_create_ata_instruction(
                spl_associated_token_account::id(),
                self.payer.pubkey(),
                ata_address,
                wallet.pubkey(),
                mint,
                self.token_program_id,
                CreateAtaInstructionType::default(),
            );

            self.ctx
                .process_and_validate_instruction(&instruction, &[Check::success()]);

            self.ata_address = Some(ata_address);
            self
        }

        /// Execute an instruction and validate with the given checks
        pub fn execute_and_validate(
            &mut self,
            instruction: &solana_program::instruction::Instruction,
            checks: &[mollusk_svm::result::Check],
        ) -> mollusk_svm::result::InstructionResult {
            self.ctx
                .process_and_validate_instruction(instruction, checks)
        }

        /// Execute an instruction expecting success
        pub fn execute_success(
            &mut self,
            instruction: &solana_program::instruction::Instruction,
        ) -> mollusk_svm::result::InstructionResult {
            self.execute_and_validate(instruction, &[mollusk_svm::result::Check::success()])
        }

        /// Execute an instruction expecting a specific error
        pub fn execute_error(
            &mut self,
            instruction: &solana_program::instruction::Instruction,
            expected_error: ProgramError,
        ) -> mollusk_svm::result::InstructionResult {
            self.execute_and_validate(
                instruction,
                &[mollusk_svm::result::Check::err(expected_error)],
            )
        }

        /// Get a reference to an account by pubkey
        pub fn get_account(&self, pubkey: Pubkey) -> Account {
            self.ctx
                .account_store
                .borrow()
                .get(&pubkey)
                .expect("account not found")
                .clone()
        }

        /// Mint tokens to the ATA (requires mint_authority and ata_address to be set)
        pub fn mint_tokens(&mut self, amount: u64) {
            let ata_address = self.ata_address.expect("ATA must be set");
            self.mint_tokens_to(ata_address, amount);
        }

        /// Mint tokens to a specific address
        pub fn mint_tokens_to(&mut self, destination: Pubkey, amount: u64) {
            let mint = self.mint.expect("Mint must be set");
            let mint_authority = self
                .mint_authority
                .as_ref()
                .expect("Mint authority must be set");

            let mint_to_ix = if self.token_program_id == spl_token_2022_interface::id() {
                spl_token_2022_interface::instruction::mint_to(
                    &self.token_program_id,
                    &mint,
                    &destination,
                    &mint_authority.pubkey(),
                    &[],
                    amount,
                )
                .unwrap()
            } else {
                spl_token_interface::instruction::mint_to(
                    &self.token_program_id,
                    &mint,
                    &destination,
                    &mint_authority.pubkey(),
                    &[],
                    amount,
                )
                .unwrap()
            };

            self.execute_success(&mint_to_ix);
        }

        /// Build a create ATA instruction for the current wallet and mint
        pub fn build_create_ata_instruction(
            &mut self,
            instruction_type: CreateAtaInstructionType,
        ) -> solana_program::instruction::Instruction {
            let wallet = self.wallet.as_ref().expect("Wallet must be set");
            let mint = self.mint.expect("Mint must be set");
            let ata_address = get_associated_token_address_with_program_id(
                &wallet.pubkey(),
                &mint,
                &self.token_program_id,
            );

            // Ensure ATA address exists as system account
            ctx_ensure_system_account_exists(&self.ctx, ata_address, 0);
            self.ata_address = Some(ata_address);

            build_create_ata_instruction(
                spl_associated_token_account::id(),
                self.payer.pubkey(),
                ata_address,
                wallet.pubkey(),
                mint,
                self.token_program_id,
                instruction_type,
            )
        }

        /// Create a nested ATA (ATA owned by another ATA) and return the nested ATA address
        pub fn create_nested_ata(&mut self, owner_ata: Pubkey) -> Pubkey {
            let mint = self.mint.expect("Mint must be set");
            let nested_ata_address = get_associated_token_address_with_program_id(
                &owner_ata,
                &mint,
                &self.token_program_id,
            );

            // Ensure system account exists for nested ATA
            ctx_ensure_system_account_exists(&self.ctx, nested_ata_address, 0);

            let instruction = build_create_ata_instruction(
                spl_associated_token_account::id(),
                self.payer.pubkey(),
                nested_ata_address,
                owner_ata,
                mint,
                self.token_program_id,
                CreateAtaInstructionType::default(),
            );

            self.ctx
                .process_and_validate_instruction(&instruction, &[Check::success()]);

            nested_ata_address
        }

        /// Create an ATA for a different owner (for error testing)
        pub fn create_ata_for_owner(&mut self, owner: Pubkey) -> Pubkey {
            let mint = self.mint.expect("Mint must be set");
            ctx_ensure_system_accounts_with_lamports(&self.ctx, &[(owner, 1_000_000)]);

            let ata_address =
                get_associated_token_address_with_program_id(&owner, &mint, &self.token_program_id);

            // Ensure system account exists for ATA
            ctx_ensure_system_account_exists(&self.ctx, ata_address, 0);

            let instruction = build_create_ata_instruction(
                spl_associated_token_account::id(),
                self.payer.pubkey(),
                ata_address,
                owner,
                mint,
                self.token_program_id,
                CreateAtaInstructionType::default(),
            );

            self.ctx
                .process_and_validate_instruction(&instruction, &[Check::success()]);

            ata_address
        }

        /// Build a recover_nested instruction and ensure all required accounts exist
        pub fn build_recover_nested_instruction(
            &mut self,
            owner_mint: Pubkey,
            nested_mint: Pubkey,
        ) -> solana_program::instruction::Instruction {
            let wallet = self.wallet.as_ref().expect("Wallet must be set");

            // Calculate all derived ATA addresses
            let owner_ata = get_associated_token_address_with_program_id(
                &wallet.pubkey(),
                &owner_mint,
                &self.token_program_id,
            );
            let destination_ata = get_associated_token_address_with_program_id(
                &wallet.pubkey(),
                &nested_mint,
                &self.token_program_id,
            );
            let nested_ata = get_associated_token_address_with_program_id(
                &owner_ata,
                &nested_mint,
                &self.token_program_id,
            );

            // Ensure all ATAs exist as system accounts
            for ata in [owner_ata, destination_ata, nested_ata] {
                ctx_ensure_system_account_exists(&self.ctx, ata, 0);
            }

            spl_associated_token_account_interface::instruction::recover_nested(
                &wallet.pubkey(),
                &owner_mint,
                &nested_mint,
                &self.token_program_id,
            )
        }
    }

    /// Simple context-based test harness for lightweight testing scenarios
    pub struct ContextHarness {
        pub ctx: MolluskContext<HashMap<Pubkey, Account>>,
        pub token_program_id: Pubkey,
        pub payer: Keypair,
        pub wallet: Option<Keypair>,
        pub mint: Option<Pubkey>,
        pub ata_address: Option<Pubkey>,
    }

    impl ContextHarness {
        /// Create a new context-based harness
        pub fn new(token_program_id: &Pubkey) -> Self {
            let mollusk = setup_mollusk_with_programs(token_program_id);
            let ctx = mollusk.with_context(HashMap::new());
            let payer = Keypair::new();

            Self {
                ctx,
                token_program_id: *token_program_id,
                payer,
                wallet: None,
                mint: None,
                ata_address: None,
            }
        }

        /// Add a wallet and mint, and fund the payer
        pub fn with_wallet_and_mint(mut self, wallet_lamports: u64, decimals: u8) -> Self {
            let wallet = Keypair::new();
            let mint = Pubkey::new_unique();

            // Fund payer
            let expected_balance = if self.token_program_id == spl_token_2022_interface::id() {
                test_calculations::token_2022_account_balance()
            } else {
                test_calculations::token_account_balance()
            };

            self.ctx.account_store.borrow_mut().insert(
                self.payer.pubkey(),
                account_builder::AccountBuilder::system_account(expected_balance),
            );

            // Add mint
            if self.token_program_id == spl_token_2022_interface::id() {
                self.ctx.account_store.borrow_mut().insert(
                    mint,
                    account_builder::AccountBuilder::extended_mint(decimals, &self.payer.pubkey()),
                );
            } else {
                self.ctx.account_store.borrow_mut().insert(
                    mint,
                    account_builder::AccountBuilder::mint(decimals, &self.payer.pubkey()),
                );
            }

            // Add wallet
            ctx_ensure_system_accounts_with_lamports(
                &self.ctx,
                &[(wallet.pubkey(), wallet_lamports)],
            );

            self.wallet = Some(wallet);
            self.mint = Some(mint);
            self
        }

        /// Build and execute a create ATA instruction
        pub fn create_ata(&mut self, instruction_type: CreateAtaInstructionType) -> Pubkey {
            let wallet = self.wallet.as_ref().expect("Wallet must be set");
            let mint = self.mint.expect("Mint must be set");
            let ata_address = get_associated_token_address_with_program_id(
                &wallet.pubkey(),
                &mint,
                &self.token_program_id,
            );

            ctx_ensure_system_account_exists(&self.ctx, ata_address, 0);

            let instruction = build_create_ata_instruction(
                spl_associated_token_account::id(),
                self.payer.pubkey(),
                ata_address,
                wallet.pubkey(),
                mint,
                self.token_program_id,
                instruction_type,
            );

            let expected_len = if self.token_program_id == spl_token_2022_interface::id() {
                test_calculations::token_2022_account_len()
            } else {
                test_calculations::token_account_len()
            };

            let expected_balance = if self.token_program_id == spl_token_2022_interface::id() {
                test_calculations::token_2022_account_balance()
            } else {
                test_calculations::token_account_balance()
            };

            self.ctx.process_and_validate_instruction(
                &instruction,
                &[
                    Check::success(),
                    Check::account(&ata_address)
                        .space(expected_len)
                        .owner(&self.token_program_id)
                        .lamports(expected_balance)
                        .build(),
                ],
            );

            self.ata_address = Some(ata_address);
            ata_address
        }

        /// Execute an instruction expecting an error
        pub fn execute_error(
            &self,
            instruction: &solana_program::instruction::Instruction,
            expected_error: ProgramError,
        ) {
            self.ctx
                .process_and_validate_instruction(instruction, &[Check::err(expected_error)]);
        }

        /// Get the current ATA address (if set)
        pub fn ata_address(&self) -> Option<Pubkey> {
            self.ata_address
        }

        /// Insert a specific account into the context
        pub fn insert_account(&self, pubkey: Pubkey, account: Account) {
            self.ctx.account_store.borrow_mut().insert(pubkey, account);
        }

        /// Create a token account with wrong owner at the ATA address (for error testing)
        pub fn insert_wrong_owner_token_account(&self, wrong_owner: Pubkey) -> Pubkey {
            let wallet = self.wallet.as_ref().expect("Wallet must be set");
            let mint = self.mint.expect("Mint must be set");
            let ata_address = get_associated_token_address_with_program_id(
                &wallet.pubkey(),
                &mint,
                &self.token_program_id,
            );

            // Create token account with wrong owner
            let wrong_account = account_builder::AccountBuilder::token_account(
                &mint,
                &wrong_owner,
                0,
                &self.token_program_id,
            );

            self.insert_account(ata_address, wrong_account);
            ctx_ensure_system_accounts_with_lamports(&self.ctx, &[(wrong_owner, 1_000_000)]);

            ata_address
        }

        /// Execute an instruction with a modified account address (for testing non-ATA addresses)
        pub fn execute_with_wrong_account_address(
            &self,
            account_keypair: &Keypair,
            expected_error: ProgramError,
        ) {
            let wallet = self.wallet.as_ref().expect("Wallet must be set");
            let mint = self.mint.expect("Mint must be set");

            // Create a token account at the wrong address
            self.insert_account(
                account_keypair.pubkey(),
                account_builder::AccountBuilder::token_account(
                    &mint,
                    &wallet.pubkey(),
                    0,
                    &self.token_program_id,
                ),
            );

            let mut instruction = build_create_ata_instruction(
                spl_associated_token_account::id(),
                self.payer.pubkey(),
                get_associated_token_address_with_program_id(
                    &wallet.pubkey(),
                    &mint,
                    &self.token_program_id,
                ),
                wallet.pubkey(),
                mint,
                self.token_program_id,
                CreateAtaInstructionType::CreateIdempotent { bump: None },
            );

            // Replace the ATA address with the wrong account address
            instruction.accounts[1] = AccountMeta::new(account_keypair.pubkey(), false);

            self.execute_error(&instruction, expected_error);
        }

        /// Create ATA instruction with custom modifications (for special cases like legacy empty data)
        pub fn create_and_check_ata_with_custom_instruction<F>(
            &mut self,
            instruction_type: CreateAtaInstructionType,
            modify_instruction: F,
        ) -> Pubkey
        where
            F: FnOnce(&mut solana_program::instruction::Instruction),
        {
            let wallet = self.wallet.as_ref().expect("Wallet must be set");
            let mint = self.mint.expect("Mint must be set");
            let ata_address = get_associated_token_address_with_program_id(
                &wallet.pubkey(),
                &mint,
                &self.token_program_id,
            );

            ctx_ensure_system_account_exists(&self.ctx, ata_address, 0);

            let mut instruction = build_create_ata_instruction(
                spl_associated_token_account::id(),
                self.payer.pubkey(),
                ata_address,
                wallet.pubkey(),
                mint,
                self.token_program_id,
                instruction_type,
            );

            // Apply custom modification
            modify_instruction(&mut instruction);

            let expected_len = if self.token_program_id == spl_token_2022_interface::id() {
                test_calculations::token_2022_account_len()
            } else {
                test_calculations::token_account_len()
            };

            let expected_balance = if self.token_program_id == spl_token_2022_interface::id() {
                test_calculations::token_2022_account_balance()
            } else {
                test_calculations::token_account_balance()
            };

            self.ctx.process_and_validate_instruction(
                &instruction,
                &[
                    Check::success(),
                    Check::account(&ata_address)
                        .space(expected_len)
                        .owner(&self.token_program_id)
                        .lamports(expected_balance)
                        .build(),
                ],
            );

            self.ata_address = Some(ata_address);
            ata_address
        }
    }

    /// Encodes the instruction data payload for ATA creation-related instructions.
    pub fn encode_create_ata_instruction_data(
        instruction_type: &CreateAtaInstructionType,
    ) -> Vec<u8> {
        match instruction_type {
            CreateAtaInstructionType::Create { bump, account_len } => {
                let mut data = vec![0]; // Discriminator for Create
                if let Some(b) = bump {
                    data.push(*b);
                    if let Some(len) = account_len {
                        data.extend_from_slice(&len.to_le_bytes());
                    }
                }
                data
            }
            CreateAtaInstructionType::CreateIdempotent { bump } => {
                let mut data = vec![1]; // Discriminator for CreateIdempotent
                if let Some(b) = bump {
                    data.push(*b);
                }
                data
            }
        }
    }

    /// Build a create associated token account instruction with a given discriminator
    pub fn build_create_ata_instruction(
        ata_program_id: Pubkey,
        payer: Pubkey,
        ata_address: Pubkey,
        wallet: Pubkey,
        mint: Pubkey,
        token_program: Pubkey,
        instruction_type: CreateAtaInstructionType,
    ) -> Instruction {
        Instruction {
            program_id: ata_program_id,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new(ata_address, false),
                AccountMeta::new_readonly(wallet, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new_readonly(token_program, false),
                AccountMeta::new_readonly(sysvar::rent::id(), false),
            ],
            data: encode_create_ata_instruction_data(&instruction_type),
        }
    }

    /// Create token account data for mollusk testing
    pub fn create_token_account_data(mint: &[u8; 32], owner: &[u8; 32], amount: u64) -> Vec<u8> {
        let mut data = vec![0u8; TOKEN_ACCOUNT_SIZE];

        // mint
        data[0..32].copy_from_slice(mint);
        // owner
        data[32..64].copy_from_slice(owner);
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

        data
    }

    /// Create mint account data for mollusk testing
    pub fn create_mollusk_mint_data(decimals: u8) -> Vec<u8> {
        let mut data = vec![0u8; MINT_ACCOUNT_SIZE];
        data[0..4].copy_from_slice(&1u32.to_le_bytes()); // state = 1 (Initialized)
        data[44] = decimals;
        data[45] = 1; // is_initialized = 1
        data
    }

    /// Create rent sysvar data for testing
    pub fn create_rent_data(
        lamports_per_byte_year: u64,
        exemption_threshold: f64,
        burn_percent: u8,
    ) -> Vec<u8> {
        lamports_per_byte_year
            .to_le_bytes()
            .into_iter()
            .chain(exemption_threshold.to_le_bytes())
            .chain([burn_percent])
            .collect()
    }

    /// Helper function to update account data in accounts vector after instruction execution
    fn update_account_from_result(
        mollusk: &Mollusk,
        instruction: &Instruction,
        accounts: &mut [(Pubkey, Account)],
        target_pubkey: Pubkey,
    ) {
        if let Some((_, acct)) = mollusk
            .process_instruction(instruction, accounts)
            .resulting_accounts
            .into_iter()
            .find(|(pk, _)| *pk == target_pubkey)
        {
            if let Some((_, a)) = accounts.iter_mut().find(|(pk, _)| *pk == target_pubkey) {
                *a = acct;
            }
        }
    }

    /// Ensures a given `address` exists as a system account with the specified `lamports`.
    /// If an account for `address` already exists, it is left unchanged.
    pub fn ensure_system_account_exists(
        accounts: &mut Vec<(Pubkey, Account)>,
        address: Pubkey,
        lamports: u64,
    ) {
        if !accounts.iter().any(|(pubkey, _)| *pubkey == address) {
            accounts.push((address, Account::new(lamports, 0, &system_program::id())));
        }
    }

    /// Ensures multiple system accounts exist with the provided lamport values.
    /// Each tuple is `(pubkey, lamports)`.
    pub fn ensure_system_accounts_with_lamports(
        accounts: &mut Vec<(Pubkey, Account)>,
        entries: &[(Pubkey, u64)],
    ) {
        for (address, lamports) in entries.iter().copied() {
            ensure_system_account_exists(accounts, address, lamports);
        }
    }

    /// Processes an instruction with Mollusk and merges resulting account updates back into `accounts`.
    /// Returns the `ProgramResult` from the execution for assertions.
    pub fn process_and_merge_instruction(
        mollusk: &Mollusk,
        instruction: &Instruction,
        accounts: &mut Vec<(Pubkey, Account)>,
    ) -> ProgramResult {
        let result = mollusk.process_instruction(instruction, accounts);

        for (updated_pubkey, updated_account) in result.resulting_accounts.into_iter() {
            if let Some((_, existing_account)) =
                accounts.iter_mut().find(|(pk, _)| *pk == updated_pubkey)
            {
                *existing_account = updated_account;
            } else {
                accounts.push((updated_pubkey, updated_account));
            }
        }

        result.program_result
    }

    /// Process and validate an instruction, then merge resulting account updates into `accounts`.
    /// Returns the full `InstructionResult` for optional further inspection.
    pub fn process_and_validate_then_merge(
        mollusk: &Mollusk,
        instruction: &Instruction,
        accounts: &mut Vec<(Pubkey, Account)>,
        checks: &[mollusk_svm::result::Check],
    ) -> mollusk_svm::result::InstructionResult {
        let result = mollusk.process_and_validate_instruction(instruction, accounts, checks);
        merge_resulting_accounts(accounts, &result);
        result
    }

    /// Merge resulting accounts from a Mollusk `InstructionResult` back into the in-memory `accounts` list.
    pub fn merge_resulting_accounts(
        accounts: &mut Vec<(Pubkey, Account)>,
        result: &mollusk_svm::result::InstructionResult,
    ) {
        for (updated_pubkey, updated_account) in result.resulting_accounts.clone().into_iter() {
            if let Some((_, existing_account)) =
                accounts.iter_mut().find(|(pk, _)| *pk == updated_pubkey)
            {
                *existing_account = updated_account;
            } else {
                accounts.push((updated_pubkey, updated_account));
            }
        }
    }

    /// Returns a cloned `Account` for the given `pubkey` from the in-memory `accounts` list.
    /// Panics with a clear message if the account is not present.
    pub fn get_account(accounts: &[(Pubkey, Account)], pubkey: Pubkey) -> Account {
        accounts
            .iter()
            .find(|(pk, _)| *pk == pubkey)
            .expect("account not found")
            .1
            .clone()
    }

    /// Creates an associated token account via the ATA program and merges account updates.
    /// Returns `(associated_token_address, program_result)`.
    #[allow(dead_code)]
    pub fn create_associated_token_account_mollusk(
        mollusk: &Mollusk,
        accounts: &mut Vec<(Pubkey, Account)>,
        payer: &Keypair,
        owner: &Pubkey,
        mint: &Pubkey,
        token_program: &Pubkey,
    ) -> (Pubkey, ProgramResult) {
        let ata_address = get_associated_token_address_with_program_id(owner, mint, token_program);

        // Ensure the provided owner (wallet) account exists in the accounts list for Mollusk metas
        ensure_system_account_exists(accounts, *owner, 0);

        // Ensure placeholder system account for ATA address
        ensure_system_account_exists(accounts, ata_address, 0);

        let instruction = build_create_ata_instruction(
            spl_associated_token_account::id(),
            payer.pubkey(),
            ata_address,
            *owner,
            *mint,
            *token_program,
            CreateAtaInstructionType::default(),
        );

        let mollusk_result = process_and_merge_instruction(mollusk, &instruction, accounts);
        (ata_address, mollusk_result)
    }

    /// Ensures all recover-nested derived ATAs exist as system accounts: owner ATA, destination ATA, nested ATA.
    #[allow(dead_code)]
    pub fn ensure_recover_nested_accounts(
        accounts: &mut Vec<(Pubkey, Account)>,
        wallet_address: &Pubkey,
        nested_token_mint_address: &Pubkey,
        owner_token_mint_address: &Pubkey,
        token_program: &Pubkey,
    ) {
        let owner_ata = get_associated_token_address_with_program_id(
            wallet_address,
            owner_token_mint_address,
            token_program,
        );
        let destination_ata = get_associated_token_address_with_program_id(
            wallet_address,
            nested_token_mint_address,
            token_program,
        );
        let nested_ata = get_associated_token_address_with_program_id(
            &owner_ata,
            nested_token_mint_address,
            token_program,
        );
        for ata in [owner_ata, destination_ata, nested_ata] {
            ensure_system_account_exists(accounts, ata, 0);
        }
    }

    /// Ensures executable program and sysvar accounts are present.
    /// Provide any mix of program IDs (e.g., token-2022, token classic, ATA, system) and it will add executable accounts.
    /// Always ensures rent sysvar is present.
    #[allow(dead_code)]
    pub fn ensure_program_accounts_present(
        accounts: &mut Vec<(Pubkey, Account)>,
        program_ids: &[Pubkey],
    ) {
        for pid in program_ids.iter().copied() {
            if !accounts.iter().any(|(pk, _)| *pk == pid) {
                accounts.push((
                    pid,
                    mollusk_svm::program::create_program_account_loader_v3(&pid),
                ));
            }
        }
        if !accounts.iter().any(|(pk, _)| *pk == sysvar::rent::id()) {
            accounts.push((
                sysvar::rent::id(),
                account_builder::AccountBuilder::rent_sysvar(),
            ));
        }
    }

    /// Convenience: build and process a mint_to, merging updates, and assert success.
    #[allow(dead_code)]
    pub fn mint_to_and_merge(
        mollusk: &Mollusk,
        accounts: &mut Vec<(Pubkey, Account)>,
        token_program: &Pubkey,
        mint: &Pubkey,
        destination: &Pubkey,
        authority: &Pubkey,
        amount: u64,
    ) -> ProgramResult {
        let ix = spl_token_2022_interface::instruction::mint_to(
            token_program,
            mint,
            destination,
            authority,
            &[],
            amount,
        )
        .unwrap();
        process_and_merge_instruction(mollusk, &ix, accounts)
    }

    /// Creates and initializes a mint account with the given parameters.
    /// Returns a vector of accounts including the initialized mint and all necessary
    /// base accounts for testing.
    pub fn create_test_mint(
        mollusk: &Mollusk,
        mint_account: &Keypair,
        mint_authority: &Keypair,
        payer: &Keypair,
        token_program: &Pubkey,
        decimals: u8,
    ) -> Vec<(Pubkey, Account)> {
        let mint_space = MINT_ACCOUNT_SIZE as u64;
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
        mollusk.process_and_validate_instruction(
            &create_mint_ix,
            &accounts,
            &[mollusk_svm::result::Check::success()],
        );
        let init_mint_ix = if *token_program == spl_token_2022_interface::id() {
            spl_token_2022_interface::instruction::initialize_mint(
                token_program,
                &mint_account.pubkey(),
                &mint_authority.pubkey(),
                Some(&mint_authority.pubkey()),
                decimals,
            )
            .unwrap()
        } else {
            spl_token_interface::instruction::initialize_mint(
                token_program,
                &mint_account.pubkey(),
                &mint_authority.pubkey(),
                Some(&mint_authority.pubkey()),
                decimals,
            )
            .unwrap()
        };

        // Refresh the mint account data after creation.
        update_account_from_result(
            mollusk,
            &create_mint_ix,
            &mut accounts,
            mint_account.pubkey(),
        );

        mollusk.process_and_validate_instruction(
            &init_mint_ix,
            &accounts,
            &[mollusk_svm::result::Check::success()],
        );

        // Final refresh so callers see the initialized state.
        update_account_from_result(mollusk, &init_mint_ix, &mut accounts, mint_account.pubkey());

        accounts
    }

    pub mod account_builder {
        use {
            super::{create_mollusk_mint_data, create_token_account_data},
            mollusk_svm::Mollusk,
            solana_program::{pubkey::Pubkey, sysvar::rent},
            solana_sdk::account::Account,
            std::vec::Vec,
        };

        pub struct AccountBuilder;

        impl AccountBuilder {
            pub fn rent_sysvar() -> Account {
                let mollusk = Mollusk::default();
                let (_, mollusk_rent_account) = mollusk.sysvars.keyed_account_for_rent_sysvar();

                Account {
                    lamports: mollusk_rent_account.lamports,
                    data: mollusk_rent_account.data,
                    owner: rent::id(),
                    executable: false,
                    rent_epoch: 0,
                }
            }
            #[allow(dead_code, reason = "exported for benchmarking consumers")]
            pub fn system_account(lamports: u64) -> Account {
                Account {
                    lamports,
                    data: Vec::new(),
                    owner: solana_system_interface::program::id(),
                    executable: false,
                    rent_epoch: 0,
                }
            }

            #[allow(dead_code, reason = "exported for benchmarking consumers")]
            pub fn executable_program(loader: Pubkey) -> Account {
                Account {
                    lamports: 0,
                    data: Vec::new(),
                    owner: loader,
                    executable: true,
                    rent_epoch: 0,
                }
            }

            #[allow(dead_code, reason = "exported for benchmarking consumers")]
            pub fn mint(decimals: u8, _mint_authority: &Pubkey) -> Account {
                let data = create_mollusk_mint_data(decimals);
                let rent = solana_sdk::rent::Rent::default();
                Account {
                    lamports: rent.minimum_balance(data.len()),
                    data,
                    owner: spl_token_interface::id(),
                    executable: false,
                    rent_epoch: 0,
                }
            }

            #[allow(dead_code, reason = "exported for benchmarking consumers")]
            pub fn extended_mint(decimals: u8, _mint_authority: &Pubkey) -> Account {
                use solana_program_option::COption;
                use spl_token_2022_interface::{
                    extension::{ExtensionType, PodStateWithExtensionsMut},
                    pod::PodMint,
                };

                // Calculate the minimum size for a Token-2022 mint without extensions
                let required_size = ExtensionType::try_calculate_account_len::<
                    spl_token_2022_interface::state::Mint,
                >(&[])
                .expect("Failed to calculate Token-2022 mint size");

                let mut data = vec![0u8; required_size];

                // Use Token-2022's proper unpacking to initialize the mint
                let mint = PodStateWithExtensionsMut::<PodMint>::unpack_uninitialized(&mut data)
                    .expect("Failed to unpack Token-2022 mint");

                // Initialize base mint fields
                mint.base.mint_authority = COption::None.into();
                mint.base.supply = 0u64.into();
                mint.base.decimals = decimals;
                mint.base.is_initialized = true.into();
                mint.base.freeze_authority = COption::None.into();

                let rent = solana_sdk::rent::Rent::default();
                Account {
                    lamports: rent.minimum_balance(data.len()),
                    data,
                    owner: spl_token_2022_interface::id(),
                    executable: false,
                    rent_epoch: 0,
                }
            }

            pub fn token_account(
                mint: &Pubkey,
                owner: &Pubkey,
                amount: u64,
                token_program: &Pubkey,
            ) -> Account {
                let data = create_token_account_data(&mint.to_bytes(), &owner.to_bytes(), amount);
                let rent = solana_sdk::rent::Rent::default();
                Account {
                    lamports: rent.minimum_balance(data.len()),
                    data,
                    owner: *token_program,
                    executable: false,
                    rent_epoch: 0,
                }
            }
        }
    }
}

// Re-exports removed to avoid unused import warnings. Import from
// `crate::utils::legacy_utils::...` at call sites instead.
