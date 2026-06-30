//! Address derivation helpers for Associated Token Account program-derived addresses.

use pinocchio::{Address, error::ProgramError};

#[cfg_attr(feature = "codama", derive(codama::CodamaPda))]
#[cfg_attr(
    feature = "codama",
    codama(seed(name = "wallet", type = public_key))
)]
#[cfg_attr(
    feature = "codama",
    codama(seed(name = "token_program", type = public_key))
)]
#[cfg_attr(
    feature = "codama",
    codama(seed(name = "mint", type = public_key))
)]
pub struct AssociatedTokenPda;

impl AssociatedTokenPda {
    /// Derives the associated token account address and bump seed
    /// for the given wallet address, token mint and token program id.
    pub fn derive_address_and_bump_seed(
        program_id: &Address,
        wallet_address: &Address,
        token_program_id: &Address,
        token_mint_address: &Address,
    ) -> (Address, u8) {
        Address::derive_program_address(
            &[
                wallet_address.as_ref(),
                token_program_id.as_ref(),
                token_mint_address.as_ref(),
            ],
            program_id,
        )
        .expect("Unable to find a viable program address bump seed")
    }

    /// Derives the associated token account address for the given wallet
    /// address, token mint and token program id.
    #[inline(always)]
    pub fn derive_address(
        program_id: &Address,
        wallet_address: &Address,
        token_program_id: &Address,
        token_mint_address: &Address,
    ) -> Address {
        Self::derive_address_and_bump_seed(
            program_id,
            wallet_address,
            token_program_id,
            token_mint_address,
        )
        .0
    }

    /// Derives the associated token account address for a caller-supplied bump.
    ///
    /// This rejects non-canonical bumps by verifying that no higher bump produces an
    /// off-curve PDA. Note: it does not verify that the supplied bump itself is off-curve.
    /// Callers must either rely on a subsequent signed PDA invocation to reject an
    /// on-curve address or manually check `is_on_curve()` before accepting the derived
    /// address.
    pub fn derive_address_with_bump_hint(
        program_id: &Address,
        wallet_address: &Address,
        token_program_id: &Address,
        token_mint_address: &Address,
        bump: u8,
    ) -> Result<Address, ProgramError> {
        let seeds = [
            wallet_address.as_ref(),
            token_program_id.as_ref(),
            token_mint_address.as_ref(),
        ];

        if bump < u8::MAX {
            #[allow(clippy::arithmetic_side_effects)]
            for higher_bump in (bump + 1)..=u8::MAX {
                let higher_bump_addr =
                    Address::derive_address(&seeds, Some(higher_bump), program_id);
                if !higher_bump_addr.is_on_curve() {
                    return Err(invalid_seeds());
                }
            }
        }

        Ok(Address::derive_address(&seeds, Some(bump), program_id))
    }
}

#[cold]
fn invalid_seeds() -> ProgramError {
    ProgramError::InvalidSeeds
}
