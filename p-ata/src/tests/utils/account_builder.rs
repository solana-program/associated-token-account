use {
    crate::tests::{
        benches::constants::*, create_mollusk_mint_data, create_mollusk_token_account_data,
    },
    mollusk_svm::Mollusk,
    solana_account::Account,
    solana_pubkey::Pubkey,
    solana_sysvar::rent,
    spl_token_2022::extension::ExtensionType,
    std::vec,
    std::vec::Vec,
};

#[cfg(feature = "full-debug-logs")]
use std::{println, string::String, string::ToString};

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

    pub fn system_account(lamports: u64) -> Account {
        Account::new(lamports, 0, &solana_system_interface::program::id())
    }

    pub fn executable_program(owner: Pubkey) -> Account {
        Account {
            lamports: 0,
            data: Vec::new(),
            owner,
            executable: true,
            rent_epoch: 0,
        }
    }

    pub fn token_account(
        mint: &Pubkey,
        owner: &Pubkey,
        amount: u64,
        token_program_id: &Pubkey,
    ) -> Account {
        Account {
            lamports: TOKEN_ACCOUNT_RENT_EXEMPT,
            data: {
                #[cfg(feature = "full-debug-logs")]
                println!(
                    "🔧 Creating token account data | Mint: {} | Owner: {}",
                    mint.to_string()[0..8].to_string(),
                    owner.to_string()[0..8].to_string()
                );

                create_mollusk_token_account_data(mint, owner, amount)
            },
            owner: *token_program_id,
            executable: false,
            rent_epoch: 0,
        }
    }

    pub fn mint(decimals: u8, token_program_id: &Pubkey) -> Account {
        Account {
            lamports: MINT_ACCOUNT_RENT_EXEMPT,
            data: create_mollusk_mint_data(decimals),
            owner: *token_program_id,
            executable: false,
            rent_epoch: 0,
        }
    }

    pub fn extended_mint(decimals: u8, token_program_id: &Pubkey) -> Account {
        use solana_program_option::COption;
        use spl_token_2022::{
            extension::{
                default_account_state::DefaultAccountState, metadata_pointer::MetadataPointer,
                non_transferable::NonTransferable, transfer_fee::TransferFeeConfig,
                transfer_hook::TransferHook, BaseStateWithExtensionsMut, PodStateWithExtensionsMut,
            },
            pod::PodMint,
            state::AccountState,
        };

        // Use extensions that are supported by our inline helper
        let extension_types = vec![
            ExtensionType::TransferFeeConfig, // Adds TransferFeeAmount to account
            ExtensionType::NonTransferable,   // Adds NonTransferableAccount to account
            ExtensionType::TransferHook,      // Adds TransferHookAccount to account
            ExtensionType::DefaultAccountState, // Mint-only extension
            ExtensionType::MetadataPointer,   // Mint-only extension
        ];

        let required_size =
            ExtensionType::try_calculate_account_len::<spl_token_2022::state::Mint>(
                &extension_types,
            )
            .expect("Failed to calculate account length");

        let mut data = vec![0u8; required_size];

        let mut mint = PodStateWithExtensionsMut::<PodMint>::unpack_uninitialized(&mut data)
            .expect("Failed to unpack mint");

        // Initialize base mint fields
        mint.base.mint_authority = COption::None.try_into().unwrap();
        mint.base.supply = 0u64.into();
        mint.base.decimals = decimals;
        mint.base.is_initialized = true.into();
        mint.base.freeze_authority = COption::None.try_into().unwrap();

        // Initialize TransferFeeConfig extension
        let transfer_fee_config = mint
            .init_extension::<TransferFeeConfig>(true)
            .expect("Failed to init TransferFeeConfig");
        transfer_fee_config.transfer_fee_config_authority = COption::None.try_into().unwrap();
        transfer_fee_config.withdraw_withheld_authority = COption::None.try_into().unwrap();
        transfer_fee_config.withheld_amount = 0u64.into();

        // Initialize NonTransferable extension
        let _non_transferable = mint
            .init_extension::<NonTransferable>(true)
            .expect("Failed to init NonTransferable");

        // Initialize TransferHook extension
        let transfer_hook = mint
            .init_extension::<TransferHook>(true)
            .expect("Failed to init TransferHook");
        transfer_hook.authority = COption::None.try_into().unwrap();
        transfer_hook.program_id = COption::None.try_into().unwrap();

        // Initialize DefaultAccountState extension
        let default_account_state = mint
            .init_extension::<DefaultAccountState>(true)
            .expect("Failed to init DefaultAccountState");
        default_account_state.state = AccountState::Initialized.into();

        // Initialize MetadataPointer extension
        let metadata_pointer = mint
            .init_extension::<MetadataPointer>(true)
            .expect("Failed to init MetadataPointer");
        metadata_pointer.authority = COption::None.try_into().unwrap();
        metadata_pointer.metadata_address = COption::None.try_into().unwrap();

        // Initialize the account type to mark as a proper mint
        mint.init_account_type()
            .expect("Failed to init account type");

        Account {
            lamports: EXTENDED_MINT_ACCOUNT_RENT_EXEMPT, // Use extended mint rent amount
            data,
            owner: *token_program_id,
            executable: false,
            rent_epoch: 0,
        }
    }
}
