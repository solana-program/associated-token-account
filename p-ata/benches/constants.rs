//! Constants used throughout the benchmark code
//!
//! This module centralizes all magic numbers to improve readability and maintainability.
//! Each constant is documented with its purpose and why that specific value is used.

/// Lamport amounts used in tests
pub mod lamports {
    /// Standard payer account balance for tests
    pub const ONE_SOL: u64 = 1_000_000_000; // 1 SOL

    pub const TOKEN_ACCOUNT_RENT_EXEMPT: u64 = 2_000_000;
}

/// Account data sizes used in tests
pub mod account_sizes {
    /// Standard SPL token account size
    ///
    /// Fixed size for all SPL token accounts as defined by the token program
    pub const TOKEN_ACCOUNT_SIZE: usize = 165;

    /// Standard mint account size
    ///
    /// Base size for mint accounts without extensions
    pub const MINT_ACCOUNT_SIZE: usize = 82;

    /// Multisig account size
    ///
    /// Size needed for multisig accounts with multiple signers
    pub const MULTISIG_ACCOUNT_SIZE: usize = 355;
}
