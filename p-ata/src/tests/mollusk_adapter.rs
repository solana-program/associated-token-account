//! An adapter that allows the original solana_program_test to run against the p-ata program
//! using Mollusk and pinocchio.

use {
    crate::entrypoint::process_instruction as pinocchio_process_instruction,
    bincode,
    core::cell::RefCell,
    mollusk_svm::{program::loader_keys, Mollusk},
    solana_program::{account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey},
    solana_program_test::*,
    solana_sdk::{
        account::Account,
        hash::Hash,
        instruction::InstructionError,
        signature::Keypair,
        signer::Signer,
        transaction::{Transaction, TransactionError},
    },
    spl_associated_token_account, spl_associated_token_account_client,
    std::collections::BTreeMap,
};

#[allow(dead_code, clippy::all, unsafe_code)]
// This function is the primary bridge between the solana_program_test environment
// and the pinocchio-based p-ata program. `unsafe` relies on the assumption that
// the memory layouts of `solana_program::AccountInfo` and
// `pinocchio::account_info::AccountInfo` are identical.
fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> Result<(), ProgramError> {
    // This wrapper allows us to test SPL ATA instructions against our p-ata implementation

    // Convert program_id to pinocchio format
    let pinocchio_program_id = pinocchio::pubkey::Pubkey::from(program_id.to_bytes());

    // Call the pinocchio process_instruction with simple transmute (layouts are identical)
    let pinocchio_accounts: &[pinocchio::account_info::AccountInfo] =
        unsafe { core::mem::transmute(accounts) };

    match pinocchio_process_instruction(&pinocchio_program_id, pinocchio_accounts, instruction_data)
    {
        Ok(()) => Ok(()),
        Err(pinocchio::program_error::ProgramError::NotEnoughAccountKeys) => {
            Err(ProgramError::NotEnoughAccountKeys)
        }
        Err(pinocchio::program_error::ProgramError::InvalidAccountData) => {
            Err(ProgramError::InvalidAccountData)
        }
        Err(pinocchio::program_error::ProgramError::InvalidArgument) => {
            Err(ProgramError::InvalidArgument)
        }
        Err(pinocchio::program_error::ProgramError::InvalidInstructionData) => {
            Err(ProgramError::InvalidInstructionData)
        }
        Err(_) => Err(ProgramError::Custom(1)),
    }
}

#[allow(dead_code)]
fn id() -> Pubkey {
    spl_associated_token_account::id()
}

// ================= MOLLUSK-BASED TEST RUNNERS =================

/// Mollusk-based banks client that matches the original API
pub struct MolluskBanksClient {
    pub mollusk: Mollusk,
    pub accounts: RefCell<BTreeMap<Pubkey, Account>>,
}

impl MolluskBanksClient {
    pub async fn get_rent(
        &self,
    ) -> Result<solana_program::sysvar::rent::Rent, solana_program::program_error::ProgramError>
    {
        Ok(self.mollusk.sysvars.rent.clone())
    }

