#[path = "../src/size.rs"]
mod size;

use {
    pinocchio::{
        account::{RuntimeAccount, NOT_BORROWED},
        error::ProgramError,
        AccountView, Address,
    },
    pinocchio_token_2022::state::{Mint, TokenAccount},
    size::{get_account_data_size, resolve_account_data_size_cpi_result},
};

// Base token account plus empty ImmutableOwner TLV header
const TOKEN_2022_BASE_ACCOUNT_DATA_SIZE: u64 = TokenAccount::BASE_LEN as u64 + 1 + 4;

#[repr(C)]
struct TestAccount<const DATA_LEN: usize> {
    runtime: RuntimeAccount,
    data: [u8; DATA_LEN],
}

impl<const DATA_LEN: usize> TestAccount<DATA_LEN> {
    fn new(address: Address, owner: Address, executable: bool) -> Self {
        Self {
            runtime: RuntimeAccount {
                borrow_state: NOT_BORROWED,
                is_signer: 0,
                is_writable: 0,
                executable: u8::from(executable),
                padding: [0; 4],
                address,
                owner,
                lamports: 0,
                data_len: DATA_LEN as u64,
            },
            data: [0; DATA_LEN],
        }
    }

    fn view(&mut self) -> AccountView {
        // SAFETY: `runtime` is immediately followed by the in-struct data buffer,
        // matching the layout expected by `AccountView`.
        unsafe { AccountView::new_unchecked(&mut self.runtime) }
    }
}

#[test]
fn get_account_data_size_rejects_non_executable_token_program() {
    let mint_address = Address::new_unique();
    let mut mint =
        TestAccount::<{ Mint::BASE_LEN }>::new(mint_address, pinocchio_token_2022::ID, false);
    let mut token_program =
        TestAccount::<0>::new(pinocchio_token::ID, Address::new_unique(), false);

    assert_eq!(
        get_account_data_size(&mint.view(), &token_program.view()),
        Err(ProgramError::IncorrectProgramId)
    );
}

#[test]
fn get_account_data_size_short_circuits_for_spl_token() {
    let mint_address = Address::new_unique();
    let mut mint =
        TestAccount::<{ Mint::BASE_LEN + 1 }>::new(mint_address, pinocchio_token_2022::ID, false);
    let mut token_program = TestAccount::<0>::new(pinocchio_token::ID, Address::new_unique(), true);

    assert_eq!(
        get_account_data_size(&mint.view(), &token_program.view()).unwrap(),
        TokenAccount::BASE_LEN as u64
    );
}

#[test]
fn get_account_data_size_short_circuits_for_base_token_2022_mint() {
    let mint_address = Address::new_unique();
    let mut mint =
        TestAccount::<{ Mint::BASE_LEN }>::new(mint_address, pinocchio_token_2022::ID, false);
    let mut token_program =
        TestAccount::<0>::new(pinocchio_token_2022::ID, Address::new_unique(), true);

    assert_eq!(
        get_account_data_size(&mint.view(), &token_program.view()).unwrap(),
        TOKEN_2022_BASE_ACCOUNT_DATA_SIZE
    );
}

#[test]
fn get_account_data_size_fallback_rejects_missing_return_data() {
    let weird_token_program = Address::new_unique();
    let mint_address = Address::new_unique();
    let mut mint = TestAccount::<{ Mint::BASE_LEN + 1 }>::new(
        mint_address,
        weird_token_program.clone(),
        false,
    );
    let mut token_program = TestAccount::<0>::new(weird_token_program, Address::new_unique(), true);

    assert_eq!(
        get_account_data_size(&mint.view(), &token_program.view()),
        Err(ProgramError::InvalidInstructionData)
    );
}

#[test]
fn resolve_account_data_size_cpi_result_propagates_cpi_errors() {
    let token_program = Address::new_unique();

    assert_eq!(
        resolve_account_data_size_cpi_result(Err(ProgramError::IllegalOwner), &token_program, None),
        Err(ProgramError::IllegalOwner)
    );
}

#[test]
fn resolve_account_data_size_cpi_result_rejects_missing_return_data() {
    let token_program = Address::new_unique();

    assert_eq!(
        resolve_account_data_size_cpi_result(Ok(()), &token_program, None),
        Err(ProgramError::InvalidInstructionData)
    );
}

#[test]
fn resolve_account_data_size_cpi_result_rejects_wrong_program_id() {
    let token_program = Address::new_unique();
    let actual_program = Address::new_unique();
    let return_data = 165_u64.to_le_bytes();

    assert_eq!(
        resolve_account_data_size_cpi_result(
            Ok(()),
            &token_program,
            Some((&actual_program, &return_data)),
        ),
        Err(ProgramError::IncorrectProgramId)
    );
}

#[test]
fn resolve_account_data_size_cpi_result_rejects_malformed_return_data() {
    let token_program = Address::new_unique();
    let malformed = [1, 2, 3, 4];

    assert_eq!(
        resolve_account_data_size_cpi_result(
            Ok(()),
            &token_program,
            Some((&token_program, &malformed)),
        ),
        Err(ProgramError::InvalidInstructionData)
    );
}

#[test]
fn resolve_account_data_size_cpi_result_accepts_valid_return_data() {
    let token_program = Address::new_unique();
    let expected = 234_u64;
    let return_data = expected.to_le_bytes();

    assert_eq!(
        resolve_account_data_size_cpi_result(
            Ok(()),
            &token_program,
            Some((&token_program, &return_data)),
        ),
        Ok(expected)
    );
}
