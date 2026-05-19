use {
    spl_associated_token_account_interface::address::get_associated_token_address_and_bump_seed,
    spl_associated_token_account_mollusk_harness::AtaTestHarness,
};

pub fn expected_bump(harness: &AtaTestHarness) -> u8 {
    let wallet = harness.wallet.unwrap();
    let mint = harness.mint.unwrap();
    let (_, bump) = get_associated_token_address_and_bump_seed(
        &wallet,
        &mint,
        &spl_associated_token_account_interface::program::id(),
        &harness.token_program_id,
    );
    bump
}
