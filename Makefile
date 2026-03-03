RUST_TOOLCHAIN_NIGHTLY = nightly-2026-01-22
SOLANA_CLI_VERSION = 3.1.8

nightly = +${RUST_TOOLCHAIN_NIGHTLY}

# This is a bit tricky -- findstring returns the found string, so we're looking
# for "directory-", returning that, and replacing "-" with "/" to change the
# first "-" to a "/". But if it isn't found, we replace "" with "", which works
# in the case where there is no subdirectory.
pattern-dir = $(firstword $(subst -, ,$1))
find-pattern-dir = $(findstring $(call pattern-dir,$1)-,$1)
make-path = $(if $(filter p-ata,$1),p-ata,$(subst $(call find-pattern-dir,$1),$(subst -,/,$(call find-pattern-dir,$1)),$1))
test-features = $(if $(filter p-ata,$1),--features std,)
test-pre = $(if $(filter p-ata,$1),[ -f $(PWD)/p-ata/target/deploy/pinocchio_ata_program.so ] || cargo build-sbf --manifest-path $(call make-path,$1)/Cargo.toml &&,$(if $(filter program,$1),[ -f $(PWD)/target/deploy/spl_associated_token_account.so ] || cargo build-sbf --manifest-path $(call make-path,$1)/Cargo.toml &&,))
test-sbf-out-dir = $(if $(filter p-ata,$1),$(PWD)/p-ata/target/deploy,$(PWD)/target/deploy)

rust-toolchain-nightly:
	@echo ${RUST_TOOLCHAIN_NIGHTLY}

solana-cli-version:
	@echo ${SOLANA_CLI_VERSION}

cargo-nightly:
	cargo $(nightly) $(ARGS)

generate-clients:
	@echo "No JavaScript clients to generate"

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
	cargo $(nightly) bench --manifest-path $(call make-path,$*)/Cargo.toml $(ARGS)

regression-%:
	mollusk run-test --proto mollusk --ignore-compute-units $(call make-path,$*)/fuzz/program-mb.so ./target/deploy/$(subst -,_,$(shell toml get $(call make-path,$*)/Cargo.toml package.name)).so $(call make-path,$*)/fuzz/blob $(shell toml get $(call make-path,$*)/Cargo.toml package.metadata.solana.program-id)

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
	$(call test-pre,$*) SBF_OUT_DIR=$(call test-sbf-out-dir,$*) cargo $(nightly) test --manifest-path $(call make-path,$*)/Cargo.toml $(call test-features,$*) $(ARGS)

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
