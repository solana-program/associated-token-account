#![cfg(any(test, feature = "std"))]
#![cfg_attr(feature = "std", allow(dead_code, unused_imports))]

use {
    pinocchio_ata_program::test_utils::shared_constants::TOKEN_ACCOUNT_SIZE,
    solana_account::Account,
    solana_instruction::AccountMeta,
    solana_pubkey::Pubkey,
    std::{
        collections::HashMap,
        format,
        string::{String, ToString},
        vec::Vec,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountType {
    Payer,
    AtaAccount,
    WalletOwner,
    Mint,
    SystemProgram,
    TokenProgram,
    RentSysvar,
    Generic(usize),
}

impl std::fmt::Display for AccountType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            AccountType::Payer => write!(f, "Payer"),
            AccountType::AtaAccount => write!(f, "ATA Account"),
            AccountType::WalletOwner => write!(f, "Wallet/Owner"),
            AccountType::Mint => write!(f, "Mint"),
            AccountType::SystemProgram => write!(f, "System Program"),
            AccountType::TokenProgram => write!(f, "Token Program"),
            AccountType::RentSysvar => write!(f, "Rent Sysvar"),
            AccountType::Generic(pos) => write!(f, "Account #{}", pos),
        }
    }
}

impl AccountType {
    pub fn from_position(pos: usize) -> Self {
        match pos {
            0 => AccountType::Payer,
            1 => AccountType::AtaAccount,
            2 => AccountType::WalletOwner,
            3 => AccountType::Mint,
            4 => AccountType::SystemProgram,
            5 => AccountType::TokenProgram,
            6 => AccountType::RentSysvar,
            _ => AccountType::Generic(pos),
        }
    }
}

/// Calculate data differences between two byte arrays with a configurable limit
fn calculate_data_differences(
    left_data: &[u8],
    right_data: &[u8],
    max_differences: usize,
) -> Vec<DataDifference> {
    let mut differences = Vec::new();
    let max_len = left_data.len().max(right_data.len());

    for i in 0..max_len.min(max_differences) {
        let left_byte = left_data.get(i).copied();
        let right_byte = right_data.get(i).copied();

        if left_byte != right_byte {
            differences.push(DataDifference {
                offset: i,
                left_value: left_byte,
                right_value: right_byte,
            });
        }
    }

    differences
}

#[derive(Debug, Clone)]
pub struct AccountComparison {
    pub account_type: AccountType,
    pub position: usize,
    pub data_match: bool,
    pub lamports_match: bool,
    pub owner_match: bool,
    pub details: ComparisonDetails,
}

impl AccountComparison {
    pub fn is_equivalent(&self) -> bool {
        match self.account_type {
            AccountType::AtaAccount => {
                // For ATA accounts, we check behavioral equivalence
                self.details.behavioral_equivalent
            }
            _ => {
                // For other accounts, all fields must match
                self.data_match && self.lamports_match && self.owner_match
            }
        }
    }

    pub fn has_issues(&self) -> bool {
        !self.is_equivalent()
    }
}

#[derive(Debug, Clone)]
pub struct ComparisonDetails {
    pub data_differences: Vec<DataDifference>,
    pub lamports_diff: Option<i64>,
    pub owner_diff: Option<(Pubkey, Pubkey)>,
    pub behavioral_equivalent: bool,
    pub token_analysis: Option<TokenAccountAnalysis>,
}

#[derive(Debug, Clone)]
pub struct DataDifference {
    pub offset: usize,
    pub left_value: Option<u8>,
    pub right_value: Option<u8>,
}

#[derive(Debug, Clone)]
pub struct TokenAccountAnalysis {
    pub amount_match: bool,
    pub state_match: bool,
    pub delegate_match: bool,
    pub delegated_amount_match: bool,
    pub left_amount: u64,
    pub right_amount: u64,
    pub left_state: u8,
    pub right_state: u8,
}

pub trait AccountComparator {
    fn compare(
        &self,
        left: &Account,
        right: &Account,
        account_type: &AccountType,
        position: usize,
    ) -> AccountComparison;
}

