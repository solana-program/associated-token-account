//! Address derivation helpers for Associated Token Account program-derived addresses.

use {pinocchio::cpi::Seed, solana_address::Address};

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
    pub fn get_address_and_bump_seed(
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

    /// Returns the PDA signer seeds for `invoke_signed`.
    pub fn signer_seeds<'a>(
        wallet_address: &'a Address,
        token_program_id: &'a Address,
        token_mint_address: &'a Address,
        bump_seed: &'a [u8],
    ) -> [Seed<'a>; 4] {
        [
            Seed::from(wallet_address.as_ref()),
            Seed::from(token_program_id.as_ref()),
            Seed::from(token_mint_address.as_ref()),
            Seed::from(bump_seed),
        ]
    }
}
