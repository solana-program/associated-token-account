//! # p-ATA: pinocchio Associated Token Account Program
//!
//! An optimized implementation of the Associated Token Account (ATA) program

#![no_std]

mod account;
mod entrypoint;
mod processor;
mod recover;
mod size;

#[cfg(any(test, feature = "std"))]
extern crate std;
#[cfg(any(test, feature = "std"))]
pub mod tests;
#[cfg(any(test, feature = "std"))]
extern crate self as pinocchio_ata_program;