pub struct TokenAccountComparator;

impl AccountComparator for TokenAccountComparator {
    fn compare(
        &self,
        left: &Account,
        right: &Account,
        account_type: &AccountType,
        position: usize,
    ) -> AccountComparison {
        let data_match = left.data == right.data;
        let lamports_match = left.lamports == right.lamports;
        let owner_match = left.owner == right.owner;

        let mut details = ComparisonDetails {
            data_differences: Vec::new(),
            lamports_diff: if lamports_match {
                None
            } else {
                Some(left.lamports as i64 - right.lamports as i64)
            },
            owner_diff: if owner_match {
                None
            } else {
                Some((left.owner, right.owner))
            },
            behavioral_equivalent: false,
            token_analysis: None,
        };

        if *account_type == AccountType::AtaAccount
            && left.data.len() >= TOKEN_ACCOUNT_SIZE
            && right.data.len() >= TOKEN_ACCOUNT_SIZE
        {
            let analysis = self.analyze_token_account_structure(&left.data, &right.data);
            details.behavioral_equivalent = data_match && lamports_match && owner_match;
            details.token_analysis = Some(analysis);
        }

        if !data_match {
            details.data_differences = self.calculate_data_differences(&left.data, &right.data);
        }

        AccountComparison {
            account_type: account_type.clone(),
            position,
            data_match,
            lamports_match,
            owner_match,
            details,
        }
    }
}

impl TokenAccountComparator {
    fn analyze_token_account_structure(
        &self,
        left_data: &[u8],
        right_data: &[u8],
    ) -> TokenAccountAnalysis {
        let left_amount = u64::from_le_bytes(left_data[64..72].try_into().unwrap_or([0u8; 8]));
        let right_amount = u64::from_le_bytes(right_data[64..72].try_into().unwrap_or([0u8; 8]));

        let left_state = left_data[108];
        let right_state = right_data[108];

        let left_delegate = &left_data[72..104];
        let right_delegate = &right_data[72..104];

        let left_delegated = u64::from_le_bytes(left_data[104..112].try_into().unwrap_or([0u8; 8]));
        let right_delegated =
            u64::from_le_bytes(right_data[104..112].try_into().unwrap_or([0u8; 8]));

        TokenAccountAnalysis {
            amount_match: left_amount == right_amount,
            state_match: left_state == right_state,
            delegate_match: left_delegate == right_delegate,
            delegated_amount_match: left_delegated == right_delegated,
            left_amount,
            right_amount,
            left_state,
            right_state,
        }
    }

    fn calculate_data_differences(
        &self,
        left_data: &[u8],
        right_data: &[u8],
    ) -> Vec<DataDifference> {
        calculate_data_differences(left_data, right_data, 100)
    }
}

pub struct StandardAccountComparator;

impl AccountComparator for StandardAccountComparator {
    fn compare(
        &self,
        left: &Account,
        right: &Account,
        account_type: &AccountType,
        position: usize,
    ) -> AccountComparison {
        let data_match = left.data == right.data;
        let lamports_match = left.lamports == right.lamports;
        let owner_match = left.owner == right.owner;

        let mut details = ComparisonDetails {
            data_differences: Vec::new(),
            lamports_diff: if lamports_match {
                None
            } else {
                Some(left.lamports as i64 - right.lamports as i64)
            },
            owner_diff: if owner_match {
                None
            } else {
                Some((left.owner, right.owner))
            },
            behavioral_equivalent: data_match && lamports_match && owner_match,
            token_analysis: None,
        };

        if !data_match {
            details.data_differences = self.calculate_data_differences(&left.data, &right.data);
        }

        AccountComparison {
            account_type: account_type.clone(),
            position,
            data_match,
            lamports_match,
            owner_match,
            details,
        }
    }
}

impl StandardAccountComparator {
    fn calculate_data_differences(
        &self,
        left_data: &[u8],
        right_data: &[u8],
    ) -> Vec<DataDifference> {
        calculate_data_differences(left_data, right_data, 20)
    }
}

