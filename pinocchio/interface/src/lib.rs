//! Pinocchio instructions and types for the Associated Token Account program.

#![no_std]

#[cfg(feature = "codama")]
use codama_macros::codama_program;

pub mod error;
pub mod instruction;
pub mod pda;

solana_address::declare_id!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");

#[cfg(feature = "codama")]
codama_program!(name = "associatedTokenAccount");
