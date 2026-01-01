//! Utility functions

use base64::Engine;

/// Simple encryption for storing passwords (not cryptographically secure, just obfuscation)
/// In production, use a proper secrets manager or encryption library
pub fn encrypt_password(password: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(password.as_bytes())
}

/// Decrypt password
pub fn decrypt_password(encrypted: &str) -> Option<String> {
    base64::engine::general_purpose::STANDARD
        .decode(encrypted)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
}

/// Format file size in human-readable format
pub fn format_size(size: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if size >= TB {
        format!("{:.2} TB", size as f64 / TB as f64)
    } else if size >= GB {
        format!("{:.2} GB", size as f64 / GB as f64)
    } else if size >= MB {
        format!("{:.2} MB", size as f64 / MB as f64)
    } else if size >= KB {
        format!("{:.2} KB", size as f64 / KB as f64)
    } else {
        format!("{} B", size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_encryption() {
        let password = "my_secret_password";
        let encrypted = encrypt_password(password);
        let decrypted = decrypt_password(&encrypted);
        assert_eq!(decrypted, Some(password.to_string()));
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(1024 * 1024), "1.00 MB");
        assert_eq!(format_size(1024 * 1024 * 1024), "1.00 GB");
        assert_eq!(format_size(1024 * 1024 * 1024 * 1024), "1.00 TB");
    }
}
