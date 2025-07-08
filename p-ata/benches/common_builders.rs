use {
    mollusk_svm::program::loader_keys::LOADER_V3,
    solana_account::Account,
    solana_instruction::{AccountMeta, Instruction},
    solana_pubkey::Pubkey,
    solana_sysvar::rent,
    spl_token_2022::extension::ExtensionType,
};

// Import types from parent crate's common module
use crate::{
    AccountBuilder, AtaImplementation, BaseTestType, TestVariant, NATIVE_LOADER_ID,
    SYSTEM_PROGRAM_ID,
};

// Helper function for topup accounts
fn modify_account_for_topup(account: &mut Account) {
    account.lamports = 1_000_000; // Some lamports but below rent-exempt
    account.data = vec![]; // No data allocated
    account.owner = SYSTEM_PROGRAM_ID; // Still system-owned
}

// ======================= CONSOLIDATED TEST CASE BUILDERS =======================

/// Configuration for building test cases
#[derive(Debug, Clone)]
pub struct TestCaseConfig {
    pub base_test: BaseTestType,
    pub token_program: Pubkey,
    pub instruction_discriminator: u8,
    pub use_extended_mint: bool,
    pub setup_topup: bool,
    pub setup_existing_ata: bool,
    pub use_fixed_accounts: bool,
    pub special_account_mods: Vec<SpecialAccountMod>,
    pub failure_mode: Option<FailureMode>,
}

/// Special account modifications for specific test cases
#[derive(Debug, Clone)]
pub enum SpecialAccountMod {
    MultisigWallet {
        threshold: u8,
        signers: Vec<Pubkey>,
    },
    NestedAta {
        owner_mint: Pubkey,
        nested_mint: Pubkey,
    },
    FixedAddresses {
        wallet: Pubkey,
        mint: Pubkey,
    },
}

/// Failure modes for deliberate test failures
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum FailureMode {
    /// Payer owned by wrong program (not system program)
    WrongPayerOwner(Pubkey),
    /// Payer not marked as signer
    PayerNotSigned,
    /// Wrong system program ID
    WrongSystemProgram(Pubkey),
    /// Wrong token program ID
    WrongTokenProgram(Pubkey),
    /// Insufficient funds for payer
    InsufficientFunds(u64),
    /// Wrong ATA address (not derived correctly)
    WrongAtaAddress(Pubkey),
    /// Mint owned by wrong program
    MintWrongOwner(Pubkey),
    /// Invalid mint structure (wrong size)
    InvalidMintStructure(usize),
    /// Invalid token account structure
    InvalidTokenAccountStructure,
    /// Invalid instruction discriminator
    InvalidDiscriminator(u8),
    /// Invalid bump value
    InvalidBumpValue(u8),
    /// ATA owned by wrong program
    AtaWrongOwner(Pubkey),
    /// ATA marked as non-writable
    AtaNotWritable,
    /// Token account wrong size
    TokenAccountWrongSize(usize),
    /// Token account points to wrong mint
    TokenAccountWrongMint(Pubkey),
    /// Token account has wrong owner
    TokenAccountWrongOwner(Pubkey),
    /// Account size wrong for extensions
    WrongAccountSizeForExtensions(usize),
    /// Missing required extensions
    MissingExtensions,
    /// Invalid extension data
    InvalidExtensionData,
    /// Recover: wallet not signer
    RecoverWalletNotSigner,
    /// Recover: multisig insufficient signers
    RecoverMultisigInsufficientSigners,
    /// Recover: wrong nested ATA address
    RecoverWrongNestedAta(Pubkey),
    /// Recover: wrong destination address
    RecoverWrongDestination(Pubkey),
    /// Recover: nested account wrong owner
    RecoverNestedWrongOwner(Pubkey),
    /// Invalid multisig data
    InvalidMultisigData,
    /// Invalid signer accounts (not in multisig list)
    InvalidSignerAccounts(Vec<Pubkey>),
    /// Uninitialized multisig
    UninitializedMultisig,
}

/// Consolidated test case builder that replaces repetitive build_*_variant methods
pub struct CommonTestCaseBuilder;

impl CommonTestCaseBuilder {
    /// Main entry point
    #[allow(dead_code)]
    pub fn build_test_case(
        base_test: BaseTestType,
        variant: TestVariant,
        ata_implementation: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let config = Self::get_config_for_test(base_test, token_program_id);
        Self::build_with_config(config, variant, ata_implementation)
    }

    /// Build a failure test case with the specified failure mode
    #[allow(dead_code)]
    pub fn build_failure_test_case(
        base_test: BaseTestType,
        variant: TestVariant,
        ata_implementation: &AtaImplementation,
        token_program_id: &Pubkey,
        failure_mode: FailureMode,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let mut config = Self::get_config_for_test(base_test, token_program_id);
        config.failure_mode = Some(failure_mode);
        Self::build_with_config(config, variant, ata_implementation)
    }

