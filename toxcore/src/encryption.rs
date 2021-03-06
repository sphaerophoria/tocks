use crate::{error::*, sys};

use toxcore_sys::*;

pub struct PassKey {
    key: *mut Tox_Pass_Key,
}

unsafe impl Send for PassKey {}

impl PassKey {
    pub fn new(passphrase: &str) -> Result<PassKey, KeyDerivationError> {
        let mut err = TOX_ERR_KEY_DERIVATION_OK;
        unsafe {
            let key =
                sys::tox_pass_key_derive(passphrase.as_ptr(), passphrase.len() as u64, &mut err);

            if err != TOX_ERR_KEY_DERIVATION_OK {
                return Err(KeyDerivationError);
            }

            Ok(PassKey { key })
        }
    }

    pub fn from_encrypted_slice(
        passphrase: &str,
        input: &[u8],
    ) -> Result<PassKey, KeyDerivationError> {
        if input.len() < TOX_PASS_ENCRYPTION_EXTRA_LENGTH as usize {
            return Err(KeyDerivationError);
        }

        unsafe {
            let mut err = TOX_ERR_GET_SALT_OK;
            let mut salt = Vec::with_capacity(TOX_PASS_SALT_LENGTH as usize);
            sys::tox_get_salt(input.as_ptr(), salt.as_mut_ptr(), &mut err);
            salt.set_len(TOX_PASS_SALT_LENGTH as usize);

            if err != TOX_ERR_GET_SALT_OK {
                return Err(KeyDerivationError);
            }

            let mut err = TOX_ERR_KEY_DERIVATION_OK;
            let key = sys::tox_pass_key_derive_with_salt(
                passphrase.as_ptr(),
                passphrase.len() as u64,
                salt.as_ptr(),
                &mut err,
            );
            if err != TOX_ERR_KEY_DERIVATION_OK {
                return Err(KeyDerivationError);
            }

            Ok(PassKey { key })
        }
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        unsafe {
            let output_len = plaintext.len() + TOX_PASS_ENCRYPTION_EXTRA_LENGTH as usize;
            let mut output = Vec::with_capacity(output_len);

            let mut err = TOX_ERR_ENCRYPTION_OK;
            sys::tox_pass_key_encrypt(
                self.key,
                plaintext.as_ptr(),
                plaintext.len() as u64,
                output.as_mut_ptr(),
                &mut err,
            );
            if err != TOX_ERR_ENCRYPTION_OK {
                return Err(EncryptionError);
            }

            output.set_len(output_len);

            Ok(output)
        }
    }

    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, DecryptionError> {
        unsafe {
            let output_len = ciphertext.len() - TOX_PASS_ENCRYPTION_EXTRA_LENGTH as usize;
            let mut output = Vec::with_capacity(output_len);

            let mut err = TOX_ERR_DECRYPTION_OK;
            sys::tox_pass_key_decrypt(
                self.key,
                ciphertext.as_ptr(),
                ciphertext.len() as u64,
                output.as_mut_ptr(),
                &mut err,
            );

            if err != TOX_ERR_DECRYPTION_OK {
                return Err(DecryptionError);
            }

            output.set_len(output_len);

            Ok(output)
        }
    }
}

impl Drop for PassKey {
    fn drop(&mut self) {
        unsafe {
            sys::tox_pass_key_free(self.key);
        }
    }
}

#[cfg(test)]
mod tests {
    // FIXME
}
