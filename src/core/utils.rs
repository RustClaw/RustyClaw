use rand::distributions::Alphanumeric;
use rand::Rng;

/// Generate a random alphanumeric code of specified length (uppercase)
pub fn generate_code(length: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect::<String>()
        .to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_code_creates_correct_length() {
        let code = generate_code(8);
        assert_eq!(code.len(), 8);
    }

    #[test]
    fn test_generate_code_is_uppercase_alphanumeric() {
        let code = generate_code(20);
        assert!(code
            .chars()
            .all(|c| c.is_alphanumeric() && (c.is_numeric() || c.is_uppercase())));
    }

    #[test]
    fn test_generate_code_different_calls_produce_different_codes() {
        let code1 = generate_code(8);
        let code2 = generate_code(8);
        // Very unlikely to be equal (probability ~1 in 36^8)
        assert_ne!(code1, code2);
    }
}
