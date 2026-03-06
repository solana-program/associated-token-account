use pinocchio::error::ProgramError;
use pinocchio_associated_token_account_interface::error::AssociatedTokenAccountError;

/// Converts interface ATA errors into pinocchio program errors.
pub(crate) trait ToProgramError {
    fn to_program_err(self) -> ProgramError;
}

impl ToProgramError for AssociatedTokenAccountError {
    fn to_program_err(self) -> ProgramError {
        ProgramError::Custom(self as u32)
    }
}
