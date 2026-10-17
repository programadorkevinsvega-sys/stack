use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const DEFAULT_AWS_ENVIRONMENT: &str = "dev";

/// Configuración de la aplicación cargada desde variables de entorno.
/// Todos los servicios usan esta struct base (o la componen con campos extra).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    // ── Postgres ──
    #[serde(alias = "DATABASE_URL")]
    pub database_url: String,
    pub database_max_connections: Option<u32>,
    pub database_acquire_timeout_ms: Option<u64>,
    pub database_idle_timeout_ms: Option<u64>,

    // ── Redis ──
    #[serde(alias = "REDIS_URL")]
    pub redis_url: String,
    pub redis_pool_size: Option<usize>,
    pub redis_timeout_ms: Option<u64>,

    // ── NATS ──
    #[serde(alias = "NATS_URL")]
    pub nats_url: String,

    // ── Server ──
    pub http_port: Option<u16>,
    pub grpc_port: Option<u16>,
    pub log_level: Option<String>,

    // ── AWS ──
    #[serde(alias = "AWS_REGION")]
    pub aws_region: Option<String>,
    pub aws_endpoint_url: Option<String>,

    // ── Security ──
    pub api_key_pepper: Option<String>,
    pub jwt_signing_secret: Option<String>,
    pub service_jwt_secret: Option<String>,

    // ── Stripe ──
    pub stripe_secret_key: Option<String>,
    pub stripe_webhook_secret: Option<String>,

    // ── OTEL ──
    pub otel_enabled: Option<bool>,
    pub otel_exporter_otlp_endpoint: Option<String>,
    pub otel_service_name: Option<String>,
}

impl AppConfig {
    /// Carga la configuración desde variables de entorno.
    /// Usa el prefijo vacío (las variables no tienen prefijo común).
    pub fn from_env() -> Result<Self, envy::Error> {
        load_base_env()?;
        load_secret_configuration()?;
        envy::from_env::<Self>()
    }

    /// Carga configuración incluyendo AWS Parameter Store y Secrets Manager.
    ///
    /// El método es asíncrono porque los SDK de AWS no ofrecen una API
    /// síncrona. Los binarios que usan AWS deben invocar este método antes de
    /// validar la configuración. No existe una ruta de recuperación a Vault:
    /// cualquier error de AWS se propaga y detiene el proceso.
    pub async fn from_env_async() -> Result<Self, envy::Error> {
        load_base_env()?;
        load_secret_configuration_async().await?;
        envy::from_env::<Self>()
    }

    pub fn log_level(&self) -> &str {
        self.log_level.as_deref().unwrap_or("info")
    }

    pub fn http_port(&self) -> u16 {
        self.http_port.unwrap_or(8080)
    }

    pub fn grpc_port(&self) -> u16 {
        self.grpc_port.unwrap_or(50051)
    }

    pub fn aws_region(&self) -> &str {
        self.aws_region.as_deref().unwrap_or("us-east-1")
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.database_url.starts_with("postgres://")
            && !self.database_url.starts_with("postgresql://")
        {
            return Err("DATABASE_URL must use postgres:// scheme".into());
        }
        if !self.redis_url.starts_with("redis://") && !self.redis_url.starts_with("rediss://") {
            return Err("REDIS_URL must use redis:// or rediss:// scheme".into());
        }
        if let Some(ref pepper) = self.api_key_pepper {
            if pepper.len() < 32 {
                return Err("API_KEY_PEPPER must have at least 32 characters".into());
            }
        }
        if let Some(ref secret) = self.jwt_signing_secret {
            if secret.len() < 32 {
                return Err("JWT_SIGNING_SECRET must have at least 32 characters".into());
            }
        }
        if let Some(ref secret) = self.service_jwt_secret {
            if secret.len() < 32 {
                return Err("SERVICE_JWT_SECRET must have at least 32 characters".into());
            }
        }
        Ok(())
    }
}

fn load_base_env() -> Result<(), envy::Error> {
    match dotenv::dotenv() {
        Ok(_) => Ok(()),
        Err(error) if error.not_found() => Ok(()),
        Err(error) => Err(envy::Error::Custom(format!(
            "failed to load base .env configuration: {error}"
        ))),
    }
}

fn load_secret_configuration() -> Result<(), envy::Error> {
    let provider = std::env::var("SECRETS_PROVIDER").unwrap_or_else(|_| "env".into());
    match provider.trim().to_ascii_lowercase().as_str() {
        "env" => Ok(()),
        "vault" => Err(envy::Error::Custom(
            "Vault runtime loading is disabled; configure SECRETS_PROVIDER=aws".into(),
        )),
        "aws" => Err(envy::Error::Custom(
            "SECRETS_PROVIDER=aws requires AppConfig::from_env_async(); refusing synchronous AWS loading".into(),
        )),
        other => Err(envy::Error::Custom(format!(
            "SECRETS_PROVIDER must be env, aws or vault; got {other}"
        ))),
    }
}

pub(crate) async fn load_secret_configuration_async() -> Result<(), envy::Error> {
    let provider = std::env::var("SECRETS_PROVIDER").unwrap_or_else(|_| "env".into());
    match provider.trim().to_ascii_lowercase().as_str() {
        "env" => Ok(()),
        "vault" => Err(envy::Error::Custom(
            "Vault runtime loading is disabled; configure SECRETS_PROVIDER=aws".into(),
        )),
        "aws" => load_aws_configuration().await,
        other => Err(envy::Error::Custom(format!(
            "SECRETS_PROVIDER must be env, aws or vault; got {other}"
        ))),
    }
}