    /// Get configuration for each test type
    fn get_config_for_test(base_test: BaseTestType, token_program_id: &Pubkey) -> TestCaseConfig {
        match base_test {
            BaseTestType::Create => TestCaseConfig {
                base_test,
                token_program: *token_program_id,
                instruction_discriminator: 0,
                use_extended_mint: false,
                setup_topup: false,
                setup_existing_ata: false,
                use_fixed_accounts: false,
                special_account_mods: vec![],
                failure_mode: None,
            },
            BaseTestType::CreateIdempotent => TestCaseConfig {
                base_test,
                token_program: *token_program_id,
                instruction_discriminator: 1,
                use_extended_mint: false,
                setup_topup: false,
                setup_existing_ata: true, // ATA already exists
                use_fixed_accounts: false,
                special_account_mods: vec![],
                failure_mode: None,
            },
            BaseTestType::CreateTopup => TestCaseConfig {
                base_test,
                token_program: *token_program_id,
                instruction_discriminator: 0,
                use_extended_mint: false,
                setup_topup: true,
                setup_existing_ata: false,
                use_fixed_accounts: false,
                special_account_mods: vec![],
                failure_mode: None,
            },
            BaseTestType::CreateTopupNoCap => TestCaseConfig {
                base_test,
                token_program: *token_program_id,
                instruction_discriminator: 0,
                use_extended_mint: false,
                setup_topup: true,
                setup_existing_ata: false,
                use_fixed_accounts: false,
                special_account_mods: vec![],
                failure_mode: None,
            },
            BaseTestType::CreateToken2022 => TestCaseConfig {
                base_test,
                token_program: Pubkey::new_from_array(pinocchio_pubkey::pubkey!(
                    "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
                )),
                instruction_discriminator: 0,
                use_extended_mint: true,
                setup_topup: false,
                setup_existing_ata: false,
                use_fixed_accounts: false,
                special_account_mods: vec![],
                failure_mode: None,
            },
            BaseTestType::RecoverNested => TestCaseConfig {
                base_test,
                token_program: *token_program_id,
                instruction_discriminator: 2,
                use_extended_mint: false,
                setup_topup: false,
                setup_existing_ata: false,
                use_fixed_accounts: true,
                special_account_mods: vec![SpecialAccountMod::NestedAta {
                    // No need to explicitly use AtaVariant::Original anymore
                    // structured_pk now automatically uses consistent addresses for mint types
                    owner_mint: crate::common::structured_pk(
                        &crate::common::AtaVariant::PAtaLegacy, // Can use any variant now
                        crate::common::TestBankId::Benchmarks,
                        base_test as u8,
                        crate::common::AccountTypeId::OwnerMint,
                    ),
                    nested_mint: crate::common::structured_pk(
                        &crate::common::AtaVariant::PAtaLegacy, // Can use any variant now
                        crate::common::TestBankId::Benchmarks,
                        base_test as u8,
                        crate::common::AccountTypeId::NestedMint,
                    ),
                }],
                failure_mode: None,
            },
            BaseTestType::RecoverMultisig => TestCaseConfig {
                base_test,
                token_program: *token_program_id,
                instruction_discriminator: 2,
                use_extended_mint: false,
                setup_topup: false,
                setup_existing_ata: false,
                use_fixed_accounts: true,
                special_account_mods: vec![
                    SpecialAccountMod::NestedAta {
                        // No need to explicitly use AtaVariant::Original anymore
                        // structured_pk now automatically uses consistent addresses for mint types
                        owner_mint: crate::common::structured_pk(
                            &crate::common::AtaVariant::PAtaLegacy, // Can use any variant now
                            crate::common::TestBankId::Benchmarks,
                            base_test as u8,
                            crate::common::AccountTypeId::OwnerMint,
                        ),
                        nested_mint: crate::common::structured_pk(
                            &crate::common::AtaVariant::PAtaLegacy, // Can use any variant now
                            crate::common::TestBankId::Benchmarks,
                            base_test as u8,
                            crate::common::AccountTypeId::NestedMint,
                        ),
                    },
                    SpecialAccountMod::MultisigWallet {
                        threshold: 2,
                        signers: vec![
                            Pubkey::new_unique(),
                            Pubkey::new_unique(),
                            Pubkey::new_unique(),
                        ],
                    },
                ],
                failure_mode: None,
            },
            BaseTestType::WorstCase => TestCaseConfig {
                base_test,
                token_program: *token_program_id,
                instruction_discriminator: 0,
                use_extended_mint: false,
                setup_topup: false,
                setup_existing_ata: false,
                use_fixed_accounts: true,
                special_account_mods: vec![SpecialAccountMod::FixedAddresses {
                    wallet: crate::common::structured_pk(
                        &crate::common::AtaVariant::SplAta,
                        crate::common::TestBankId::Benchmarks,
                        base_test as u8,
                        crate::common::AccountTypeId::Wallet,
                    ),
                    mint: crate::common::structured_pk(
                        &crate::common::AtaVariant::SplAta,
                        crate::common::TestBankId::Benchmarks,
                        base_test as u8,
                        crate::common::AccountTypeId::Mint,
                    ),
                }],
                failure_mode: None,
            },
        }
    }

