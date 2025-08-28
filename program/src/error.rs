//! Error types

#[allow(deprecated)]
use solana_decode_error::DecodeError;
use {num_derive::FromPrimitive, solana_program::program_error::ProgramError, thiserror::Error};

/// Errors that may be returned by the program.
#[derive(Clone, Debug, Eq, Error, FromPrimitive, PartialEq)]
pub enum AssociatedTokenAccountError {
    // 0
    /// Associated token account owner does not match address derivation
    #[error("Associated token account owner does not match address derivation")]
    InvalidOwner,
}
impl From<AssociatedTokenAccountError> for ProgramError {
    fn from(e: AssociatedTokenAccountError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
#[allow(deprecated)]
impl<T> DecodeError<T> for AssociatedTokenAccountError {
    fn type_of() -> &'static str {
        "AssociatedTokenAccountError"
    }
}