async fn load_aws_configuration() -> Result<(), envy::Error> {
    let region = std::env::var("AWS_REGION")
        .or_else(|_| std::env::var("AWS_DEFAULT_REGION"))
        .unwrap_or_else(|_| "us-east-1".into());
    let behavior = aws_config::BehaviorVersion::latest();
    let aws_config = aws_config::defaults(behavior)
        .region(aws_types::region::Region::new(region.clone()))
        .load()
        .await;
    let secrets_client = aws_sdk_secretsmanager::Client::new(&aws_config);
    let ssm_client = aws_sdk_ssm::Client::new(&aws_config);
    let environment = std::env::var("AWS_ENVIRONMENT")
        .unwrap_or_else(|_| DEFAULT_AWS_ENVIRONMENT.into())
        .trim()
        .to_owned();
    if environment.is_empty() {
        return Err(envy::Error::Custom(
            "AWS_ENVIRONMENT must not be empty when SECRETS_PROVIDER=aws".into(),
        ));
    }

    let service = service_name();
    let secrets_prefix =
        std::env::var("AWS_SECRETS_PREFIX").unwrap_or_else(|_| format!("ai-gateway/{environment}"));
    let runtime_secret_id = std::env::var("AWS_RUNTIME_SECRET_ID")
        .or_else(|_| std::env::var("AWS_SECRETS_MANAGER_SECRET_ID"))
        .unwrap_or_else(|_| format!("{secrets_prefix}/runtime"));
    load_secret_json(&secrets_client, &runtime_secret_id).await?;

    let parameter_prefix = std::env::var("AWS_PARAMETER_PREFIX")
        .unwrap_or_else(|_| format!("/ai-gateway/{environment}"));
    let common_path = format!("{}/common", parameter_prefix.trim_end_matches('/'));
    load_ssm_path(&ssm_client, &common_path).await?;
    let service_path = format!("{}/{}", parameter_prefix.trim_end_matches('/'), service);
    if service_path != common_path {
        load_ssm_path(&ssm_client, &service_path).await?;
    }
    Ok(())
}

async fn load_secret_json(
    client: &aws_sdk_secretsmanager::Client,
    secret_id: &str,
) -> Result<(), envy::Error> {
    let response = client
        .get_secret_value()
        .secret_id(secret_id)
        .send()
        .await
        .map_err(|error| {
            envy::Error::Custom(format!(
                "AWS Secrets Manager could not load runtime secret {secret_id}: {error}"
            ))
        })?;
    let secret_string = response.secret_string().ok_or_else(|| {
        envy::Error::Custom(format!(
            "AWS Secrets Manager secret {secret_id} has no SecretString; binary secrets are not valid runtime configuration"
        ))
    })?;
    let values: serde_json::Value = serde_json::from_str(secret_string).map_err(|error| {
        envy::Error::Custom(format!(
            "AWS Secrets Manager secret {secret_id} is not valid JSON: {error}"
        ))
    })?;
    let object = values.as_object().ok_or_else(|| {
        envy::Error::Custom(format!(
            "AWS Secrets Manager secret {secret_id} must contain a JSON object"
        ))
    })?;
    for (name, value) in object {
        let value = value.as_str().ok_or_else(|| {
            envy::Error::Custom(format!(
                "AWS runtime secret {secret_id} field {name} must be a string"
            ))
        })?;
        std::env::set_var(name, value);
    }
    Ok(())
}

async fn load_ssm_path(client: &aws_sdk_ssm::Client, path: &str) -> Result<(), envy::Error> {
    let mut next_token = None;
    loop {
        let mut request = client
            .get_parameters_by_path()
            .path(path)
            .recursive(true)
            .with_decryption(true);
        if let Some(token) = next_token.take() {
            request = request.next_token(token);
        }
        let response = request.send().await.map_err(|error| {
            envy::Error::Custom(format!(
                "AWS Systems Manager Parameter Store could not load path {path}: {error}"
            ))
        })?;
        for parameter in response.parameters() {
            let name = parameter.name().ok_or_else(|| {
                envy::Error::Custom(format!(
                    "AWS Systems Manager returned a parameter without a name under {path}"
                ))
            })?;
            let value = parameter.value().ok_or_else(|| {
                envy::Error::Custom(format!("AWS Systems Manager parameter {name} has no value"))
            })?;
            let variable = name.rsplit('/').next().unwrap_or(name);
            if variable.is_empty() {
                return Err(envy::Error::Custom(format!(
                    "AWS Systems Manager parameter {name} has an empty environment variable name"
                )));
            }
            if is_sensitive_runtime_name(variable) {
                return Err(envy::Error::Custom(format!(
                    "AWS Systems Manager parameter {name} is classified as sensitive; store it in Secrets Manager"
                )));
            }
            std::env::set_var(variable, value);
        }
        match response.next_token() {
            Some(token) if !token.is_empty() => next_token = Some(token.to_owned()),
            _ => break,
        }
    }
    Ok(())
}

fn is_sensitive_runtime_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    upper == "DATABASE_URL"
        || upper.ends_with("_DATABASE_URL")
        || upper == "REDIS_URL"
        || upper == "NATS_URL"
        || upper.contains("PASSWORD")
        || upper.contains("SECRET")
        || upper.contains("API_KEY")
        || upper.ends_with("_TOKEN")
        || upper.contains("PRIVATE_KEY")
}

fn service_name() -> String {
    if let Ok(value) = std::env::var("SERVICE_NAME") {
        if !value.trim().is_empty() {
            return value.trim().to_owned();
        }
    }
    std::env::args()
        .next()
        .and_then(|value| {
            PathBuf::from(value)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "common".into())
}
