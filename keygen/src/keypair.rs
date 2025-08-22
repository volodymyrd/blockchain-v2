use rpassword::prompt_password;
use std::error;

/// Prompts user for a passphrase and then asks for confirmation to check for mistakes.
pub(crate) fn prompt_passphrase(prompt: &str) -> Result<String, Box<dyn error::Error>> {
    let passphrase = prompt_password(prompt)?;
    if !passphrase.is_empty() {
        let confirmed = prompt_password("Enter same passphrase again: ")?;
        if confirmed != passphrase {
            return Err("Passphrases did not match".into());
        }
    }
    Ok(passphrase)
}