pub struct AccountComparisonService {
    token_comparator: TokenAccountComparator,
    standard_comparator: StandardAccountComparator,
}

impl AccountComparisonService {
    pub fn new() -> Self {
        Self {
            token_comparator: TokenAccountComparator,
            standard_comparator: StandardAccountComparator,
        }
    }

    pub fn compare_account_states(
        &self,
        left_accounts: &[(Pubkey, Account)],
        right_accounts: &[(Pubkey, Account)],
        left_instruction_accounts: &[AccountMeta],
        right_instruction_accounts: &[AccountMeta],
    ) -> Vec<AccountComparison> {
        let left_map: HashMap<&Pubkey, &Account> =
            left_accounts.iter().map(|(k, v)| (k, v)).collect();
        let right_map: HashMap<&Pubkey, &Account> =
            right_accounts.iter().map(|(k, v)| (k, v)).collect();

        let mut results = Vec::new();
        let max_accounts = left_instruction_accounts
            .len()
            .max(right_instruction_accounts.len());

        for i in 0..max_accounts {
            let left_meta = left_instruction_accounts.get(i);
            let right_meta = right_instruction_accounts.get(i);

            match (left_meta, right_meta) {
                (Some(left_meta), Some(right_meta)) => {
                    // Only compare writable accounts (the ones that change)
                    if left_meta.is_writable || right_meta.is_writable {
                        let account_type = self.get_account_type_by_position(i);

                        let left_account = left_map.get(&left_meta.pubkey);
                        let right_account = right_map.get(&right_meta.pubkey);

                        match (left_account, right_account) {
                            (Some(&left_acc), Some(&right_acc)) => {
                                let comparison = self.compare_single_account(
                                    left_acc,
                                    right_acc,
                                    &account_type,
                                    i,
                                );
                                results.push(comparison);
                            }
                            _ => {
                                // Handle missing accounts
                                results
                                    .push(self.create_missing_account_comparison(i, &account_type));
                            }
                        }
                    }
                }
                _ => {
                    // Handle mismatched instruction lengths - this indicates SysvarRent differences
                    if let Some(meta) = left_meta.or(right_meta) {
                        let account_type = self.get_account_type_by_position(i);
                        results.push(self.create_instruction_mismatch_comparison(
                            i,
                            &account_type,
                            meta.pubkey,
                        ));
                    }
                }
            }
        }

        results
    }

    fn compare_single_account(
        &self,
        left: &Account,
        right: &Account,
        account_type: &AccountType,
        position: usize,
    ) -> AccountComparison {
        match account_type {
            AccountType::AtaAccount => {
                self.token_comparator
                    .compare(left, right, account_type, position)
            }
            _ => self
                .standard_comparator
                .compare(left, right, account_type, position),
        }
    }

    fn create_missing_account_comparison(
        &self,
        position: usize,
        account_type: &AccountType,
    ) -> AccountComparison {
        AccountComparison {
            account_type: account_type.clone(),
            position,
            data_match: false,
            lamports_match: false,
            owner_match: false,
            details: ComparisonDetails {
                data_differences: Vec::new(),
                lamports_diff: None,
                owner_diff: None,
                behavioral_equivalent: false,
                token_analysis: None,
            },
        }
    }

    fn create_instruction_mismatch_comparison(
        &self,
        position: usize,
        account_type: &AccountType,
        pubkey: Pubkey,
    ) -> AccountComparison {
        // Check if this is a SysvarRent difference (expected optimization)
        let is_sysvar_rent = pubkey.to_string() == "SysvarRent111111111111111111111111111111111";

        AccountComparison {
            account_type: account_type.clone(),
            position,
            data_match: is_sysvar_rent, // SysvarRent differences are expected
            lamports_match: is_sysvar_rent,
            owner_match: is_sysvar_rent,
            details: ComparisonDetails {
                data_differences: Vec::new(),
                lamports_diff: None,
                owner_diff: None,
                behavioral_equivalent: is_sysvar_rent,
                token_analysis: None,
            },
        }
    }

