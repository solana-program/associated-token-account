//! Instruction types for the Associated Token Account program.

#[cfg(feature = "codama")]
use codama_macros::{CodamaInstructions, CodamaType};
use {
    pinocchio::error::ProgramError,
    solana_nullable::{MaybeNull, Nullable},
    solana_zero_copy::unaligned::U32,
    wincode::{SchemaRead, SchemaWrite},
};

/// Instructions supported by the `AssociatedTokenAccount` program
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, SchemaRead, SchemaWrite)]
#[wincode(tag_encoding = "u8")]
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
        codama(account(name = "token_program", docs = "SPL Token program"))
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
        codama(account(name = "token_program", docs = "SPL Token program"))
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
    ///      account. An SPL Token / Token-2022 multisig wallet does not sign and
    ///      is instead authorized by the signer accounts in `8.`
    ///   6. `[]` Token program for the owner mint
    ///   7. `[]` Optional token program for the nested mint, if different from
    ///      the owner mint's token program. Required when the wallet is a
    ///      multisig, even if equal to `6.`
    ///   8. `..8+M` `[signer]` M multisig signer accounts that authorize the
    ///      wallet when it is a multisig account
    #[cfg_attr(
        feature = "codama",
        codama(optional_account_strategy = omitted),
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
            signer = "either",
            writable,
            docs = "Wallet address for the owner associated token account. An SPL Token / \
                    Token-2022 multisig wallet does not sign and is instead authorized by \
                    trailing multisig signer accounts"
        )),
        codama(account(
            name = "owner_token_program",
            docs = "Token program for the owner mint"
        )),
        codama(account(
            name = "nested_token_program",
            optional,
            docs = "Optional token program for the nested mint, if different from the owner \
                    mint's token program. Required when the wallet is a multisig, even if equal \
                    to the owner token program"
        ))
    )]
    RecoverNested,
    /// Creates an associated token account for the given wallet address and
    /// token mint. Accepts optional optimization arguments to lower CU usage.
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
        codama(optional_account_strategy = omitted),
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
    CreateWithArgs {
        /// Selects whether behaves like `Create` or `CreateIdempotent`.
        mode: CreateMode,
        /// The ATA PDA bump seed.
        #[cfg_attr(feature = "codama", codama(type = number(u8)))]
        bump: MaybeNull<BumpSeedHint>,
        /// The account data length for the new ATA.
        #[cfg_attr(feature = "codama", codama(type = number(u32)))]
        account_len: MaybeNull<AccountLenHint>,
    },
}

impl AssociatedTokenAccountInstruction {
    #[inline(always)]
    pub fn try_from_bytes(instruction_data: &[u8]) -> Result<Self, ProgramError> {
        match instruction_data {
            [] | [0] => Ok(Self::Create),
            [1] => Ok(Self::CreateIdempotent),
            [2] => Ok(Self::RecoverNested),
            [3, ..] => wincode::deserialize_exact(instruction_data)
                .map_err(|_| ProgramError::InvalidInstructionData),
            _ => Err(ProgramError::InvalidInstructionData),
        }
    }
}

/// Specify when to create the associated token account.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, SchemaRead, SchemaWrite)]
#[wincode(tag_encoding = "u8")]
#[cfg_attr(feature = "codama", derive(CodamaType))]
pub enum CreateMode {
    /// Always try to create the associated token account.
    Always = 0,
    /// Only try to create the associated token account if non-existent.
    Idempotent = 1,
}

/// The ATA PDA bump seed hint. `0` is reserved as the null value.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, SchemaRead, SchemaWrite)]
#[wincode(assert_zero_copy)]
pub struct BumpSeedHint(u8);

impl BumpSeedHint {
    /// Reserving `0` keeps the optional bump hint as a single zero-copy byte in the wire format,
    /// without an extra option tag. A PDA bump of `0` is valid but very unlikely. The trade-off
    /// is that it forfeits the bump-hint optimization for that rare case.
    pub const fn new(value: u8) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }
}

impl Nullable for BumpSeedHint {
    const NONE: Self = Self(0);
}

impl From<BumpSeedHint> for u8 {
    fn from(value: BumpSeedHint) -> Self {
        value.0
    }
}

/// The account data length hint for the new ATA. `0` is reserved as the null value.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, SchemaRead, SchemaWrite)]
#[wincode(assert_zero_copy)]
pub struct AccountLenHint(U32);

