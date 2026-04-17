#### 2026-04-13 11:34:02.533475 UTC

Solana CLI Version: solana-cli 3.1.8 (src:2717084a; feat:1620780344, client:Agave)

| Name | CUs | Delta |
|------|------|-------|
| create (spl-token) | 3223 | +1 |
| create (spl-token, w/ rent account) | 3147 | - new - |
| create (token-2022) | 5267 | +1 |
| create (token-2022, w/ rent account) | 5191 | - new - |
| create_idempotent (new, spl-token) | 4318 | +1 |
| create_idempotent (new, spl-token, w/ rent account) | 4242 | - new - |
| create_idempotent (new, token-2022) | 5638 | +1 |
| create_idempotent (new, token-2022, w/ rent account) | 5562 | - new - |
| create_idempotent (existing, spl-token) | 564 | -- |
| create_idempotent (existing, token-2022) | 1650 | -- |
| create (prefunded, spl-token) | 3223 | +1 |
| create (prefunded, spl-token, w/ rent account) | 3147 | - new - |
| create (prefunded, token-2022) | 5267 | +1 |
| create (prefunded, token-2022, w/ rent account) | 5191 | - new - |
| recover_nested (owner=spl-token, nested=spl-token) | 5245 | -- |
| recover_nested (owner=token-2022, nested=token-2022) | 7109 | -- |
| recover_nested (owner=spl-token, nested=token-2022) | 9665 | -- |
| recover_nested (owner=token-2022, nested=spl-token) | 5615 | -- |

#### 2026-04-09 15:12:48.895565 UTC

Solana CLI Version: solana-cli 3.1.8 (src:2717084a; feat:1620780344, client:Agave)

| Name | CUs | Delta |
|------|------|-------|
| create (spl-token) | 3222 | -206 |
| create (token-2022) | 5266 | -1,211 |
| create_idempotent (new, spl-token) | 4317 | -206 |
| create_idempotent (new, token-2022) | 5637 | -1,211 |
| create_idempotent (existing, spl-token) | 564 | -- |
| create_idempotent (existing, token-2022) | 1650 | -- |
| create (prefunded, spl-token) | 3222 | -206 |
| create (prefunded, token-2022) | 5266 | -1,211 |
| recover_nested (owner=spl-token, nested=spl-token) | 5245 | -166 |
| recover_nested (owner=token-2022, nested=token-2022) | 7109 | -6 |
| recover_nested (owner=spl-token, nested=token-2022) | 9665 | +26 |
| recover_nested (owner=token-2022, nested=spl-token) | 5615 | -166 |

#### 2026-04-08 22:10:58.302753 UTC

Solana CLI Version: solana-cli 3.1.8 (src:2717084a; feat:1620780344, client:Agave)

| Name | CUs | Delta |
|------|------|-------|
| create (spl-token) | 3428 | -3,981 |
| create (token-2022) | 6477 | +351 |
| create_idempotent (new, spl-token) | 4523 | -3,982 |
| create_idempotent (new, token-2022) | 6848 | +350 |
| create_idempotent (existing, spl-token) | 564 | -1 |
| create_idempotent (existing, token-2022) | 1650 | -1 |
| create (prefunded, spl-token) | 3428 | -3,981 |
| create (prefunded, token-2022) | 6477 | +351 |
| recover_nested (owner=spl-token, nested=spl-token) | 5411 | - new - |
| recover_nested (owner=token-2022, nested=token-2022) | 7115 | - new - |
| recover_nested (owner=spl-token, nested=token-2022) | 9639 | - new - |
| recover_nested (owner=token-2022, nested=spl-token) | 5781 | - new - |

#### 2026-04-06 19:56:19.955539 UTC

Solana CLI Version: solana-cli 3.1.5 (src:67963d68; feat:2086771155, client:Agave)

| Name | CUs | Delta |
|------|------|-------|
| create (spl-token) | 7409 | -2,719 |
| create (token-2022) | 6126 | -2,045 |
| create_idempotent (new, spl-token) | 8505 | -2,719 |
| create_idempotent (new, token-2022) | 6498 | -2,046 |
| create_idempotent (existing, spl-token) | 565 | -- |
| create_idempotent (existing, token-2022) | 1651 | -- |
| create (prefunded, spl-token) | 7409 | -2,719 |
| create (prefunded, token-2022) | 6126 | -2,045 |
| recover_nested | 14726 | -- |

#### Pinocchio ATA baseline

Solana CLI Version: solana-cli 3.1.5 (src:67963d68; feat:2086771155, client:Agave)

| Name | CUs | Delta |
|------|------|-------|
| create (spl-token) | 10128 | -8,305 |
| create (token-2022) | 8171 | -5,796 |
| create_idempotent (new, spl-token) | 11224 | -11,716 |
| create_idempotent (new, token-2022) | 8544 | -6,930 |
| create_idempotent (existing, spl-token) | 565 | -3,145 |
| create_idempotent (existing, token-2022) | 1651 | -6,559 |
| create (prefunded, spl-token) | 10128 | -11,593 |
| create (prefunded, token-2022) | 8171 | -9,084 |
| recover_nested | 14726 | -12,080 |

#### Legacy ATA baseline

Solana CLI Version: solana-cli 3.1.5 (src:67963d68; feat:2086771155, client:Agave)

| Name | CUs | Delta |
|------|------|-------|
| create (spl-token) | 18433 | - new - |
| create (token-2022) | 13967 | - new - |
| create_idempotent (new, spl-token) | 22940 | - new - |
| create_idempotent (new, token-2022) | 15474 | - new - |
| create_idempotent (existing, spl-token) | 3710 | - new - |
| create_idempotent (existing, token-2022) | 8210 | - new - |
| create (prefunded, spl-token) | 21721 | - new - |
| create (prefunded, token-2022) | 17255 | - new - |
| recover_nested | 26806 | - new - |
