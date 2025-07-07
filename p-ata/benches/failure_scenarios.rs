use {
    mollusk_svm::program::loader_keys::LOADER_V3,
    solana_account::Account,
    solana_instruction::{AccountMeta, Instruction},
    solana_logger,
    solana_pubkey::Pubkey,
};

#[path = "common.rs"]
mod common;
use common::*;

#[path = "common_builders.rs"]
mod common_builders;
use common_builders::{CommonTestCaseBuilder, FailureMode};

// ================================ FAILURE TEST CONSTANTS ================================

const FAKE_SYSTEM_PROGRAM_ID: Pubkey = Pubkey::new_from_array([1u8; 32]);
const FAKE_TOKEN_PROGRAM_ID: Pubkey = Pubkey::new_from_array([2u8; 32]);

// ================================ FAILURE TEST BUILDERS ================================

/// Failure test builders using the consolidated builder pattern where possible.
/// Complex scenarios that require custom logic are implemented directly.

// Helper function for complex cases that need custom logic
fn build_base_failure_accounts(
    base_test: BaseTestType,
    variant: TestVariant,
    ata_implementation: &AtaImplementation,
) -> (Pubkey, Pubkey, Pubkey) {
    let test_number = common_builders::calculate_failure_test_number(base_test, variant);
    let [payer, mint, wallet] = crate::common::structured_pk_multi(
        &ata_implementation.variant,
        crate::common::TestBankId::Failures,
        test_number,
        [
            crate::common::AccountTypeId::Payer,
            crate::common::AccountTypeId::Mint,
            crate::common::AccountTypeId::Wallet,
        ],
    );
    (payer, mint, wallet)
}

struct FailureTestBuilder;

