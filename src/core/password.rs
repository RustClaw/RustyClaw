use anyhow::{anyhow, Result};
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use rand::Rng;

/// Hash a password using Argon2id
pub fn hash_password(password: &str) -> Result<String> {
    let argon2 = Argon2::default();

    // Generate a random salt
    let mut rng = rand::thread_rng();
    let mut salt_bytes = [0u8; 16];
    rng.fill(&mut salt_bytes);
    let salt =
        SaltString::encode_b64(&salt_bytes).map_err(|e| anyhow!("Failed to encode salt: {}", e))?;

    // Hash the password
    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow!("Failed to hash password: {}", e))?;

    Ok(password_hash.to_string())
}

/// Verify a password against its hash
pub fn verify_password(password: &str, hash: &str) -> Result<bool> {
    let parsed_hash =
        PasswordHash::new(hash).map_err(|e| anyhow!("Failed to parse password hash: {}", e))?;

    let argon2 = Argon2::default();
    match argon2.verify_password(password.as_bytes(), &parsed_hash) {
        Ok(_) => Ok(true),
        Err(argon2::password_hash::Error::Password) => Ok(false),
        Err(e) => Err(anyhow!("Password verification error: {}", e)),
    }
}

/// Check if a string is already hashed (starts with Argon2id prefix)
pub fn is_hashed(s: &str) -> bool {
    s.starts_with("$argon2id$")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_password_creates_valid_hash() {
        let password = "test_password_123";
        let hash = hash_password(password).expect("Failed to hash password");

        assert!(hash.starts_with("$argon2id$"));
        assert!(!hash.contains(password));
    }

    #[test]
    fn test_verify_password_with_correct_password() {
        let password = "test_password_123";
        let hash = hash_password(password).expect("Failed to hash password");

        let result = verify_password(password, &hash).expect("Failed to verify password");
        assert!(result);
    }

    #[test]
    fn test_verify_password_with_incorrect_password() {
        let password = "test_password_123";
        let wrong_password = "wrong_password";
        let hash = hash_password(password).expect("Failed to hash password");

        let result = verify_password(wrong_password, &hash).expect("Failed to verify password");
        assert!(!result);
    }

    #[test]
    fn test_is_hashed_detects_argon2_hash() {
        let hash = "$argon2id$v=19$m=19456,t=2,p=1$test$hash";
        assert!(is_hashed(hash));
    }

    #[test]
    fn test_is_hashed_rejects_plaintext() {
        let plaintext = "my_password";
        assert!(!is_hashed(plaintext));
    }

    #[test]
    fn test_different_passwords_produce_different_hashes() {
        let password1 = "password1";
        let password2 = "password2";

        let hash1 = hash_password(password1).expect("Failed to hash password");
        let hash2 = hash_password(password2).expect("Failed to hash password");

        assert_ne!(hash1, hash2);
    }
}
