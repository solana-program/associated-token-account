mod utils;

use crate::utils::test_util_exports::{ATATestHarness, CreateAtaInstructionType};

#[test]
fn success_create() {
    let mut harness =
        ATATestHarness::new(&spl_token_interface::id()).with_wallet_and_mint(1_000_000, 6);
    harness.create_ata(CreateAtaInstructionType::default());
}

#[test]
fn success_using_deprecated_instruction_creator() {
    let mut harness =
        ATATestHarness::new(&spl_token_interface::id()).with_wallet_and_mint(1_000_000, 6);

    harness.create_and_check_ata_with_custom_instruction(
        CreateAtaInstructionType::default(),
        |instruction| {
            instruction.data = vec![]; // Legacy deprecated instruction had empty data
        },
    );
}
