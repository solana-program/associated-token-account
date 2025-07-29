# `p-ata`

A `pinocchio`-based Associated Token Account program.

## Overview

`p-ata` uses [`pinocchio`](https://github.com/anza-xyz/pinocchio) to optimize compute units while being fully compatible with the original implementation – i.e., support the exact same instruction and account layouts as SPL Associated Token Account, byte for byte.

## Features

- `no_std` crate
- Same instruction and account layout as SPL Associated Token Account
- Minimized CU usage

p-ata (pinocchio-ata) is a drop-in replacement for SPL ATA. Following in the footsteps of [p-token](https://github.com/solana-program/token/tree/main/p-token), it uses pinocchio instead of solana-program to reduce compute usage. Plus, it includes a number of additional improvements.

## Additional features
- `RecoverNested` works with multisig accounts (satisfying #24)
- `CreatePrefundedAccount` is supported for cheaper calls of p-ata's `Create` when the account rent has been topped up in advance. Conditional on [SIMD-312](https://github.com/solana-foundation/solana-improvement-documents/pull/312), but alternative code is provided if `not(feature = "create-account-prefunded")`. Enabling this feature saves this flow ~2500 CUs (Compute Units). Currently this PR patches in branches with `CreatePrefundedAccount` support in `agave`, `system`, `pinocchio`, and `mollusk`.
- In descending order of significance,`bump`, `rent`, and `token_account_len` can be passed in by client to save compute.

## Notable Performance Improvements
- No strings attached, of course. Developed using [pinocchio](https://github.com/anza-xyz/pinocchio).
- SPL ATA always calls `InitializeImmutableOwner` via CPI. `InitializeImmutableOwner` is a no-op in Token, though not in Token 2022. In p-ata, if the relevant program is Token (not 2022), all `ImmutableOwner` logic is skipped.
- In p-ata, SPL Token ATA data length is assumed to be `TokenAccount::LEN`. For Token-2022, this program avoids CPI calls by using its own `calculate_account_size_from_mint_extensions`. For any other token program, ATA data length is checked via CPI. Of course, token length may be passed in as `token_account_len` to save compute.
- A few assertions are removed to save compute, when ignoring them fails later in the ATA transaction anyway. This results in different errors in a few cases (see below).

## Test Suites
The test suites included are:
1. `/src/tests/mollusk_adapter.rs` - The original SPL ATA suite is run with a Mollusk adapter, allowing the unmodified solana_program_test files for SPL ATA to be run on p-ata.
2. `/src/tests/migrated` - (Redundancy) A migrated version of the same tests, written to run on Mollusk.
3. `/src/tests` - Unit tests for the various helper functions in processor.rs.
4. `/src/tests/test_extension_size_exhaustive.rs` - Exhaustive tests for the `calculate_account_size_from_mint_extensions` function, which tests the results of this function for all possible combinations of token extensions against the results from Token-2022's `process_get_account_data_size`.
5. `/benches` A benchmark suite, which benches categories of operations in p-ata against SPL ATA and verifies that accounts are changed in the same way, byte-for-byte. See "Benchmarking" below.

```
cargo build --features build-programs && cargo test
```

## Benchmarking
Set `BENCH_ITERATIONS` to average a number of runs. If only 1 iteration is used, optimal bump wallets will be found instead of random wallets each run.

```
BENCH_ITERATIONS=1 cargo bench
```

### "Best run" numbers (ideal bumps) *as of 2025-07-28, ad6fbac*

| Test                  | SPL ATA | p-ata | bump arg | all optimizations |
|-----------------------|--------:|------:|---------:|------------------:|
| create_idempotent     |   3669  |  1805 |    1806  |              1806 |
| create                |  12364  |  4976 |    3415  |              3313 |
| create_token2022      |  14692  |  7778 |    6217  |              6084 |
| create_topup          |  15817  |  4842 |    3281  |              3179 |
| create_topup_nocap    |  15817  |  7609 |    6048  |              5946 |
| create_extended       |  17620  |  9927 |    8366  |              8094 |
| recover_nested        |  12851  |  8104 |    8104  |              8104 |
| recover_multisig      |      0  |  8550 |    8550  |              8550 |
| worst_case_create     |  19864  | 15187 |    3415  |              3313 |

### Average 10,000 random wallet runs *as of 2025-07-28, ad6fbac*

| Test                  | SPL ATA | p-ata | bump arg | all optimizations |
|-----------------------|--------:|------:|---------:|------------------:|
| create_idempotent     |  4914   |  3743 |     3264 |              3264 |
| create                | 14194   |  6654 |     3741 |              3731 |
| create_token2022      | 16057   |  9366 |     6613 |              6354 |
| create_topup          | 17317   |  6534 |     3749 |              3550 |
| create_topup_no_cap   | 17287   |  9169 |     6409 |              6229 |
| create_extended       | 19420   | 11441 |     8735 |              8459 |
| recover_nested        | 17066   | 12409 |    12538 |             12538 |
| recover_multisig      |     0   | 13185 |    12660 |             12660 |

All benchmarks also check for byte-for-byte equivalence with SPL ATA.

"optimum args" are:
- `bump`
- for Token-2022, `token_account_len` passed in (after `bump`)
- for `create` tests other than `create_idemp`, `rent` passed in as an optional additional account

To benchmark (and run a set of failure tests and byte-for-byte equivalence tests) from the /p-ata directory:

```
cargo build --features build-programs && cargo bench
```

Mollusk's extensive debug logs are filtered out *unless* a test has an unexpected result. To show all of them, run `cargo bench --features full-debug-logs`.

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
--- Testing recover_nested__rent --- ✅ Byte-for-Byte Identical
--- Testing recover_nested__bump --- 🚀 P-ATA optimization working

--- Testing variant recover_multisig ---
--- Testing recover_multisig_ --- 🚀 P-ATA optimization working
--- Testing recover_multisig__rent --- 🚀 P-ATA optimization working
--- Testing recover_multisig__bump --- 🚀 P-ATA optimization working
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
