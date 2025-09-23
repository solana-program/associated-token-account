use {
    mollusk_svm::{program::loader_keys::LOADER_V3, result::Check, Mollusk, MolluskContext},
    solana_account::Account,
    solana_instruction::{AccountMeta, Instruction},
    solana_program_error::ProgramError,
    solana_program_option::COption,
    solana_program_pack::Pack,
    solana_pubkey::Pubkey,
    solana_rent::Rent,
    solana_system_interface::program as system_program,
    solana_sysvar::rent,
    spl_associated_token_account_interface::address::get_associated_token_address_with_program_id,
    spl_token_2022_interface::{extension::ExtensionType, state::Account as Token2022Account},
    spl_token_interface::{state::Account as TokenAccount, state::AccountState, state::Mint},
    std::{collections::HashMap, vec::Vec},
};

/// Setup mollusk with local ATA and token programs
pub fn setup_mollusk_with_programs(token_program_id: &Pubkey) -> Mollusk {
    let ata_program_id = spl_associated_token_account_interface::program::id();
    let mut mollusk = Mollusk::new(&ata_program_id, "spl_associated_token_account");

    if *token_program_id == spl_token_2022_interface::id() {
        mollusk.add_program(token_program_id, "spl_token_2022", &LOADER_V3);
    } else {
        mollusk.add_program(token_program_id, "pinocchio_token_program", &LOADER_V3);
    }

    mollusk
}

/// The type of ATA creation instruction to build.
#[derive(Debug)]
pub enum CreateAtaInstructionType {
    /// The standard `Create` instruction, which can optionally include a bump seed and account length.
    Create {
        bump: Option<u8>,
        account_len: Option<u16>,
    },
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

/// Calculate the expected account length for a Token-2022 account with `ImmutableOwner` extension
pub fn token_2022_immutable_owner_account_len() -> usize {
    ExtensionType::try_calculate_account_len::<Token2022Account>(&[ExtensionType::ImmutableOwner])
        .expect("Failed to calculate Token-2022 account length")
}

/// Calculate the rent-exempt balance for a Token-2022 account with `ImmutableOwner` extension
pub fn token_2022_immutable_owner_rent_exempt_balance() -> u64 {
    Rent::default().minimum_balance(token_2022_immutable_owner_account_len())
}

/// Calculate the rent-exempt balance for a standard SPL token account
pub fn token_account_rent_exempt_balance() -> u64 {
    Rent::default().minimum_balance(TokenAccount::LEN)
}

/// Test harness for ATA testing scenarios
pub struct AtaTestHarness {
    pub ctx: MolluskContext<HashMap<Pubkey, Account>>,
    pub token_program_id: Pubkey,
    pub payer: Pubkey,
    pub wallet: Option<Pubkey>,
    pub mint: Option<Pubkey>,
    pub mint_authority: Option<Pubkey>,
    pub ata_address: Option<Pubkey>,
}

impl AtaTestHarness {
    /// Ensure an account exists in the context store with the given lamports.
    /// If the account does not exist, it will be created as a system account.
    /// However, this can be called on a non-system account (to be used for
    /// example when testing accidental nested owners).
    pub fn ensure_account_exists_with_lamports(&self, address: Pubkey, lamports: u64) {
        let mut store = self.ctx.account_store.borrow_mut();
        if let Some(existing) = store.get_mut(&address) {
            if existing.lamports < lamports {
                existing.lamports = lamports;
            }
        } else {
            store.insert(address, AccountBuilder::system_account(lamports));
        }
    }

    /// Ensure multiple system accounts exist in the context store with the provided lamports
    pub fn ensure_system_accounts_with_lamports(&self, entries: &[(Pubkey, u64)]) {
        for (address, lamports) in entries.iter().copied() {
            self.ensure_account_exists_with_lamports(address, lamports);
        }
    }

    /// Internal: create the mint account owned by the token program with given space
    fn create_mint_account(&mut self, mint_account: Pubkey, space: usize, mint_program_id: Pubkey) {
        let mint_rent = Rent::default().minimum_balance(space);
        let create_mint_ix = solana_system_interface::instruction::create_account(
            &self.payer,
            &mint_account,
            mint_rent,
            space as u64,
            &mint_program_id,
        );

        self.ctx
            .process_and_validate_instruction(&create_mint_ix, &[Check::success()]);
    }