    /// Build test case with given configuration
    fn build_with_config(
        config: TestCaseConfig,
        variant: TestVariant,
        ata_implementation: &AtaImplementation,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        // Use structured addressing to prevent cross-contamination
        let test_bank = if config.failure_mode.is_some() {
            crate::common::TestBankId::Failures
        } else {
            crate::common::TestBankId::Benchmarks
        };
        let test_number = if matches!(
            config.base_test,
            BaseTestType::RecoverNested | BaseTestType::RecoverMultisig
        ) {
            // For recovery tests, use base test number (no variant offset) to ensure same addresses
            calculate_test_number(config.base_test, TestVariant::BASE, config.setup_topup)
        } else {
            calculate_test_number(config.base_test, variant, config.setup_topup)
        };

        let (payer, mint, wallet) = Self::get_structured_addresses(
            &config,
            &ata_implementation.variant,
            test_bank,
            test_number,
            ata_implementation,
        );

        // The processor will always use instruction.program_id for PDA operations
        let derivation_program_id = ata_implementation.program_id;

        let (ata, bump) = if matches!(
            config.base_test,
            BaseTestType::RecoverNested | BaseTestType::RecoverMultisig
        ) {
            // For recover operations, we need to use the SAME wallet that will be used in the accounts
            // Get the actual wallet that will be used (with optimal bump for owner_mint)
            let (owner_mint, _) = if let Some(SpecialAccountMod::NestedAta {
                owner_mint,
                nested_mint,
            }) = config
                .special_account_mods
                .iter()
                .find(|m| matches!(m, SpecialAccountMod::NestedAta { .. }))
            {
                (*owner_mint, *nested_mint)
            } else {
                (
                    crate::common::structured_pk(
                        &ata_implementation.variant,
                        test_bank,
                        test_number,
                        crate::common::AccountTypeId::OwnerMint,
                    ),
                    crate::common::structured_pk(
                        &ata_implementation.variant,
                        test_bank,
                        test_number,
                        crate::common::AccountTypeId::NestedMint,
                    ),
                )
            };

            // Use the SAME wallet calculation as in build_recover_accounts
            // For recovery tests, use a simple consistent wallet address (no optimal bump needed)
            let actual_wallet = crate::common::structured_pk(
                &ata_implementation.variant,
                test_bank,
                test_number,
                crate::common::AccountTypeId::Wallet,
            );

            // Calculate owner_ata address using the actual wallet that will be used
            Pubkey::find_program_address(
                &[
                    actual_wallet.as_ref(),
                    config.token_program.as_ref(),
                    owner_mint.as_ref(),
                ],
                &derivation_program_id,
            )
        } else {
            // Standard case - use derivation program ID (executing ID for bump, reference ID for non-bump)
            let result = Pubkey::find_program_address(
                &[
                    wallet.as_ref(),
                    config.token_program.as_ref(),
                    mint.as_ref(),
                ],
                &derivation_program_id,
            );

            // Debug output suppressed for cleaner test runs

            result
        };

        // Build accounts based on test type
        let mut accounts = Self::build_accounts(
            &config,
            variant,
            ata_implementation,
            payer,
            ata,
            wallet,
            mint,
            bump,
        );

        // Build instruction
        let mut ix = Self::build_instruction(&config, variant, ata_implementation, &accounts, bump);

        // Apply failure mode if specified
        if let Some(failure_mode) = &config.failure_mode {
            Self::apply_failure_mode(
                failure_mode,
                &mut ix,
                &mut accounts,
                &config,
                payer,
                ata,
                wallet,
                mint,
                bump,
            );
        }

        (ix, accounts)
    }

    /// Get structured account addresses
    fn get_structured_addresses(
        config: &TestCaseConfig,
        variant: &crate::common::AtaVariant,
        test_bank: crate::common::TestBankId,
        test_number: u8,
        ata_implementation: &AtaImplementation,
    ) -> (Pubkey, Pubkey, Pubkey) {
        if config.use_fixed_accounts {
            // Use fixed addresses for specific tests
            if let Some(SpecialAccountMod::FixedAddresses { wallet, mint }) = config
                .special_account_mods
                .iter()
                .find(|m| matches!(m, SpecialAccountMod::FixedAddresses { .. }))
            {
                let payer = crate::common::structured_pk(
                    variant,
                    test_bank,
                    test_number,
                    crate::common::AccountTypeId::Payer,
                );
                return (payer, *mint, *wallet);
            }
        }

        let payer = crate::common::structured_pk(
            variant,
            test_bank,
            test_number,
            crate::common::AccountTypeId::Payer,
        );
        let mint = crate::common::structured_pk(
            variant,
            test_bank,
            test_number,
            crate::common::AccountTypeId::Mint,
        );
        let wallet = if matches!(
            config.base_test,
            BaseTestType::RecoverNested | BaseTestType::RecoverMultisig
        ) {
            // For recovery tests, use a simple consistent wallet address (no optimal bump needed)
            // This ensures both P-ATA and Original ATA use the exact same wallet address
            crate::common::structured_pk(
                variant,
                test_bank,
                test_number,
                crate::common::AccountTypeId::Wallet,
            )
        } else {
            // For non-recovery tests, use optimal bump as usual
            crate::common::structured_pk_with_optimal_bump(
                variant,
                test_bank,
                test_number,
                crate::common::AccountTypeId::Wallet,
                &ata_implementation.program_id,
                &config.token_program,
                &mint,
            )
        };
        (payer, mint, wallet)
    }

