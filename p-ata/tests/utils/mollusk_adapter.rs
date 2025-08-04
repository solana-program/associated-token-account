//! An adapter that allows the original solana_program_test to run against the p-ata program
//! using Mollusk and pinocchio.
//!
//! The SPL ATA unit tests have been rewritten for Mollusk in ./migrated; however, this file
//! allows the original tests to run against the p-ata program without a single modification.

use {
    bincode,
    core::cell::RefCell,
    mollusk_svm::{program::loader_keys, Mollusk},
    pinocchio_ata_program::entrypoint::process_instruction as pinocchio_process_instruction,
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
    std::{collections::BTreeMap, vec::Vec},
};

#[allow(dead_code, clippy::all, unsafe_code)]
// Bridge between solana_program_test and pinocchio-based p-ata program.
// `unsafe` assumes identical memory layouts of AccountInfo types.
fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> Result<(), ProgramError> {
    let pinocchio_program_id = pinocchio::pubkey::Pubkey::from(program_id.to_bytes());
    let pinocchio_accounts: &[pinocchio::account_info::AccountInfo] =
        unsafe { core::mem::transmute(accounts) };

    pinocchio_process_instruction(&pinocchio_program_id, pinocchio_accounts, instruction_data)
        .map_err(|err| match err {
            pinocchio::program_error::ProgramError::NotEnoughAccountKeys => {
                ProgramError::NotEnoughAccountKeys
            }
            pinocchio::program_error::ProgramError::InvalidAccountData => {
                ProgramError::InvalidAccountData
            }
            pinocchio::program_error::ProgramError::InvalidArgument => {
                ProgramError::InvalidArgument
            }
            pinocchio::program_error::ProgramError::InvalidInstructionData => {
                ProgramError::InvalidInstructionData
            }
            _ => ProgramError::Custom(1),
        })
}

