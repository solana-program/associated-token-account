# `p-ata`

A `pinocchio`-based Associated Token Account program.

## Overview

`p-ata` uses [`pinocchio`](https://github.com/anza-xyz/pinocchio) to optimize compute units while being fully compatible with the original implementation – i.e., support the exact same instruction and account layouts as SPL Associated Token Account, byte for byte.

## Features

- `no_std` crate
- Same instruction and account layout as SPL Associated Token Account
- Minimized CU usage

Expanded Functionality:

- `RecoverNested` works with multisig accounts (satisfying [#24](https://github.com/solana-program/associated-token-account/issues/24))
- `CreateAccountPrefunded` is supported for cheaper calls of p-ata's `Create` when the account rent has been topped up in advance. Conditional on [SIMD-312](https://github.com/solana-foundation/solana-improvement-documents/pull/312), but alternative code is provided if `not(feature = "create-account-prefunded")`. Enabling this feature saves this flow ~2500 CUs (Compute Units). Currently, branches of `agave`, `system`, `pinocchio`, and `mollusk` with `CreateAccountPrefunded` support are patched in.
- In descending order of significance,`bump`, `rent`, and (TokenAccount) `token_account_len` can be passed in by client to save compute.

## Testing and Benchmarking

Benchmarking averages a number of runs. 1000 completes in about 5 seconds on modern hardware:

`cargo build --features build-programs && BENCH_ITERATIONS=1000 cargo bench`

Mollusk's extensive debug logs are filtered out unless a test has an unexpected result. To show all of them, run `cargo bench --features full-debug-logs`.

*as of 2025-07-09, 3747f8d*

"optimum args" are:
- `bump`
- for Token-2022, `token_account_len` passed in the data
- for some items, `rent` passed in as an optional additional account

| Test                   |    SPL ATA     | p-ata, no new args   | p-ata w/ bump | p-ata w/ optimum args | Notes                                                 |
|------------------------|----------|---------|----------|------------------|--------------------------------------------------------|
| create_idemp           |   3,669  |    241  |       --      |       --        |                                 |
| create_base            |  12,364  |  4,715  |  3,195 | 3,098        |                                   |
| create_topup           |  15,817  |  4,718  | 3,198 |    3,101        | create-account-prefunded      |
| create_topup_no_cap    |  15,817  |  7,205  |    5,685 |  5,588        | no create-account-prefunded   |
| create_token2022       |  14,692  |  7,461  |     5,941  | 5,817        |                                                |
| recover_nested             |  14,356  |  4,428  |    2,904 | 2,904        |                                            |
| recover_multisig       |    --   |  4,668  |      3,144 | 3,144        |                                         |
| worst_case_create      |  19,864  | 15,187  |     3,195 | 3,098        | Hard-to-find bump   |

All benchmarks also check for byte-for-byte equivalence with SPL ATA.

To benchmark (and run a set of failure tests and byte-for-byte equivalence tests) from the /p-ata directory:

```
cargo build --features build-programs && cargo bench
```

### Notable Improvements (beyond noalloc/pinocchio)
- SPL ATA always calls `InitializeImmutableOwner` via CPI. `InitializeImmutableOwner` is a no-op in Token, though not in Token 2022. In p-ata, if the relevant program is Token (not 2022), all `ImmutableOwner` logic is skipped.
- Account data length is assumed to be standard (or passed in) token account length when possible, instead of using `get_account_data_size`.