# `pinocchio-associated-token-account-interface`

Pinocchio instructions and types for interacting with the Associated Token Account program.

## Codama

This crate includes Codama macros and an IDL generator binary.

Generate the IDL from repo root:

```bash
cargo run -p pinocchio-associated-token-account-interface --features codama --bin generate-idl
```

This writes:

```text
pinocchio/interface/idl.json
```

Client generation tooling and generated clients live in:

```text
pinocchio/clients
```

See:

- `pinocchio/clients/README.md`