    /// Build accounts vector based on test configuration
    fn build_accounts(
        config: &TestCaseConfig,
        variant: TestVariant,
        ata_implementation: &AtaImplementation,
        payer: Pubkey,
        ata: Pubkey,
        wallet: Pubkey,
        mint: Pubkey,
        _bump: u8,
    ) -> Vec<(Pubkey, Account)> {
        let mut accounts = Vec::new();

        // Handle special test cases
        if matches!(
            config.base_test,
            BaseTestType::RecoverNested | BaseTestType::RecoverMultisig
        ) {
            return Self::build_recover_accounts(
                config,
                variant,
                ata_implementation,
                payer,
                ata,
                wallet,
                mint,
            );
        }

        // Standard accounts
        accounts.push((payer, AccountBuilder::system_account(1_000_000_000)));

        // ATA account
        let ata_account = if config.setup_existing_ata {
            AccountBuilder::token_account(&mint, &wallet, 0, &config.token_program)
        } else {
            let mut acc = AccountBuilder::system_account(0);
            if config.setup_topup {
                modify_account_for_topup(&mut acc);
                // Debug output suppressed for cleaner test runs
            }
            acc
        };
        accounts.push((ata, ata_account));

        // Wallet account
        accounts.push((wallet, AccountBuilder::system_account(0)));

        // Mint account
        let mint_account = if config.token_program
            == Pubkey::new_from_array(pinocchio_pubkey::pubkey!(
                "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
            )) {
            // Use special Token-2022 mint data format
            AccountBuilder::token_2022_mint_account(0, &config.token_program)
        } else {
            // Use standard or extended mint format
            AccountBuilder::mint_account(0, &config.token_program, config.use_extended_mint)
        };
        accounts.push((mint, mint_account));

        // Standard program accounts
        accounts.push((
            SYSTEM_PROGRAM_ID,
            AccountBuilder::executable_program(NATIVE_LOADER_ID),
        ));
        accounts.push((
            config.token_program,
            AccountBuilder::executable_program(LOADER_V3),
        ));

        // Conditional accounts
        if variant.rent_arg {
            accounts.push((rent::id(), AccountBuilder::rent_sysvar()));
        }

        accounts
    }

    /// Build recover-specific accounts
    fn build_recover_accounts(
        config: &TestCaseConfig,
        variant: TestVariant,
        ata_implementation: &AtaImplementation,
        _payer: Pubkey,
        _ata: Pubkey,
        _wallet: Pubkey,
        _mint: Pubkey,
    ) -> Vec<(Pubkey, Account)> {
        let mut accounts = Vec::new();

        let test_bank = if config.failure_mode.is_some() {
            crate::common::TestBankId::Failures
        } else {
            crate::common::TestBankId::Benchmarks
        };
        let test_number = if matches!(
            config.base_test,
            BaseTestType::RecoverNested | BaseTestType::RecoverMultisig
        ) {
            // For recovery tests, use base test number (no variant offset) to ensure same addresses
            calculate_test_number(config.base_test, TestVariant::BASE, config.setup_topup)
        } else {
            calculate_test_number(config.base_test, variant, config.setup_topup)
        };

        // Find nested ATA configuration
        let (owner_mint, nested_mint) = if let Some(SpecialAccountMod::NestedAta {
            owner_mint,
            nested_mint,
        }) = config
            .special_account_mods
            .iter()
            .find(|m| matches!(m, SpecialAccountMod::NestedAta { .. }))
        {
            (*owner_mint, *nested_mint)
        } else {
            // Use default values for recover tests - these should be consistent across implementations
            // No need to worry about which variant we pass - structured_pk automatically uses
            // consistent addresses for mint types
            (
                crate::common::structured_pk(
                    &ata_implementation.variant,
                    test_bank,
                    test_number,
                    crate::common::AccountTypeId::OwnerMint,
                ),
                crate::common::structured_pk(
                    &ata_implementation.variant,
                    test_bank,
                    test_number,
                    crate::common::AccountTypeId::NestedMint,
                ),
            )
        };

        // For recovery tests, use a simple consistent wallet address (no optimal bump needed)
        // This ensures both P-ATA and Original ATA use the exact same wallet address
        let actual_wallet = crate::common::structured_pk(
            &ata_implementation.variant,
            test_bank,
            test_number,
            crate::common::AccountTypeId::Wallet,
        );

        let (owner_ata, _) = Pubkey::find_program_address(
            &[
                actual_wallet.as_ref(),
                config.token_program.as_ref(),
                owner_mint.as_ref(),
            ],
            &ata_implementation.program_id,
        );

        let (nested_ata, _) = Pubkey::find_program_address(
            &[
                owner_ata.as_ref(),
                config.token_program.as_ref(),
                nested_mint.as_ref(),
            ],
            &ata_implementation.program_id,
        );

        // For recover instructions, the bump should be for the destination ATA
        let (dest_ata, _dest_bump) = Pubkey::find_program_address(
            &[
                actual_wallet.as_ref(),
                config.token_program.as_ref(),
                nested_mint.as_ref(),
            ],
            &ata_implementation.program_id,
        );

        // Build accounts
        accounts.push((
            nested_ata,
            AccountBuilder::token_account(&nested_mint, &owner_ata, 100, &config.token_program),
        ));
        accounts.push((
            nested_mint,
            AccountBuilder::mint_account(0, &config.token_program, false),
        ));
        accounts.push((
            dest_ata,
            AccountBuilder::token_account(&nested_mint, &actual_wallet, 0, &config.token_program),
        ));
        accounts.push((
            owner_ata,
            AccountBuilder::token_account(&owner_mint, &actual_wallet, 0, &config.token_program),
        ));
        accounts.push((
            owner_mint,
            AccountBuilder::mint_account(0, &config.token_program, false),
        ));
        accounts.push((actual_wallet, AccountBuilder::system_account(1_000_000_000)));
        accounts.push((
            config.token_program,
            AccountBuilder::executable_program(LOADER_V3),
        ));
        accounts.push((
            Pubkey::from(spl_token_interface::program::ID),
            AccountBuilder::executable_program(LOADER_V3),
        ));

        // Handle multisig if needed
        if let Some(SpecialAccountMod::MultisigWallet { threshold, signers }) = config
            .special_account_mods
            .iter()
            .find(|m| matches!(m, SpecialAccountMod::MultisigWallet { .. }))
        {
            // Replace wallet with multisig account
            if let Some(wallet_pos) = accounts.iter().position(|(pk, _)| *pk == actual_wallet) {
                accounts[wallet_pos] = (
                    actual_wallet,
                    Account {
                        lamports: 1_000_000_000,
                        data: AccountBuilder::multisig_data(*threshold, signers),
                        owner: config.token_program,
                        executable: false,
                        rent_epoch: 0,
                    },
                );
            }

            // Add signer accounts
            for signer in signers {
                accounts.push((*signer, AccountBuilder::system_account(1_000_000_000)));
            }
        }

        accounts
    }

