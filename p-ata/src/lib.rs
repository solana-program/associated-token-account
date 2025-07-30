//! # p-ATA: pinocchio Associated Token Account Program
//!
//! An optimized implementation of the Associated Token Account (ATA) program

#![no_std]

mod account;
mod entrypoint;
mod processor;
mod recover;
mod size;

#[cfg(test)]
extern crate std;
#[cfg(test)]
mod tests;
