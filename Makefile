RUST_TOOLCHAIN_NIGHTLY = nightly-2026-01-22
SOLANA_CLI_VERSION = 3.1.8

nightly = +${RUST_TOOLCHAIN_NIGHTLY}

# This is a bit tricky -- findstring returns the found string, so we're looking
# for "directory-", returning that, and replacing "-" with "/" to change the
# first "-" to a "/". But if it isn't found, we replace "" with "", which works
# in the case where there is no subdirectory.
pattern-dir = $(firstword $(subst -, ,$1))
find-pattern-dir = $(findstring $(call pattern-dir,$1)-,$1)
make-path = $(subst $(call find-pattern-dir,$1),$(subst -,/,$(call find-pattern-dir,$1)),$1)

rust-toolchain-nightly:
	@echo ${RUST_TOOLCHAIN_NIGHTLY}

solana-cli-version:
	@echo ${SOLANA_CLI_VERSION}

cargo-nightly:
	cargo $(nightly) $(ARGS)

generate-clients:
	@echo "No JavaScript clients to generate"

generate-idl-%:
	@cargo install --locked --version =0.9.0 codama-cli
	codama-rs generate-idl $(call make-path,$*) -o $(call make-path,$*)/idl.json --pretty $(ARGS)
	node scripts/postprocess-idl-$*.mjs

audit:
	cargo audit \
			--ignore RUSTSEC-2022-0093 \
			--ignore RUSTSEC-2024-0344 \
			--ignore RUSTSEC-2024-0376 $(ARGS)

spellcheck:
	cargo spellcheck --code 1 $(ARGS)

clippy-%:
	cargo $(nightly) clippy --manifest-path $(call make-path,$*)/Cargo.toml \
	  --all-targets \
	  --all-features \
		-- \
		--deny=warnings \
		--deny=clippy::default_trait_access \
		--deny=clippy::arithmetic_side_effects \
		--deny=clippy::manual_let_else \
		--deny=clippy::used_underscore_binding $(ARGS)

format-check-%:
	cargo $(nightly) fmt --check --manifest-path $(call make-path,$*)/Cargo.toml $(ARGS)

powerset-%:
	cargo $(nightly) hack check --feature-powerset --all-targets --manifest-path $(call make-path,$*)/Cargo.toml $(ARGS)

semver-check-%:
	cargo semver-checks --manifest-path $(call make-path,$*)/Cargo.toml $(ARGS)

shellcheck:
	git ls-files -- '*.sh' | xargs shellcheck --color=always --external-sources --shell=bash $(ARGS)

sort-check:
	cargo $(nightly) sort --workspace --check $(ARGS)

bench-%:
	SBF_OUT_DIR=$(PWD)/target/deploy cargo $(nightly) bench --manifest-path $(call make-path,$*)/Cargo.toml $(ARGS)

# $(1) = blob dir, $(2..N) = --test names
define run-fixture-tests
RUST_LOG=error \
SBF_OUT_DIR=$(PWD)/target/deploy \
EJECT_FUZZ_FIXTURES=$(PWD)/$(1) \
cargo $(nightly) test --features mollusk-svm/fuzz --manifest-path program/Cargo.toml \
	$(foreach t,$(2),--test $(t))
endef

generate-fixtures:
	rm -rf pinocchio/program/fuzz/blob pinocchio/program/fuzz/blob-mock pinocchio/program/fuzz/program-mb.so
	mkdir -p pinocchio/program/fuzz/blob pinocchio/program/fuzz/blob-mock
	$(call run-fixture-tests,pinocchio/program/fuzz/blob,create_always create_idempotent create_shared extended_mint recover_nested)
	$(call run-fixture-tests,pinocchio/program/fuzz/blob-mock,create_return_data)
	cp target/deploy/spl_associated_token_account.so pinocchio/program/fuzz/program-mb.so

# $(1) = pattern target (e.g. pinocchio-program), $(2) = Token-2022 ELF, $(3) = blob dir
define run-mollusk-regression
mollusk run-test \
	--proto mollusk \
	--config $(call make-path,$(1))/fuzz/mollusk-config.json \
	--add-program-with-loader-and-elf TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA BPFLoader2111111111111111111111111111111111 program/tests/fixtures/pinocchio_token_program.so \
	--add-program-with-loader-and-elf TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb BPFLoaderUpgradeab1e11111111111111111111111 $(2) \
	$(call make-path,$(1))/fuzz/program-mb.so ./target/deploy/$(subst -,_,$(shell toml get $(call make-path,$(1))/Cargo.toml package.name)).so $(3) $(shell toml get $(call make-path,$(1))/Cargo.toml package.metadata.solana.program-id)
endef

regression-%:
	cargo build-sbf --manifest-path $(call make-path,$*)/Cargo.toml
	$(call run-mollusk-regression,$*,program/tests/fixtures/spl_token_2022.so,pinocchio/program/fuzz/blob)
	$(call run-mollusk-regression,$*,program/tests/fixtures/mock_token_program.so,pinocchio/program/fuzz/blob-mock)

format-rust:
	cargo $(nightly) fmt --all $(ARGS)

build-sbf-%:
	cargo build-sbf --manifest-path $(call make-path,$*)/Cargo.toml $(ARGS)

build-wasm-%:
	cargo build --target wasm32-unknown-unknown --manifest-path $(call make-path,$*)/Cargo.toml --all-features $(ARGS)

build-doc-%:
	RUSTDOCFLAGS="--cfg docsrs -D warnings" cargo $(nightly) doc --all-features --no-deps --manifest-path $(call make-path,$*)/Cargo.toml $(ARGS)

test-doc-%:
	cargo $(nightly) test --doc --all-features --manifest-path $(call make-path,$*)/Cargo.toml $(ARGS)

test-%:
	SBF_OUT_DIR=$(PWD)/target/deploy cargo $(nightly) test --manifest-path $(call make-path,$*)/Cargo.toml $(ARGS)

miri-%:
	cargo $(nightly) miri test --lib --manifest-path $(call make-path,$*)/Cargo.toml $(ARGS)

# Helpers for publishing
tag-name = $(lastword $(subst /, ,$(call make-path,$1)))
preid-arg = $(subst pre,--preid $2,$(findstring pre,$1))
package-version = $(subst ",,$(shell jq -r '.version' $(call make-path,$1)/package.json))
crate-version = $(subst ",,$(shell toml get $(call make-path,$1)/Cargo.toml package.version))

git-tag-rust-%:
	@echo "$(call tag-name,$*)@v$(call crate-version,$*)"

publish-rust-%:
	cd "$(call make-path,$*)" && cargo release $(LEVEL) --tag-name "$(call tag-name,$*)@v{{version}}" --execute --no-confirm --dependent-version fix

publish-rust-dry-run-%:
	cd "$(call make-path,$*)" && cargo release $(LEVEL) --tag-name "$(call tag-name,$*)@v{{version}}"
