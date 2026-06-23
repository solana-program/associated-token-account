use {
    core::mem::size_of,
    mollusk_svm_result::Check,
    solana_address::Address,
    solana_program_option::COption,
    solana_program_pack::Pack,
    solana_rent::Rent,
    spl_associated_token_account_mollusk_harness::{
        AtaProgram, AtaTestHarness, CreateAtaInstructionType,
        token_2022_immutable_owner_account_len,
    },
    spl_token_2022_interface::{
        extension::{
            BaseStateWithExtensionsMut, ExtensionType, StateWithExtensionsMut,
            account_len::try_calculate_account_len_from_mint_data,
            mint_close_authority::MintCloseAuthority, non_transferable::NonTransferable,
            pausable::PausableConfig, transfer_fee::TransferFeeConfig, transfer_hook::TransferHook,
        },
        state::{Account as Token2022Account, Mint},
    },
    test_case::test_case,
};

const CREATE_FAST_PATH_INNER_IX_COUNT: usize = 2; // `CreateAccountAllowPrefund` and Batch
const FAILED_SIZE_CPI_FALLBACK_INNER_IX_COUNT: usize = 1; // `GetAccountDataSize`

fn token_2022_raw_mint_harness(mint_extensions: &[ExtensionType]) -> (AtaTestHarness, usize) {
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

    let account_len =
        try_calculate_account_len_from_mint_data(&mint_data, &[ExtensionType::ImmutableOwner])
            .unwrap();

    (
        AtaTestHarness::new_with_ata_program(
            &spl_token_2022_interface::id(),
            AtaProgram::Pinocchio,
        )
        .with_wallet(1_000_000)
        .with_raw_mint(
            spl_token_2022_interface::id(),
            Rent::default().minimum_balance(mint_space),
            mint_data,
        ),
        account_len,
    )
}

fn assert_create_uses_fast_path(
    mut harness: AtaTestHarness,
    instruction_type: CreateAtaInstructionType,
    account_len: usize,
) {
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

#[test_case(CreateAtaInstructionType::Create)]
#[test_case(CreateAtaInstructionType::CreateIdempotent)]
fn base_mint_uses_fast_path(instruction_type: CreateAtaInstructionType) {
    let harness = AtaTestHarness::new_with_ata_program(
        &spl_token_2022_interface::id(),
        AtaProgram::Pinocchio,
    )
    .with_wallet_and_mint(1_000_000, 6);
    assert_create_uses_fast_path(
        harness,
        instruction_type,
        token_2022_immutable_owner_account_len(),
    );
}

#[test_case(&[ExtensionType::MintCloseAuthority]; "without account-side extension, stays at base size")]
#[test_case(&[ExtensionType::TransferFeeConfig]; "with account-side extension, grows beyond base size")]
#[test_case(&[
    ExtensionType::TransferFeeConfig,
    ExtensionType::NonTransferable,
    ExtensionType::TransferHook,
    ExtensionType::Pausable,
]; "multiple extensions")]
fn mint_with_extensions_uses_fast_path(mint_extensions: &[ExtensionType]) {
    let (harness, account_len) = token_2022_raw_mint_harness(mint_extensions);
    assert_create_uses_fast_path(harness, CreateAtaInstructionType::Create, account_len);
}

#[test]
fn invalid_mint_extension_data_falls_back_to_cpi() {
    let mint_space =
        ExtensionType::try_calculate_account_len::<Mint>(&[ExtensionType::MintCloseAuthority])
            .unwrap();
    let mut mint_data = vec![0u8; mint_space];
    let mut state = StateWithExtensionsMut::<Mint>::unpack_uninitialized(&mut mint_data).unwrap();
    state.init_extension::<MintCloseAuthority>(true).unwrap();
    state.base = Mint {
        mint_authority: COption::Some(Address::new_unique()),
        supply: 1_000_000,
        decimals: 6,
        is_initialized: true,
        freeze_authority: COption::None,
    };
    state.pack_base();
    state.init_account_type().unwrap();

    // Corrupt the first extension's type discriminant so local parsing fails
    let extension_type_offset = Token2022Account::LEN.checked_add(size_of::<u8>()).unwrap();
    mint_data[extension_type_offset..extension_type_offset + size_of::<u16>()]
        .copy_from_slice(&u16::MAX.to_le_bytes());

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
    let instruction = harness.build_create_ata_instruction(CreateAtaInstructionType::Create);
    let result = harness.ctx.process_instruction(&instruction);

    assert!(result.raw_result.is_err());
    assert_eq!(
        result.inner_instructions.len(),
        FAILED_SIZE_CPI_FALLBACK_INNER_IX_COUNT
    );
}
