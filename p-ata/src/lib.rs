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