    /// Create a new test harness with the specified token program
    pub fn new(token_program_id: &Pubkey) -> Self {
        let mollusk = setup_mollusk_with_programs(token_program_id);
        let payer = Pubkey::new_unique();
        let ctx = mollusk.with_context(HashMap::new());

        let harness = Self {
            ctx,
            token_program_id: *token_program_id,
            payer,
            wallet: None,
            mint: None,
            mint_authority: None,
            ata_address: None,
        };
        harness.ensure_account_exists_with_lamports(payer, 10_000_000_000);
        harness
    }

    /// Add a wallet with the specified lamports
    pub fn with_wallet(mut self, lamports: u64) -> Self {
        let wallet = Pubkey::new_unique();
        self.ensure_system_accounts_with_lamports(&[(wallet, lamports)]);
        self.wallet = Some(wallet);
        self
    }

    /// Add an additional wallet (e.g. for sender/receiver scenarios) - returns harness and the new wallet
    pub fn with_additional_wallet(self, lamports: u64) -> (Self, Pubkey) {
        let additional_wallet = Pubkey::new_unique();
        self.ensure_system_accounts_with_lamports(&[(additional_wallet, lamports)]);
        (self, additional_wallet)
    }

    /// Create and initialize a mint with the specified decimals
    pub fn with_mint(mut self, decimals: u8) -> Self {
        let [mint_authority, mint_account] = [Pubkey::new_unique(); 2];

        self.create_mint_account(mint_account, Mint::LEN, self.token_program_id);

        self.mint = Some(mint_account);
        self.mint_authority = Some(mint_authority);
        self.initialize_mint(decimals)
    }

    /// Create and initialize a Token-2022 mint with specific extensions
    pub fn with_mint_with_extensions(mut self, extensions: &[ExtensionType]) -> Self {
        if self.token_program_id != spl_token_2022_interface::id() {
            panic!("with_mint_with_extensions() can only be used with Token-2022 program");
        }

        let [mint_authority, mint_account] = [Pubkey::new_unique(); 2];

        // Calculate space needed for extensions
        let space =
            ExtensionType::try_calculate_account_len::<spl_token_2022_interface::state::Mint>(
                extensions,
            )
            .expect("Failed to calculate mint space with extensions");

        self.create_mint_account(mint_account, space, spl_token_2022_interface::id());

        self.mint = Some(mint_account);
        self.mint_authority = Some(mint_authority);
        self
    }

    /// Initialize transfer fee extension on the current mint (requires Token-2022 mint with `TransferFeeConfig` extension)
    pub fn initialize_transfer_fee(self, transfer_fee_basis_points: u16, maximum_fee: u64) -> Self {
        let mint = self.mint.expect("Mint must be set");
        let mint_authority = self.mint_authority.expect("Mint authority must be set");

        let init_fee_ix = spl_token_2022_interface::extension::transfer_fee::instruction::initialize_transfer_fee_config(
            &spl_token_2022_interface::id(),
            &mint,
            Some(&mint_authority),
            Some(&mint_authority),
            transfer_fee_basis_points,
            maximum_fee,
        )
        .expect("Failed to create initialize_transfer_fee_config instruction");

        self.ctx
            .process_and_validate_instruction(&init_fee_ix, &[Check::success()]);
        self
    }

    /// Initialize mint (must be called after extensions are initialized)
    pub fn initialize_mint(self, decimals: u8) -> Self {
        let mint = self.mint.expect("Mint must be set");
        let mint_authority = self.mint_authority.expect("Mint authority must be set");

        let init_mint_ix = spl_token_2022_interface::instruction::initialize_mint(
            &self.token_program_id,
            &mint,
            &mint_authority,
            Some(&mint_authority),
            decimals,
        )
        .expect("Failed to create initialize_mint instruction");

        self.ctx
            .process_and_validate_instruction(&init_mint_ix, &[Check::success()]);
        self
    }

