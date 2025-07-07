# `p-ata`

A `pinocchio`-based Associated Token Account program.

## Individual Test Results

<!-- BENCHMARK_BADGES_START -->
*Badges will be automatically updated on benchmark runs*
<!-- BENCHMARK_BADGES_END -->

## Overview

`p-ata` uses [`pinocchio`](https://github.com/anza-xyz/pinocchio) to optimize compute units while being fully compatible with the original implementation – i.e., support the exact same instruction and account layouts as SPL Associated Token Account, byte for byte.

## Features

- `no_std` crate
- Same instruction and account layout as SPL Associated Token Account
- Minimal CU usage

## Additional Features

Minor requested features for ATA have also been included:

- RecoverNested support for multisigs
- CreateAccountPrefunded support for cheaper flows that transfer rent before creating account -  [SIMD-312](https://github.com/solana-foundation/solana-improvement-documents/pull/312)

## Testing

cargo build --features build-programs && cargo bench
