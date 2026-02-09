//! # p-ATA: pinocchio Associated Token Account Program
//!
//! An optimized implementation of the Associated Token Account (ATA) program

#![no_std]

pub mod account;
pub mod entrypoint;
pub mod processor;
pub mod recover;
pub mod size;

// Compile-time check to ensure tests/benches use --features std
#[cfg(all(test, not(feature = "std")))]
compile_error!("Tests require the 'std' feature. Use: cargo test --features std");
#[cfg(any(test, feature = "std"))]
extern crate std;
