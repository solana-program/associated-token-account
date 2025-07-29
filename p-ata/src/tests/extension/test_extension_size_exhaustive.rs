#[allow(unexpected_cfgs)]
use {crate::size::calculate_account_size_from_mint_extensions, std::vec::Vec};

#[cfg(feature = "test-debug")]
use std::eprintln;

use crate::tests::extension::test_extension_utils::{
    calculate_expected_ata_data_size, create_mint_data_with_extensions, get_extensions_by_category,
    is_valid_extension_combination, ExtensionCategory,
};

/// Exhaustively test every combination of supported mint extensions
#[test]
fn exhaustive_extension_combinations() {
    // Get all extension types
    let extensions = get_extensions_by_category(ExtensionCategory::Include);

    let total = 1usize << extensions.len();

    for mask in 0..total {
        let mut combo = Vec::new();
        for (idx, ext) in extensions.iter().enumerate() {
            if (mask >> idx) & 1 == 1 {
                combo.push(*ext);
            }
        }

        if !is_valid_extension_combination(&combo) {
            continue;
        }

        #[cfg(feature = "test-debug")]
        {
            eprintln!("combo: {:?}", combo);
        }

        // Create mint data
        let mint_data = create_mint_data_with_extensions(&combo);
        let inline_size = calculate_account_size_from_mint_extensions(&mint_data);

        // Use shared account size calculation
        let expected_size = calculate_expected_ata_data_size(&combo);

        #[cfg(feature = "test-debug")]
        {
            eprintln!("expected_size: {:?}", expected_size);
        }

        #[cfg(feature = "test-debug")]
        {
            eprintln!("inline_size: {:?}", inline_size);
        }

        assert_eq!(
            inline_size,
            Some(expected_size),
            "Mismatch for extensions {:?}",
            combo
        );
    }
}