#[allow(dead_code)]
fn id() -> Pubkey {
    spl_associated_token_account::id()
}

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
        for instruction in &transaction.message.instructions {
            let program_id =
                transaction.message.account_keys[instruction.program_id_index as usize];

            let instruction_accounts: Vec<_> = instruction
                .accounts
                .iter()
                .map(|&account_index| {
                    let account_key = transaction.message.account_keys[account_index as usize];
                    let account = self
                        .accounts
                        .borrow()
                        .get(&account_key)
                        .cloned()
                        .unwrap_or_else(|| Account::new(0, 0, &solana_sdk::system_program::id()));
                    (account_key, account)
                })
                .collect();

            let header = &transaction.message.header;
            let writable_signer_end = header.num_required_signatures as usize
                - header.num_readonly_signed_accounts as usize;
            let all_signers_end = header.num_required_signatures as usize;
            let writable_nonsigner_end = transaction.message.account_keys.len()
                - header.num_readonly_unsigned_accounts as usize;

            let mollusk_accounts: Vec<_> = instruction
                .accounts
                .iter()
                .map(|&account_index| {
                    let account_key = transaction.message.account_keys[account_index as usize];
                    let account_idx = account_index as usize;
                    let is_signer = account_idx < header.num_required_signatures as usize;
                    let is_writable = match account_idx {
                        idx if idx < writable_signer_end => true,
                        idx if idx < all_signers_end => false,
                        idx if idx < writable_nonsigner_end => true,
                        _ => false,
                    };

                    if is_writable {
                        solana_sdk::instruction::AccountMeta::new(account_key, is_signer)
                    } else {
                        solana_sdk::instruction::AccountMeta::new_readonly(account_key, is_signer)
                    }
                })
                .collect();

            let mollusk_instruction = solana_sdk::instruction::Instruction {
                program_id,
                accounts: mollusk_accounts,
                data: instruction.data.clone(),
            };

            let result = self
                .mollusk
                .process_instruction(&mollusk_instruction, &instruction_accounts);

            for (pubkey, account) in result.resulting_accounts {
                self.accounts.borrow_mut().insert(pubkey, account);
            }

            match result.program_result {
                mollusk_svm::result::ProgramResult::Success => {
                    // Handle recover_nested: delete the nested account (Mollusk keeps closed accounts)
                    if mollusk_instruction.program_id == spl_associated_token_account::id()
                        && mollusk_instruction.data.get(0) == Some(&2)
                    {
                        if let Some(nested_account_key) = mollusk_instruction.accounts.first() {
                            self.accounts
                                .borrow_mut()
                                .remove(&nested_account_key.pubkey);
                        }
                    }
                }
                mollusk_svm::result::ProgramResult::Failure(err) => {
                    let instruction_error =
                        map_mollusk_error_to_original(&mollusk_instruction, err);
                    return Err(BanksClientError::TransactionError(
                        TransactionError::InstructionError(0, instruction_error),
                    ));
                }
                mollusk_svm::result::ProgramResult::UnknownError(_) => {
                    let instruction_error =
                        if mollusk_instruction.program_id == spl_associated_token_account::id() {
                            match mollusk_instruction.data.get(0) {
                                Some(&2) => InstructionError::IllegalOwner, // Recover nested
                                Some(&0) | Some(&1) => InstructionError::InvalidSeeds, // Create/CreateIdempotent with address mismatch
                                _ => InstructionError::ProgramFailedToComplete,
                            }
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
        Ok(self.accounts.borrow().get(&pubkey).cloned())
    }

    pub async fn get_balance(
        &self,
        pubkey: Pubkey,
    ) -> Result<u64, solana_program::program_error::ProgramError> {
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
        let mut accounts = self.accounts;

        // Add required accounts
        accounts.insert(
            payer.pubkey(),
            Account::new(1_000_000_000, 0, &solana_sdk::system_program::id()),
        );

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

        accounts.insert(
            solana_sdk::system_program::id(),
            Account::new(0, 0, &solana_sdk::native_loader::id()),
        );
        accounts.insert(
            self.token_program_id,
            Account::new(0, 0, &loader_keys::LOADER_V3),
        );

        let rent_data = bincode::serialize(&self.mollusk.sysvars.rent).expect("serialize rent");
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

        (
            MolluskBanksClient {
                mollusk: self.mollusk,
                accounts: RefCell::new(accounts),
            },
            payer,
            Hash::default(),
        )
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

/// Mollusk-based equivalent of program_test_2022
pub fn mollusk_program_test_2022(token_mint_address: Pubkey) -> MolluskProgramTest {
    let mut mollusk = Mollusk::default();
    mollusk.add_program(
        &spl_associated_token_account::id(),
        "target/deploy/pinocchio_ata_program",
        &loader_keys::LOADER_V3,
    );
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
    mollusk.add_program(
        &spl_associated_token_account::id(),
        "target/deploy/pinocchio_ata_program",
        &loader_keys::LOADER_V3,
    );
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
fn map_mollusk_error_to_original(
    instruction: &solana_sdk::instruction::Instruction,
    error: ProgramError,
) -> InstructionError {
    if instruction.program_id != spl_associated_token_account::id() {
        return InstructionError::from(u64::from(error));
    }

    let instruction_type = instruction.data.get(0);
    let is_recover_nested = instruction_type == Some(&2);
    let is_idempotent_create = instruction_type == Some(&1);

    match error {
        ProgramError::Custom(0) if instruction_type == Some(&0) => InstructionError::IllegalOwner,
        ProgramError::IllegalOwner => InstructionError::Custom(0),
        ProgramError::InvalidInstructionData => InstructionError::InvalidInstructionData,
        ProgramError::InvalidAccountData if is_recover_nested => InstructionError::IllegalOwner,
        ProgramError::InvalidAccountData if is_idempotent_create => InstructionError::InvalidSeeds,
        ProgramError::IncorrectProgramId if is_recover_nested => InstructionError::IllegalOwner,
        ProgramError::MissingRequiredSignature => InstructionError::MissingRequiredSignature,
        ProgramError::InvalidSeeds => InstructionError::InvalidSeeds,
        ProgramError::InvalidArgument if instruction.accounts.len() >= 6 => {
            let expected_ata = spl_associated_token_account_client::address::get_associated_token_address_with_program_id(
                &instruction.accounts[2].pubkey,
                &instruction.accounts[3].pubkey,
                &instruction.accounts[5].pubkey,
            );
            if instruction.accounts[1].pubkey != expected_ata {
                InstructionError::InvalidSeeds
            } else {
                InstructionError::from(u64::from(error))
            }
        }
        _ => InstructionError::from(u64::from(error)),
    }
}
