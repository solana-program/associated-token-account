use {
    core::mem::size_of,
    mollusk_svm_result::Check,
    solana_address::Address,
    solana_program_option::COption,
    solana_program_pack::Pack,
    solana_rent::Rent,
    spl_associated_token_account_mollusk_harness::{
        AtaProgram, AtaTestHarness, CreateAtaInstructionType,
    },
    spl_token_2022_interface::{
        extension::{
            BaseStateWithExtensionsMut, ExtensionType, StateWithExtensionsMut,
            confidential_mint_burn::ConfidentialMintBurn,
            confidential_transfer::ConfidentialTransferMint,
            confidential_transfer_fee::ConfidentialTransferFeeConfig,
            default_account_state::DefaultAccountState, group_member_pointer::GroupMemberPointer,
            group_pointer::GroupPointer, interest_bearing_mint::InterestBearingConfig,
            metadata_pointer::MetadataPointer, mint_close_authority::MintCloseAuthority,
            non_transferable::NonTransferable, pausable::PausableConfig,
            permanent_delegate::PermanentDelegate, permissioned_burn::PermissionedBurnConfig,
            scaled_ui_amount::ScaledUiAmountConfig, transfer_fee::TransferFeeConfig,
            transfer_hook::TransferHook,
        },
        state::{Account, Mint},
    },
    spl_token_group_interface::state::{TokenGroup, TokenGroupMember},
    test_case::test_matrix,
};

fn token_2022_raw_mint_harness(mint_extensions: &[ExtensionType]) -> AtaTestHarness {
    let mint_space = ExtensionType::try_calculate_account_len::<Mint>(mint_extensions).unwrap();
    let mut mint_data = vec![0; mint_space];
    let mut state = StateWithExtensionsMut::<Mint>::unpack_uninitialized(&mut mint_data).unwrap();

    for extension_type in mint_extensions {
        match extension_type {
            ExtensionType::TransferFeeConfig => {
                state.init_extension::<TransferFeeConfig>(true).unwrap();
            }
            ExtensionType::NonTransferable => {
                state.init_extension::<NonTransferable>(true).unwrap();
            }
            ExtensionType::TransferHook => {
                state.init_extension::<TransferHook>(true).unwrap();
            }
            ExtensionType::Pausable => {
                state.init_extension::<PausableConfig>(true).unwrap();
            }
            ExtensionType::MintCloseAuthority => {
                state.init_extension::<MintCloseAuthority>(true).unwrap();
            }
            ExtensionType::ConfidentialTransferMint => {
                state
                    .init_extension::<ConfidentialTransferMint>(true)
                    .unwrap();
            }
            ExtensionType::DefaultAccountState => {
                state.init_extension::<DefaultAccountState>(true).unwrap();
            }
            ExtensionType::InterestBearingConfig => {
                state.init_extension::<InterestBearingConfig>(true).unwrap();
            }
            ExtensionType::PermanentDelegate => {
                state.init_extension::<PermanentDelegate>(true).unwrap();
            }
            ExtensionType::ConfidentialTransferFeeConfig => {
                state
                    .init_extension::<ConfidentialTransferFeeConfig>(true)
                    .unwrap();
            }
            ExtensionType::MetadataPointer => {
                state.init_extension::<MetadataPointer>(true).unwrap();
            }
            ExtensionType::GroupPointer => {
                state.init_extension::<GroupPointer>(true).unwrap();
            }
            ExtensionType::GroupMemberPointer => {
                state.init_extension::<GroupMemberPointer>(true).unwrap();
            }
            ExtensionType::TokenGroup => {
                state.init_extension::<TokenGroup>(true).unwrap();
            }
            ExtensionType::TokenGroupMember => {
                state.init_extension::<TokenGroupMember>(true).unwrap();
            }
            ExtensionType::ConfidentialMintBurn => {
                state.init_extension::<ConfidentialMintBurn>(true).unwrap();
            }
            ExtensionType::ScaledUiAmount => {
                state.init_extension::<ScaledUiAmountConfig>(true).unwrap();
            }
            ExtensionType::PermissionedBurn => {
                state
                    .init_extension::<PermissionedBurnConfig>(true)
                    .unwrap();
            }
            _ => panic!("unsupported raw mint extension for this test"),
        }
    }

    state.base = Mint {
        mint_authority: COption::Some(Address::new_unique()),
        supply: 1_000_000,
        decimals: 6,
        is_initialized: true,
        freeze_authority: COption::None,
    };
    state.pack_base();
    state.init_account_type().unwrap();

    AtaTestHarness::new_with_ata_program(&spl_token_2022_interface::id(), AtaProgram::Pinocchio)
        .with_wallet(1_000_000)
        .with_raw_mint(
            spl_token_2022_interface::id(),
            Rent::default().minimum_balance(mint_space),
            mint_data,
        )
}

