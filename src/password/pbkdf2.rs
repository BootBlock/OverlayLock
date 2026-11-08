//! PBKDF2-HMAC-SHA256 and random bytes from Windows CNG (`bcrypt.dll`), so no cryptography crate
//! is needed.

use windows::Win32::Security::Cryptography::{
    BCRYPT_ALG_HANDLE, BCRYPT_ALG_HANDLE_HMAC_FLAG, BCRYPT_SHA256_ALGORITHM,
    BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptCloseAlgorithmProvider, BCryptDeriveKeyPBKDF2,
    BCryptGenRandom, BCryptOpenAlgorithmProvider,
};

/// An open HMAC-SHA256 provider, which derives PBKDF2 keys. It is opened once and used for every
/// key, because opening a provider costs far more than one derivation at a low iteration count.
#[derive(Debug)]
pub struct Pbkdf2 {
    algorithm: BCRYPT_ALG_HANDLE,
}

impl Pbkdf2 {
    /// Opens the HMAC-SHA256 provider.
    pub fn open() -> windows::core::Result<Self> {
        let mut algorithm = BCRYPT_ALG_HANDLE::default();
        // SAFETY: `algorithm` is writable; the algorithm name is a static wide string, and `None`
        // asks for the default implementation.
        unsafe {
            BCryptOpenAlgorithmProvider(
                &raw mut algorithm,
                BCRYPT_SHA256_ALGORITHM,
                None,
                BCRYPT_ALG_HANDLE_HMAC_FLAG,
            )
        }
        .ok()?;
        Ok(Self { algorithm })
    }

    /// Derives `output.len()` bytes from `password` and `salt` with `iterations` rounds.
    pub fn derive(
        &self,
        password: &[u8],
        salt: &[u8],
        iterations: u32,
        output: &mut [u8],
    ) -> windows::core::Result<()> {
        // SAFETY: the provider is open until `drop`, it was opened with the HMAC flag as PBKDF2
        // needs, and every slice outlives the call.
        unsafe {
            BCryptDeriveKeyPBKDF2(
                self.algorithm,
                Some(password),
                Some(salt),
                u64::from(iterations),
                output,
                0,
            )
        }
        .ok()
    }
}

impl Drop for Pbkdf2 {
    fn drop(&mut self) {
        // SAFETY: the provider was opened by `open` and is closed once, here.
        let _ = unsafe { BCryptCloseAlgorithmProvider(self.algorithm, 0) };
    }
}

/// Fills `buffer` with random bytes from the system's preferred random number generator.
pub fn fill_random(buffer: &mut [u8]) -> windows::core::Result<()> {
    // SAFETY: `buffer` is writable for its length; no handle is needed with this flag.
    unsafe { BCryptGenRandom(None, buffer, BCRYPT_USE_SYSTEM_PREFERRED_RNG) }.ok()
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;

    use super::{Pbkdf2, fill_random};

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().fold(String::new(), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
    }

    /// The published PBKDF2-HMAC-SHA256 test vectors for "password" and "salt".
    #[test]
    fn derives_the_published_test_vectors() {
        let pbkdf2 = Pbkdf2::open().expect("the provider opens");
        for (iterations, expected) in [
            (
                1,
                "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b",
            ),
            (
                2,
                "ae4d0c95af6b46d32d0adff928f06dd02a303f8ef3c251dfd6e2d85a95474c43",
            ),
            (
                4096,
                "c5e478d59288c841aa530db6845c4c8d962893a001ce4e11a4963873aa98134a",
            ),
        ] {
            let mut output = [0u8; 32];
            pbkdf2
                .derive(b"password", b"salt", iterations, &mut output)
                .expect("the key derives");
            assert_eq!(hex(&output), expected, "{iterations} iterations");
        }
    }

    #[test]
    fn random_bytes_differ_between_calls() {
        let (mut first, mut second) = ([0u8; 16], [0u8; 16]);
        fill_random(&mut first).expect("random bytes");
        fill_random(&mut second).expect("random bytes");
        assert_ne!(first, second);
    }
}