    /// Build instruction based on configuration
    fn build_instruction(
        config: &TestCaseConfig,
        variant: TestVariant,
        ata_implementation: &AtaImplementation,
        accounts: &[(Pubkey, Account)],
        bump: u8,
    ) -> Instruction {
        let metas = Self::build_metas(config, variant, accounts);
        let data = Self::build_instruction_data(config, variant, ata_implementation, bump);

        Instruction {
            program_id: ata_implementation.program_id,
            accounts: metas,
            data,
        }
    }

    /// Build account metas based on test type
    fn build_metas(
        config: &TestCaseConfig,
        variant: TestVariant,
        accounts: &[(Pubkey, Account)],
    ) -> Vec<AccountMeta> {
        match config.base_test {
            BaseTestType::RecoverNested | BaseTestType::RecoverMultisig => {
                Self::build_recover_metas(config, accounts)
            }
            _ => Self::build_standard_metas(config, variant, accounts),
        }
    }

    /// Build standard account metas
    fn build_standard_metas(
        _config: &TestCaseConfig,
        variant: TestVariant,
        accounts: &[(Pubkey, Account)],
    ) -> Vec<AccountMeta> {
        let mut metas = vec![
            AccountMeta::new(accounts[0].0, true),           // payer
            AccountMeta::new(accounts[1].0, false),          // ata
            AccountMeta::new_readonly(accounts[2].0, false), // wallet
            AccountMeta::new_readonly(accounts[3].0, false), // mint
            AccountMeta::new_readonly(accounts[4].0, false), // system program
            AccountMeta::new_readonly(accounts[5].0, false), // token program
        ];

        if variant.rent_arg {
            metas.push(AccountMeta::new_readonly(rent::id(), false));
        }

        metas
    }

    /// Build recover-specific account metas
    fn build_recover_metas(
        config: &TestCaseConfig,
        accounts: &[(Pubkey, Account)],
    ) -> Vec<AccountMeta> {
        // For multisig tests, the wallet (multisig account) should not be a signer
        // Only individual signers should be marked as signers
        let wallet_is_signer = !matches!(config.base_test, BaseTestType::RecoverMultisig);

        let mut metas = vec![
            AccountMeta::new(accounts[0].0, false),          // nested_ata
            AccountMeta::new_readonly(accounts[1].0, false), // nested_mint
            AccountMeta::new(accounts[2].0, false),          // dest_ata
            AccountMeta::new(accounts[3].0, false),          // owner_ata
            AccountMeta::new_readonly(accounts[4].0, false), // owner_mint
            AccountMeta::new(accounts[5].0, wallet_is_signer), // wallet
            AccountMeta::new_readonly(accounts[6].0, false), // token_program
            AccountMeta::new_readonly(accounts[7].0, false), // spl_token_interface
        ];

        // Add multisig signers if present
        if matches!(config.base_test, BaseTestType::RecoverMultisig) {
            // Add signer accounts (last 3 accounts)
            let signer_start = accounts.len() - 3;
            metas.push(AccountMeta::new_readonly(accounts[signer_start].0, true));
            metas.push(AccountMeta::new_readonly(
                accounts[signer_start + 1].0,
                true,
            ));
            metas.push(AccountMeta::new_readonly(
                accounts[signer_start + 2].0,
                false,
            ));
        }

        metas
    }

