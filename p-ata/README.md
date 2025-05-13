# `p-ata`

A `pinocchio`-based Associated Token Account program.

## Overview

`p-ata` follows `p-token` as a highly-optimized core Solana program. One of the most popular programs on Solana, `p-ata` uses [`pinocchio`](https://github.com/anza-xyz/pinocchio) to optimize compute units while being fully compatible with the original implementation &mdash; i.e., support the exact same instruction and account layouts as SPL Associated Token Account, byte for byte.

## Features

- `no_std` crate
- Same instruction and account layout as SPL Token
- Minimal CU usage

## Additional Features

Minor requested features for ATA have also been included:
(todo)

## License

The code is licensed under the [Apache License Version 2.0](LICENSE)

## Testing

export BPF_OUT_DIR=$(pwd)/target/deploy && cargo test
