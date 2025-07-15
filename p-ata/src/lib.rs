#![no_std]

#[cfg(test)]
extern crate alloc;
#[cfg(test)]
extern crate std;

mod account;
mod entrypoint;
mod processor;

#[cfg(test)]
mod tests;
