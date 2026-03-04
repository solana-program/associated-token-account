# `pinocchio-associated-token-account-program`

A `pinocchio`-based Associated Token Account program.

## Overview

pinocchio-associated-token-account-program (p-ata) is a drop-in replacement for SPL ATA. Following in the footsteps of 
[p-token](https://github.com/solana-program/token/tree/main/pinocchio), it uses pinocchio instead of solana-program to
reduce compute usage. Plus, it includes a number of additional improvements.

- `no_std` crate
- Fully compatible with instruction and account layout of SPL Associated Token Account
- Minimized CU usage