impl AccountLenHint {
    pub const fn new(value: u32) -> Option<Self> {
        if value == 0 {
            None
        } else {
            Some(Self(U32::from_primitive(value)))
        }
    }
}

impl Nullable for AccountLenHint {
    const NONE: Self = Self(U32::from_primitive(0));
}

impl From<AccountLenHint> for u32 {
    fn from(value: AccountLenHint) -> Self {
        value.0.into()
    }
}

#[cfg(test)]
mod tests {
    use {
        super::{AccountLenHint, AssociatedTokenAccountInstruction, BumpSeedHint, CreateMode},
        pinocchio::error::ProgramError,
        solana_nullable::{MaybeNull, Nullable},
        wincode::Serialize,
    };

    fn assert_wire<const N: usize>(
        instruction: AssociatedTokenAccountInstruction,
        expected: [u8; N],
    ) {
        let mut bytes = [0; N];
        AssociatedTokenAccountInstruction::serialize_into(bytes.as_mut_slice(), &instruction)
            .unwrap();
        assert_eq!(bytes, expected);
        assert_eq!(
            AssociatedTokenAccountInstruction::try_from_bytes(&expected).unwrap(),
            instruction
        );
        let decoded: AssociatedTokenAccountInstruction =
            wincode::deserialize_exact(&expected).unwrap();
        assert_eq!(decoded, instruction);
    }

    #[test]
    fn instruction_wire_format_is_stable() {
        assert_wire(AssociatedTokenAccountInstruction::Create, [0]);
        assert_wire(AssociatedTokenAccountInstruction::CreateIdempotent, [1]);
        assert_wire(AssociatedTokenAccountInstruction::RecoverNested, [2]);
        assert_wire(
            AssociatedTokenAccountInstruction::CreateWithArgs {
                mode: CreateMode::Always,
                bump: MaybeNull::from(BumpSeedHint::NONE),
                account_len: MaybeNull::from(AccountLenHint::NONE),
            },
            [3, 0, 0, 0, 0, 0, 0],
        );
        assert_wire(
            AssociatedTokenAccountInstruction::CreateWithArgs {
                mode: CreateMode::Idempotent,
                bump: BumpSeedHint::new(253).unwrap().into(),
                account_len: MaybeNull::from(AccountLenHint::NONE),
            },
            [3, 1, 253, 0, 0, 0, 0],
        );
        assert_wire(
            AssociatedTokenAccountInstruction::CreateWithArgs {
                mode: CreateMode::Always,
                bump: MaybeNull::from(BumpSeedHint::NONE),
                account_len: AccountLenHint::new(u32::from_le_bytes([1, 2, 3, 4]))
                    .unwrap()
                    .into(),
            },
            [3, 0, 0, 1, 2, 3, 4],
        );
        assert_wire(
            AssociatedTokenAccountInstruction::CreateWithArgs {
                mode: CreateMode::Idempotent,
                bump: BumpSeedHint::new(253).unwrap().into(),
                account_len: AccountLenHint::new(u32::from_le_bytes([1, 2, 3, 4]))
                    .unwrap()
                    .into(),
            },
            [3, 1, 253, 1, 2, 3, 4],
        );
    }

    #[test]
    fn empty_instruction_data_remains_create() {
        assert_eq!(
            AssociatedTokenAccountInstruction::try_from_bytes(&[]).unwrap(),
            AssociatedTokenAccountInstruction::Create
        );
    }

    #[test]
    fn instruction_parser_rejects_non_canonical_payloads() {
        let cases: &[&[u8]] = &[
            &[4],                      // unknown discriminator
            &[0, 0],                   // trailing byte after Create
            &[1, 9, 9],                // trailing bytes after CreateIdempotent
            &[2, 0],                   // trailing byte after RecoverNested
            &[3],                      // missing CreateWithArgs mode
            &[3, 2, 0, 0, 0, 0, 0],    // invalid CreateWithArgs mode
            &[3, 0],                   // missing bump hint
            &[3, 0, 0],                // missing account_len hint
            &[3, 0, 0, 0, 0, 0],       // truncated account_len hint
            &[3, 0, 0, 0, 0, 0, 0, 0], // trailing byte after CreateWithArgs
        ];

        for data in cases {
            assert_eq!(
                AssociatedTokenAccountInstruction::try_from_bytes(data),
                Err(ProgramError::InvalidInstructionData)
            );
        }
    }
}
