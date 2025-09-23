use ata_mollusk_harness::{AtaTestHarness, CreateAtaInstructionType};

#[test]
fn success_create() {
    let mut harness =
        AtaTestHarness::new(&spl_token_interface::id()).with_wallet_and_mint(1_000_000, 6);
    harness.create_ata(CreateAtaInstructionType::default());
}

#[test]
fn success_using_deprecated_instruction_creator() {
    let mut harness =
        AtaTestHarness::new(&spl_token_interface::id()).with_wallet_and_mint(1_000_000, 6);

    harness.create_and_check_ata_with_custom_instruction(
        CreateAtaInstructionType::default(),
        |instruction| {
            instruction.data = vec![];
            instruction
                .accounts
                .push(solana_instruction::AccountMeta::new_readonly(
                    solana_sysvar::rent::id(),
                    false,
                ));
        },
    );
}
