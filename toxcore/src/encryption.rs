use crate::{
    error::*,
    sys::{ToxEncryptSaveApi, ToxEncryptSaveImpl},
};

use toxcore_sys::*;

pub struct PassKey {
    inner: PassKeyImpl<ToxEncryptSaveImpl>,
}

impl PassKey {
    pub fn new(passphrase: &str) -> Result<PassKey, KeyDerivationError> {
        Ok(PassKey {
            inner: PassKeyImpl::new(ToxEncryptSaveImpl {}, passphrase)?,
        })
    }

    pub fn from_encrypted_slice(
        passphrase: &str,
        input: &[u8],
    ) -> Result<PassKey, KeyDerivationError> {
        Ok(PassKey {
            inner: PassKeyImpl::from_encrypted_slice(ToxEncryptSaveImpl {}, passphrase, input)?,
        })
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        self.inner.encrypt(plaintext)
    }

    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, DecryptionError> {
        self.inner.decrypt(ciphertext)
    }
}

struct PassKeyImpl<Api: ToxEncryptSaveApi> {
    api: Api,
    key: *const Tox_Pass_Key,
}

unsafe impl<Api: ToxEncryptSaveApi> Send for PassKeyImpl<Api> {}

impl<Api: ToxEncryptSaveApi> PassKeyImpl<Api> {
    fn new(api: Api, passphrase: &str) -> Result<PassKeyImpl<Api>, KeyDerivationError> {
        let mut err = TOX_ERR_KEY_DERIVATION_OK;
        unsafe {
            let key = api.pass_key_derive(passphrase.as_ptr(), passphrase.len() as u64, &mut err);

            if err != TOX_ERR_KEY_DERIVATION_OK {
                return Err(KeyDerivationError);
            }

            Ok(PassKeyImpl { api, key })
        }
    }

    fn from_encrypted_slice(
        api: Api,
        passphrase: &str,
        input: &[u8],
    ) -> Result<PassKeyImpl<Api>, KeyDerivationError> {
        if input.len() < TOX_PASS_ENCRYPTION_EXTRA_LENGTH as usize {
            return Err(KeyDerivationError);
        }

        unsafe {
            let mut err = TOX_ERR_GET_SALT_OK;
            let mut salt = Vec::with_capacity(TOX_PASS_SALT_LENGTH as usize);
            api.get_salt(input.as_ptr(), salt.as_mut_ptr(), &mut err);
            salt.set_len(TOX_PASS_SALT_LENGTH as usize);

            if err != TOX_ERR_GET_SALT_OK {
                return Err(KeyDerivationError);
            }

            let mut err = TOX_ERR_KEY_DERIVATION_OK;
            let key = api.pass_key_derive_with_salt(
                passphrase.as_ptr(),
                passphrase.len() as u64,
                salt.as_ptr(),
                &mut err,
            );
            if err != TOX_ERR_KEY_DERIVATION_OK {
                return Err(KeyDerivationError);
            }

            Ok(PassKeyImpl { api, key })
        }
    }

    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        unsafe {
            let output_len = plaintext.len() + TOX_PASS_ENCRYPTION_EXTRA_LENGTH as usize;
            let mut output = Vec::with_capacity(output_len);

            let mut err = TOX_ERR_ENCRYPTION_OK;
            self.api.pass_key_encrypt(
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

    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, DecryptionError> {
        unsafe {
            let output_len = ciphertext.len() - TOX_PASS_ENCRYPTION_EXTRA_LENGTH as usize;
            let mut output = Vec::with_capacity(output_len);

            let mut err = TOX_ERR_DECRYPTION_OK;
            self.api.pass_key_decrypt(
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

#[cfg(test)]
mod tests {
    // FIXME
}