impl FailureTestBuilder {
    fn build_fail_wrong_payer_owner(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let ata_impl =
            CommonTestCaseBuilder::create_ata_implementation_from_program_id(*program_id);
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::Create,
            TestVariant::BASE,
            &ata_impl,
            token_program_id,
            FailureMode::WrongPayerOwner(*token_program_id),
        )
    }

    fn build_fail_payer_not_signed(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let ata_impl =
            CommonTestCaseBuilder::create_ata_implementation_from_program_id(*program_id);
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::Create,
            TestVariant::BASE,
            &ata_impl,
            token_program_id,
            FailureMode::PayerNotSigned,
        )
    }

    fn build_fail_wrong_system_program(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let ata_impl =
            CommonTestCaseBuilder::create_ata_implementation_from_program_id(*program_id);
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::Create,
            TestVariant::BASE,
            &ata_impl,
            token_program_id,
            FailureMode::WrongSystemProgram(FAKE_SYSTEM_PROGRAM_ID),
        )
    }

    fn build_fail_wrong_token_program(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let ata_impl =
            CommonTestCaseBuilder::create_ata_implementation_from_program_id(*program_id);
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::Create,
            TestVariant::BASE,
            &ata_impl,
            token_program_id,
            FailureMode::WrongTokenProgram(FAKE_TOKEN_PROGRAM_ID),
        )
    }

    fn build_fail_insufficient_funds(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let ata_impl =
            CommonTestCaseBuilder::create_ata_implementation_from_program_id(*program_id);
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::Create,
            TestVariant::BASE,
            &ata_impl,
            token_program_id,
            FailureMode::InsufficientFunds(1000),
        )
    }

    fn build_fail_wrong_ata_address(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let ata_impl =
            CommonTestCaseBuilder::create_ata_implementation_from_program_id(*program_id);
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::Create,
            TestVariant::BASE,
            &ata_impl,
            token_program_id,
            FailureMode::WrongAtaAddress(crate::common::structured_pk(
                &ata_impl.variant,
                crate::common::TestBankId::Failures,
                173,
                crate::common::AccountTypeId::Ata,
            )),
        )
    }

    /// Build CREATE failure test with mint owned by wrong program
    fn build_fail_mint_wrong_owner(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let ata_impl =
            CommonTestCaseBuilder::create_ata_implementation_from_program_id(*program_id);
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::Create,
            TestVariant::BASE,
            &ata_impl,
            token_program_id,
            FailureMode::MintWrongOwner(SYSTEM_PROGRAM_ID),
        )
    }

    /// Build CREATE failure test with invalid mint structure
    fn build_fail_invalid_mint_structure(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let ata_impl =
            CommonTestCaseBuilder::create_ata_implementation_from_program_id(*program_id);
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::Create,
            TestVariant::BASE,
            &ata_impl,
            token_program_id,
            FailureMode::InvalidMintStructure(50), // Wrong size - should be 82
        )
    }

    /// Build CREATE_IDEMPOTENT failure test with invalid token account structure
    fn build_fail_invalid_token_account_structure(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let ata_impl =
            CommonTestCaseBuilder::create_ata_implementation_from_program_id(*program_id);
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::CreateIdempotent,
            TestVariant::BASE,
            &ata_impl,
            token_program_id,
            FailureMode::InvalidTokenAccountStructure,
        )
    }

    /// Build RECOVER failure test with wallet not signer
    fn build_fail_recover_wallet_not_signer(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let ata_impl =
            CommonTestCaseBuilder::create_ata_implementation_from_program_id(*program_id);
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::RecoverNested,
            TestVariant::BASE,
            &ata_impl,
            token_program_id,
            FailureMode::RecoverWalletNotSigner,
        )
    }

    /// Build RECOVER failure test with multisig insufficient signers
    fn build_fail_recover_multisig_insufficient_signers(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let ata_impl =
            CommonTestCaseBuilder::create_ata_implementation_from_program_id(*program_id);
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::RecoverMultisig,
            TestVariant::BASE,
            &ata_impl,
            token_program_id,
            FailureMode::RecoverMultisigInsufficientSigners,
        )
    }

    /// Build failure test with invalid instruction discriminator
    fn build_fail_invalid_discriminator(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let ata_impl =
            CommonTestCaseBuilder::create_ata_implementation_from_program_id(*program_id);
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::Create,
            TestVariant::BASE,
            &ata_impl,
            token_program_id,
            FailureMode::InvalidDiscriminator(99), // Invalid discriminator (should be 0, 1, or 2)
        )
    }

    /// Build failure test with invalid bump value
    fn build_fail_invalid_bump_value(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let ata_impl =
            CommonTestCaseBuilder::create_ata_implementation_from_program_id(*program_id);
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::Create,
            TestVariant {
                bump_arg: true,
                ..TestVariant::BASE
            },
            &ata_impl,
            token_program_id,
            FailureMode::InvalidBumpValue(99), // Invalid bump (not the correct bump)
        )
    }

    /// Build CREATE failure test with ATA owned by system program (existing ATA with wrong owner)
    fn build_fail_ata_owned_by_system_program(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let ata_impl =
            CommonTestCaseBuilder::create_ata_implementation_from_program_id(*program_id);
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::Create,
            TestVariant::BASE,
            &ata_impl,
            token_program_id,
            FailureMode::AtaWrongOwner(SYSTEM_PROGRAM_ID),
        )
    }

    /// Build RECOVER failure test with wrong nested ATA address
    fn build_fail_recover_wrong_nested_ata_address(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let ata_impl =
            CommonTestCaseBuilder::create_ata_implementation_from_program_id(*program_id);
        let test_number = common_builders::calculate_failure_test_number(
            BaseTestType::RecoverNested,
            TestVariant::BASE,
        );
        let [wrong_nested_ata, nested_mint, dest_ata, owner_ata, owner_mint, wallet] =
            crate::common::structured_pk_multi(
                &ata_impl.variant,
                crate::common::TestBankId::Failures,
                test_number,
                [
                    crate::common::AccountTypeId::NestedAta, // wrong_nested_ata - will be wrong in the test
                    crate::common::AccountTypeId::NestedMint,
                    crate::common::AccountTypeId::Ata, // dest_ata
                    crate::common::AccountTypeId::OwnerAta,
                    crate::common::AccountTypeId::OwnerMint,
                    crate::common::AccountTypeId::Wallet,
                ],
            );

        let accounts = vec![
            // Wrong nested ATA address (doesn't match proper derivation)
            (
                wrong_nested_ata,
                AccountBuilder::token_account(&nested_mint, &owner_ata, 100, token_program_id),
            ),
            (
                nested_mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (
                dest_ata,
                AccountBuilder::token_account(&nested_mint, &wallet, 0, token_program_id),
            ),
            (
                owner_ata,
                AccountBuilder::token_account(&owner_mint, &wallet, 0, token_program_id),
            ),
            (
                owner_mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (wallet, AccountBuilder::system_account(1_000_000_000)),
            (
                *token_program_id,
                AccountBuilder::executable_program(LOADER_V3),
            ),
            (
                Pubkey::from(spl_token_interface::program::ID),
                AccountBuilder::executable_program(LOADER_V3),
            ),
        ];

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(wrong_nested_ata, false), // Wrong nested ATA
                AccountMeta::new_readonly(nested_mint, false),
                AccountMeta::new(dest_ata, false),
                AccountMeta::new(owner_ata, false),
                AccountMeta::new_readonly(owner_mint, false),
                AccountMeta::new(wallet, true),
                AccountMeta::new_readonly(*token_program_id, false),
                AccountMeta::new_readonly(Pubkey::from(spl_token_interface::program::ID), false),
            ],
            data: vec![2u8],
        };

        (ix, accounts)
    }

    /// Build RECOVER failure test with wrong destination address
    fn build_fail_recover_wrong_destination_address(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let ata_impl =
            CommonTestCaseBuilder::create_ata_implementation_from_program_id(*program_id);
        let test_number = common_builders::calculate_failure_test_number(
            BaseTestType::RecoverNested,
            TestVariant::BASE,
        );
        let [nested_ata, nested_mint, wrong_dest_ata, owner_ata, owner_mint, wallet] =
            crate::common::structured_pk_multi(
                &ata_impl.variant,
                crate::common::TestBankId::Failures,
                test_number,
                [
                    crate::common::AccountTypeId::NestedAta,
                    crate::common::AccountTypeId::NestedMint,
                    crate::common::AccountTypeId::Ata, // wrong_dest_ata
                    crate::common::AccountTypeId::OwnerAta,
                    crate::common::AccountTypeId::OwnerMint,
                    crate::common::AccountTypeId::Wallet,
                ],
            );

        let accounts = vec![
            (
                nested_ata,
                AccountBuilder::token_account(&nested_mint, &owner_ata, 100, token_program_id),
            ),
            (
                nested_mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            // Wrong destination ATA
            (
                wrong_dest_ata,
                AccountBuilder::token_account(&nested_mint, &wallet, 0, token_program_id),
            ),
            (
                owner_ata,
                AccountBuilder::token_account(&owner_mint, &wallet, 0, token_program_id),
            ),
            (
                owner_mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (wallet, AccountBuilder::system_account(1_000_000_000)),
            (
                *token_program_id,
                AccountBuilder::executable_program(LOADER_V3),
            ),
            (
                Pubkey::from(spl_token_interface::program::ID),
                AccountBuilder::executable_program(LOADER_V3),
            ),
        ];

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(nested_ata, false),
                AccountMeta::new_readonly(nested_mint, false),
                AccountMeta::new(wrong_dest_ata, false), // Wrong destination
                AccountMeta::new(owner_ata, false),
                AccountMeta::new_readonly(owner_mint, false),
                AccountMeta::new(wallet, true),
                AccountMeta::new_readonly(*token_program_id, false),
                AccountMeta::new_readonly(Pubkey::from(spl_token_interface::program::ID), false),
            ],
            data: vec![2u8],
        };

        (ix, accounts)
    }

    /// Build RECOVER failure test with invalid bump for RecoverNested
    fn build_fail_recover_invalid_bump_value(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let ata_impl =
            CommonTestCaseBuilder::create_ata_implementation_from_program_id(*program_id);
        let test_number = common_builders::calculate_failure_test_number(
            BaseTestType::RecoverNested,
            TestVariant::BASE,
        );
        let [nested_ata, nested_mint, dest_ata, owner_ata, owner_mint, wallet] =
            crate::common::structured_pk_multi(
                &ata_impl.variant,
                crate::common::TestBankId::Failures,
                test_number,
                [
                    crate::common::AccountTypeId::NestedAta,
                    crate::common::AccountTypeId::NestedMint,
                    crate::common::AccountTypeId::Ata, // dest_ata
                    crate::common::AccountTypeId::OwnerAta,
                    crate::common::AccountTypeId::OwnerMint,
                    crate::common::AccountTypeId::Wallet,
                ],
            );

        let accounts = vec![
            (
                nested_ata,
                AccountBuilder::token_account(&nested_mint, &owner_ata, 100, token_program_id),
            ),
            (
                nested_mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (
                dest_ata,
                AccountBuilder::token_account(&nested_mint, &wallet, 0, token_program_id),
            ),
            (
                owner_ata,
                AccountBuilder::token_account(&owner_mint, &wallet, 0, token_program_id),
            ),
            (
                owner_mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (wallet, AccountBuilder::system_account(1_000_000_000)),
            (
                *token_program_id,
                AccountBuilder::executable_program(LOADER_V3),
            ),
            (
                Pubkey::from(spl_token_interface::program::ID),
                AccountBuilder::executable_program(LOADER_V3),
            ),
        ];

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(nested_ata, false),
                AccountMeta::new_readonly(nested_mint, false),
                AccountMeta::new(dest_ata, false),
                AccountMeta::new(owner_ata, false),
                AccountMeta::new_readonly(owner_mint, false),
                AccountMeta::new(wallet, true),
                AccountMeta::new_readonly(*token_program_id, false),
                AccountMeta::new_readonly(Pubkey::from(spl_token_interface::program::ID), false),
            ],
            data: vec![2u8, 99u8], // RecoverNested with invalid bump
        };

        (ix, accounts)
    }

    /// Build CREATE failure test with wrong token account size
    fn build_fail_wrong_token_account_size(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let ata_impl =
            CommonTestCaseBuilder::create_ata_implementation_from_program_id(*program_id);
        let (payer, mint, wallet) = build_base_failure_accounts(
            BaseTestType::CreateIdempotent,
            TestVariant::BASE,
            &ata_impl,
        );
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            // ATA exists but wrong size
            (
                ata,
                Account {
                    lamports: 2_000_000,
                    data: vec![0u8; 100], // Wrong size - should be 165
                    owner: *token_program_id,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (wallet, AccountBuilder::system_account(0)),
            (
                mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (
                SYSTEM_PROGRAM_ID,
                AccountBuilder::executable_program(NATIVE_LOADER_ID),
            ),
            (
                *token_program_id,
                AccountBuilder::executable_program(LOADER_V3),
            ),
        ];

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new(ata, false),
                AccountMeta::new_readonly(wallet, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new_readonly(*token_program_id, false),
            ],
            data: vec![1u8], // CreateIdempotent instruction
        };

        (ix, accounts)
    }

    /// Build CREATE failure test with token account pointing to wrong mint
    fn build_fail_token_account_wrong_mint(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let ata_impl =
            CommonTestCaseBuilder::create_ata_implementation_from_program_id(*program_id);
        let (payer, mint, wallet) = build_base_failure_accounts(
            BaseTestType::CreateIdempotent,
            TestVariant::BASE,
            &ata_impl,
        );
        let test_number = common_builders::calculate_failure_test_number(
            BaseTestType::CreateIdempotent,
            TestVariant::BASE,
        );
        let wrong_mint = crate::common::structured_pk(
            &ata_impl.variant,
            crate::common::TestBankId::Failures,
            test_number + 1, // offset for different account
            crate::common::AccountTypeId::Mint,
        );
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            // ATA points to wrong mint
            (
                ata,
                AccountBuilder::token_account(&wrong_mint, &wallet, 0, token_program_id),
            ),
            (wallet, AccountBuilder::system_account(0)),
            (
                mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (
                wrong_mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (
                SYSTEM_PROGRAM_ID,
                AccountBuilder::executable_program(NATIVE_LOADER_ID),
            ),
            (
                *token_program_id,
                AccountBuilder::executable_program(LOADER_V3),
            ),
        ];

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new(ata, false),
                AccountMeta::new_readonly(wallet, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new_readonly(*token_program_id, false),
            ],
            data: vec![1u8], // CreateIdempotent instruction
        };

        (ix, accounts)
    }

    /// Build CREATE failure test with token account having wrong owner
    fn build_fail_token_account_wrong_owner(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let ata_impl =
            CommonTestCaseBuilder::create_ata_implementation_from_program_id(*program_id);
        let (payer, mint, wallet) = build_base_failure_accounts(
            BaseTestType::CreateIdempotent,
            TestVariant::BASE,
            &ata_impl,
        );
        let test_number = common_builders::calculate_failure_test_number(
            BaseTestType::CreateIdempotent,
            TestVariant::BASE,
        );
        let wrong_owner = crate::common::structured_pk(
            &ata_impl.variant,
            crate::common::TestBankId::Failures,
            test_number + 1,
            crate::common::AccountTypeId::Wallet,
        );
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            // ATA has wrong owner
            (
                ata,
                AccountBuilder::token_account(&mint, &wrong_owner, 0, token_program_id),
            ),
            (wallet, AccountBuilder::system_account(0)),
            (
                mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (
                SYSTEM_PROGRAM_ID,
                AccountBuilder::executable_program(NATIVE_LOADER_ID),
            ),
            (
                *token_program_id,
                AccountBuilder::executable_program(LOADER_V3),
            ),
        ];

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new(ata, false),
                AccountMeta::new_readonly(wallet, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new_readonly(*token_program_id, false),
            ],
            data: vec![1u8], // CreateIdempotent instruction
        };

        (ix, accounts)
    }

    /// Build CREATE failure test with immutable account (non-writable)
    fn build_fail_immutable_account(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let ata_impl =
            CommonTestCaseBuilder::create_ata_implementation_from_program_id(*program_id);
        CommonTestCaseBuilder::build_failure_test_case(
            BaseTestType::Create,
            TestVariant::BASE,
            &ata_impl,
            token_program_id,
            FailureMode::AtaNotWritable,
        )
    }

    /// Build RECOVER failure test with nested account having wrong owner
    fn build_fail_recover_nested_wrong_owner(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let ata_impl =
            CommonTestCaseBuilder::create_ata_implementation_from_program_id(*program_id);
        let test_number = common_builders::calculate_failure_test_number(
            BaseTestType::RecoverNested,
            TestVariant::BASE,
        );
        let [nested_ata, nested_mint, dest_ata, owner_ata, owner_mint, wallet, wrong_owner] =
            crate::common::structured_pk_multi(
                &ata_impl.variant,
                crate::common::TestBankId::Failures,
                test_number,
                [
                    crate::common::AccountTypeId::NestedAta,
                    crate::common::AccountTypeId::NestedMint,
                    crate::common::AccountTypeId::Ata, // dest_ata
                    crate::common::AccountTypeId::OwnerAta,
                    crate::common::AccountTypeId::OwnerMint,
                    crate::common::AccountTypeId::Wallet,
                    crate::common::AccountTypeId::Signer1, // wrong_owner
                ],
            );

        let accounts = vec![
            // Nested ATA owned by wrong owner (not the owner_ata)
            (
                nested_ata,
                AccountBuilder::token_account(&nested_mint, &wrong_owner, 100, token_program_id),
            ),
            (
                nested_mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (
                dest_ata,
                AccountBuilder::token_account(&nested_mint, &wallet, 0, token_program_id),
            ),
            (
                owner_ata,
                AccountBuilder::token_account(&owner_mint, &wallet, 0, token_program_id),
            ),
            (
                owner_mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (wallet, AccountBuilder::system_account(1_000_000_000)),
            (
                *token_program_id,
                AccountBuilder::executable_program(LOADER_V3),
            ),
            (
                Pubkey::from(spl_token_interface::program::ID),
                AccountBuilder::executable_program(LOADER_V3),
            ),
        ];

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(nested_ata, false),
                AccountMeta::new_readonly(nested_mint, false),
                AccountMeta::new(dest_ata, false),
                AccountMeta::new(owner_ata, false),
                AccountMeta::new_readonly(owner_mint, false),
                AccountMeta::new(wallet, true),
                AccountMeta::new_readonly(*token_program_id, false),
                AccountMeta::new_readonly(Pubkey::from(spl_token_interface::program::ID), false),
            ],
            data: vec![2u8],
        };

        (ix, accounts)
    }

    /// Build CREATE failure test with wrong account size for extensions
    fn build_fail_wrong_account_size_for_extensions(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let ata_impl =
            CommonTestCaseBuilder::create_ata_implementation_from_program_id(*program_id);
        let (payer, mint, wallet) = build_base_failure_accounts(
            BaseTestType::CreateIdempotent,
            TestVariant::BASE,
            &ata_impl,
        );
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            // ATA with wrong size for extensions (too small for ImmutableOwner)
            (
                ata,
                Account {
                    lamports: 2_000_000,
                    data: vec![0u8; 165], // Standard size, but mint has extensions
                    owner: *token_program_id,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (wallet, AccountBuilder::system_account(0)),
            // Extended mint that requires larger ATA
            (
                mint,
                AccountBuilder::mint_account(0, token_program_id, true),
            ), // extended = true
            (
                SYSTEM_PROGRAM_ID,
                AccountBuilder::executable_program(NATIVE_LOADER_ID),
            ),
            (
                *token_program_id,
                AccountBuilder::executable_program(LOADER_V3),
            ),
        ];

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new(ata, false),
                AccountMeta::new_readonly(wallet, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new_readonly(*token_program_id, false),
            ],
            data: vec![1u8], // CreateIdempotent instruction
        };

        (ix, accounts)
    }

    /// Build CREATE failure test with missing extensions
    fn build_fail_missing_extensions(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let ata_impl =
            CommonTestCaseBuilder::create_ata_implementation_from_program_id(*program_id);
        let (payer, mint, wallet) = build_base_failure_accounts(
            BaseTestType::CreateIdempotent,
            TestVariant::BASE,
            &ata_impl,
        );
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            // ATA missing required extensions
            (
                ata,
                Account {
                    lamports: 2_000_000,
                    data: vec![0u8; 200], // Large enough but missing extension data
                    owner: *token_program_id,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (wallet, AccountBuilder::system_account(0)),
            // Extended mint that requires extensions in ATA
            (
                mint,
                AccountBuilder::mint_account(0, token_program_id, true),
            ), // extended = true
            (
                SYSTEM_PROGRAM_ID,
                AccountBuilder::executable_program(NATIVE_LOADER_ID),
            ),
            (
                *token_program_id,
                AccountBuilder::executable_program(LOADER_V3),
            ),
        ];

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new(ata, false),
                AccountMeta::new_readonly(wallet, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new_readonly(*token_program_id, false),
            ],
            data: vec![1u8], // CreateIdempotent instruction
        };

        (ix, accounts)
    }

    /// Build CREATE failure test with invalid extension data
    fn build_fail_invalid_extension_data(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let ata_impl =
            CommonTestCaseBuilder::create_ata_implementation_from_program_id(*program_id);
        let (payer, mint, wallet) = build_base_failure_accounts(
            BaseTestType::CreateIdempotent,
            TestVariant::BASE,
            &ata_impl,
        );
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            // ATA with malformed extension headers
            (
                ata,
                Account {
                    lamports: 2_000_000,
                    data: {
                        let mut data = vec![0u8; 200];
                        // Add invalid extension header at the end
                        data[165..169].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // Invalid extension type
                        data
                    },
                    owner: *token_program_id,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (wallet, AccountBuilder::system_account(0)),
            (
                mint,
                AccountBuilder::mint_account(0, token_program_id, true),
            ), // extended = true
            (
                SYSTEM_PROGRAM_ID,
                AccountBuilder::executable_program(NATIVE_LOADER_ID),
            ),
            (
                *token_program_id,
                AccountBuilder::executable_program(LOADER_V3),
            ),
        ];

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new(ata, false),
                AccountMeta::new_readonly(wallet, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new_readonly(*token_program_id, false),
            ],
            data: vec![1u8], // CreateIdempotent instruction
        };

        (ix, accounts)
    }

    /// Build RECOVER failure test with invalid multisig data
    fn build_fail_invalid_multisig_data(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let ata_impl =
            CommonTestCaseBuilder::create_ata_implementation_from_program_id(*program_id);
        let test_number = common_builders::calculate_failure_test_number(
            BaseTestType::RecoverMultisig,
            TestVariant::BASE,
        );
        let [nested_ata, nested_mint, dest_ata, owner_ata, owner_mint, wallet_ms] =
            crate::common::structured_pk_multi(
                &ata_impl.variant,
                crate::common::TestBankId::Failures,
                test_number,
                [
                    crate::common::AccountTypeId::NestedAta,
                    crate::common::AccountTypeId::NestedMint,
                    crate::common::AccountTypeId::Ata, // dest_ata
                    crate::common::AccountTypeId::OwnerAta,
                    crate::common::AccountTypeId::OwnerMint,
                    crate::common::AccountTypeId::Wallet, // wallet_ms (multisig)
                ],
            );

        let accounts = vec![
            (
                nested_ata,
                AccountBuilder::token_account(&nested_mint, &owner_ata, 100, token_program_id),
            ),
            (
                nested_mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (
                dest_ata,
                AccountBuilder::token_account(&nested_mint, &wallet_ms, 0, token_program_id),
            ),
            (
                owner_ata,
                AccountBuilder::token_account(&owner_mint, &wallet_ms, 0, token_program_id),
            ),
            (
                owner_mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (
                wallet_ms,
                Account {
                    lamports: 1_000_000_000,
                    data: vec![0xFF; 355], // Invalid multisig data (all 0xFF)
                    owner: *token_program_id,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                *token_program_id,
                AccountBuilder::executable_program(LOADER_V3),
            ),
            (
                Pubkey::from(spl_token_interface::program::ID),
                AccountBuilder::executable_program(LOADER_V3),
            ),
        ];

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(nested_ata, false),
                AccountMeta::new_readonly(nested_mint, false),
                AccountMeta::new(dest_ata, false),
                AccountMeta::new(owner_ata, false),
                AccountMeta::new_readonly(owner_mint, false),
                AccountMeta::new(wallet_ms, false), // Multisig wallet with invalid data
                AccountMeta::new_readonly(*token_program_id, false),
                AccountMeta::new_readonly(Pubkey::from(spl_token_interface::program::ID), false),
            ],
            data: vec![2u8],
        };

        (ix, accounts)
    }

    /// Build RECOVER failure test with invalid signer accounts (not in multisig list)
    fn build_fail_invalid_signer_accounts(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let ata_impl =
            CommonTestCaseBuilder::create_ata_implementation_from_program_id(*program_id);
        let test_number = common_builders::calculate_failure_test_number(
            BaseTestType::RecoverMultisig,
            TestVariant::BASE,
        );
        let [nested_ata, nested_mint, dest_ata, owner_ata, owner_mint, wallet_ms] =
            crate::common::structured_pk_multi(
                &ata_impl.variant,
                crate::common::TestBankId::Failures,
                test_number,
                [
                    crate::common::AccountTypeId::NestedAta,
                    crate::common::AccountTypeId::NestedMint,
                    crate::common::AccountTypeId::Ata, // dest_ata
                    crate::common::AccountTypeId::OwnerAta,
                    crate::common::AccountTypeId::OwnerMint,
                    crate::common::AccountTypeId::Wallet, // wallet_ms (multisig)
                ],
            );

        let signer1 = Pubkey::new_unique();
        let signer2 = Pubkey::new_unique();
        let signer3 = Pubkey::new_unique();
        let wrong_signer = Pubkey::new_unique(); // Not in multisig list

        let accounts = vec![
            (
                nested_ata,
                AccountBuilder::token_account(&nested_mint, &owner_ata, 100, token_program_id),
            ),
            (
                nested_mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (
                dest_ata,
                AccountBuilder::token_account(&nested_mint, &wallet_ms, 0, token_program_id),
            ),
            (
                owner_ata,
                AccountBuilder::token_account(&owner_mint, &wallet_ms, 0, token_program_id),
            ),
            (
                owner_mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (
                wallet_ms,
                Account {
                    lamports: 1_000_000_000,
                    data: AccountBuilder::multisig_data(2, &[signer1, signer2, signer3]), // m=2
                    owner: *token_program_id,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                *token_program_id,
                AccountBuilder::executable_program(LOADER_V3),
            ),
            (
                Pubkey::from(spl_token_interface::program::ID),
                AccountBuilder::executable_program(LOADER_V3),
            ),
            (wrong_signer, AccountBuilder::system_account(1_000_000_000)),
        ];

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(nested_ata, false),
                AccountMeta::new_readonly(nested_mint, false),
                AccountMeta::new(dest_ata, false),
                AccountMeta::new(owner_ata, false),
                AccountMeta::new_readonly(owner_mint, false),
                AccountMeta::new(wallet_ms, false), // Multisig wallet
                AccountMeta::new_readonly(*token_program_id, false),
                AccountMeta::new_readonly(Pubkey::from(spl_token_interface::program::ID), false),
                AccountMeta::new_readonly(wrong_signer, true), // Wrong signer (not in multisig list)
            ],
            data: vec![2u8],
        };

        (ix, accounts)
    }
}

// ================================ FAILURE TEST COMPARISON RUNNER ================================

struct FailureTestRunner;

impl FailureTestRunner {
    /// Run a failure test against both implementations and compare results
    fn run_failure_comparison_test<F>(
        name: &str,
        test_builder: F,
        p_ata_impl: &AtaImplementation,
        original_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> ComparisonResult
    where
        F: Fn(&Pubkey, &Pubkey) -> (Instruction, Vec<(Pubkey, Account)>),
    {
        // Build test for P-ATA
        let (p_ata_ix, p_ata_accounts) = test_builder(&p_ata_impl.program_id, token_program_id);

        // Build test for Original ATA (separate account set with correct ATA addresses)
        let (original_ix, original_accounts) =
            test_builder(&original_impl.program_id, token_program_id);

        // Run benchmarks
        let p_ata_result = ComparisonRunner::run_single_benchmark(
            name,
            &p_ata_ix,
            &p_ata_accounts,
            p_ata_impl,
            token_program_id,
        );
        let original_result = ComparisonRunner::run_single_benchmark(
            name,
            &original_ix,
            &original_accounts,
            original_impl,
            token_program_id,
        );

        // Create comparison result
        ComparisonRunner::create_comparison_result(name, p_ata_result, original_result)
    }

    /// Run comprehensive failure test comparison between implementations
    fn run_comprehensive_failure_comparison(
        p_ata_impl: &AtaImplementation,
        original_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> Vec<ComparisonResult> {
        println!("\n=== P-ATA VS ORIGINAL ATA FAILURE SCENARIOS COMPARISON ===");
        println!(
            "This validates that P-ATA maintains the same security properties as the original ATA"
        );

        let mut results = Vec::new();

        // Basic account ownership failure tests
        println!("\n--- Basic Account Ownership Failure Tests ---");

        let basic_tests = [
            (
                "fail_wrong_payer_owner",
                FailureTestBuilder::build_fail_wrong_payer_owner
                    as fn(&Pubkey, &Pubkey) -> (Instruction, Vec<(Pubkey, Account)>),
            ),
            (
                "fail_payer_not_signed",
                FailureTestBuilder::build_fail_payer_not_signed
                    as fn(&Pubkey, &Pubkey) -> (Instruction, Vec<(Pubkey, Account)>),
            ),
            (
                "fail_wrong_system_program",
                FailureTestBuilder::build_fail_wrong_system_program
                    as fn(&Pubkey, &Pubkey) -> (Instruction, Vec<(Pubkey, Account)>),
            ),
            (
                "fail_wrong_token_program",
                FailureTestBuilder::build_fail_wrong_token_program
                    as fn(&Pubkey, &Pubkey) -> (Instruction, Vec<(Pubkey, Account)>),
            ),
            (
                "fail_insufficient_funds",
                FailureTestBuilder::build_fail_insufficient_funds
                    as fn(&Pubkey, &Pubkey) -> (Instruction, Vec<(Pubkey, Account)>),
            ),
        ];

        for (test_name, test_builder) in basic_tests {
            let comparison = Self::run_failure_comparison_test(
                test_name,
                test_builder,
                p_ata_impl,
                original_impl,
                token_program_id,
            );
            common::ComparisonRunner::print_comparison_result(&comparison);
            results.push(comparison);
        }

        // Address derivation and structure failure tests
        println!("\n--- Address Derivation and Structure Failure Tests ---");

        let structure_tests = [
            (
                "fail_wrong_ata_address",
                FailureTestBuilder::build_fail_wrong_ata_address
                    as fn(&Pubkey, &Pubkey) -> (Instruction, Vec<(Pubkey, Account)>),
            ),
            (
                "fail_mint_wrong_owner",
                FailureTestBuilder::build_fail_mint_wrong_owner
                    as fn(&Pubkey, &Pubkey) -> (Instruction, Vec<(Pubkey, Account)>),
            ),
            (
                "fail_invalid_mint_structure",
                FailureTestBuilder::build_fail_invalid_mint_structure
                    as fn(&Pubkey, &Pubkey) -> (Instruction, Vec<(Pubkey, Account)>),
            ),
            (
                "fail_invalid_token_account_structure",
                FailureTestBuilder::build_fail_invalid_token_account_structure
                    as fn(&Pubkey, &Pubkey) -> (Instruction, Vec<(Pubkey, Account)>),
            ),
            (
                "fail_invalid_discriminator",
                FailureTestBuilder::build_fail_invalid_discriminator
                    as fn(&Pubkey, &Pubkey) -> (Instruction, Vec<(Pubkey, Account)>),
            ),
            (
                "fail_invalid_bump_value",
                FailureTestBuilder::build_fail_invalid_bump_value
                    as fn(&Pubkey, &Pubkey) -> (Instruction, Vec<(Pubkey, Account)>),
            ),
        ];

        for (test_name, test_builder) in structure_tests {
            let comparison = Self::run_failure_comparison_test(
                test_name,
                test_builder,
                p_ata_impl,
                original_impl,
                token_program_id,
            );
            common::ComparisonRunner::print_comparison_result(&comparison);
            results.push(comparison);
        }

        // Recovery-specific failure tests
        println!("\n--- Recovery Operation Failure Tests ---");

        let recovery_tests = [
            (
                "fail_recover_wallet_not_signer",
                FailureTestBuilder::build_fail_recover_wallet_not_signer
                    as fn(&Pubkey, &Pubkey) -> (Instruction, Vec<(Pubkey, Account)>),
            ),
            (
                "fail_recover_multisig_insufficient_signers",
                FailureTestBuilder::build_fail_recover_multisig_insufficient_signers
                    as fn(&Pubkey, &Pubkey) -> (Instruction, Vec<(Pubkey, Account)>),
            ),
            (
                "fail_recover_wrong_nested_ata_address",
                FailureTestBuilder::build_fail_recover_wrong_nested_ata_address
                    as fn(&Pubkey, &Pubkey) -> (Instruction, Vec<(Pubkey, Account)>),
            ),
            (
                "fail_recover_wrong_destination_address",
                FailureTestBuilder::build_fail_recover_wrong_destination_address
                    as fn(&Pubkey, &Pubkey) -> (Instruction, Vec<(Pubkey, Account)>),
            ),
            (
                "fail_recover_invalid_bump_value",
                FailureTestBuilder::build_fail_recover_invalid_bump_value
                    as fn(&Pubkey, &Pubkey) -> (Instruction, Vec<(Pubkey, Account)>),
            ),
        ];

        for (test_name, test_builder) in recovery_tests {
            let comparison = Self::run_failure_comparison_test(
                test_name,
                test_builder,
                p_ata_impl,
                original_impl,
                token_program_id,
            );
            common::ComparisonRunner::print_comparison_result(&comparison);
            results.push(comparison);
        }

        // Additional validation tests
        println!("\n--- Additional Validation Coverage Tests ---");

        let validation_tests = [
            (
                "fail_ata_owned_by_system_program",
                FailureTestBuilder::build_fail_ata_owned_by_system_program
                    as fn(&Pubkey, &Pubkey) -> (Instruction, Vec<(Pubkey, Account)>),
            ),
            (
                "fail_wrong_token_account_size",
                FailureTestBuilder::build_fail_wrong_token_account_size
                    as fn(&Pubkey, &Pubkey) -> (Instruction, Vec<(Pubkey, Account)>),
            ),
            (
                "fail_token_account_wrong_mint",
                FailureTestBuilder::build_fail_token_account_wrong_mint
                    as fn(&Pubkey, &Pubkey) -> (Instruction, Vec<(Pubkey, Account)>),
            ),
            (
                "fail_token_account_wrong_owner",
                FailureTestBuilder::build_fail_token_account_wrong_owner
                    as fn(&Pubkey, &Pubkey) -> (Instruction, Vec<(Pubkey, Account)>),
            ),
            (
                "fail_immutable_account",
                FailureTestBuilder::build_fail_immutable_account
                    as fn(&Pubkey, &Pubkey) -> (Instruction, Vec<(Pubkey, Account)>),
            ),
        ];

        for (test_name, test_builder) in validation_tests {
            let comparison = Self::run_failure_comparison_test(
                test_name,
                test_builder,
                p_ata_impl,
                original_impl,
                token_program_id,
            );
            common::ComparisonRunner::print_comparison_result(&comparison);
            results.push(comparison);
        }

        Self::print_failure_summary(&results);
        Self::output_failure_test_data(&results);
        results
    }

    fn output_failure_test_data(results: &[ComparisonResult]) {
        let mut json_entries = Vec::new();

        for result in results {
            let status = match (&result.p_ata.success, &result.original.success) {
                (true, true) => "pass", // Both succeeded (might be unexpected for failure tests)
                (false, false) => {
                    // Both failed - check if errors are the same type
                    let p_ata_error = result.p_ata.error_message.as_deref().unwrap_or("Unknown");
                    let original_error = result
                        .original
                        .error_message
                        .as_deref()
                        .unwrap_or("Unknown");

                    // Simple error type comparison - look for key differences
                    if p_ata_error.contains("InvalidInstructionData")
                        != original_error.contains("InvalidInstructionData")
                        || p_ata_error.contains("Custom(") != original_error.contains("Custom(")
                        || p_ata_error.contains("PrivilegeEscalation")
                            != original_error.contains("PrivilegeEscalation")
                    {
                        "failed, but different error"
                    } else {
                        "failed with same error"
                    }
                }
                (true, false) => "pass", // P-ATA works, original fails (P-ATA optimization)
                (false, true) => "fail", // P-ATA fails, original works (concerning)
            };

            let p_ata_error_json = match &result.p_ata.error_message {
                Some(msg) => format!(r#""{}""#, msg.replace('"', r#"\""#)),
                None => "null".to_string(),
            };

            let original_error_json = match &result.original.error_message {
                Some(msg) => format!(r#""{}""#, msg.replace('"', r#"\""#)),
                None => "null".to_string(),
            };

            let entry = format!(
                r#"    "{}": {{
      "status": "{}",
      "p_ata_success": {},
      "original_success": {},
      "p_ata_error": {},
      "original_error": {},
      "type": "failure_test"
    }}"#,
                result.test_name,
                status,
                result.p_ata.success,
                result.original.success,
                p_ata_error_json,
                original_error_json
            );
            json_entries.push(entry);
        }

        let output = format!(
            r#"{{
  "timestamp": "{}",
  "failure_tests": {{
{}
  }}
}}"#,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            json_entries.join(",\n")
        );

        // Create benchmark_results directory if it doesn't exist
        std::fs::create_dir_all("benchmark_results").ok();

        // Write failure test results
        if let Err(e) = std::fs::write("benchmark_results/failure_results.json", output) {
            eprintln!("Failed to write failure results: {}", e);
        } else {
            println!("\n🧪 Failure test results written to benchmark_results/failure_results.json");
        }
    }

    /// Print failure test summary with compatibility analysis
    fn print_failure_summary(results: &[ComparisonResult]) {
        println!("\n=== FAILURE TEST COMPATIBILITY SUMMARY ===");

        let total_tests = results.len();
        let both_rejected = results
            .iter()
            .filter(|r| matches!(r.compatibility_status, CompatibilityStatus::BothRejected))
            .count();
        let incompatible_failures = results
            .iter()
            .filter(|r| {
                matches!(
                    r.compatibility_status,
                    CompatibilityStatus::IncompatibleFailure
                )
            })
            .count();
        let unexpected_success = results
            .iter()
            .filter(|r| {
                matches!(
                    r.compatibility_status,
                    CompatibilityStatus::IncompatibleSuccess
                )
            })
            .count();
        let both_succeeded = results
            .iter()
            .filter(|r| matches!(r.compatibility_status, CompatibilityStatus::Identical))
            .count();
        let optimized_behavior = results
            .iter()
            .filter(|r| {
                matches!(
                    r.compatibility_status,
                    CompatibilityStatus::OptimizedBehavior
                )
            })
            .count();

        println!("Total Failure Tests: {}", total_tests);
        println!(
            "Both Implementations Rejected (Compatible): {} ({:.1}%)",
            both_rejected,
            (both_rejected as f64 / total_tests as f64) * 100.0
        );
        println!(
            "Failed with Different Errors: {} ({:.1}%)",
            incompatible_failures,
            (incompatible_failures as f64 / total_tests as f64) * 100.0
        );
        println!(
            "Optimized Behavior: {} ({:.1}%)",
            optimized_behavior,
            (optimized_behavior as f64 / total_tests as f64) * 100.0
        );
        println!(
            "Unexpected Success/Failure: {} ({:.1}%)",
            unexpected_success,
            (unexpected_success as f64 / total_tests as f64) * 100.0
        );
        println!(
            "Both Succeeded (Test Issue): {} ({:.1}%)",
            both_succeeded,
            (both_succeeded as f64 / total_tests as f64) * 100.0
        );

        if incompatible_failures > 0 || unexpected_success > 0 || optimized_behavior > 0 {
            println!("\n⚠️  TESTS WITH DIFFERENT BEHAVIORS:");
            for result in results
                .iter()
                .filter(|r| !matches!(r.compatibility_status, CompatibilityStatus::BothRejected))
            {
                match &result.compatibility_status {
                    CompatibilityStatus::IncompatibleFailure => {
                        println!("  {} - Different Error Messages:", result.test_name);
                        if result.p_ata.success {
                            println!("    P-ATA:     Success");
                        } else {
                            println!(
                                "    P-ATA:     {}",
                                result
                                    .p_ata
                                    .error_message
                                    .as_deref()
                                    .unwrap_or("Unknown error")
                            );
                        }
                        if result.original.success {
                            println!("    Original:  Success");
                        } else {
                            println!(
                                "    Original:  {}",
                                result
                                    .original
                                    .error_message
                                    .as_deref()
                                    .unwrap_or("Unknown error")
                            );
                        }
                    }
                    CompatibilityStatus::OptimizedBehavior => {
                        println!("  {} - Optimized Behavior:", result.test_name);
                        if result.p_ata.success {
                            println!("    P-ATA:     Success");
                        } else {
                            println!(
                                "    P-ATA:     {}",
                                result
                                    .p_ata
                                    .error_message
                                    .as_deref()
                                    .unwrap_or("Unknown error")
                            );
                        }
                        if result.original.success {
                            println!("    Original:  Success");
                        } else {
                            println!(
                                "    Original:  {}",
                                result
                                    .original
                                    .error_message
                                    .as_deref()
                                    .unwrap_or("Unknown error")
                            );
                        }
                    }
                    CompatibilityStatus::IncompatibleSuccess => {
                        println!("  {} - Incompatible Success/Failure:", result.test_name);
                        if result.p_ata.success {
                            println!("    P-ATA:     Success");
                        } else {
                            println!(
                                "    P-ATA:     {}",
                                result
                                    .p_ata
                                    .error_message
                                    .as_deref()
                                    .unwrap_or("Unknown error")
                            );
                        }
                        if result.original.success {
                            println!("    Original:  Success");
                        } else {
                            println!(
                                "    Original:  {}",
                                result
                                    .original
                                    .error_message
                                    .as_deref()
                                    .unwrap_or("Unknown error")
                            );
                        }
                    }
                    _ => {
                        println!("  {} - {:?}", result.test_name, result.compatibility_status);
                    }
                }
            }
        } else if both_rejected == total_tests {
            println!("\n✅ ALL FAILURE TESTS SHOW IDENTICAL ERRORS");
        }
    }
}

// ================================ MAIN FUNCTION ================================

fn main() {
    // Setup logging
    let _ = solana_logger::setup_with(
        "info,solana_runtime=info,solana_program_runtime=info,mollusk=debug",
    );

    // Get manifest directory and setup environment
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    println!("CARGO_MANIFEST_DIR: {}", manifest_dir);
    println!("🔨 P-ATA vs Original ATA Failure Scenarios Test Suite");

    BenchmarkSetup::setup_sbf_environment(manifest_dir);

    // Load program IDs
    let (standard_program_id, prefunded_program_id, original_ata_program_id, token_program_id) =
        BenchmarkSetup::load_all_program_ids(manifest_dir);

    let prefunded_program_id = prefunded_program_id.unwrap();
    let original_ata_program_id = original_ata_program_id.unwrap();

    // Create implementation structures
    let p_ata_impl = AtaImplementation::p_ata_prefunded(prefunded_program_id);

    println!("P-ATA Program ID: {}", standard_program_id);
    println!("Prefunded Program ID: {}", prefunded_program_id);
    println!("Original ATA Program ID: {}", original_ata_program_id);
    println!("Token Program ID: {}", token_program_id);

    let original_impl = AtaImplementation::original(original_ata_program_id);
    println!("Original ATA Program ID: {}", original_ata_program_id);

    println!("\n🔍 Running comprehensive failure comparison between implementations");

    // Validate both setups work
    let p_ata_mollusk =
        ComparisonRunner::create_mollusk_for_all_ata_implementations(&token_program_id);
    let original_mollusk =
        ComparisonRunner::create_mollusk_for_all_ata_implementations(&token_program_id);

    if let Err(e) =
        BenchmarkSetup::validate_setup(&p_ata_mollusk, &p_ata_impl.program_id, &token_program_id)
    {
        panic!("P-ATA failure test setup validation failed: {}", e);
    }

    if let Err(e) = BenchmarkSetup::validate_setup(
        &original_mollusk,
        &original_impl.program_id,
        &token_program_id,
    ) {
        panic!("Original ATA failure test setup validation failed: {}", e);
    }

    // Run comprehensive failure comparison
    let comparison_results = FailureTestRunner::run_comprehensive_failure_comparison(
        &p_ata_impl,
        &original_impl,
        &token_program_id,
    );

    // Print summary
    FailureTestRunner::print_failure_summary(&comparison_results);

    // Check for critical issues that indicate security problems or test failures
    let unexpected_success = comparison_results
        .iter()
        .filter(|r| {
            matches!(
                r.compatibility_status,
                CompatibilityStatus::IncompatibleSuccess
            )
        })
        .count();
    let both_succeeded = comparison_results
        .iter()
        .filter(|r| {
            matches!(r.compatibility_status, CompatibilityStatus::Identical)
                && r.p_ata.success
                && r.original.success
        })
        .count();

    if unexpected_success == 0 && both_succeeded == 0 {
        println!(
            "\n✅ Failure comparison completed successfully - No critical security issues detected"
        );
    } else {
        println!("\n🚨 FAILURE COMPARISON - ISSUES DETECTED");
        if unexpected_success > 0 {
            println!(
                "    {} SECURITY VULNERABILITIES: P-ATA succeeded where original correctly failed",
                unexpected_success
            );
        }
        if both_succeeded > 0 {
            println!(
                "    {} TEST ISSUES: Both implementations succeeded when they should have failed",
                both_succeeded
            );
        }
    }
}