    /// Create an ATA for the wallet and mint (requires wallet and mint to be set)
    pub fn with_ata(mut self) -> Self {
        let wallet = self.wallet.expect("Wallet must be set before creating ATA");
        let mint = self.mint.expect("Mint must be set before creating ATA");

        let ata_address =
            get_associated_token_address_with_program_id(&wallet, &mint, &self.token_program_id);

        let instruction = build_create_ata_instruction(
            spl_associated_token_account_interface::program::id(),
            self.payer,
            ata_address,
            wallet,
            mint,
            self.token_program_id,
            CreateAtaInstructionType::default(),
        );

        self.ctx
            .process_and_validate_instruction(&instruction, &[Check::success()]);

        self.ata_address = Some(ata_address);
        self
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

    /// Mint tokens to the ATA (requires `mint`, `mint_authority` and `ata_address` to be set)
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

        let mint_to_ix = spl_token_2022_interface::instruction::mint_to(
            &self.token_program_id,
            &mint,
            &destination,
            mint_authority,
            &[],
            amount,
        )
        .unwrap();

        self.ctx
            .process_and_validate_instruction(&mint_to_ix, &[Check::success()]);
    }

    /// Build a create ATA instruction for the current wallet and mint
    pub fn build_create_ata_instruction(
        &mut self,
        instruction_type: CreateAtaInstructionType,
    ) -> solana_instruction::Instruction {
        let wallet = self.wallet.expect("Wallet must be set");
        let mint = self.mint.expect("Mint must be set");
        let ata_address =
            get_associated_token_address_with_program_id(&wallet, &mint, &self.token_program_id);

        self.ata_address = Some(ata_address);

        build_create_ata_instruction(
            spl_associated_token_account_interface::program::id(),
            self.payer,
            ata_address,
            wallet,
            mint,
            self.token_program_id,
            instruction_type,
        )
    }

