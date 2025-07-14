#![no_std]

#[cfg(test)]
extern crate alloc;

mod account;
mod entrypoint;
mod processor;

#[cfg(test)]
mod tests;

#[cfg(test)]
pub fn id() -> solana_program::pubkey::Pubkey {
    // SPL ATA program ID here for some old tests
    use solana_program::pubkey;
    pubkey!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL")
}