    fn get_account_type_by_position(&self, pos: usize) -> AccountType {
        AccountType::from_position(pos)
    }

    pub fn all_accounts_equivalent(&self, comparisons: &[AccountComparison]) -> bool {
        comparisons.iter().all(|c| c.is_equivalent())
    }

    pub fn has_expected_differences(&self, comparisons: &[AccountComparison]) -> bool {
        comparisons
            .iter()
            .any(|c| c.account_type == AccountType::RentSysvar && c.is_equivalent())
    }
}

pub struct ComparisonFormatter;

impl ComparisonFormatter {
    pub fn new() -> Self {
        Self
    }

    pub fn format_comparison_results(&self, comparisons: &[AccountComparison]) -> Vec<String> {
        let mut output = Vec::new();

        for comparison in comparisons {
            if comparison.has_issues() {
                output.push(format!(
                    "\n📋 {} (Position {})",
                    comparison.account_type, comparison.position
                ));

                if !comparison.data_match {
                    output.push(format!(
                        "  📊 Data: Different ({} differences)",
                        comparison.details.data_differences.len()
                    ));

                    if let Some(ref token_analysis) = comparison.details.token_analysis {
                        output.extend(self.format_token_analysis(token_analysis));
                        // Also show byte differences for token accounts to debug issues
                        if !comparison.details.data_differences.is_empty() {
                            output.extend(
                                self.format_raw_differences(&comparison.details.data_differences),
                            );
                        }
                    } else {
                        output.extend(
                            self.format_raw_differences(&comparison.details.data_differences),
                        );
                    }
                }

                if !comparison.lamports_match {
                    if let Some(diff) = comparison.details.lamports_diff {
                        output.push("  ❌ Lamports: MISMATCH!".to_string());
                        output.push(format!("     Difference: {} lamports", diff));
                    }
                }

                if !comparison.owner_match {
                    if let Some((left, right)) = comparison.details.owner_diff {
                        output.push("  ❌ Owner: MISMATCH!".to_string());
                        output.push(format!("     Left: {}", left));
                        output.push(format!("     Right: {}", right));
                    }
                }
            }
        }

        output
    }

    fn format_token_analysis(&self, analysis: &TokenAccountAnalysis) -> Vec<String> {
        let mut output = Vec::new();
        output.push("     🔍 Token Account Analysis:".to_string());

        if !analysis.amount_match {
            output.push(format!(
                "       ❌ Amount differs: Left={}, Right={}",
                analysis.left_amount, analysis.right_amount
            ));
        } else {
            output.push(format!("       ✅ Amount: {} tokens", analysis.left_amount));
        }

        if !analysis.state_match {
            output.push(format!(
                "       ❌ State differs: Left={}, Right={}",
                analysis.left_state, analysis.right_state
            ));
        } else {
            output.push(format!(
                "       ✅ State: {} (correct)",
                analysis.left_state
            ));
        }

        if !analysis.delegate_match {
            output.push("       ❌ Delegate differs - structural issue!".to_string());
        } else {
            output.push("       ✅ Delegate: Identical".to_string());
        }

        if !analysis.delegated_amount_match {
            output.push("       ❌ Delegated amount differs - structural issue!".to_string());
        } else {
            output.push("       ✅ Delegated amount: Identical".to_string());
        }

        output
    }

    fn format_raw_differences(&self, differences: &[DataDifference]) -> Vec<String> {
        let mut output = Vec::new();
        output.push("     📊 Byte-by-byte differences:".to_string());

        for diff in differences.iter().take(20) {
            output.push(format!(
                "       Offset {}: Left={:02x?}, Right={:02x?}",
                diff.offset, diff.left_value, diff.right_value
            ));
        }

        if differences.len() > 20 {
            output.push(format!(
                "       ... and {} more differences",
                differences.len() - 20
            ));
        }

        output.push(format!(
            "     Total differences: {} bytes",
            differences.len()
        ));
        output
    }
}
