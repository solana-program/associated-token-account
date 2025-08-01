#![cfg_attr(feature = "std", allow(dead_code, unused_imports))]

use {
    crate::tests::{
        account_builder::AccountBuilder,
        address_gen::{
            derive_address_with_bump, find_optimal_wallet_for_mints, random_seeded_pk,
            structured_pk,
        },
        benches::{account_templates::*, common::*, constants::*},
    },
    solana_account::Account,
    solana_instruction::{AccountMeta, Instruction},
    solana_pubkey::Pubkey,
    solana_sysvar::rent,
    spl_token_2022::extension::ExtensionType,
    std::vec,
    std::vec::Vec,
};

#[cfg(feature = "full-debug-logs")]
use std::{println, string::String, string::ToString};

/// Configuration for building test cases
#[derive(Debug, Clone)]
pub struct TestCaseConfig {
    pub base_test: BaseTestType,
    pub token_program: Pubkey,
    pub instruction_discriminator: u8,
    pub setup_topup: bool,
    pub setup_existing_ata: bool,
    pub use_fixed_mint_owner_payer: bool,
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
        payer: Pubkey,
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
    /// ATA address mismatch allowing lamport drain from uninitialized ATA
    AtaAddressMismatchLamportDrain,
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
    /// Recover: multisig duplicate signers (vulnerability test)
    RecoverMultisigDuplicateSigners,
    /// Recover: multisig account passed but not marked as signer
    RecoverMultisigNonSignerAccount,
    /// Recover: multisig wallet owned by wrong program
    RecoverMultisigWrongWalletOwner(Pubkey),
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
        Self::build_with_config(config, variant, ata_implementation, None)
    }

    /// Build test case with specific iteration for random wallet generation
    #[allow(dead_code)]
    pub fn build_test_case_with_iteration(
        base_test: BaseTestType,
        variant: TestVariant,
        ata_implementation: &AtaImplementation,
        token_program_id: &Pubkey,
        iteration: usize,
        run_entropy: u64,
        max_iterations: Option<usize>,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let config = Self::get_config_for_test(base_test, token_program_id);
        Self::build_with_config_and_iteration(
            config,
            variant,
            ata_implementation,
            None,
            iteration,
            run_entropy,
            max_iterations,
        )
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
        Self::build_with_config(config, variant, ata_implementation, None)
    }

    /// Build a failure test case with the specified failure mode and test name
    #[allow(dead_code)]
    pub fn build_failure_test_case_with_name(
        base_test: BaseTestType,
        variant: TestVariant,
        ata_implementation: &AtaImplementation,
        token_program_id: &Pubkey,
        failure_mode: FailureMode,
        test_name: &str,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let mut config = Self::get_config_for_test(base_test, token_program_id);
        config.failure_mode = Some(failure_mode);
        Self::build_with_config(config, variant, ata_implementation, Some(test_name))
    }

    /// Get configuration for each test type
    fn get_config_for_test(base_test: BaseTestType, token_program_id: &Pubkey) -> TestCaseConfig {
        match base_test {
            BaseTestType::Create => TestCaseConfig {
                base_test,
                token_program: *token_program_id,
                instruction_discriminator: 0,
                setup_topup: false,
                setup_existing_ata: false,
                use_fixed_mint_owner_payer: true,
                special_account_mods: vec![],
                failure_mode: None,
            },
            BaseTestType::CreateIdempotent => TestCaseConfig {
                base_test,
                token_program: *token_program_id,
                instruction_discriminator: 1,
                setup_topup: false,
                setup_existing_ata: true, // Idempotent
                use_fixed_mint_owner_payer: true,
                special_account_mods: vec![],
                failure_mode: None,
            },
            BaseTestType::CreateTopup => TestCaseConfig {
                base_test,
                token_program: *token_program_id,
                instruction_discriminator: 0,
                setup_topup: true,
                setup_existing_ata: false,
                use_fixed_mint_owner_payer: true,
                special_account_mods: vec![],
                failure_mode: None,
            },
            BaseTestType::CreateTopupNoCap => TestCaseConfig {
                base_test,
                token_program: *token_program_id,
                instruction_discriminator: 0,
                setup_topup: true,
                setup_existing_ata: false,
                use_fixed_mint_owner_payer: true,
                special_account_mods: vec![],
                failure_mode: None,
            },
            BaseTestType::CreateToken2022 => TestCaseConfig {
                base_test,
                token_program: Pubkey::new_from_array(pinocchio_pubkey::pubkey!(
                    "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
                )),
                instruction_discriminator: 0,
                setup_topup: false,
                setup_existing_ata: false,
                use_fixed_mint_owner_payer: true,
                special_account_mods: vec![],
                failure_mode: None,
            },
            BaseTestType::CreateExtended => TestCaseConfig {
                base_test,
                token_program: Pubkey::new_from_array(pinocchio_pubkey::pubkey!(
                    "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
                )),
                instruction_discriminator: 0,
                setup_topup: false,
                setup_existing_ata: false,
                use_fixed_mint_owner_payer: true,
                special_account_mods: vec![],
                failure_mode: None,
            },
            BaseTestType::RecoverNested => TestCaseConfig {
                base_test,
                token_program: *token_program_id,
                instruction_discriminator: 2,
                setup_topup: false,
                setup_existing_ata: false,
                use_fixed_mint_owner_payer: true,
                special_account_mods: vec![SpecialAccountMod::NestedAta {
                    // No need to explicitly use AtaVariant::Original anymore
                    // structured_pk now automatically uses consistent addresses for mint types
                    owner_mint: structured_pk(
                        &crate::tests::benches::common::AtaVariant::PAtaLegacy, // Can use any variant now
                        crate::tests::benches::common::TestBankId::Benchmarks,
                        base_test as u8,
                        crate::tests::benches::common::AccountTypeId::OwnerMint,
                    ),
                    nested_mint: structured_pk(
                        &crate::tests::benches::common::AtaVariant::PAtaLegacy, // Can use any variant now
                        crate::tests::benches::common::TestBankId::Benchmarks,
                        base_test as u8,
                        crate::tests::benches::common::AccountTypeId::NestedMint,
                    ),
                }],
                failure_mode: None,
            },
            BaseTestType::RecoverMultisig => TestCaseConfig {
                base_test,
                token_program: *token_program_id,
                instruction_discriminator: 2,
                setup_topup: false,
                setup_existing_ata: false,
                use_fixed_mint_owner_payer: true,
                special_account_mods: vec![
                    SpecialAccountMod::NestedAta {
                        // No need to explicitly use AtaVariant::Original anymore
                        // structured_pk now automatically uses consistent addresses for mint types
                        owner_mint: structured_pk(
                            &crate::tests::benches::common::AtaVariant::PAtaLegacy, // Can use any variant now
                            crate::tests::benches::common::TestBankId::Benchmarks,
                            base_test as u8,
                            crate::tests::benches::common::AccountTypeId::OwnerMint,
                        ),
                        nested_mint: structured_pk(
                            &crate::tests::benches::common::AtaVariant::PAtaLegacy, // Can use any variant now
                            crate::tests::benches::common::TestBankId::Benchmarks,
                            base_test as u8,
                            crate::tests::benches::common::AccountTypeId::NestedMint,
                        ),
                    },
                    SpecialAccountMod::MultisigWallet {
                        threshold: 2,
                        signers: vec![
                            structured_pk(
                                &crate::tests::benches::common::AtaVariant::SplAta,
                                crate::tests::benches::common::TestBankId::Benchmarks,
                                base_test as u8,
                                crate::tests::benches::common::AccountTypeId::Signer1,
                            ),
                            structured_pk(
                                &crate::tests::benches::common::AtaVariant::SplAta,
                                crate::tests::benches::common::TestBankId::Benchmarks,
                                base_test as u8,
                                crate::tests::benches::common::AccountTypeId::Signer2,
                            ),
                            structured_pk(
                                &crate::tests::benches::common::AtaVariant::SplAta,
                                crate::tests::benches::common::TestBankId::Benchmarks,
                                base_test as u8,
                                crate::tests::benches::common::AccountTypeId::Signer3,
                            ),
                        ],
                    },
                ],
                failure_mode: None,
            },
            BaseTestType::WorstCase => TestCaseConfig {
                base_test,
                token_program: *token_program_id,
                instruction_discriminator: 0,
                setup_topup: false,
                setup_existing_ata: false,
                use_fixed_mint_owner_payer: true,
                special_account_mods: vec![SpecialAccountMod::FixedAddresses {
                    payer: structured_pk(
                        &crate::tests::benches::common::AtaVariant::SplAta,
                        crate::tests::benches::common::TestBankId::Benchmarks,
                        base_test as u8,
                        crate::tests::benches::common::AccountTypeId::Payer,
                    ),
                    wallet: structured_pk(
                        &crate::tests::benches::common::AtaVariant::SplAta,
                        crate::tests::benches::common::TestBankId::Benchmarks,
                        base_test as u8,
                        crate::tests::benches::common::AccountTypeId::Wallet,
                    ),
                    mint: structured_pk(
                        &crate::tests::benches::common::AtaVariant::SplAta,
                        crate::tests::benches::common::TestBankId::Benchmarks,
                        base_test as u8,
                        crate::tests::benches::common::AccountTypeId::Mint,
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
        _test_name: Option<&str>,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        // Generate simple entropy for this call since we don't have run-specific entropy available
        let simple_entropy = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        Self::build_with_config_and_iteration(
            config,
            variant,
            ata_implementation,
            _test_name,
            42,
            simple_entropy,
            None, // max_iterations not available in this context
        )
    }

    /// Build test case with given configuration and iteration for random wallet
    fn build_with_config_and_iteration(
        config: TestCaseConfig,
        variant: TestVariant,
        ata_implementation: &AtaImplementation,
        _test_name: Option<&str>,
        iteration: usize,
        run_entropy: u64,
        max_iterations: Option<usize>,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        // Use structured addressing to prevent cross-contamination
        let test_bank = if config.failure_mode.is_some() {
            crate::tests::benches::common::TestBankId::Failures
        } else {
            crate::tests::benches::common::TestBankId::Benchmarks
        };
        // For address generation, always use the actual variant for test number calculation
        // This ensures P-ATA and SPL ATA use the same test number for the same variant,
        // even though SPL ATA strips variant-specific instruction data
        let test_number = calculate_test_number(config.base_test, variant, config.setup_topup);

        let (payer, mint, mut wallet) =
            Self::get_structured_addresses(&config, test_bank, test_number, iteration, run_entropy);

        // For single iterations, replace wallet with optimal bump wallet
        if let Some(1) = max_iterations {
            let search_entropy = run_entropy
                .wrapping_add(test_number as u64)
                .wrapping_add(iteration as u64);

            if matches!(config.base_test, BaseTestType::RecoverNested) {
                // For RecoverNested operations, find wallet optimal for both owner_mint and nested_mint
                if let Some(SpecialAccountMod::NestedAta {
                    owner_mint,
                    nested_mint,
                }) = config
                    .special_account_mods
                    .iter()
                    .find(|m| matches!(m, SpecialAccountMod::NestedAta { .. }))
                {
                    // For recover operations, optimize ALL three ATAs: Owner, Destination, AND Nested
                    let all_implementations =
                        crate::tests::benches::common::AtaImplementation::all();
                    let ata_program_ids = vec![
                        all_implementations.spl_impl.program_id,
                        all_implementations.pata_legacy_impl.program_id,
                        all_implementations.pata_prefunded_impl.program_id,
                    ];

                    let mut attempt_entropy = search_entropy;
                    while {
                        // 1. Find wallet optimal for Owner ATA and Destination ATA
                        let candidate_wallet = find_optimal_wallet_for_mints(
                            &config.token_program,
                            &[*owner_mint, *nested_mint],
                            &ata_program_ids[..],
                            attempt_entropy,
                        );

                        // 2. Check if Nested ATA also has bump 255 for all programs
                        let mut all_nested_optimal = true;
                        for ata_program_id in &ata_program_ids {
                            let (owner_ata_address, _) = Pubkey::find_program_address(
                                &[
                                    candidate_wallet.as_ref(),
                                    config.token_program.as_ref(),
                                    owner_mint.as_ref(),
                                ],
                                ata_program_id,
                            );

                            let (_, nested_bump) = Pubkey::find_program_address(
                                &[
                                    owner_ata_address.as_ref(),
                                    config.token_program.as_ref(),
                                    nested_mint.as_ref(),
                                ],
                                ata_program_id,
                            );

                            if nested_bump != 255 {
                                all_nested_optimal = false;
                                break;
                            }
                        }

                        if all_nested_optimal {
                            wallet = candidate_wallet;
                            false // exit while loop
                        } else {
                            attempt_entropy = attempt_entropy.wrapping_add(1);
                            true // continue while loop
                        }
                    } {}
                }
            } else if matches!(config.base_test, BaseTestType::RecoverMultisig) {
                // For RecoverMultisig, don't modify the wallet address because it needs to
                // be consistent with the multisig signer configuration. The signers are
                // generated with specific structured addresses that must match the wallet.
                // Optimal bump search would break this relationship.
                #[cfg(feature = "full-debug-logs")]
                {
                    println!("🔍 [DEBUG] Skipping optimal bump for RecoverMultisig to preserve multisig relationship");
                    println!("    Using deterministic wallet: {}", wallet);
                }
            } else if !matches!(config.base_test, BaseTestType::WorstCase) {
                // For standard create operations, find wallet optimal for mint across all ATA programs
                let all_implementations = crate::tests::benches::common::AtaImplementation::all();
                let ata_program_ids = vec![
                    all_implementations.spl_impl.program_id,
                    all_implementations.pata_legacy_impl.program_id,
                    all_implementations.pata_prefunded_impl.program_id,
                ];
                wallet = find_optimal_wallet_for_mints(
                    &config.token_program,
                    &[mint],
                    &ata_program_ids[..],
                    search_entropy,
                );
            }
            // Note: WorstCase tests intentionally use sub-optimal wallets, so skip optimization
        }

        #[cfg(feature = "full-debug-logs")]
        {
            let base_test_name = config.base_test.to_string();
            let display_test_name = _test_name.unwrap_or(&base_test_name);
            println!(
                "🔍 Test: {} | Implementation: {} | Mint: {} | Owner: {} | Payer: {}",
                display_test_name,
                ata_implementation.name,
                mint.to_string()[0..8].to_string(),
                wallet.to_string()[0..8].to_string(),
                payer.to_string()[0..8].to_string()
            );

            // Log full addresses for debugging address consistency
            println!(
                "    Full addresses: Mint: {} | Owner: {} | Payer: {}",
                mint, wallet, payer
            );
        }

        let derivation_program_id = ata_implementation.program_id;

        let (ata, bump) = if matches!(
            config.base_test,
            BaseTestType::RecoverNested | BaseTestType::RecoverMultisig
        ) {
            let actual_wallet = wallet;

            #[allow(unused_variables)]
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
                panic!("Could not find NestedAta config for recover test");
            };

            // Find the correct bump for the random wallet
            Pubkey::find_program_address(
                &[
                    actual_wallet.as_ref(),
                    config.token_program.as_ref(),
                    owner_mint.as_ref(),
                ],
                &derivation_program_id,
            )
        } else if matches!(config.base_test, BaseTestType::WorstCase) {
            // WorstCase test uses a non-optimal wallet, so find the canonical bump.
            Pubkey::find_program_address(
                &[
                    wallet.as_ref(),
                    config.token_program.as_ref(),
                    mint.as_ref(),
                ],
                &derivation_program_id,
            )
        } else {
            // Standard tests: find the correct bump for the random wallet
            // Special handling for InvalidBumpValue failure mode
            if let Some(FailureMode::InvalidBumpValue(invalid_bump)) = &config.failure_mode {
                // For InvalidBumpValue tests, derive address using the invalid bump
                let seeds = &[
                    wallet.as_ref(),
                    config.token_program.as_ref(),
                    mint.as_ref(),
                ];
                let invalid_address =
                    derive_address_with_bump(seeds, *invalid_bump, &derivation_program_id);
                (invalid_address, *invalid_bump)
            } else {
                // Normal case: find the canonical bump
                Pubkey::find_program_address(
                    &[
                        wallet.as_ref(),
                        config.token_program.as_ref(),
                        mint.as_ref(),
                    ],
                    &derivation_program_id,
                )
            }
        };

        let mut accounts = Self::build_accounts(
            &config,
            variant,
            ata_implementation,
            payer,
            ata,
            wallet,
            mint,
        );
        let mut ix = Self::build_instruction(&config, variant, ata_implementation, &accounts, bump);

        #[cfg(feature = "full-debug-logs")]
        println!("🐛 [DEBUG] Built instruction with data: {:?}", ix.data);

        if let Some(failure_mode) = &config.failure_mode {
            #[cfg(feature = "full-debug-logs")]
            println!("🐛 [DEBUG] Applying failure mode: {:?}", failure_mode);
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

    fn get_structured_addresses(
        config: &TestCaseConfig,
        test_bank: crate::tests::benches::common::TestBankId,
        test_number: u8,
        iteration: usize,
        run_entropy: u64,
    ) -> (Pubkey, Pubkey, Pubkey) {
        if config.use_fixed_mint_owner_payer {
            // Use fixed addresses for specific tests
            if let Some(SpecialAccountMod::FixedAddresses {
                payer,
                wallet,
                mint,
            }) = config
                .special_account_mods
                .iter()
                .find(|m| matches!(m, SpecialAccountMod::FixedAddresses { .. }))
            {
                return (*payer, *mint, *wallet);
            }
        }

        // Use consistent variant for mint and wallet to enable byte-for-byte comparison
        let consistent_variant = &crate::tests::benches::common::AtaVariant::SplAta;

        let payer = structured_pk(
            consistent_variant,
            test_bank,
            test_number,
            crate::tests::benches::common::AccountTypeId::Payer,
        );

        let mint = structured_pk(
            consistent_variant,
            test_bank,
            test_number,
            crate::tests::benches::common::AccountTypeId::Mint,
        );
        let wallet = if matches!(
            config.base_test,
            BaseTestType::RecoverNested | BaseTestType::RecoverMultisig
        ) {
            // For recover tests, the wallet must be engineered using the owner_mint as a seed.
            #[allow(unused_variables)]
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
                // This should not happen if config is built correctly
                panic!("Could not find NestedAta config for recover test");
            };

            // For multisig tests, generate a deterministic multisig account address
            // For nested tests, use random seeded pubkey
            if matches!(config.base_test, BaseTestType::RecoverMultisig) {
                // Generate deterministic multisig wallet address using same base parameters as signers
                structured_pk(
                    consistent_variant,
                    test_bank,
                    test_number,
                    crate::tests::benches::common::AccountTypeId::Wallet,
                )
            } else {
                // Use random seeded pubkey for recover nested tests
                random_seeded_pk(
                    consistent_variant,
                    test_bank,
                    test_number,
                    crate::tests::benches::common::AccountTypeId::Wallet,
                    iteration,
                    run_entropy,
                )
            }
        } else if matches!(config.base_test, BaseTestType::WorstCase) {
            structured_pk(
                consistent_variant,
                test_bank,
                test_number,
                crate::tests::benches::common::AccountTypeId::Wallet,
            )
        } else {
            // Use random seeded pubkey for standard tests - optimal bump logic will be added later
            random_seeded_pk(
                consistent_variant,
                test_bank,
                test_number,
                crate::tests::benches::common::AccountTypeId::Wallet,
                iteration,
                run_entropy,
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
    ) -> Vec<(Pubkey, Account)> {
        if matches!(
            config.base_test,
            BaseTestType::RecoverNested | BaseTestType::RecoverMultisig
        ) {
            return Self::build_recover_accounts(config, ata_implementation, wallet, ata);
        }

        let mut account_set =
            StandardAccountSet::new(payer, ata, wallet, mint, &config.token_program);

        if config.setup_existing_ata {
            account_set = account_set.with_existing_ata(&mint, &wallet, &config.token_program);
        }

        if config.setup_topup {
            account_set = account_set.with_topup_ata();
        }

        // For real Token-2022 program, use Token-2022 mint layout
        if config.token_program
            == Pubkey::new_from_array(pinocchio_pubkey::pubkey!(
                "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
            ))
        {
            account_set = account_set.with_token_2022_mint(0);
        }

        // For CreateExtended test, use extended mint with multiple extensions
        if matches!(config.base_test, BaseTestType::CreateExtended) {
            account_set = account_set.with_extended_mint(0);
        }

        // Convert to accounts vector, adding rent sysvar if needed
        let accounts = if variant.rent_arg {
            account_set.with_rent_sysvar().to_vec()
        } else {
            account_set.to_vec()
        };

        accounts
    }

    /// Build recover-specific accounts using RecoverAccountSet template
    fn build_recover_accounts(
        config: &TestCaseConfig,
        ata_implementation: &AtaImplementation,
        actual_wallet: Pubkey,
        owner_ata: Pubkey,
    ) -> Vec<(Pubkey, Account)> {
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
            // This case should ideally not be hit if config is constructed correctly
            panic!("Recover test requires NestedAta modification");
        };

        // Debug logging for recover_multisig address calculations
        #[cfg(feature = "full-debug-logs")]
        if matches!(config.base_test, BaseTestType::RecoverMultisig) {
            println!("🔍 [DEBUG] Address calculation in build_recover_accounts:");
            println!("    wallet: {}", actual_wallet);
            println!("    token_program: {}", config.token_program);
            println!("    owner_mint: {}", owner_mint);
            println!(
                "    ata_implementation.program_id: {}",
                ata_implementation.program_id
            );
            println!("    owner_ata (from caller): {}", owner_ata);
        }

        let (nested_ata, _) = Pubkey::find_program_address(
            &[
                owner_ata.as_ref(),
                config.token_program.as_ref(),
                nested_mint.as_ref(),
            ],
            &ata_implementation.program_id,
        );

        let (dest_ata, _) = Pubkey::find_program_address(
            &[
                actual_wallet.as_ref(),
                config.token_program.as_ref(),
                nested_mint.as_ref(),
            ],
            &ata_implementation.program_id,
        );

        #[cfg(feature = "full-debug-logs")]
        if matches!(config.base_test, BaseTestType::RecoverMultisig) {
            println!("🔍 [DEBUG] Calculated addresses:");
            println!("    nested_ata: {}", nested_ata);
            println!("    dest_ata: {}", dest_ata);
            println!("    nested_mint: {}", nested_mint);
        }

        let mut account_set = RecoverAccountSet::new(
            nested_ata,
            nested_mint,
            dest_ata,
            owner_ata,
            owner_mint,
            actual_wallet,
            &config.token_program,
            100, // token amount
        );

        // Handle multisig if needed
        if let Some(SpecialAccountMod::MultisigWallet { threshold, signers }) = config
            .special_account_mods
            .iter()
            .find(|m| matches!(m, SpecialAccountMod::MultisigWallet { .. }))
        {
            account_set = account_set.with_multisig(*threshold, signers.clone());
        }

        account_set.to_vec()
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
        let data =
            Self::build_instruction_data(config, variant, ata_implementation, accounts, bump);

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
        ];

        // Add multisig signers if present
        if matches!(config.base_test, BaseTestType::RecoverMultisig) {
            #[cfg(feature = "full-debug-logs")]
            {
                println!("🔍 [DEBUG] RecoverMultisig meta building:");
                println!("    Total accounts: {}", accounts.len());
                for (i, (pubkey, _)) in accounts.iter().enumerate() {
                    println!("    accounts[{}]: {}", i, pubkey);
                }

                // Show what we're passing to the program
                println!("🔍 [DEBUG] RecoverMultisig instruction accounts:");
                println!("    [0] nested_ata: {}", accounts[0].0);
                println!("    [1] nested_mint: {}", accounts[1].0);
                println!("    [2] dest_ata: {}", accounts[2].0);
                println!("    [3] owner_ata: {}", accounts[3].0);
                println!("    [4] owner_mint: {}", accounts[4].0);
                println!("    [5] wallet (multisig): {}", accounts[5].0);
                println!("    [6] token_program: {}", accounts[6].0);
            }

            // For 2-of-3 multisig, only pass in the 2 accounts that are actually signing
            let signer_start = accounts.len() - 3;

            #[cfg(feature = "full-debug-logs")]
            {
                println!("    Signer start index: {}", signer_start);
                println!("    Signer 1: {}", accounts[signer_start].0);
                println!("    Signer 2: {}", accounts[signer_start + 1].0);
                println!(
                    "    Signer 3 (not included): {}",
                    accounts[signer_start + 2].0
                );

                // Also check what's in the multisig account data
                let multisig_data = &accounts[5].1.data;
                // Minimum length = header (3 bytes) + first signer (32 bytes)
                if multisig_data.len() >= 35 {
                    let m = multisig_data[0];
                    let n = multisig_data[1];
                    let initialized = multisig_data[2];
                    println!(
                        "    Multisig config: m={}, n={}, initialized={}",
                        m, n, initialized
                    );

                    for i in 0..n {
                        let offset = 3 + (i as usize) * 32;
                        if offset + 32 <= multisig_data.len() {
                            let signer_bytes = &multisig_data[offset..offset + 32];
                            let signer_pubkey = Pubkey::try_from(signer_bytes).unwrap_or_default();
                            println!("    Multisig signer {}: {}", i, signer_pubkey);
                        }
                    }
                }

                // Show the final instruction meta that will be sent
                println!("🔍 [DEBUG] Final instruction metas being built:");
            }

            metas.push(AccountMeta::new_readonly(accounts[signer_start].0, true));
            metas.push(AccountMeta::new_readonly(
                accounts[signer_start + 1].0,
                true,
            ));
            // Don't include the third signer since it's not signing
        }

        #[cfg(feature = "full-debug-logs")]
        if matches!(config.base_test, BaseTestType::RecoverMultisig) {
            println!("🔍 [DEBUG] Final instruction metas for RecoverMultisig:");
            for (i, meta) in metas.iter().enumerate() {
                println!(
                    "    meta[{}]: {} (writable: {}, signer: {})",
                    i, meta.pubkey, meta.is_writable, meta.is_signer
                );
            }
        }

        metas
    }

    /// Build instruction data
    fn build_instruction_data(
        config: &TestCaseConfig,
        variant: TestVariant,
        ata_implementation: &AtaImplementation,
        accounts: &[(Pubkey, Account)],
        bump: u8,
    ) -> Vec<u8> {
        // Delegate to shared encoder for standard Create / CreateIdempotent cases to avoid duplicate logic.
        if config.instruction_discriminator <= 1 {
            use crate::tests::test_utils::{
                encode_create_ata_instruction_data, CreateAtaInstructionType,
            };
            // Compute optional account length when token_account_len_arg is requested.
            let account_len_opt: Option<u16> = if variant.token_account_len_arg {
                if config.token_program
                    == Pubkey::new_from_array(pinocchio_pubkey::pubkey!(
                        "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
                    ))
                {
                    // Token-2022 path
                    if matches!(config.base_test, BaseTestType::CreateExtended) {
                        let account_extensions =
                            ExtensionType::get_required_init_account_extensions(&[
                                ExtensionType::TransferFeeConfig,
                                ExtensionType::NonTransferable,
                                ExtensionType::TransferHook,
                                ExtensionType::DefaultAccountState,
                                ExtensionType::MetadataPointer,
                            ]);
                        Some(
                            ExtensionType::try_calculate_account_len::<spl_token_2022::state::Account>(
                                &account_extensions,
                            )
                            .expect("failed to calculate extended account length") as u16,
                        )
                    } else {
                        Some(
                            ExtensionType::try_calculate_account_len::<spl_token_2022::state::Account>(&[
                                ExtensionType::ImmutableOwner,
                            ])
                            .expect("failed to calculate Token-2022 account length") as u16,
                        )
                    }
                } else {
                    // Standard SPL Token account size
                    Some(165u16)
                }
            } else {
                None
            };

            let instruction_type = if config.instruction_discriminator == 0 {
                // Create
                CreateAtaInstructionType::Create {
                    bump: if variant.bump_arg || variant.token_account_len_arg {
                        Some(bump)
                    } else {
                        None
                    },
                    account_len: account_len_opt,
                }
            } else {
                // CreateIdempotent
                CreateAtaInstructionType::CreateIdempotent {
                    bump: if variant.bump_arg { Some(bump) } else { None },
                }
            };

            #[cfg(feature = "full-debug-logs")]
            println!(
                "🐛 [DEBUG] Early return path - building instruction with instruction_type: {:?}",
                instruction_type
            );
            let data = encode_create_ata_instruction_data(&instruction_type);
            #[cfg(feature = "full-debug-logs")]
            println!("🐛 [DEBUG] Early return path - encoded data: {:?}", data);
            return data;
        }

        let mut data = vec![config.instruction_discriminator];

        // Special handling for RecoverNested/RecoverMultisig bump variants
        if (matches!(
            config.base_test,
            BaseTestType::RecoverNested | BaseTestType::RecoverMultisig
        )) && variant.bump_arg
        {
            // For recover operations with bump optimization, we need 3 bumps:
            // owner_bump, nested_bump, destination_bump
            // Calculate each bump correctly based on the account addresses in the instruction

            // Use the provided bump as the owner_bump (already calculated correctly)
            data.push(bump); // owner_bump (already calculated)

            // Calculate the actual nested_bump and destination_bump from the account addresses
            // Account layout for recover operations:
            // accounts[0]: nested_ata, accounts[1]: nested_mint, accounts[2]: dest_ata,
            // accounts[3]: owner_ata, accounts[4]: owner_mint, accounts[5]: wallet,
            // accounts[6]: token_program

            // Calculate nested_bump: find_program_address([owner_ata, token_program, nested_mint], ata_program)
            let (_, nested_bump) = Pubkey::find_program_address(
                &[
                    accounts[3].0.as_ref(), // owner_ata
                    accounts[6].0.as_ref(), // token_program
                    accounts[1].0.as_ref(), // nested_mint
                ],
                &ata_implementation.program_id,
            );

            // Calculate destination_bump: find_program_address([wallet, token_program, nested_mint], ata_program)
            let (_, destination_bump) = Pubkey::find_program_address(
                &[
                    accounts[5].0.as_ref(), // wallet
                    accounts[6].0.as_ref(), // token_program
                    accounts[1].0.as_ref(), // nested_mint
                ],
                &ata_implementation.program_id,
            );

            data.push(nested_bump);
            data.push(destination_bump);
            return data;
        }

        // Bump / length logic for discriminator > 1 (Recover etc.)
        if variant.bump_arg {
            data.push(bump);
        }

        data
    }

    /// Apply failure mode to instruction and accounts using focused helper functions
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
            // Account owner modifications
            FailureMode::WrongPayerOwner(owner) => {
                FailureAccountBuilder::set_wrong_owner(accounts, payer, *owner);
            }
            FailureMode::MintWrongOwner(wrong_owner) => {
                FailureAccountBuilder::set_wrong_owner(accounts, mint, *wrong_owner);
            }
            FailureMode::AtaWrongOwner(wrong_owner) => {
                FailureAccountBuilder::set_custom_account_state(
                    accounts,
                    ata,
                    vec![0u8; TOKEN_ACCOUNT_SIZE],
                    *wrong_owner,
                    2_000_000,
                );
            }

            // Account balance modifications
            FailureMode::InsufficientFunds(amount) => {
                FailureAccountBuilder::set_insufficient_balance(accounts, payer, *amount);
            }

            // Account data size modifications
            FailureMode::InvalidMintStructure(wrong_size) => {
                FailureAccountBuilder::set_wrong_data_size(accounts, mint, *wrong_size);
            }
            FailureMode::TokenAccountWrongSize(wrong_size) => {
                FailureAccountBuilder::set_custom_account_state(
                    accounts,
                    ata,
                    vec![0u8; *wrong_size],
                    config.token_program,
                    2_000_000,
                );
            }
            FailureMode::WrongAccountSizeForExtensions(wrong_size) => {
                FailureAccountBuilder::set_custom_account_state(
                    accounts,
                    ata,
                    vec![0u8; *wrong_size],
                    config.token_program,
                    2_000_000,
                );
            }

            // Account structure modifications
            FailureMode::InvalidTokenAccountStructure => {
                FailureAccountBuilder::set_invalid_token_account_structure(
                    accounts,
                    ata,
                    &config.token_program,
                );
            }
            FailureMode::MissingExtensions => {
                FailureAccountBuilder::set_custom_account_state(
                    accounts,
                    ata,
                    vec![0u8; 200], // Large but missing extension data
                    config.token_program,
                    2_000_000,
                );
            }
            FailureMode::InvalidExtensionData => {
                let mut data = vec![0u8; 200];
                data[TOKEN_ACCOUNT_SIZE..169].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // Invalid extension type
                FailureAccountBuilder::set_custom_account_state(
                    accounts,
                    ata,
                    data,
                    config.token_program,
                    2_000_000,
                );
            }

            // Token account specific modifications
            FailureMode::TokenAccountWrongMint(wrong_mint) => {
                FailureAccountBuilder::set_token_account_wrong_mint(
                    accounts,
                    ata,
                    *wrong_mint,
                    &wallet,
                    &config.token_program,
                );
            }
            FailureMode::TokenAccountWrongOwner(wrong_owner) => {
                FailureAccountBuilder::set_token_account_wrong_owner(
                    accounts,
                    ata,
                    &mint,
                    wrong_owner,
                    &config.token_program,
                );
            }

            // Multisig account modifications
            FailureMode::InvalidMultisigData => {
                FailureAccountBuilder::set_invalid_multisig_data(
                    accounts,
                    wallet,
                    &config.token_program,
                );
            }
            FailureMode::UninitializedMultisig => {
                let signer1 = Pubkey::new_unique();
                let mut data = vec![0u8; MULTISIG_ACCOUNT_SIZE];
                data[0] = 1; // m = 1
                data[1] = 1; // n = 1
                data[2] = 0; // is_initialized = false
                data[3..35].copy_from_slice(signer1.as_ref());
                FailureAccountBuilder::set_custom_account_state(
                    accounts,
                    wallet,
                    data,
                    config.token_program,
                    0,
                );
                FailureAccountBuilder::add_account(
                    accounts,
                    signer1,
                    AccountBuilder::system_account(1_000_000_000),
                );
            }

            // Instruction meta modifications
            FailureMode::PayerNotSigned => {
                FailureInstructionBuilder::set_account_signer_status(ix, payer, false);
            }
            FailureMode::AtaNotWritable => {
                FailureInstructionBuilder::set_account_writable_status(ix, ata, false);
            }
            FailureMode::AtaAddressMismatchLamportDrain => {
                // Handled by the custom builder in failure_scenarios.rs
                // This complex scenario requires custom instruction and account setup
            }
            FailureMode::RecoverWalletNotSigner => {
                if matches!(
                    config.base_test,
                    BaseTestType::RecoverNested | BaseTestType::RecoverMultisig
                ) {
                    FailureInstructionBuilder::set_account_signer_status_by_index(ix, 5, false);
                } else {
                    FailureInstructionBuilder::set_account_signer_status(ix, wallet, false);
                }
            }
            FailureMode::RecoverMultisigInsufficientSigners => {
                // metas layout: 0..6 standard accounts, 7.. signers (max 3 signers in test)
                // Keep only the *first* signer account as a true signer and clear the rest.
                if ix.accounts.len() > 7 {
                    // signer 0 remains signer
                    FailureInstructionBuilder::set_account_signer_status_by_index(ix, 7, true);

                    // signer 1 – unset signer flag
                    if ix.accounts.len() > 8 {
                        FailureInstructionBuilder::set_account_signer_status_by_index(ix, 8, false);
                    }

                    // signer 2 – unset signer flag (if present)
                    if ix.accounts.len() > 9 {
                        FailureInstructionBuilder::set_account_signer_status_by_index(ix, 9, false);
                    }
                }
            }
            FailureMode::RecoverMultisigDuplicateSigners => {
                // Handled by the custom builder in failure_scenarios.rs
                // The custom builder duplicates a signer account to exploit the vulnerability
            }
            FailureMode::RecoverMultisigNonSignerAccount => {
                // Handled by the custom builder in failure_scenarios.rs
                // The custom builder passes a multisig account but does not mark it as a signer
            }
            FailureMode::RecoverMultisigWrongWalletOwner(wrong_owner) => {
                // Set the multisig wallet to be owned by the wrong program
                FailureAccountBuilder::set_wrong_owner(accounts, wallet, *wrong_owner);
            }

            // Address replacement (both instruction and accounts)
            FailureMode::WrongSystemProgram(wrong_id) => {
                FailureInstructionBuilder::replace_account_everywhere(
                    ix,
                    accounts,
                    solana_system_interface::program::id(),
                    *wrong_id,
                );
            }
            FailureMode::WrongTokenProgram(wrong_id) => {
                FailureInstructionBuilder::replace_account_everywhere(
                    ix,
                    accounts,
                    config.token_program,
                    *wrong_id,
                );
            }
            FailureMode::WrongAtaAddress(wrong_ata) => {
                FailureInstructionBuilder::replace_account_everywhere(
                    ix, accounts, ata, *wrong_ata,
                );
            }
            FailureMode::RecoverWrongNestedAta(wrong_nested) => {
                FailureInstructionBuilder::replace_account_everywhere_by_index(
                    ix,
                    accounts,
                    0,
                    *wrong_nested,
                );
            }
            FailureMode::RecoverWrongDestination(wrong_dest) => {
                FailureInstructionBuilder::replace_account_everywhere_by_index(
                    ix,
                    accounts,
                    2,
                    *wrong_dest,
                );
            }

            // Instruction data modifications
            FailureMode::InvalidDiscriminator(disc) => {
                FailureInstructionBuilder::set_discriminator(ix, *disc);
            }
            FailureMode::InvalidBumpValue(invalid_bump) => {
                #[cfg(feature = "full-debug-logs")]
                println!(
                    "🐛 [DEBUG] Applying InvalidBumpValue({}) to instruction. Data before: {:?}",
                    invalid_bump, ix.data
                );
                FailureInstructionBuilder::set_bump_value(ix, *invalid_bump);
                #[cfg(feature = "full-debug-logs")]
                println!(
                    "🐛 [DEBUG] InvalidBumpValue applied. Data after: {:?}",
                    ix.data
                );
            }

            // Complex recovery modifications
            FailureMode::RecoverNestedWrongOwner(wrong_owner) => {
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
            FailureMode::InvalidSignerAccounts(wrong_signers) => {
                for (i, wrong_signer) in wrong_signers.iter().enumerate() {
                    FailureInstructionBuilder::replace_account_meta_by_index(
                        ix,
                        8 + i,
                        *wrong_signer,
                    );
                    FailureAccountBuilder::add_account(
                        accounts,
                        *wrong_signer,
                        AccountBuilder::system_account(1_000_000_000),
                    );
                }
            }
        }
    }
}

/// Calculate test number from base test type and variant.
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
        BaseTestType::CreateExtended => 51,
        BaseTestType::RecoverNested => 60,
        BaseTestType::RecoverMultisig => 70,
        BaseTestType::WorstCase => 80,
    };

    // Currently len cannot be true if bump is false. Those should be unreachable.
    let variant_offset = match (
        variant.rent_arg,
        variant.bump_arg,
        variant.token_account_len_arg,
    ) {
        (false, false, false) => 0,
        (true, false, false) => 1,
        (false, true, false) => 2,
        (false, false, true) => panic!("token_account_len cannot be true if bump is false"),
        (true, true, false) => 4,
        (true, false, true) => panic!("token_account_len cannot be true if bump is false"),
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
            BaseTestType::CreateExtended => 50,
            BaseTestType::RecoverNested => 60,
            BaseTestType::RecoverMultisig => 70,
            BaseTestType::WorstCase => 80,
        };

    let variant_offset = match (
        variant.rent_arg,
        variant.bump_arg,
        variant.token_account_len_arg,
    ) {
        (false, false, false) => 0,
        (true, false, false) => 1,
        (false, true, false) => 2,
        (false, false, true) => panic!("token_account_len arg without bump arg"),
        (true, true, false) => 4,
        (true, false, true) => panic!("token_account_len arg without bump arg"),
        (true, true, true) => 6,
        _ => 7,
    };

    let failure_id = FAILURE_COUNTER.fetch_add(1, Ordering::SeqCst);
    base + variant_offset + (failure_id % 8)
}
