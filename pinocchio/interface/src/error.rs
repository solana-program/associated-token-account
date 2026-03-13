//! Error types for the Associated Token Account program interface.

#[cfg(feature = "codama")]
use codama_macros::CodamaErrors;

/// Errors that may be returned by the associated token account program.
#[cfg_attr(feature = "codama", derive(CodamaErrors))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum AssociatedTokenAccountError {
    /// Associated token account owner does not match address derivation.
    #[cfg_attr(
        feature = "codama",
        codama(error(
            message = "Associated token account owner does not match address derivation"
        ))
    )]
    InvalidOwner,
}

impl From<AssociatedTokenAccountError> for pinocchio::error::ProgramError {
    fn from(e: AssociatedTokenAccountError) -> Self {
        pinocchio::error::ProgramError::Custom(e as u32)
    }
}