fn token_2022_required_account_len_for_mint_extensions(mint_extensions: &[ExtensionType]) -> usize {
    let mut account_extensions = vec![ExtensionType::ImmutableOwner];
    account_extensions.extend(ExtensionType::get_required_init_account_extensions(
        mint_extensions,
    ));

    ExtensionType::try_calculate_account_len::<Account>(&account_extensions).unwrap()
}

const CREATE_FAST_PATH_INNER_IX_COUNT: usize = 2; // `CreateAccountAllowPrefund` & Batch
const CREATE_CPI_FALLBACK_INNER_IX_COUNT: usize = 3; // above plus `GetAccountDataSize`

#[test_matrix(
    [CreateAtaInstructionType::Create, CreateAtaInstructionType::CreateIdempotent],
    [
        ExtensionType::TransferFeeConfig,
        ExtensionType::NonTransferable,
        ExtensionType::TransferHook,
        ExtensionType::Pausable,
        ExtensionType::MintCloseAuthority,
        ExtensionType::ConfidentialTransferMint,
        ExtensionType::DefaultAccountState,
        ExtensionType::InterestBearingConfig,
        ExtensionType::PermanentDelegate,
        ExtensionType::ConfidentialTransferFeeConfig,
        ExtensionType::MetadataPointer,
        ExtensionType::GroupPointer,
        ExtensionType::TokenGroup,
        ExtensionType::GroupMemberPointer,
        ExtensionType::TokenGroupMember,
        ExtensionType::ConfidentialMintBurn,
        ExtensionType::ScaledUiAmount,
    ]
)]
fn token_2022_create_uses_known_lens_short_circuit_for_single_mint_extension(
    instruction_type: CreateAtaInstructionType,
    mint_extension: ExtensionType,
) {
    let mut harness = token_2022_raw_mint_harness(&[mint_extension]);
    let account_len = token_2022_required_account_len_for_mint_extensions(&[mint_extension]);
    let instruction = harness.build_create_ata_instruction(instruction_type);
    let ata_address = harness.ata_address.unwrap();

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[
            Check::success(),
            Check::inner_instruction_count(CREATE_FAST_PATH_INNER_IX_COUNT),
            Check::account(&ata_address)
                .space(account_len)
                .owner(&spl_token_2022_interface::id())
                .lamports(Rent::default().minimum_balance(account_len))
                .build(),
        ],
    );
}

#[test]
fn token_2022_create_falls_back_to_cpi_for_multiple_mint_extensions() {
    let mint_extensions = [
        ExtensionType::TransferFeeConfig,
        ExtensionType::NonTransferable,
        ExtensionType::TransferHook,
        ExtensionType::Pausable,
    ];
    let mut harness = token_2022_raw_mint_harness(&mint_extensions);
    let account_len = token_2022_required_account_len_for_mint_extensions(&mint_extensions);
    let instruction = harness.build_create_ata_instruction(CreateAtaInstructionType::Create);
    let ata_address = harness.ata_address.unwrap();

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[
            Check::success(),
            Check::inner_instruction_count(CREATE_CPI_FALLBACK_INNER_IX_COUNT),
            Check::account(&ata_address)
                .space(account_len)
                .owner(&spl_token_2022_interface::id())
                .lamports(Rent::default().minimum_balance(account_len))
                .build(),
        ],
    );
}

#[test]
fn token_2022_create_falls_back_to_cpi_for_unmapped_extension_variant() {
    // Exercises the `_ => Ok(None)` wildcard arm
    let mint_space = Account::LEN
        .checked_add(size_of::<u8>())
        .and_then(|len| len.checked_add(4))
        .unwrap();
    let mut mint_data = vec![0u8; mint_space];
    let mut state = StateWithExtensionsMut::<Mint>::unpack_uninitialized(&mut mint_data).unwrap();
    state.base = Mint {
        mint_authority: COption::Some(Address::new_unique()),
        supply: 1_000_000,
        decimals: 6,
        is_initialized: true,
        freeze_authority: COption::None,
    };
    state.pack_base();
    state.init_account_type().unwrap();

    let mut harness = AtaTestHarness::new_with_ata_program(
        &spl_token_2022_interface::id(),
        AtaProgram::Pinocchio,
    )
    .with_wallet(1_000_000)
    .with_raw_mint(
        spl_token_2022_interface::id(),
        Rent::default().minimum_balance(mint_space),
        mint_data,
    );
    let account_len =
        ExtensionType::try_calculate_account_len::<Account>(&[ExtensionType::ImmutableOwner])
            .unwrap();
    let instruction = harness.build_create_ata_instruction(CreateAtaInstructionType::Create);
    let ata_address = harness.ata_address.unwrap();

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[
            Check::success(),
            Check::inner_instruction_count(CREATE_CPI_FALLBACK_INNER_IX_COUNT),
            Check::account(&ata_address)
                .space(account_len)
                .owner(&spl_token_2022_interface::id())
                .lamports(Rent::default().minimum_balance(account_len))
                .build(),
        ],
    );
}