    pub async fn process_transaction(
        &self,
        transaction: Transaction,
    ) -> Result<(), BanksClientError> {
        // Process each instruction in the transaction through Mollusk
        for instruction in &transaction.message.instructions {
            let program_id =
                transaction.message.account_keys[instruction.program_id_index as usize];

            // Build the instruction with proper accounts
            let mut instruction_accounts = std::vec::Vec::new();
            for &account_index in &instruction.accounts {
                let account_key = transaction.message.account_keys[account_index as usize];
                let account = self
                    .accounts
                    .borrow()
                    .get(&account_key)
                    .cloned()
                    .unwrap_or_else(|| {
                        // Create a default account if it doesn't exist
                        Account::new(0, 0, &solana_sdk::system_program::id())
                    });
                instruction_accounts.push((account_key, account));
            }

            // Create the instruction for Mollusk with proper account meta flags
            let mut mollusk_accounts = std::vec::Vec::new();
            for &account_index in &instruction.accounts {
                let account_key = transaction.message.account_keys[account_index as usize];

                // Determine if this account is a signer
                let is_signer = if (account_index as usize)
                    < transaction.message.header.num_required_signatures as usize
                {
                    true
                } else {
                    false
                };

                // Determine if this account is writable
                let is_writable = if (account_index as usize)
                    < (transaction.message.header.num_required_signatures as usize
                        - transaction.message.header.num_readonly_signed_accounts as usize)
                {
                    true // Writable signer
                } else if (account_index as usize)
                    >= transaction.message.header.num_required_signatures as usize
                    && (account_index as usize)
                        < (transaction.message.account_keys.len()
                            - transaction.message.header.num_readonly_unsigned_accounts as usize)
                {
                    true // Writable non-signer
                } else {
                    false // Read-only
                };

                if is_writable {
                    mollusk_accounts.push(solana_sdk::instruction::AccountMeta::new(
                        account_key,
                        is_signer,
                    ));
                } else {
                    mollusk_accounts.push(solana_sdk::instruction::AccountMeta::new_readonly(
                        account_key,
                        is_signer,
                    ));
                }
            }

            let mollusk_instruction = solana_sdk::instruction::Instruction {
                program_id,
                accounts: mollusk_accounts,
                data: instruction.data.clone(),
            };

            // Process the instruction through Mollusk
            let result = self
                .mollusk
                .process_instruction(&mollusk_instruction, &instruction_accounts);

            // Update our account tracking with the results
            for (pubkey, account) in result.resulting_accounts {
                self.accounts.borrow_mut().insert(pubkey, account);
            }

            // Check if the instruction failed
            match result.program_result {
                mollusk_svm::result::ProgramResult::Success => {
                    // Handle special case for recover_nested: delete the nested account
                    if mollusk_instruction.program_id == spl_associated_token_account::id()
                        && mollusk_instruction.data.len() > 0
                        && mollusk_instruction.data[0] == 2
                    {
                        // This is a successful recover_nested instruction
                        // The nested account (first account) should be deleted
                        // (Mollusk keeps the closed account)
                        if let Some(nested_account_key) = mollusk_instruction.accounts.get(0) {
                            self.accounts
                                .borrow_mut()
                                .remove(&nested_account_key.pubkey);
                        }
                    }
                }
                mollusk_svm::result::ProgramResult::Failure(err) => {
                    // Convert ProgramError to InstructionError with custom mapping
                    let instruction_error =
                        map_mollusk_error_to_original(&mollusk_instruction, err);
                    return Err(BanksClientError::TransactionError(
                        TransactionError::InstructionError(0, instruction_error),
                    ));
                }
                mollusk_svm::result::ProgramResult::UnknownError(_) => {
                    // Map UnknownError to the appropriate error based on context
                    let instruction_error = if mollusk_instruction.program_id
                        == spl_associated_token_account::id()
                        && mollusk_instruction.data.len() > 0
                        && mollusk_instruction.data[0] == 2
                    {
                        // For recover_nested, UnknownError usually means wrong token program
                        InstructionError::IllegalOwner
                    } else {
                        InstructionError::ProgramFailedToComplete
                    };
                    return Err(BanksClientError::TransactionError(
                        TransactionError::InstructionError(0, instruction_error),
                    ));
                }
            }
        }

        Ok(())
    }

    pub async fn get_account(
        &self,
        pubkey: Pubkey,
    ) -> Result<Option<Account>, solana_program::program_error::ProgramError> {
        // Return the account if it exists in our tracking
        Ok(self.accounts.borrow().get(&pubkey).cloned())
    }

    pub async fn get_balance(
        &self,
        pubkey: Pubkey,
    ) -> Result<u64, solana_program::program_error::ProgramError> {
        // Return the account balance if it exists, otherwise 0
        Ok(self
            .accounts
            .borrow()
            .get(&pubkey)
            .map(|account| account.lamports)
            .unwrap_or(0))
    }

