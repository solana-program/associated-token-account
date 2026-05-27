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
        ExtensionType::PermissionedBurn,
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

/// Recognized mint extensions exercised by the correctness proptest below.
/// Each entry is one the program's match table either sizes locally (the four
/// size-affecting types and the mint-only types that contribute zero), or
/// silently bails to CPI for. The correctness property is invariant to which
/// path the program chooses — only the resulting size must be right.
const RECOGNIZED_MINT_EXTENSIONS: &[ExtensionType] = &[
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
    ExtensionType::PermissionedBurn,
];

/// Locks in the multi-extension fast-path performance claim with a single
/// deterministic case. The proptest below covers correctness; this test
/// covers "we still chose the local path for a known-extension subset" so a
/// future refactor that silently regresses to CPI is caught here.
#[test]
fn token_2022_create_uses_fast_path_for_multi_extension_known_mint() {
    // Mix of size-affecting and mint-only extensions across both arms of
    // `compute_account_size_from_mint`'s match.
    let mint_extensions = [
        ExtensionType::TransferFeeConfig,
        ExtensionType::Pausable,
        ExtensionType::MetadataPointer,
        ExtensionType::ConfidentialTransferMint,
    ];
    let mut harness = token_2022_raw_mint_harness(&mint_extensions);
    let account_len = token_2022_required_account_len_for_mint_extensions(&mint_extensions);
    let instruction = harness.build_create_ata_instruction(CreateAtaInstructionType::Create);
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

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config {
        // Pure size-correctness over a 2^18 input space; with the linear-sum
        // shape of `compute_account_size_from_mint`, a few hundred random
        // subsets exercise every match arm many times. Lower than 2000 to
        // keep the suite fast — additional cases yield diminishing returns.
        cases: 256,
        ..proptest::test_runner::Config::default()
    })]

    /// Correctness invariant: for any non-empty subset of recognized mint
    /// extensions, the ATA's resulting account space must equal the canonical
    /// `try_calculate_account_len::<Account>` computation. The program is
    /// free to choose fast path or CPI fallback — only the result is
    /// asserted. Catches "match arm pointed at wrong delta constant" and
    /// "extension misplaced into wrong arm" without coupling to the
    /// implementation's choice of path.
    #[test]
    fn ata_account_size_matches_canonical_for_any_known_extension_subset(
        mint_extensions in proptest::sample::subsequence(
            RECOGNIZED_MINT_EXTENSIONS.to_vec(),
            1..=RECOGNIZED_MINT_EXTENSIONS.len(),
        ),
    ) {
        let mut harness = token_2022_raw_mint_harness(&mint_extensions);
        let account_len = token_2022_required_account_len_for_mint_extensions(&mint_extensions);
        let instruction = harness.build_create_ata_instruction(CreateAtaInstructionType::Create);
        let ata_address = harness.ata_address.unwrap();

        harness.ctx.process_and_validate_instruction(
            &instruction,
            &[
                Check::success(),
                Check::account(&ata_address)
                    .space(account_len)
                    .owner(&spl_token_2022_interface::id())
                    .lamports(Rent::default().minimum_balance(account_len))
                    .build(),
            ],
        );
    }
}

#[test]
fn token_2022_create_falls_back_to_cpi_for_unmapped_extension_variant() {
    // Exercises the wildcard arm of pinocchio's `required_account_extensions_for_mint_extension()`
    // which returns a `None` and bails to the CPI call
    let tlv_header_len: usize = 4;
    let mint_space = Account::LEN
        .checked_add(size_of::<u8>())
        .and_then(|len| len.checked_add(tlv_header_len))
        .unwrap();
    let mut mint_data = vec![0u8; mint_space];
    {
        let mut state =
            StateWithExtensionsMut::<Mint>::unpack_uninitialized(&mut mint_data).unwrap();
        state.base = Mint {
            mint_authority: COption::Some(Address::new_unique()),
            supply: 1_000_000,
            decimals: 6,
            is_initialized: true,
            freeze_authority: COption::None,
        };
        state.pack_base();
        state.init_account_type().unwrap();
    }
    // Write a zero-length `TransferFeeAmount` TLV header (type=2, length=0).
    // Account-side discriminants on a mint trigger the upstream mapping's wildcard arm.
    let tlv_start = Account::LEN.checked_add(size_of::<u8>()).unwrap();
    let type_bytes = (ExtensionType::TransferFeeAmount as u16).to_le_bytes();
    mint_data[tlv_start..tlv_start.checked_add(2).unwrap()].copy_from_slice(&type_bytes);

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

#[test]
fn token_2022_create_falls_back_to_cpi_when_mint_overflows_extension_buffer() {
    // Exercises the buffer-overflow safety net: hand-craft a mint with 33 TLV
    // entries (one more than the program's 32-slot stack buffer). Pinocchio's
    // `get_extension_types` returns `AccountDataTooSmall` on the 33rd entry,
    // which our fast path maps to `Ok(None)` and the create flow falls back
    // to the runtime `GetAccountDataSize` CPI.
    //
    // We use zero-length `MintCloseAuthority` TLV headers as filler since the
    // parser walks any non-zero discriminant and doesn't validate against
    // duplicates. The runtime CPI dedupes when computing the account length,
    // so the resulting ATA is just base + `ImmutableOwner`.
    const NUM_TLVS: usize = 33;
    let tlv_header_len: usize = 4;
    let tlv_bytes = NUM_TLVS.checked_mul(tlv_header_len).unwrap();
    let mint_space = Account::LEN
        .checked_add(size_of::<u8>())
        .and_then(|len| len.checked_add(tlv_bytes))
        .unwrap();
    let mut mint_data = vec![0u8; mint_space];
    {
        let mut state =
            StateWithExtensionsMut::<Mint>::unpack_uninitialized(&mut mint_data).unwrap();
        state.base = Mint {
            mint_authority: COption::Some(Address::new_unique()),
            supply: 1_000_000,
            decimals: 6,
            is_initialized: true,
            freeze_authority: COption::None,
        };
        state.pack_base();
        state.init_account_type().unwrap();
    }
    let tlv_start = Account::LEN.checked_add(size_of::<u8>()).unwrap();
    let type_bytes = (ExtensionType::MintCloseAuthority as u16).to_le_bytes();
    for i in 0..NUM_TLVS {
        let offset = tlv_start
            .checked_add(i.checked_mul(tlv_header_len).unwrap())
            .unwrap();
        mint_data[offset..offset.checked_add(2).unwrap()].copy_from_slice(&type_bytes);
        // length bytes already zero from vec init
    }

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
