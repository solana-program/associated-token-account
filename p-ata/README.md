# `p-ata`

A `pinocchio`-based Associated Token Account program.

## Overview

p-ata (pinocchio-ata) is a drop-in replacement for SPL ATA. Following in the footsteps of [p-token](https://github.com/solana-program/token/tree/main/p-token), it uses pinocchio instead of solana-program to reduce compute usage. Plus, it includes a number of additional improvements.

- `no_std` crate
- Fully compatible with instruction and account layout of SPL Associated Token Account
- Minimized CU usage

## New Features (not available in SPL ATA)
- `RecoverNested` works with multisig accounts (satisfying #24)
- `CreatePrefundedAccount` is supported for cheaper calls of p-ata's `Create` when the account rent has been topped up in advance. Conditional on [SIMD-312](https://github.com/solana-foundation/solana-improvement-documents/pull/312), but alternative code is provided if `not(feature = "create-prefunded-account")`. Enabling this feature saves this flow ~2500 CUs (Compute Units). Currently this PR patches in branches with `CreatePrefundedAccount` support in `agave`, `system`, `pinocchio`, and `mollusk`.
- In descending order of significance,`bump`, `rent`, and `token_account_len` can be passed in by client to save compute.

## Notable Performance Improvements
- No strings attached, of course. Developed using [pinocchio](https://github.com/anza-xyz/pinocchio).
- SPL ATA always calls `InitializeImmutableOwner` via CPI. `InitializeImmutableOwner` is a no-op in Token, though not in Token 2022. In p-ata, if the relevant program is Token (not 2022), all `ImmutableOwner` logic is skipped.
- In p-ata, SPL Token ATA data length is assumed to be `TokenAccount::LEN`. For Token-2022, this program avoids CPI calls by using its own `calculate_account_size_from_mint_extensions`. For any other token program, ATA data length is checked via CPI. Of course, token length may be passed in as `token_account_len` to save compute.
- A few assertions are removed to save compute, when ignoring them fails later in the ATA transaction anyway. This results in different errors in a few cases (see below).

## Test Suites
General test capabilities included are:
1. `/src/tests/utils/mollusk_adapter.rs` - The original SPL ATA suite is run with a Mollusk adapter, allowing the unmodified solana_program_test files for SPL ATA to be run on p-ata.
2. `/src/tests/migrated` - (Redundancy) A migrated version of the same tests, written to run on Mollusk.
3. `/src/tests` - Unit tests for the various helper functions in processor.rs.
4. `/src/tests/token_account_len` - Tests for token account data length logic, whether passed in or calculated in-program. Includes exhaustive tests for the `calculate_account_size_from_mint_extensions` function, testing the results of this function for all possible combinations of token extensions against the results from Token-2022's `GetAccountDataSize` logic.
5. `/src/tests/bump` - Mollusk tests ensuring the safety of various scenarios where `bump` is passed in.

The branch `p-ata-bencher` adds these:
6. `/src/tests/benches` - A benchmark suite, which benches categories of operations in p-ata against SPL ATA and verifies that accounts are changed in the same way, byte-for-byte. See "Benchmarking" below.
7. `/src/tests/benches/failure_scenarios.rs` - 26 failure tests which compare errors yielded by p-ata against those by SPL ATA. All scenarios must ensure that baseline succeeds before mutating inputs to failure state.

Items 1 to 5 are run on `cargo test`

```
cargo build --features build-programs && cargo test
```

Items 6 and 7 are run on `cargo bench --features std`, in branch `p-ata-bencher`:

## Benchmarking
Set `BENCH_ITERATIONS` to average a number of runs. If only 1 iteration is used, optimal bump wallets will be found instead of random wallets each run.

```
BENCH_ITERATIONS=1 cargo bench --features std
```

### "Best run" numbers (ideal bumps) *as of 2025-08-11, ecde2ca*

| Test                  | SPL ATA | p-ata | bump arg | all optimizations |  notes                       |
|-----------------------|--------:|------:|---------:|------------------:|-----------------:|
| create_idempotent     |   3669  |  1732 |     590  |               590 |  |
| create                |  12364  |  4842 |    3323  |              3216 |  |
| create_token2022      |  14692  |  7642 |    6123  |              5987 |  |
| create_topup          |  15817  |  4710 |    3191  |              3083 | `CreateAccountPrefunded` |
| create_topup_nocap    |  15817  |  7476 |    5959  |              5851 | no `CreateAccountPrefunded` |
| create_extended       |  17620  |  9771 |    8251  |              7997 |  |
| recover_nested        |  12851  |  8100 |    N/A   |              8100 | rare instruction  |
| recover_multisig      |    N/A  |  8412 |    N/A   |              8412 | rare instruction  |
| worst_case_create     |  19864  | 15187 |    3323  |              3216 | hard-to-find bump |

### Average of 10,000 random wallets *as of 2025-08-11, ecde2cad*

| Test                  | SPL ATA | p-ata | bump arg | all optimizations |  notes                       |
|-----------------------|--------:|------:|---------:|------------------:|-----------------:|
| create_idempotent     |  4914   |  3234 |      948 |               948 |  |
| create                | 14194   |  6343 |     3692 |              3589 |  |
| create_token2022      | 16057   |  9140 |     6491 |              6362 |  |
| create_topup          | 17317   |  6183 |     3561 |              3449 | `CreateAccountPrefunded` |
| create_topup_no_cap   | 17287   |  8987 |     6324 |              6209 | no `CreateAccountPrefunded` |
| create_extended       | 19420   | 11289 |     8618 |              8366 |  |
| recover_nested        | 17066   | 12576 |      N/A |             12576 | rare instruction  |
| recover_multisig      |   N/A   | 12875 |      N/A |             12875 | rare instruction  |

All benchmarks also check for byte-for-byte equivalence with SPL ATA.

"optimum args" are:
- no special optimizations for `recover` tests
- for all `create` tests, `bump`
- for Token-2022, `token_account_len` passed in (after `bump`)
- for `create` tests other than `create_idemp`, `rent` passed in as an optional additional account

To benchmark (and run a set of failure tests and byte-for-byte equivalence tests) from the /p-ata directory on branch `p-ata-bencher`:

```
cargo build --features build-programs && cargo bench --features std
```

Mollusk's extensive debug logs are filtered out *unless* a test has an unexpected result. To show all of them, run `cargo bench --features std,full-debug-logs`.

## Tests with byte-for-byte checking on changed accounts
(byte-for-byte is irrelevant for "P-ATA optimization working" tests)
```
--- Testing variant create_idempotent ---
--- Testing create_idempotent_ --- ✅ Byte-for-Byte Identical
--- Testing create_idempotent__rent --- ✅ Byte-for-Byte Identical
--- Testing create_idempotent__bump --- 🚀 P-ATA optimization working
--- Testing create_idempotent__rent_bump --- 🚀 P-ATA optimization working

--- Testing variant create ---
--- Testing create_ --- ✅ Byte-for-Byte Identical
--- Testing create__rent --- ✅ Byte-for-Byte Identical
--- Testing create__bump --- 🚀 P-ATA optimization working
--- Testing create__rent_bump --- 🚀 P-ATA optimization working

--- Testing variant create_topup ---
Using P-ATA prefunded binary for create_topup
--- Testing create_topup_ --- ✅ Byte-for-Byte Identical
--- Testing create_topup__rent --- ✅ Byte-for-Byte Identical
--- Testing create_topup__bump --- 🚀 P-ATA optimization working
--- Testing create_topup__rent_bump --- 🚀 P-ATA optimization working

--- Testing variant create_topup_no_cap ---
--- Testing create_topup_no_cap_ --- ✅ Byte-for-Byte Identical
--- Testing create_topup_no_cap__rent --- ✅ Byte-for-Byte Identical
--- Testing create_topup_no_cap__bump --- 🚀 P-ATA optimization working
--- Testing create_topup_no_cap__rent_bump --- 🚀 P-ATA optimization working

--- Testing variant create_token2022 ---
--- Testing create_token2022_ --- ✅ Byte-for-Byte Identical
--- Testing create_token2022__rent --- ✅ Byte-for-Byte Identical
--- Testing create_token2022__bump --- 🚀 P-ATA optimization working
--- Testing create_token2022__rent_bump --- 🚀 P-ATA optimization working
--- Testing create_token2022__bump_token_account_len --- 🚀 P-ATA optimization working
--- Testing create_token2022__rent_bump_token_account_len --- 🚀 P-ATA optimization working

--- Testing variant recover_nested ---
--- Testing recover_nested_ --- ✅ Byte-for-Byte Identical

--- Testing variant recover_multisig ---
--- Testing recover_multisig_ --- 🚀 P-ATA optimization working
```

### Should-Fail Test Results
```
--- Basic Account Ownership Failure Tests ---
Test: fail_wrong_payer_owner
    ✅ Both failed (expected)
Test: fail_payer_not_signed
    ✅ Both failed (expected)
Test: fail_wrong_system_program
    ✅ Both failed (expected)
Test: fail_wrong_token_program
    ⚠️ Different error messages (both failed)
Test: fail_insufficient_funds
    ✅ Both failed (expected)

--- Address Derivation and Structure Failure Tests ---
Test: fail_wrong_ata_address
    ⚠️ Different error messages (both failed)
Test: fail_mint_wrong_owner
    ✅ Both failed (expected)
Test: fail_invalid_mint_structure
    ✅ Both failed (expected)
Test: fail_invalid_token_account_structure
    ✅ Both failed (expected)
Test: fail_invalid_discriminator
    ✅ Both failed (expected)
Test: fail_invalid_bump_value
    ✅ Failed as expected (P-ATA-only feature)

--- Recovery Operation Failure Tests ---
Test: fail_recover_wallet_not_signer
    ✅ Both failed (expected)
Test: fail_recover_multisig_insufficient_signers
    ✅ Both failed (expected)
Test: fail_recover_wrong_nested_ata_address
    ⚠️ Different error messages (both failed)
Test: fail_recover_wrong_destination_address
    ⚠️ Different error messages (both failed)
Test: fail_recover_invalid_bump_value
    ✅ Failed as expected (P-ATA-only feature)

--- Additional Validation Coverage Tests ---
Test: fail_ata_owned_by_system_program
    ✅ Both failed (expected)
Test: fail_wrong_token_account_size
    ✅ Both failed (expected)
Test: fail_token_account_wrong_mint
    ✅ Both failed (expected)
Test: fail_token_account_wrong_owner
    ⚠️ Different error messages (both failed)
Test: fail_immutable_account
    ✅ Both failed (expected)
Test: fail_create_extended_mint_v1
    ✅ Both failed (expected)
    
⚠️  "Different Error" Details:
  fail_wrong_token_program - Different Error Messages:
    P-ATA:     UnknownError(PrivilegeEscalation)
   SPL ATA:  Failure(InvalidSeeds)
  fail_wrong_ata_address - Different Error Messages:
    P-ATA:     UnknownError(PrivilegeEscalation)
   SPL ATA:  Failure(InvalidSeeds)
  fail_recover_wrong_nested_ata_address - Different Error Messages:
    P-ATA:     UnknownError(PrivilegeEscalation)
   SPL ATA:  Failure(InvalidSeeds)
  fail_recover_wrong_destination_address - Different Error Messages:
    P-ATA:     UnknownError(PrivilegeEscalation)
   SPL ATA:  Failure(InvalidSeeds)
  fail_token_account_wrong_owner - Different Error Messages:
    P-ATA:     Failure(IllegalOwner)
   SPL ATA:  Failure(Custom(0))
```