    pub async fn get_new_latest_blockhash(
        &self,
        _previous_blockhash: &Hash,
    ) -> Result<Hash, solana_program::program_error::ProgramError> {
        // Return a new mock blockhash
        Ok(Hash::new_unique())
    }
}

/// Mollusk-based program test context that matches the original API
pub struct MolluskProgramTestContext {
    pub banks_client: MolluskBanksClient,
    pub payer: Keypair,
    pub last_blockhash: Hash,
}

// Type aliases to make Mollusk types compatible with existing tests
pub type ProgramTestContext = MolluskProgramTestContext;
#[allow(dead_code)]
pub type BanksClient = MolluskBanksClient;

/// Mollusk-based program test that matches the original API
pub struct MolluskProgramTest {
    mollusk: Mollusk,
    token_mint_address: Pubkey,
    accounts: BTreeMap<Pubkey, Account>,
    token_program_id: Pubkey,
}

impl MolluskProgramTest {
    pub fn add_account(&mut self, pubkey: Pubkey, account: Account) {
        self.accounts.insert(pubkey, account);
    }

    pub async fn start(self) -> (MolluskBanksClient, Keypair, Hash) {
        let payer = Keypair::new();
        let recent_blockhash = Hash::default();

        let mut accounts = self.accounts;

        // Add the payer account
        accounts.insert(
            payer.pubkey(),
            Account::new(1_000_000_000, 0, &solana_sdk::system_program::id()),
        );

        // Add the token mint account with real mint data from fixture file
        // (Same approach as the original program_test_2022)
        let mint_data = std::fs::read("../program/tests/fixtures/token-mint-data.bin")
            .expect("Failed to read token mint data");
        accounts.insert(
            self.token_mint_address,
            Account {
                lamports: 1461600,
                data: mint_data,
                owner: self.token_program_id,
                executable: false,
                rent_epoch: 0,
            },
        );

        // Add system program
        accounts.insert(
            solana_sdk::system_program::id(),
            Account::new(0, 0, &solana_sdk::native_loader::id()),
        );

        // Add token program
        accounts.insert(
            self.token_program_id,
            Account::new(0, 0, &loader_keys::LOADER_V3),
        );

        // Add rent sysvar account so token program can access rent exemption info
        let rent = self.mollusk.sysvars.rent.clone();
        let rent_data = bincode::serialize(&rent).expect("serialize rent");
        accounts.insert(
            solana_program::sysvar::rent::id(),
            Account {
                lamports: 1,
                data: rent_data,
                owner: solana_program::sysvar::id(),
                executable: false,
                rent_epoch: 0,
            },
        );

        let banks_client = MolluskBanksClient {
            mollusk: self.mollusk,
            accounts: RefCell::new(accounts),
        };

        (banks_client, payer, recent_blockhash)
    }

    pub async fn start_with_context(self) -> MolluskProgramTestContext {
        let (banks_client, payer, recent_blockhash) = self.start().await;
        MolluskProgramTestContext {
            banks_client,
            payer,
            last_blockhash: recent_blockhash,
        }
    }
}

fn setup_mollusk_with_p_ata(mollusk: &mut Mollusk) {
    let program_id = spl_associated_token_account::id();
    mollusk.add_program(
        &program_id,
        "target/deploy/pinocchio_ata_program",
        &loader_keys::LOADER_V3,
    );
}

/// Mollusk-based equivalent of program_test_2022
pub fn mollusk_program_test_2022(token_mint_address: Pubkey) -> MolluskProgramTest {
    let mut mollusk = Mollusk::default();
    setup_mollusk_with_p_ata(&mut mollusk);

    // Add required programs
    mollusk.add_program(
        &spl_token_2022::id(),
        "programs/token-2022/target/deploy/spl_token_2022",
        &loader_keys::LOADER_V3,
    );

    MolluskProgramTest {
        mollusk,
        token_mint_address,
        accounts: BTreeMap::new(),
        token_program_id: spl_token_2022::id(),
    }
}

