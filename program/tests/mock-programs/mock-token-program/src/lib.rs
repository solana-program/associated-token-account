use {
    solana_account_info::{next_account_info, AccountInfo},
    solana_cpi::{invoke, set_return_data},
    solana_instruction::Instruction,
    solana_program_error::{ProgramError, ProgramResult},
    solana_pubkey::Pubkey,
};

// The tests encode the desired mock behavior in the first byte of the mint
// account data so a single fake token program can cover all return-data cases.
const NO_RETURN_DATA: u8 = 0;
const MALFORMED_RETURN_DATA: u8 = 1;
const FORWARD_CHILD_RETURN_DATA: u8 = 2;
const VALID_RETURN_DATA: u8 = 3;

// Match the normal token-account size so callers only fail because of the
// return-data path under test.
const EXPECTED_ACCOUNT_SIZE: u64 = 165;

solana_program_entrypoint::entrypoint!(process_instruction);

fn process_instruction(_program_id: &Pubkey, accounts: &[AccountInfo], _input: &[u8]) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();

    // The first account is always the mint. Its first data byte selects which
    // return-data scenario to simulate.
    let mint = next_account_info(accounts_iter)?;
    let behavior = mint.try_borrow_data()?.first().copied().unwrap_or(NO_RETURN_DATA);

    match behavior {
        // Simulate a token program that succeeds without setting any return data.
        NO_RETURN_DATA => Ok(()),
        MALFORMED_RETURN_DATA => {
            // Return the wrong number of bytes so the caller fails its
            // `u64::from_le_bytes` conversion.
            set_return_data(&[1, 2, 3, 4]);
            Ok(())
        }
        FORWARD_CHILD_RETURN_DATA => {
            // Simulate a token program that CPI-invokes another program and
            // never overwrites the nested program's return data on the way back.
            // The next account must be the executable child program account.
            let child_program = next_account_info(accounts_iter)?;
            invoke(
                &Instruction {
                    program_id: *child_program.key,
                    accounts: vec![],
                    data: vec![],
                },
                core::slice::from_ref(child_program),
            )
        }
        VALID_RETURN_DATA => {
            // Normal success case: set a well-formed account size directly.
            set_return_data(&EXPECTED_ACCOUNT_SIZE.to_le_bytes());
            Ok(())
        }
        _ => Err(ProgramError::InvalidAccountData),
    }
}
