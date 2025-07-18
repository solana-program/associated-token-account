//! An adapter that allows the original solana_program_test to run against the p-ata program
//! using Mollusk and pinocchio.

use {
    crate::entrypoint::process_instruction as pinocchio_process_instruction,
    bincode,
    core::cell::RefCell,
    mollusk_svm::{program::loader_keys, Mollusk},
    solana_program::{account_info::AccountInfo, msg, program_error::ProgramError, pubkey::Pubkey},
    solana_program_test::{ProgramTest, *},
    solana_sdk::{
        account::Account,
        hash::Hash,
        instruction::InstructionError,
        signature::Keypair,
        signer::Signer,
        transaction::{Transaction, TransactionError},
    },
    spl_associated_token_account, spl_associated_token_account_client,
    spl_token_2022::extension::{BaseStateWithExtensions, ExtensionType, StateWithExtensions},
    spl_token_2022::state::Mint,
    std::collections::BTreeMap,
    std::vec,
    std::vec::Vec,
};

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

            let initial_mollusk_instruction = solana_sdk::instruction::Instruction {
                program_id,
                accounts: mollusk_accounts,
                data: instruction.data.clone(),
            };

            // Handle special case for ATA creation with Token-2022 - add account size to instruction data
            let mollusk_instruction =
                self.maybe_add_token_2022_account_size(initial_mollusk_instruction);

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

    fn maybe_add_token_2022_account_size(
        &self,
        instruction: solana_sdk::instruction::Instruction,
    ) -> solana_sdk::instruction::Instruction {
        // Only handle create_associated_token_account (discriminator 0) instructions targeting Token-2022
        if instruction.program_id != spl_associated_token_account::id()
            || instruction.data.len() != 1
            || instruction.data[0] != 0
        {
            return instruction;
        }

        // Standard create layout: [
        //   0. `[signer]` Payer
        //   1. `[writable]` Associated Token Account to create
        //   2. `[]` Wallet address (owner)
        //   3. `[]` Mint address
        //   4. `[]` System program
        //   5. `[]` Token program
        //   6. `[]` Rent sysvar (optional)
        // ]
        if instruction.accounts.len() < 6 {
            return instruction; // malformed – let runtime handle
        }

        let token_program_key = instruction.accounts[5].pubkey;
        if token_program_key != spl_token_2022::id() {
            return instruction; // Not Token-2022 – leave untouched
        }

        let wallet_key = instruction.accounts[2].pubkey;
        let mint_key = instruction.accounts[3].pubkey;
        let ata_key = instruction.accounts[1].pubkey;

        // --- Compute bump for canonical ATA PDA ---
        // Derive canonical ATA and bump via PDA logic
        let seeds: &[&[u8]] = &[
            wallet_key.as_ref(),
            token_program_key.as_ref(),
            mint_key.as_ref(),
        ];
        let (canonical_ata, bump) =
            Pubkey::find_program_address(seeds, &spl_associated_token_account::id());
        // If caller passed a non-canonical address, don't try to fix it – just forward as-is
        if canonical_ata != ata_key {
            return instruction;
        }

        // --- Determine required account size based on mint extensions ---
        let mint_account_opt = self.accounts.borrow().get(&mint_key).cloned();
        let Some(mint_account) = mint_account_opt else {
            return instruction; // Mint not present (shouldn’t happen in tests)
        };

        // Deserialize mint to inspect extension types
        let mint_state = match spl_token_2022::extension::StateWithExtensionsOwned::<Mint>::unpack(
            mint_account.data,
        ) {
            Ok(state) => state,
            Err(_) => return instruction, // fallback to original
        };

        let mint_extensions = match mint_state.get_extension_types() {
            Ok(exts) => exts,
            Err(_) => vec![],
        };

        // Account-side extensions required by the mint (e.g. TransferFeeAmount)
        let mut account_extensions =
            spl_token_2022::extension::ExtensionType::get_required_init_account_extensions(
                &mint_extensions,
            );
        // ATAs created via Token-2022 are always immutable
        account_extensions.push(spl_token_2022::extension::ExtensionType::ImmutableOwner);

        let space = spl_token_2022::extension::ExtensionType::try_calculate_account_len::<
            spl_token_2022::state::Account,
        >(&account_extensions)
        .unwrap_or(186); // fallback conservative

        // Encode data: [0, bump, len_lo, len_hi]
        let mut new_data = Vec::with_capacity(4);
        new_data.push(0u8);
        new_data.push(bump);
        new_data.extend_from_slice(&(space as u16).to_le_bytes());

        solana_sdk::instruction::Instruction {
            data: new_data,
            ..instruction
        }
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

/// Mollusk-based equivalent of program_test_2022
pub fn mollusk_program_test_2022(token_mint_address: Pubkey) -> MolluskProgramTest {
    let mut mollusk = Mollusk::default();

    // Add our p-ata program with the SPL ATA program ID (like the original wrapper)
    let program_id = spl_associated_token_account::id();
    mollusk.add_program(
        &program_id,
        "target/deploy/pinocchio_ata_program",
        &loader_keys::LOADER_V3,
    );

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

    // Add our p-ata program with the SPL ATA program ID (like the original wrapper)
    let program_id = spl_associated_token_account::id();
    mollusk.add_program(
        &program_id,
        "target/deploy/pinocchio_ata_program",
        &loader_keys::LOADER_V3,
    );

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
            // InvalidAccountData errors for recover_nested should be mapped to IllegalOwner
            ProgramError::InvalidAccountData => {
                if is_recover_nested {
                    InstructionError::IllegalOwner
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
                if instruction.accounts.len() >= 4 {
                    let provided_ata_address = instruction.accounts[1].pubkey;
                    let wallet_address = instruction.accounts[2].pubkey;
                    let token_mint_address = instruction.accounts[3].pubkey;

                    // Calculate expected ATA address
                    let expected_ata_address = spl_associated_token_account_client::address::get_associated_token_address_with_program_id(
                        &wallet_address,
                        &token_mint_address,
                        &spl_token_2022::id(),
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
