use crate::application::ports::KeyGenerator;
use crate::domain::entities::GeneratedApiKey;
use crate::domain::errors::KeyServiceError;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;

pub struct HmacApiKeyGenerator {
    pepper: Vec<u8>,
}
impl HmacApiKeyGenerator {
    pub fn new(pepper: String) -> Result<Self, KeyServiceError> {
        if pepper.len() < 32 {
            return Err(KeyServiceError::Configuration(
                "API_KEY_PEPPER must contain at least 32 characters".into(),
            ));
        }
        Ok(Self {
            pepper: pepper.into_bytes(),
        })
    }
}
impl KeyGenerator for HmacApiKeyGenerator {
    fn generate(&self) -> Result<GeneratedApiKey, KeyServiceError> {
        let mut bytes = [0_u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        let secret = URL_SAFE_NO_PAD.encode(bytes);
        let prefix = format!("sk-live-{}", &secret[..8]);
        let plaintext = format!("{prefix}{secret}");
        let hash = self.hash(&plaintext)?;
        Ok(GeneratedApiKey {
            plaintext,
            hash,
            prefix,
        })
    }
    fn hash(&self, plaintext: &str) -> Result<String, KeyServiceError> {
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.pepper)
            .map_err(|_| KeyServiceError::Configuration("invalid API_KEY_PEPPER".into()))?;
        mac.update(plaintext.as_bytes());
        Ok(hex::encode(mac.finalize().into_bytes()))
    }
}
