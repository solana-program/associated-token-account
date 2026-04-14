//! Instruction types for the Associated Token Account program.

#[cfg(feature = "codama")]
use codama_macros::CodamaInstructions;

/// Instructions supported by the `AssociatedTokenAccount` program
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "codama", derive(CodamaInstructions))]
pub enum AssociatedTokenAccountInstruction {
    /// Creates an associated token account for the given wallet address and
    /// token mint Returns an error if the account exists.
    ///
    ///   0. `[writeable,signer]` Funding account (must be a system account)
    ///   1. `[writeable]` Associated token account address to be created
    ///   2. `[]` Wallet address for the new associated token account
    ///   3. `[]` The token mint for the new associated token account
    ///   4. `[]` System program
    ///   5. `[]` SPL Token program
    ///   6. `[]` Optional rent sysvar
    #[cfg_attr(
        feature = "codama",
        codama(account(
            name = "funder",
            signer,
            writable,
            docs = "Funding account (must be a system account)"
        )),
        codama(account(
            name = "associated_token_account",
            writable,
            docs = "Associated token account address to be created"
        )),
        codama(account(name = "wallet", docs = "Wallet address for the new associated token account")),
        codama(account(name = "mint", docs = "The token mint for the new associated token account")),
        codama(account(
            name = "system_program",
            docs = "System program",
            default_value = program("system")
        )),
        codama(account(name = "token_program", docs = "SPL Token program")),
        codama(account(
            name = "rent_sysvar",
            optional,
            default_value = sysvar("rent"),
            docs = "Optional rent sysvar"
        ))
    )]
    Create,
    /// Creates an associated token account for the given wallet address and
    /// token mint, if it doesn't already exist.  Returns an error if the
    /// account exists, but with a different owner.
    ///
    ///   0. `[writeable,signer]` Funding account (must be a system account)
    ///   1. `[writeable]` Associated token account address to be created
    ///   2. `[]` Wallet address for the new associated token account
    ///   3. `[]` The token mint for the new associated token account
    ///   4. `[]` System program
    ///   5. `[]` SPL Token program
    ///   6. `[]` Optional rent sysvar
    #[cfg_attr(
        feature = "codama",
        codama(account(
            name = "funder",
            signer,
            writable,
            docs = "Funding account (must be a system account)"
        )),
        codama(account(
            name = "associated_token_account",
            writable,
            docs = "Associated token account address to be created"
        )),
        codama(account(name = "wallet", docs = "Wallet address for the new associated token account")),
        codama(account(name = "mint", docs = "The token mint for the new associated token account")),
        codama(account(
            name = "system_program",
            docs = "System program",
            default_value = program("system")
        )),
        codama(account(name = "token_program", docs = "SPL Token program")),
        codama(account(
            name = "rent_sysvar",
            optional,
            default_value = sysvar("rent"),
            docs = "Optional rent sysvar"
        ))
    )]
    CreateIdempotent,
    /// Transfers from and closes a nested associated token account: an
    /// associated token account owned by an associated token account.
    ///
    /// The tokens are moved from the nested associated token account to the
    /// wallet's associated token account, and the nested account lamports are
    /// moved to the wallet.
    ///
    /// Note: Nested token accounts are an anti-pattern, and almost always
    /// created unintentionally, so this instruction should only be used to
    /// recover from errors.
    ///
    ///   0. `[writeable]` Nested associated token account, must be owned by `3`
    ///   1. `[]` Token mint for the nested associated token account
    ///   2. `[writeable]` Wallet's associated token account
    ///   3. `[]` Owner associated token account address, must be owned by `5`
    ///   4. `[]` Token mint for the owner associated token account
    ///   5. `[writeable, signer]` Wallet address for the owner associated token
    ///      account
    ///   6. `[]` Token program for the owner mint
    ///   7. `[]` Optional token program for the nested mint, if different from
    ///      the owner mint's token program
    #[cfg_attr(
        feature = "codama",
        codama(account(
            name = "nested_associated_token_account",
            writable,
            docs = "Nested associated token account, must be owned by \
                    `owner_associated_token_account`"
        )),
        codama(account(
            name = "nested_mint",
            docs = "Token mint for the nested associated token account"
        )),
        codama(account(
            name = "destination_associated_token_account",
            writable,
            docs = "Wallet's associated token account"
        )),
        codama(account(
            name = "owner_associated_token_account",
            docs = "Owner associated token account address, must be owned by `wallet`"
        )),
        codama(account(
            name = "owner_mint",
            docs = "Token mint for the owner associated token account"
        )),
        codama(account(
            name = "wallet",
            signer,
            writable,
            docs = "Wallet address for the owner associated token account"
        )),
        codama(account(
            name = "owner_token_program",
            docs = "Token program for the owner mint"
        )),
        codama(account(
            name = "nested_token_program",
            optional,
            docs = "Optional token program for the nested mint, if different from the owner \
                    mint's token program"
        ))
    )]
    RecoverNested,
}

impl TryFrom<u8> for AssociatedTokenAccountInstruction {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Create),
            1 => Ok(Self::CreateIdempotent),
            2 => Ok(Self::RecoverNested),
            _ => Err(()),
        }
    }
}

impl From<AssociatedTokenAccountInstruction> for u8 {
    fn from(value: AssociatedTokenAccountInstruction) -> Self {
        value as u8
    }
}

#[cfg(test)]
mod tests {
    use super::AssociatedTokenAccountInstruction;

    #[test]
    fn discriminants_match_legacy_layout() {
        assert_eq!(u8::from(AssociatedTokenAccountInstruction::Create), 0);
        assert_eq!(
            u8::from(AssociatedTokenAccountInstruction::CreateIdempotent),
            1
        );
        assert_eq!(
            u8::from(AssociatedTokenAccountInstruction::RecoverNested),
            2
        );
    }
}
