//! Error types

use {
    core::{error::Error, fmt},
    num_derive::FromPrimitive,
    solana_program_error::ProgramError,
};

/// Errors that may be returned by the program.
#[derive(Clone, Debug, Eq, FromPrimitive, PartialEq)]
pub enum AssociatedTokenAccountError {
    // 0
    /// Associated token account owner does not match address derivation
    InvalidOwner,
}

impl Error for AssociatedTokenAccountError {}

impl fmt::Display for AssociatedTokenAccountError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AssociatedTokenAccountError::InvalidOwner => {
                f.write_str("Associated token account owner does not match address derivation")
            }
        }
    }
}

impl From<AssociatedTokenAccountError> for ProgramError {
    fn from(e: AssociatedTokenAccountError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