    /// Build instruction data
    fn build_instruction_data(
        config: &TestCaseConfig,
        variant: TestVariant,
        ata_implementation: &AtaImplementation,
        bump: u8,
    ) -> Vec<u8> {
        let mut raw_data = vec![config.instruction_discriminator];

        // If len_arg is specified, we MUST also include bump (P-ATA requirement)
        if variant.bump_arg || variant.len_arg {
            raw_data.push(bump);
            // Debug output suppressed for cleaner test runs
        }

        if variant.len_arg {
            let account_len: u16 = if config.token_program
                == Pubkey::new_from_array(pinocchio_pubkey::pubkey!(
                    "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
                )) {
                // For Token-2022, calculate the actual required length with extensions
                ExtensionType::try_calculate_account_len::<spl_token_2022::state::Account>(&[
                    ExtensionType::ImmutableOwner,
                ])
                .expect("failed to calculate Token-2022 account length") as u16
            } else if config.use_extended_mint {
                170 // Standard extended mint case
            } else {
                165 // Standard token account size
            };
            raw_data.extend_from_slice(&account_len.to_le_bytes());
        }

        let final_data = ata_implementation.adapt_instruction_data(raw_data);

        // Debug output suppressed for cleaner test runs

        final_data
    }

    /// Apply failure mode to instruction and accounts
    fn apply_failure_mode(
        failure_mode: &FailureMode,
        ix: &mut Instruction,
        accounts: &mut Vec<(Pubkey, Account)>,
        config: &TestCaseConfig,
        payer: Pubkey,
        ata: Pubkey,
        wallet: Pubkey,
        mint: Pubkey,
        _bump: u8,
    ) {
        match failure_mode {
            FailureMode::WrongPayerOwner(owner) => {
                // Change payer account owner
                if let Some(pos) = accounts.iter().position(|(pk, _)| *pk == payer) {
                    accounts[pos].1.owner = *owner;
                }
            }
            FailureMode::PayerNotSigned => {
                // Change payer from signer to non-signer in instruction
                if let Some(meta) = ix.accounts.get_mut(0) {
                    if meta.pubkey == payer {
                        meta.is_signer = false;
                    }
                }
            }
            FailureMode::WrongSystemProgram(wrong_id) => {
                // Replace system program with wrong program ID
                if let Some(pos) = accounts.iter().position(|(pk, _)| *pk == SYSTEM_PROGRAM_ID) {
                    accounts[pos] = (*wrong_id, accounts[pos].1.clone());
                }
                // Update instruction account meta
                if let Some(meta) = ix
                    .accounts
                    .iter_mut()
                    .find(|m| m.pubkey == SYSTEM_PROGRAM_ID)
                {
                    meta.pubkey = *wrong_id;
                }
            }
            FailureMode::WrongTokenProgram(wrong_id) => {
                // Replace token program with wrong program ID
                if let Some(pos) = accounts
                    .iter()
                    .position(|(pk, _)| *pk == config.token_program)
                {
                    accounts[pos] = (*wrong_id, accounts[pos].1.clone());
                }
                // Update instruction account meta
                if let Some(meta) = ix
                    .accounts
                    .iter_mut()
                    .find(|m| m.pubkey == config.token_program)
                {
                    meta.pubkey = *wrong_id;
                }
            }
            FailureMode::InsufficientFunds(amount) => {
                // Set payer lamports to insufficient amount
                if let Some(pos) = accounts.iter().position(|(pk, _)| *pk == payer) {
                    accounts[pos].1.lamports = *amount;
                }
            }
            FailureMode::WrongAtaAddress(wrong_ata) => {
                // Replace ATA with wrong address
                if let Some(pos) = accounts.iter().position(|(pk, _)| *pk == ata) {
                    accounts[pos] = (*wrong_ata, accounts[pos].1.clone());
                }
                // Update instruction account meta
                if let Some(meta) = ix.accounts.iter_mut().find(|m| m.pubkey == ata) {
                    meta.pubkey = *wrong_ata;
                }
            }
            FailureMode::MintWrongOwner(wrong_owner) => {
                // Change mint account owner
                if let Some(pos) = accounts.iter().position(|(pk, _)| *pk == mint) {
                    accounts[pos].1.owner = *wrong_owner;
                }
            }
            FailureMode::InvalidMintStructure(wrong_size) => {
                // Change mint data size
                if let Some(pos) = accounts.iter().position(|(pk, _)| *pk == mint) {
                    accounts[pos].1.data = vec![0u8; *wrong_size];
                }
            }
            FailureMode::InvalidTokenAccountStructure => {
                // Set ATA with invalid token account data
                if let Some(pos) = accounts.iter().position(|(pk, _)| *pk == ata) {
                    accounts[pos].1.data = vec![0xFF; 165]; // Invalid data
                    accounts[pos].1.owner = config.token_program;
                    accounts[pos].1.lamports = 2_000_000;
                }
            }
            FailureMode::InvalidDiscriminator(disc) => {
                // Change instruction discriminator
                if !ix.data.is_empty() {
                    ix.data[0] = *disc;
                }
            }
            FailureMode::InvalidBumpValue(invalid_bump) => {
                // Change bump value in instruction data
                if ix.data.len() >= 2 {
                    ix.data[1] = *invalid_bump;
                }
            }
            FailureMode::AtaWrongOwner(wrong_owner) => {
                // Create a fresh account owned by wrong_owner with existing data
                // This simulates an account "already in use" by another program
                if let Some(pos) = accounts.iter().position(|(pk, _)| *pk == ata) {
                    let new_account = Account {
                        lamports: 2_000_000,
                        data: vec![0u8; 165], // Non-empty data indicates "already in use"
                        owner: *wrong_owner,  // Should be SYSTEM_PROGRAM_ID for this test
                        executable: false,
                        rent_epoch: 0,
                    };

                    // Debug output suppressed for cleaner test runs

                    accounts[pos].1 = new_account;
                }
            }
            FailureMode::AtaNotWritable => {
                // Mark ATA as non-writable in instruction
                if let Some(meta) = ix.accounts.iter_mut().find(|m| m.pubkey == ata) {
                    meta.is_writable = false;
                }
            }
            FailureMode::TokenAccountWrongSize(wrong_size) => {
                // Set ATA with wrong size
                if let Some(pos) = accounts.iter().position(|(pk, _)| *pk == ata) {
                    accounts[pos].1.data = vec![0u8; *wrong_size];
                    accounts[pos].1.owner = config.token_program;
                    accounts[pos].1.lamports = 2_000_000;
                }
            }
            FailureMode::TokenAccountWrongMint(wrong_mint) => {
                // Set ATA with wrong mint
                if let Some(pos) = accounts.iter().position(|(pk, _)| *pk == ata) {
                    accounts[pos].1 = AccountBuilder::token_account(
                        wrong_mint,
                        &wallet,
                        0,
                        &config.token_program,
                    );
                }
                // Add the wrong mint account
                accounts.push((
                    *wrong_mint,
                    AccountBuilder::mint_account(0, &config.token_program, false),
                ));
            }
            FailureMode::TokenAccountWrongOwner(wrong_owner) => {
                // Set ATA with wrong owner
                if let Some(pos) = accounts.iter().position(|(pk, _)| *pk == ata) {
                    accounts[pos].1 =
                        AccountBuilder::token_account(&mint, wrong_owner, 0, &config.token_program);
                }
            }
            FailureMode::WrongAccountSizeForExtensions(wrong_size) => {
                // Set ATA with wrong size for extensions
                if let Some(pos) = accounts.iter().position(|(pk, _)| *pk == ata) {
                    accounts[pos].1.data = vec![0u8; *wrong_size];
                    accounts[pos].1.owner = config.token_program;
                    accounts[pos].1.lamports = 2_000_000;
                }
            }
            FailureMode::MissingExtensions => {
                // Set ATA with missing extensions
                if let Some(pos) = accounts.iter().position(|(pk, _)| *pk == ata) {
                    accounts[pos].1.data = vec![0u8; 200]; // Large but missing extension data
                    accounts[pos].1.owner = config.token_program;
                    accounts[pos].1.lamports = 2_000_000;
                }
            }
            FailureMode::InvalidExtensionData => {
                // Set ATA with invalid extension data
                if let Some(pos) = accounts.iter().position(|(pk, _)| *pk == ata) {
                    let mut data = vec![0u8; 200];
                    data[165..169].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // Invalid extension type
                    accounts[pos].1.data = data;
                    accounts[pos].1.owner = config.token_program;
                    accounts[pos].1.lamports = 2_000_000;
                }
            }
            FailureMode::RecoverWalletNotSigner => {
                // Mark wallet as not signer in recover instruction
                // For recover instructions, wallet is at index 5
                if matches!(
                    config.base_test,
                    BaseTestType::RecoverNested | BaseTestType::RecoverMultisig
                ) {
                    if ix.accounts.len() > 5 {
                        ix.accounts[5].is_signer = false;
                    }
                } else if let Some(meta) = ix.accounts.iter_mut().find(|m| m.pubkey == wallet) {
                    meta.is_signer = false;
                }
            }
            FailureMode::RecoverMultisigInsufficientSigners => {
                if ix.accounts.len() > 9 {
                    ix.accounts[8].is_signer = true;
                    ix.accounts[9].is_signer = false;
                    if ix.accounts.len() > 10 {
                        ix.accounts[10].is_signer = false;
                    }
                }
            }
            FailureMode::RecoverWrongNestedAta(wrong_nested) => {
                // Replace nested ATA with wrong address
                if let Some(meta) = ix.accounts.get_mut(0) {
                    meta.pubkey = *wrong_nested;
                }
                // Update accounts
                if let Some(pos) = accounts
                    .iter()
                    .position(|(pk, _)| pk == &ix.accounts[0].pubkey)
                {
                    accounts[pos] = (*wrong_nested, accounts[pos].1.clone());
                }
            }
            FailureMode::RecoverWrongDestination(wrong_dest) => {
                // Replace destination ATA with wrong address
                if let Some(meta) = ix.accounts.get_mut(2) {
                    meta.pubkey = *wrong_dest;
                }
                // Update accounts
                if let Some(pos) = accounts
                    .iter()
                    .position(|(pk, _)| pk == &ix.accounts[2].pubkey)
                {
                    accounts[pos] = (*wrong_dest, accounts[pos].1.clone());
                }
            }
            FailureMode::RecoverNestedWrongOwner(wrong_owner) => {
                // Set nested ATA with wrong owner
                if let Some(pos) = accounts
                    .iter()
                    .position(|(pk, _)| pk == &ix.accounts[0].pubkey)
                {
                    let nested_mint = ix.accounts[1].pubkey;
                    accounts[pos].1 = AccountBuilder::token_account(
                        &nested_mint,
                        wrong_owner,
                        100,
                        &config.token_program,
                    );
                }
            }
            FailureMode::InvalidMultisigData => {
                // Set multisig with invalid data
                if let Some(pos) = accounts.iter().position(|(pk, _)| *pk == wallet) {
                    accounts[pos].1.data = vec![0xFF; 355]; // Invalid multisig data
                    accounts[pos].1.owner = config.token_program;
                }
            }
            FailureMode::InvalidSignerAccounts(wrong_signers) => {
                // Add wrong signer accounts
                for (i, wrong_signer) in wrong_signers.iter().enumerate() {
                    if let Some(meta) = ix.accounts.get_mut(8 + i) {
                        meta.pubkey = *wrong_signer;
                    }
                    accounts.push((*wrong_signer, AccountBuilder::system_account(1_000_000_000)));
                }
            }
            FailureMode::UninitializedMultisig => {
                // Set multisig as uninitialized
                if let Some(pos) = accounts.iter().position(|(pk, _)| *pk == wallet) {
                    let signer1 = Pubkey::new_unique();
                    let mut data = vec![0u8; 355];
                    data[0] = 1; // m = 1
                    data[1] = 1; // n = 1
                    data[2] = 0; // is_initialized = false
                    data[3..35].copy_from_slice(signer1.as_ref());
                    accounts[pos].1.data = data;
                    accounts[pos].1.owner = config.token_program;
                    accounts.push((signer1, AccountBuilder::system_account(1_000_000_000)));
                }
            }
        }
    }
}

