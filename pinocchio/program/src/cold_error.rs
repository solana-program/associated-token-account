//! Error constructors marked `#[cold]` to keep rare failure paths off the hot
//! success path, plus `#[inline(never)]` on the logging ones so their `sol_log_`
//! setup isn't inlined back into the caller.

use {
    pinocchio::error::ProgramError,
    pinocchio_associated_token_account_interface::error::AssociatedTokenAccountError,
    pinocchio_log::log,
};

#[cold]
pub(crate) fn invalid_account_data() -> ProgramError {
    ProgramError::InvalidAccountData
}

#[cold]
pub(crate) fn incorrect_program_id() -> ProgramError {
    ProgramError::IncorrectProgramId
}

#[cold]
pub(crate) fn invalid_owner() -> ProgramError {
    ProgramError::Custom(AssociatedTokenAccountError::InvalidOwner as u32)
}

#[cold]
pub(crate) fn invalid_seeds() -> ProgramError {
    ProgramError::InvalidSeeds
}

#[cold]
pub(crate) fn illegal_owner() -> ProgramError {
    ProgramError::IllegalOwner
}

#[cold]
pub(crate) fn missing_required_signature() -> ProgramError {
    ProgramError::MissingRequiredSignature
}

#[cold]
pub(crate) fn not_enough_account_keys() -> ProgramError {
    ProgramError::NotEnoughAccountKeys
}

#[cold]
pub(crate) fn uninitialized_account() -> ProgramError {
    ProgramError::UninitializedAccount
}

#[cold]
#[inline(never)]
pub(crate) fn owner_associated_address_mismatch() -> ProgramError {
    log!("Error: Owner associated address does not match seed derivation");
    ProgramError::InvalidSeeds
}

#[cold]
#[inline(never)]
pub(crate) fn nested_associated_address_mismatch() -> ProgramError {
    log!("Error: Nested associated address does not match seed derivation");
    ProgramError::InvalidSeeds
}

#[cold]
#[inline(never)]
pub(crate) fn destination_associated_address_mismatch() -> ProgramError {
    log!("Error: Destination associated address does not match seed derivation");
    ProgramError::InvalidSeeds
}

#[cold]
#[inline(never)]
pub(crate) fn wallet_missing_required_signature() -> ProgramError {
    log!("Wallet of the owner associated token account must sign");
    ProgramError::MissingRequiredSignature
}

#[cold]
#[inline(never)]
pub(crate) fn owner_mint_illegal_owner() -> ProgramError {
    log!("Owner mint not owned by provided token program");
    ProgramError::IllegalOwner
}

#[cold]
#[inline(never)]
pub(crate) fn owner_ata_illegal_owner() -> ProgramError {
    log!(
        "Owner associated token account not owned by provided token program, recreate the owner \
         associated token account first"
    );
    ProgramError::IllegalOwner
}

#[cold]
#[inline(never)]
pub(crate) fn owner_ata_invalid_owner() -> ProgramError {
    log!("Owner associated token account not owned by provided wallet");
    ProgramError::Custom(AssociatedTokenAccountError::InvalidOwner as u32)
}

#[cold]
#[inline(never)]
pub(crate) fn nested_ata_illegal_owner() -> ProgramError {
    log!("Nested associated token account not owned by provided token program");
    ProgramError::IllegalOwner
}

#[cold]
#[inline(never)]
pub(crate) fn nested_ata_invalid_owner() -> ProgramError {
    log!("Nested associated token account not owned by provided associated token account");
    ProgramError::Custom(AssociatedTokenAccountError::InvalidOwner as u32)
}

#[cold]
#[inline(never)]
pub(crate) fn nested_mint_illegal_owner() -> ProgramError {
    log!("Nested mint account not owned by provided token program");
    ProgramError::IllegalOwner
}

#[cold]
#[inline(never)]
pub(crate) fn not_enough_multisig_signers() -> ProgramError {
    log!("Not enough multisig signers for wallet");
    ProgramError::MissingRequiredSignature
}

#[cold]
#[inline(never)]
pub(crate) fn no_account_size_return_data() -> ProgramError {
    log!("Error: token program returned no account size data");
    ProgramError::InvalidInstructionData
}

#[cold]
#[inline(never)]
pub(crate) fn unexpected_return_data_program() -> ProgramError {
    log!("Error: return data came from unexpected program");
    ProgramError::IncorrectProgramId
}

#[cold]
#[inline(never)]
pub(crate) fn invalid_account_size_return_data_len(len: usize) -> ProgramError {
    log!("Error: invalid account size return data length: {}", len);
    ProgramError::InvalidInstructionData
}