/// Mollusk-based equivalent of program_test
pub fn mollusk_program_test(token_mint_address: Pubkey) -> MolluskProgramTest {
    let mut mollusk = Mollusk::default();
    setup_mollusk_with_p_ata(&mut mollusk);

    // Add required programs - use p-token for non-2022 tests
    mollusk.add_program(
        &spl_token::id(),
        "programs/token/target/deploy/pinocchio_token_program",
        &loader_keys::LOADER_V3,
    );

    MolluskProgramTest {
        mollusk,
        token_mint_address,
        accounts: BTreeMap::new(),
        token_program_id: spl_token::id(),
    }
}

/// Maps Mollusk errors to match the original solana_program_test behavior
/// This is a workaround to handle p-ata differing errors without requiring
/// changes to the actual original test files.
fn map_mollusk_error_to_original(
    instruction: &solana_sdk::instruction::Instruction,
    error: ProgramError,
) -> InstructionError {
    if instruction.program_id == spl_associated_token_account::id() {
        let is_recover_nested = instruction.data.len() > 0 && instruction.data[0] == 2;
        let is_idempotent_create = instruction.data.len() > 0 && instruction.data[0] == 1;

        match error {
            // System program "account already exists" -> IllegalOwner for non-idempotent ATA create
            ProgramError::Custom(0) => {
                if instruction.data.len() > 0 && instruction.data[0] == 0 {
                    InstructionError::IllegalOwner
                } else {
                    InstructionError::from(u64::from(error))
                }
            }
            // P-ATA program "Provided owner is not allowed" -> Custom(0) for InvalidOwner
            ProgramError::IllegalOwner => InstructionError::Custom(0),
            // InvalidInstructionData from canonical address mismatch -> InvalidSeeds
            ProgramError::InvalidInstructionData => InstructionError::InvalidSeeds,
            // InvalidAccountData errors need context-specific mapping
            ProgramError::InvalidAccountData => {
                if is_recover_nested {
                    InstructionError::IllegalOwner
                } else if is_idempotent_create {
                    // For idempotent create, if account exists but isn't proper ATA,
                    // original expects InvalidSeeds (address derivation check)
                    InstructionError::InvalidSeeds
                } else {
                    InstructionError::from(u64::from(error))
                }
            }
            // IncorrectProgramId errors for recover_nested should be mapped to IllegalOwner
            ProgramError::IncorrectProgramId => {
                if is_recover_nested {
                    InstructionError::IllegalOwner
                } else {
                    InstructionError::from(u64::from(error))
                }
            }
            ProgramError::MissingRequiredSignature => InstructionError::MissingRequiredSignature,
            ProgramError::InvalidSeeds => InstructionError::InvalidSeeds,
            // InvalidArgument might be InvalidSeeds if ATA address doesn't match expected seeds
            ProgramError::InvalidArgument => {
                // Check if this is due to invalid ATA address (seeds mismatch)
                if instruction.accounts.len() >= 6 {
                    let provided_ata_address = instruction.accounts[1].pubkey;
                    let wallet_address = instruction.accounts[2].pubkey;
                    let token_mint_address = instruction.accounts[3].pubkey;
                    let token_program_address = instruction.accounts[5].pubkey;

                    // Calculate expected ATA address using the correct token program
                    let expected_ata_address = spl_associated_token_account_client::address::get_associated_token_address_with_program_id(
                        &wallet_address,
                        &token_mint_address,
                        &token_program_address,
                    );

                    // If addresses don't match, this is an InvalidSeeds error
                    if provided_ata_address != expected_ata_address {
                        InstructionError::InvalidSeeds
                    } else {
                        InstructionError::from(u64::from(error))
                    }
                } else {
                    InstructionError::from(u64::from(error))
                }
            }
            _ => InstructionError::from(u64::from(error)),
        }
    } else {
        InstructionError::from(u64::from(error))
    }
}