/// Calculate test number from base test type and variant
pub fn calculate_test_number(
    base_test: BaseTestType,
    variant: TestVariant,
    setup_topup: bool,
) -> u8 {
    let base = match base_test {
        BaseTestType::Create => {
            if setup_topup {
                10
            } else {
                0
            }
        }
        BaseTestType::CreateIdempotent => 20,
        BaseTestType::CreateTopup => 30,
        BaseTestType::CreateTopupNoCap => 40,
        BaseTestType::CreateToken2022 => 50,
        BaseTestType::RecoverNested => 60,
        BaseTestType::RecoverMultisig => 70,
        BaseTestType::WorstCase => 80,
    };

    let variant_offset = match (variant.rent_arg, variant.bump_arg, variant.len_arg) {
        (false, false, false) => 0,
        (true, false, false) => 1,
        (false, true, false) => 2,
        (false, false, true) => 3,
        (true, true, false) => 4,
        (true, false, true) => 5,
        (true, true, true) => 6,
        _ => 7,
    };

    base + variant_offset
}

/// Calculate test number for failure scenarios with collision avoidance
#[allow(dead_code)]
pub fn calculate_failure_test_number(base_test: BaseTestType, variant: TestVariant) -> u8 {
    use std::sync::atomic::{AtomicU8, Ordering};
    static FAILURE_COUNTER: AtomicU8 = AtomicU8::new(0);

    // Failure tests start at 100 to avoid collisions with normal tests
    let base = 100
        + match base_test {
            BaseTestType::Create => 0,
            BaseTestType::CreateIdempotent => 10,
            BaseTestType::CreateTopup => 20,
            BaseTestType::CreateTopupNoCap => 30,
            BaseTestType::CreateToken2022 => 40,
            BaseTestType::RecoverNested => 50,
            BaseTestType::RecoverMultisig => 60,
            BaseTestType::WorstCase => 70,
        };

    let variant_offset = match (variant.rent_arg, variant.bump_arg, variant.len_arg) {
        (false, false, false) => 0,
        (true, false, false) => 1,
        (false, true, false) => 2,
        (false, false, true) => 3,
        (true, true, false) => 4,
        (true, false, true) => 5,
        (true, true, true) => 6,
        _ => 7,
    };

    // Auto-increment failure counter to ensure uniqueness
    let failure_id = FAILURE_COUNTER.fetch_add(1, Ordering::SeqCst);
    base + variant_offset + (failure_id % 8)
}
