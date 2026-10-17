/// Prefijo visible de una API key.
/// Formato: `sk-live-xxxxxxxx` (16 caracteres).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ApiKeyPrefix {
    pub prefix: String,
}

impl ApiKeyPrefix {
    pub fn new(prefix: impl Into<String>) -> Result<Self, &'static str> {
        let prefix = prefix.into();
        if !prefix.starts_with("sk-live-") || prefix.len() != 16 || !prefix.is_ascii() {
            return Err("prefix must be 16 chars starting with 'sk-live-'");
        }
        Ok(Self { prefix })
    }

    /// Devuelve la versión enmascarada para mostrar en dashboards:
    /// `sk-live-abc1****`
    pub fn masked(&self) -> String {
        format!("{}****", &self.prefix[..12])
    }
}

impl std::fmt::Display for ApiKeyPrefix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.masked())
    }
}

impl<'de> serde::Deserialize<'de> for ApiKeyPrefix {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let prefix = String::deserialize(deserializer)?;
        ApiKeyPrefix::new(prefix).map_err(serde::de::Error::custom)
    }
}