    /// Create an ATA for any owner. Ensure the owner exists as a system account,
    /// creating it with the given lamports if it does not exist.
    pub fn create_ata_for_owner(&mut self, owner: Pubkey, owner_lamports: u64) -> Pubkey {
        let mint = self.mint.expect("Mint must be set");
        self.ensure_system_accounts_with_lamports(&[(owner, owner_lamports)]);

        let ata_address =
            get_associated_token_address_with_program_id(&owner, &mint, &self.token_program_id);

        let instruction = build_create_ata_instruction(
            spl_associated_token_account_interface::program::id(),
            self.payer,
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

    /// Build a `recover_nested` instruction and ensure all required accounts exist
    pub fn build_recover_nested_instruction(
        &mut self,
        owner_mint: Pubkey,
        nested_mint: Pubkey,
    ) -> solana_instruction::Instruction {
        let wallet = self.wallet.as_ref().expect("Wallet must be set");

        spl_associated_token_account_interface::instruction::recover_nested(
            wallet,
            &owner_mint,
            &nested_mint,
            &self.token_program_id,
        )
    }

    /// Add a wallet and mint (convenience method)
    pub fn with_wallet_and_mint(self, wallet_lamports: u64, decimals: u8) -> Self {
        self.with_wallet(wallet_lamports).with_mint(decimals)
    }

    /// Build and execute a create ATA instruction
    pub fn create_ata(&mut self, instruction_type: CreateAtaInstructionType) -> Pubkey {
        let wallet = self.wallet.expect("Wallet must be set");
        let mint = self.mint.expect("Mint must be set");
        let ata_address =
            get_associated_token_address_with_program_id(&wallet, &mint, &self.token_program_id);

        let instruction = build_create_ata_instruction(
            spl_associated_token_account_interface::program::id(),
            self.payer,
            ata_address,
            wallet,
            mint,
            self.token_program_id,
            instruction_type,
        );

        let expected_len = if self.token_program_id == spl_token_2022_interface::id() {
            token_2022_immutable_owner_account_len()
        } else {
            TokenAccount::LEN
        };

        let expected_balance = if self.token_program_id == spl_token_2022_interface::id() {
            token_2022_immutable_owner_rent_exempt_balance()
        } else {
            token_account_rent_exempt_balance()
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

    /// Create a token account with wrong owner at the ATA address (for error testing)
    pub fn insert_wrong_owner_token_account(&self, wrong_owner: Pubkey) -> Pubkey {
        let wallet = self.wallet.as_ref().expect("Wallet must be set");
        let mint = self.mint.expect("Mint must be set");
        self.ensure_system_accounts_with_lamports(&[(wrong_owner, 1_000_000)]);
        let ata_address =
            get_associated_token_address_with_program_id(wallet, &mint, &self.token_program_id);
        // Create token account with wrong owner at the ATA address
        let wrong_account =
            AccountBuilder::token_account(&mint, &wrong_owner, 0, &self.token_program_id);
        self.ctx
            .account_store
            .borrow_mut()
            .insert(ata_address, wrong_account);
        ata_address
    }

    /// Execute an instruction with a modified account address (for testing non-ATA addresses)
    pub fn execute_with_wrong_account_address(
        &self,
        wrong_account: Pubkey,
        expected_error: ProgramError,
    ) {
        let wallet = self.wallet.expect("Wallet must be set");
        let mint = self.mint.expect("Mint must be set");

        // Create a token account at the wrong address
        self.ctx.account_store.borrow_mut().insert(
            wrong_account,
            AccountBuilder::token_account(&mint, &wallet, 0, &self.token_program_id),
        );

        let mut instruction = build_create_ata_instruction(
            spl_associated_token_account_interface::program::id(),
            self.payer,
            get_associated_token_address_with_program_id(&wallet, &mint, &self.token_program_id),
            wallet,
            mint,
            self.token_program_id,
            CreateAtaInstructionType::CreateIdempotent { bump: None },
        );

        // Replace the ATA address with the wrong account address
        instruction.accounts[1] = AccountMeta::new(wrong_account, false);

        self.ctx
            .process_and_validate_instruction(&instruction, &[Check::err(expected_error)]);
    }

    /// Create ATA instruction with custom modifications (for special cases like legacy empty data)
    pub fn create_and_check_ata_with_custom_instruction<F>(
        &mut self,
        instruction_type: CreateAtaInstructionType,
        modify_instruction: F,
    ) -> Pubkey
    where
        F: FnOnce(&mut solana_instruction::Instruction),
    {
        let wallet = self.wallet.expect("Wallet must be set");
        let mint = self.mint.expect("Mint must be set");
        let ata_address =
            get_associated_token_address_with_program_id(&wallet, &mint, &self.token_program_id);

        let mut instruction = build_create_ata_instruction(
            spl_associated_token_account_interface::program::id(),
            self.payer,
            ata_address,
            wallet,
            mint,
            self.token_program_id,
            instruction_type,
        );

        // Apply custom modification
        modify_instruction(&mut instruction);

        let expected_len = if self.token_program_id == spl_token_2022_interface::id() {
            token_2022_immutable_owner_account_len()
        } else {
            TokenAccount::LEN
        };

        let expected_balance = if self.token_program_id == spl_token_2022_interface::id() {
            token_2022_immutable_owner_rent_exempt_balance()
        } else {
            token_account_rent_exempt_balance()
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
pub fn encode_create_ata_instruction_data(instruction_type: &CreateAtaInstructionType) -> Vec<u8> {
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
            AccountMeta::new_readonly(rent::id(), false),
        ],
        data: encode_create_ata_instruction_data(&instruction_type),
    }
}

pub struct AccountBuilder;

impl AccountBuilder {
    pub fn system_account(lamports: u64) -> Account {
        Account {
            lamports,
            data: Vec::new(),
            owner: solana_system_interface::program::id(),
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
        let account_data = TokenAccount {
            mint: *mint,
            owner: *owner,
            amount,
            delegate: COption::None,
            state: AccountState::Initialized,
            is_native: COption::None,
            delegated_amount: 0,
            close_authority: COption::None,
        };

        if *token_program == spl_token_2022_interface::id() {
            mollusk_svm_programs_token::token2022::create_account_for_token_account(account_data)
        } else {
            mollusk_svm_programs_token::token::create_account_for_token_account(account_data)
        }
    }
}
