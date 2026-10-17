use async_trait::async_trait;
use azure_core::credentials::TokenCredential;
use azure_identity::{
    DeveloperToolsCredential, ManagedIdentityCredential, ManagedIdentityCredentialOptions,
    UserAssignedId, WorkloadIdentityCredential,
};
use azure_security_keyvault_secrets::SecretClient;
use chrono::{DateTime, TimeZone, Utc};
use futures_util::{stream::Stream, StreamExt};
use serde::Deserialize;
use std::fmt;
use std::io;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;
use tokio_stream::wrappers::TcpListenerStream;
use vaultrs::api::pki::requests::GenerateCertificateRequest;
use vaultrs::client::{Client as _, VaultClient, VaultClientSettingsBuilder};
use x509_parser::extensions::GeneralName;
use x509_parser::prelude::{FromDer, X509Certificate};
use zeroize::Zeroize;

const DEFAULT_REFRESH_SECONDS: u64 = 300;
const DEFAULT_MIN_VALIDITY_SECONDS: u64 = 3600;

/// Política de transporte. El origen del certificado se configura por separado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsMode {
    Disabled,
    Tls,
    Mtls,
}

impl TlsMode {
    fn parse(value: &str) -> Result<Self, TlsError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "disabled" | "off" => Ok(Self::Disabled),
            "tls" => Ok(Self::Tls),
            "mtls" => Ok(Self::Mtls),
            other => Err(TlsError::Configuration(format!(
                "TLS_MODE must be disabled, tls, or mtls; got {other}"
            ))),
        }
    }
}

impl fmt::Display for TlsMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => write!(f, "disabled"),
            Self::Tls => write!(f, "tls"),
            Self::Mtls => write!(f, "mtls"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsProvider {
    Local,
    Aws,
    Azure,
    Vault,
}

impl TlsProvider {
    fn parse(value: &str) -> Result<Self, TlsError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "local" => Ok(Self::Local),
            "aws" => Ok(Self::Aws),
            "azure" => Ok(Self::Azure),
            "vault" => Ok(Self::Vault),
            other => Err(TlsError::Configuration(format!(
                "TLS_CERT_PROVIDER must be local, aws, azure, or vault; got {other}"
            ))),
        }
    }
}

impl fmt::Display for TlsProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local => write!(f, "local"),
            Self::Aws => write!(f, "aws"),
            Self::Azure => write!(f, "azure"),
            Self::Vault => write!(f, "vault"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum AzureAuthMode {
    ManagedIdentity,
    WorkloadIdentity,
    DeveloperTools,
}

#[derive(Debug, Clone)]
enum VaultAuth {
    TokenFile {
        path: PathBuf,
    },
    Kubernetes {
        mount: String,
        role: String,
        jwt_path: PathBuf,
    },
    AppRole {
        mount: String,
        role_id_path: PathBuf,
        secret_id_path: PathBuf,
    },
}

#[derive(Debug, Clone)]
enum VaultSource {
    Kv {
        mount: String,
        path: String,
    },
    Pki {
        mount: String,
        role: String,
        common_name: String,
        alt_names: String,
        ip_sans: Option<String>,
        ttl: String,
    },
}

#[derive(Debug, Clone)]
enum ProviderConfig {
    Local {
        directory: PathBuf,
    },
    Aws {
        secret_id: String,
    },
    Azure {
        vault_url: String,
        secret_name: String,
        auth_mode: AzureAuthMode,
        managed_identity_client_id: Option<String>,
    },
    Vault {
        address: String,
        namespace: Option<String>,
        auth: VaultAuth,
        source: VaultSource,
    },
}

#[derive(Clone)]
struct CachedMaterial {
    loaded_at: Instant,
    material: Arc<TlsMaterial>,
}

/// Configuración TLS compartida por el servidor y los clientes gRPC de un servicio.
#[derive(Clone)]
pub struct ServiceTlsConfig {
    service_name: String,
    pub mode: TlsMode,
    pub provider: TlsProvider,
    provider_config: ProviderConfig,
    refresh_interval: Duration,
    min_validity: Duration,
    expected_dns_name: Option<String>,
    cache: Arc<RwLock<Option<CachedMaterial>>>,
}

impl fmt::Debug for ServiceTlsConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServiceTlsConfig")
            .field("service_name", &self.service_name)
            .field("mode", &self.mode)
            .field("provider", &self.provider)
            .field("refresh_interval", &self.refresh_interval)
            .field("min_validity", &self.min_validity)
            .field("expected_dns_name", &self.expected_dns_name)
            .finish_non_exhaustive()
    }
}

impl ServiceTlsConfig {
    pub fn from_env(service_name: &str) -> Result<Self, TlsError> {
        if service_name.trim().is_empty() {
            return Err(TlsError::Configuration(
                "TLS service name cannot be empty".into(),
            ));
        }

        let mode = TlsMode::parse(&std::env::var("TLS_MODE").unwrap_or_default())?;
        let provider = TlsProvider::parse(
            &std::env::var("TLS_CERT_PROVIDER").unwrap_or_else(|_| "local".into()),
        )?;
        let prefix = service_name.to_ascii_uppercase().replace('-', "_");
        let namespace = optional_env("TLS_CERT_NAMESPACE");
        let refresh_interval = Duration::from_secs(u64_env(
            "TLS_CERT_REFRESH_SECONDS",
            DEFAULT_REFRESH_SECONDS,
        )?);
        let min_validity = Duration::from_secs(u64_env(
            "TLS_MIN_VALIDITY_SECONDS",
            DEFAULT_MIN_VALIDITY_SECONDS,
        )?);
        if refresh_interval.is_zero() {
            return Err(TlsError::Configuration(
                "TLS_CERT_REFRESH_SECONDS must be greater than zero".into(),
            ));
        }

        let expected_dns_name = service_env(&prefix, "TLS_EXPECTED_DNS_NAME").or_else(|| {
            optional_env("TLS_DNS_SUFFIX").map(|suffix| format!("{service_name}.{suffix}"))
        });

        let provider_config = match provider {
            TlsProvider::Local => {
                let directory = if let Some(path) = service_env(&prefix, "TLS_DIR") {
                    PathBuf::from(path)
                } else {
                    PathBuf::from(std::env::var("TLS_DIR").unwrap_or_else(|_| "certs".into()))
                        .join(service_name)
                };
                ProviderConfig::Local { directory }
            }
            TlsProvider::Aws => {
                let secret_id = service_env(&prefix, "TLS_SECRET_ID")
                    .or_else(|| optional_env("AWS_TLS_SECRET_ID"))
                    .or_else(|| {
                        namespace
                            .as_ref()
                            .map(|value| format!("{}/{service_name}", value.trim_end_matches('/')))
                    })
                    .ok_or_else(|| {
                        TlsError::Configuration(format!(
                            "{prefix}_TLS_SECRET_ID, AWS_TLS_SECRET_ID, or TLS_CERT_NAMESPACE is required for the AWS TLS provider"
                        ))
                    })?;
                ProviderConfig::Aws { secret_id }
            }
            TlsProvider::Azure => {
                let vault_url = required_env("AZURE_KEY_VAULT_URL")?;
                require_https_provider_url("AZURE_KEY_VAULT_URL", &vault_url)?;
                let secret_name = service_env(&prefix, "TLS_SECRET_NAME")
                    .or_else(|| optional_env("AZURE_TLS_SECRET_NAME"))
                    .or_else(|| {
                        namespace
                            .as_ref()
                            .map(|value| azure_secret_name(&format!("{value}-{service_name}")))
                    })
                    .ok_or_else(|| {
                        TlsError::Configuration(format!(
                            "{prefix}_TLS_SECRET_NAME, AZURE_TLS_SECRET_NAME, or TLS_CERT_NAMESPACE is required for the Azure TLS provider"
                        ))
                    })?;
                let auth_mode = match std::env::var("AZURE_AUTH_MODE")
                    .unwrap_or_else(|_| "managed_identity".into())
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "managed_identity" => AzureAuthMode::ManagedIdentity,
                    "workload_identity" => AzureAuthMode::WorkloadIdentity,
                    "developer_tools" => AzureAuthMode::DeveloperTools,
                    other => {
                        return Err(TlsError::Configuration(format!(
                            "AZURE_AUTH_MODE must be managed_identity, workload_identity, or developer_tools; got {other}"
                        )))
                    }
                };
                ProviderConfig::Azure {
                    vault_url,
                    secret_name,
                    auth_mode,
                    managed_identity_client_id: optional_env("AZURE_CLIENT_ID"),
                }
            }
            TlsProvider::Vault => {
                let address = required_env("VAULT_ADDR")?;
                let auth = match std::env::var("VAULT_AUTH_METHOD")
                    .unwrap_or_else(|_| "token_file".into())
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "token_file" => VaultAuth::TokenFile {
                        path: PathBuf::from(
                            std::env::var("VAULT_TOKEN_FILE")
                                .unwrap_or_else(|_| "/var/run/secrets/vault/token".into()),
                        ),
                    },
                    "kubernetes" => VaultAuth::Kubernetes {
                        mount: std::env::var("VAULT_AUTH_MOUNT")
                            .unwrap_or_else(|_| "kubernetes".into()),
                        role: required_env("VAULT_KUBERNETES_ROLE")?,
                        jwt_path: PathBuf::from(
                            std::env::var("VAULT_KUBERNETES_JWT_FILE").unwrap_or_else(|_| {
                                "/var/run/secrets/kubernetes.io/serviceaccount/token".into()
                            }),
                        ),
                    },
                    "approle" => VaultAuth::AppRole {
                        mount: std::env::var("VAULT_AUTH_MOUNT")
                            .unwrap_or_else(|_| "approle".into()),
                        role_id_path: PathBuf::from(required_env("VAULT_APPROLE_ROLE_ID_FILE")?),
                        secret_id_path: PathBuf::from(required_env(
                            "VAULT_APPROLE_SECRET_ID_FILE",
                        )?),
                    },
                    other => {
                        return Err(TlsError::Configuration(format!(
                        "VAULT_AUTH_METHOD must be token_file, kubernetes, or approle; got {other}"
                    )))
                    }
                };
                let source = match std::env::var("VAULT_TLS_SOURCE")
                    .unwrap_or_else(|_| "pki".into())
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "kv" => VaultSource::Kv {
                        mount: std::env::var("VAULT_KV_MOUNT")
                            .unwrap_or_else(|_| "secret".into()),
                        path: service_env(&prefix, "TLS_SECRET_PATH")
                            .or_else(|| optional_env("VAULT_TLS_SECRET_PATH"))
                            .or_else(|| {
                                namespace.as_ref().map(|value| {
                                    format!("{}/{service_name}", value.trim_matches('/'))
                                })
                            })
                            .ok_or_else(|| {
                                TlsError::Configuration(format!(
                                    "{prefix}_TLS_SECRET_PATH, VAULT_TLS_SECRET_PATH, or TLS_CERT_NAMESPACE is required for Vault KV"
                                ))
                            })?,
                    },
                    "pki" => {
                        let common_name = service_env(&prefix, "TLS_COMMON_NAME")
                            .or_else(|| {
                                optional_env("TLS_DNS_SUFFIX")
                                    .map(|suffix| format!("{service_name}.{suffix}"))
                            })
                            .unwrap_or_else(|| service_name.to_string());
                        let default_alt_names = format!("{service_name},{common_name}");
                        let alt_names = service_env(&prefix, "TLS_ALT_NAMES").unwrap_or_else(|| {
                            optional_env("TLS_ALT_NAMES")
                                .map(|extra| format!("{default_alt_names},{extra}"))
                                .unwrap_or(default_alt_names)
                        });
                        VaultSource::Pki {
                            mount: required_env("VAULT_PKI_MOUNT")?,
                            role: required_env("VAULT_PKI_ROLE")?,
                            common_name,
                            alt_names,
                            ip_sans: service_env(&prefix, "TLS_IP_SANS")
                                .or_else(|| optional_env("TLS_IP_SANS")),
                            ttl: std::env::var("VAULT_PKI_TTL")
                                .unwrap_or_else(|_| "24h".into()),
                        }
                    }
                    other => {
                        return Err(TlsError::Configuration(format!(
                            "VAULT_TLS_SOURCE must be kv or pki; got {other}"
                        )))
                    }
                };
                ProviderConfig::Vault {
                    address,
                    namespace: optional_env("VAULT_NAMESPACE"),
                    auth,
                    source,
                }
            }
        };

        Ok(Self {
            service_name: service_name.to_string(),
            mode,
            provider,
            provider_config,
            refresh_interval,
            min_validity,
            expected_dns_name,
            cache: Arc::new(RwLock::new(None)),
        })
    }

    pub fn is_enabled(&self) -> bool {
        self.mode != TlsMode::Disabled
    }

    pub fn is_mtls(&self) -> bool {
        self.mode == TlsMode::Mtls
    }

    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    pub async fn material(&self) -> Result<Arc<TlsMaterial>, TlsError> {
        if !self.is_enabled() {
            return Err(TlsError::Configuration(format!(
                "TLS is disabled for {}",
                self.service_name
            )));
        }
        ensure_process_crypto_provider();

        if let Some(cached) = self.cache.read().await.as_ref() {
            if cached.loaded_at.elapsed() < self.refresh_interval
                && cached.material.remaining_validity() >= self.min_validity
            {
                return Ok(Arc::clone(&cached.material));
            }
        }

        let mut cache = self.cache.write().await;
        if let Some(cached) = cache.as_ref() {
            if cached.loaded_at.elapsed() < self.refresh_interval
                && cached.material.remaining_validity() >= self.min_validity
            {
                return Ok(Arc::clone(&cached.material));
            }
        }

        let provider = provider_from_config(&self.provider_config);
        let material = match provider.load(&self.service_name).await {
            Ok(material) => material,
            Err(error) => {
                metrics::counter!(
                    "tls_material_load_errors_total",
                    "service" => self.service_name.clone(),
                    "provider" => self.provider.to_string()
                )
                .increment(1);
                return Err(error);
            }
        };
        if let Err(error) = material.validate(self.min_validity, self.expected_dns_name.as_deref())
        {
            metrics::counter!(
                "tls_material_validation_errors_total",
                "service" => self.service_name.clone(),
                "provider" => self.provider.to_string()
            )
            .increment(1);
            return Err(error);
        }
        let material = Arc::new(material);
        metrics::counter!(
            "tls_material_load_total",
            "service" => self.service_name.clone(),
            "provider" => self.provider.to_string()
        )
        .increment(1);
        metrics::gauge!(
            "tls_certificate_expiry_seconds",
            "service" => self.service_name.clone(),
            "provider" => self.provider.to_string()
        )
        .set(material.remaining_validity().as_secs_f64());
        *cache = Some(CachedMaterial {
            loaded_at: Instant::now(),
            material: Arc::clone(&material),
        });
        Ok(material)
    }
}

pub struct TlsMaterial {
    pub certificate_chain_pem: Vec<u8>,
    pub private_key_pem: Vec<u8>,
    pub ca_bundle_pem: Vec<u8>,
    pub expires_at: DateTime<Utc>,
    pub version: String,
}

impl Clone for TlsMaterial {
    fn clone(&self) -> Self {
        Self {
            certificate_chain_pem: self.certificate_chain_pem.clone(),
            private_key_pem: self.private_key_pem.clone(),
            ca_bundle_pem: self.ca_bundle_pem.clone(),
            expires_at: self.expires_at,
            version: self.version.clone(),
        }
    }
}

impl fmt::Debug for TlsMaterial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TlsMaterial")
            .field("certificate_chain_pem", &"<redacted>")
            .field("private_key_pem", &"<redacted>")
            .field("ca_bundle_pem", &"<redacted>")
            .field("expires_at", &self.expires_at)
            .field("version", &self.version)
            .finish()
    }
}

impl Drop for TlsMaterial {
    fn drop(&mut self) {
        self.certificate_chain_pem.zeroize();
        self.private_key_pem.zeroize();
        self.ca_bundle_pem.zeroize();
    }
}

impl TlsMaterial {
    pub fn server_config(
        &self,
        require_client_auth: bool,
    ) -> Result<rustls::ServerConfig, TlsError> {
        let certificates = parse_certificates(&self.certificate_chain_pem, "certificate chain")?;
        let private_key = parse_private_key(&self.private_key_pem)?;
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let builder = rustls::ServerConfig::builder_with_provider(Arc::clone(&provider))
            .with_safe_default_protocol_versions()
            .map_err(|error| {
                TlsError::InvalidMaterial(format!(
                    "failed to configure safe TLS protocol versions: {error}"
                ))
            })?;
        let server_config = if require_client_auth {
            let mut roots = rustls::RootCertStore::empty();
            let (added, _) = roots
                .add_parsable_certificates(parse_certificates(&self.ca_bundle_pem, "CA bundle")?);
            if added == 0 {
                return Err(TlsError::InvalidMaterial(
                    "CA bundle contains no trusted certificates".into(),
                ));
            }
            let verifier = rustls::server::WebPkiClientVerifier::builder_with_provider(
                Arc::new(roots),
                provider,
            )
            .build()
            .map_err(|error| {
                TlsError::InvalidMaterial(format!(
                    "failed to configure mTLS client certificate verifier: {error}"
                ))
            })?;
            builder
                .with_client_cert_verifier(verifier)
                .with_single_cert(certificates, private_key)
                .map_err(|error| {
                    TlsError::InvalidMaterial(format!(
                        "server certificate and private key are incompatible: {error}"
                    ))
                })
        } else {
            builder
                .with_no_client_auth()
                .with_single_cert(certificates, private_key)
                .map_err(|error| {
                    TlsError::InvalidMaterial(format!(
                        "server certificate and private key are incompatible: {error}"
                    ))
                })
        }?;
        let mut server_config = server_config;
        server_config.alpn_protocols = vec![b"h2".to_vec()];
        Ok(server_config)
    }

    fn remaining_validity(&self) -> Duration {
        (self.expires_at - Utc::now())
            .to_std()
            .unwrap_or(Duration::ZERO)
    }

    fn validate(
        &self,
        min_validity: Duration,
        expected_dns_name: Option<&str>,
    ) -> Result<(), TlsError> {
        let certs = parse_certificates(&self.certificate_chain_pem, "certificate chain")?;
        let ca_certs = parse_certificates(&self.ca_bundle_pem, "CA bundle")?;
        if certs.is_empty() || ca_certs.is_empty() {
            return Err(TlsError::InvalidMaterial(
                "certificate chain and CA bundle must not be empty".into(),
            ));
        }
        let private_key = parse_private_key(&self.private_key_pem)?;

        rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(|error| {
            TlsError::InvalidMaterial(format!(
                "failed to configure safe TLS protocol versions: {error}"
            ))
        })?
        .with_root_certificates({
            let mut roots = rustls::RootCertStore::empty();
            let (added, _) = roots.add_parsable_certificates(ca_certs);
            if added == 0 {
                return Err(TlsError::InvalidMaterial(
                    "CA bundle contains no trusted certificates".into(),
                ));
            }
            roots
        })
        .with_client_auth_cert(certs, private_key)
        .map_err(|error| {
            TlsError::InvalidMaterial(format!(
                "certificate and private key are incompatible: {error}"
            ))
        })?;

        let (_, pem) =
            x509_parser::pem::parse_x509_pem(&self.certificate_chain_pem).map_err(|error| {
                TlsError::InvalidMaterial(format!("invalid leaf certificate: {error}"))
            })?;
        let (_, certificate) = X509Certificate::from_der(&pem.contents).map_err(|error| {
            TlsError::InvalidMaterial(format!("invalid X.509 certificate: {error}"))
        })?;
        let certificate_expiry = Utc
            .timestamp_opt(certificate.validity().not_after.timestamp(), 0)
            .single()
            .ok_or_else(|| {
                TlsError::InvalidMaterial("certificate expiration is out of range".into())
            })?;
        if certificate_expiry <= Utc::now()
            || (certificate_expiry - Utc::now())
                .to_std()
                .unwrap_or(Duration::ZERO)
                < min_validity
        {
            return Err(TlsError::CertificateExpiring {
                expires_at: certificate_expiry,
                min_validity_seconds: min_validity.as_secs(),
            });
        }
        if self.expires_at > certificate_expiry + chrono::Duration::seconds(1) {
            return Err(TlsError::InvalidMaterial(
                "provider expiration is later than the X.509 certificate expiration".into(),
            ));
        }

        if let Some(expected) = expected_dns_name {
            let matches = certificate
                .subject_alternative_name()
                .map_err(|error| {
                    TlsError::InvalidMaterial(format!(
                        "failed to parse certificate SAN extension: {error}"
                    ))
                })?
                .map(|extension| {
                    extension.value.general_names.iter().any(
                        |name| matches!(name, GeneralName::DNSName(value) if *value == expected),
                    )
                })
                .unwrap_or(false);
            if !matches {
                return Err(TlsError::InvalidMaterial(format!(
                    "certificate SAN does not contain expected DNS name {expected}"
                )));
            }
        }

        Ok(())
    }
}

#[derive(Clone)]
pub struct GrpcListenerSupervisor {
    config: ServiceTlsConfig,
}

impl GrpcListenerSupervisor {
    pub fn new(config: ServiceTlsConfig) -> Self {
        Self { config }
    }

    pub fn incoming(
        &self,
        listener: TcpListener,
    ) -> impl Stream<Item = Result<TlsStream<TcpStream>, io::Error>> + Send {
        let config = self.config.clone();
        TcpListenerStream::new(listener).then(move |accepted| {
            let config = config.clone();
            async move {
                let stream = prepare_tcp_stream(accepted?)?;
                let material = config.material().await.map_err(|error| {
                    tracing::error!(
                        service = config.service_name(),
                        error = %error,
                        "failed to load TLS material for incoming gRPC connection"
                    );
                    io::Error::other(error.to_string())
                })?;
                let server_config = material.server_config(config.is_mtls()).map_err(|error| {
                    tracing::error!(
                        service = config.service_name(),
                        error = %error,
                        "failed to build dynamic gRPC TLS server configuration"
                    );
                    io::Error::other(error.to_string())
                })?;
                TlsAcceptor::from(Arc::new(server_config))
                    .accept(stream)
                    .await
                    .map_err(|error| {
                        metrics::counter!(
                            "grpc_tls_handshake_errors_total",
                            "service" => config.service_name().to_owned()
                        )
                        .increment(1);
                        tracing::warn!(
                            service = config.service_name(),
                            error = %error,
                            "gRPC TLS handshake failed"
                        );
                        io::Error::other(error.to_string())
                    })
            }
        })
    }
}

pub fn dynamic_tls_incoming(
    listener: TcpListener,
    config: ServiceTlsConfig,
) -> impl Stream<Item = Result<TlsStream<TcpStream>, io::Error>> + Send {
    GrpcListenerSupervisor::new(config).incoming(listener)
}

pub async fn validate_server_tls(config: &ServiceTlsConfig) -> Result<(), TlsError> {
    if config.is_enabled() {
        let material = config.material().await?;
        material.server_config(config.is_mtls())?;
        tracing::info!(
            service = config.service_name(),
            mode = %config.mode,
            provider = %config.provider,
            certificate_expires_at = %material.expires_at,
            "dynamic gRPC TLS listener material validated"
        );
    }
    Ok(())
}

pub fn tcp_incoming(
    listener: TcpListener,
) -> impl Stream<Item = Result<TcpStream, io::Error>> + Send {
    TcpListenerStream::new(listener).map(|accepted| accepted.and_then(prepare_tcp_stream))
}

fn prepare_tcp_stream(stream: TcpStream) -> Result<TcpStream, io::Error> {
    stream.set_nodelay(true)?;
    Ok(stream)
}

#[derive(Debug, Deserialize)]
struct StoredTlsBundle {
    #[serde(alias = "cert_pem", alias = "certificate_pem")]
    certificate_chain_pem: String,
    #[serde(alias = "key_pem")]
    private_key_pem: String,
    #[serde(alias = "ca_pem")]
    ca_bundle_pem: String,
    #[serde(default)]
    expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    version: Option<String>,
}

impl StoredTlsBundle {
    fn into_material(self, source_version: Option<String>) -> Result<TlsMaterial, TlsError> {
        let derived_expiry = certificate_expiry(self.certificate_chain_pem.as_bytes())?;
        Ok(TlsMaterial {
            certificate_chain_pem: self.certificate_chain_pem.into_bytes(),
            private_key_pem: self.private_key_pem.into_bytes(),
            ca_bundle_pem: self.ca_bundle_pem.into_bytes(),
            expires_at: self.expires_at.unwrap_or(derived_expiry),
            version: source_version
                .or(self.version)
                .unwrap_or_else(|| "unversioned".into()),
        })
    }
}

#[async_trait]
pub trait CertificateProvider: Send + Sync {
    async fn load(&self, service_name: &str) -> Result<TlsMaterial, TlsError>;
}

struct LocalCertificateProvider {
    directory: PathBuf,
}

#[async_trait]
impl CertificateProvider for LocalCertificateProvider {
    async fn load(&self, _service_name: &str) -> Result<TlsMaterial, TlsError> {
        let cert_path = self.directory.join("server.crt");
        let key_path = self.directory.join("server.key");
        let ca_path = self.directory.join("ca.crt");
        let certificate_chain_pem = read_tls_file(&cert_path, "certificate").await?;
        let private_key_pem = read_tls_file(&key_path, "private key").await?;
        let ca_bundle_pem = read_tls_file(&ca_path, "CA bundle").await?;
        let expires_at = certificate_expiry(&certificate_chain_pem)?;
        Ok(TlsMaterial {
            certificate_chain_pem,
            private_key_pem,
            ca_bundle_pem,
            expires_at,
            version: format!("local:{}", cert_path.display()),
        })
    }
}

struct AwsCertificateProvider {
    secret_id: String,
}

#[async_trait]
impl CertificateProvider for AwsCertificateProvider {
    async fn load(&self, _service_name: &str) -> Result<TlsMaterial, TlsError> {
        let aws_config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let client = aws_sdk_secretsmanager::Client::new(&aws_config);
        let response = client
            .get_secret_value()
            .secret_id(&self.secret_id)
            .send()
            .await
            .map_err(|error| TlsError::Provider {
                provider: "aws",
                message: error.to_string(),
            })?;
        let bytes = response
            .secret_string()
            .map(|value| value.as_bytes().to_vec())
            .or_else(|| {
                response
                    .secret_binary()
                    .map(|value| value.as_ref().to_vec())
            })
            .ok_or_else(|| TlsError::Provider {
                provider: "aws",
                message: format!(
                    "secret {} has no SecretString or SecretBinary",
                    self.secret_id
                ),
            })?;
        decode_stored_bundle(&bytes, response.version_id().map(str::to_string))
    }
}

struct AzureCertificateProvider {
    vault_url: String,
    secret_name: String,
    auth_mode: AzureAuthMode,
    managed_identity_client_id: Option<String>,
}

#[async_trait]
impl CertificateProvider for AzureCertificateProvider {
    async fn load(&self, _service_name: &str) -> Result<TlsMaterial, TlsError> {
        let credential: Arc<dyn TokenCredential> = match self.auth_mode {
            AzureAuthMode::ManagedIdentity => {
                let options = ManagedIdentityCredentialOptions {
                    user_assigned_id: self
                        .managed_identity_client_id
                        .clone()
                        .map(UserAssignedId::ClientId),
                    ..Default::default()
                };
                ManagedIdentityCredential::new(Some(options)).map_err(|error| {
                    TlsError::Provider {
                        provider: "azure",
                        message: error.to_string(),
                    }
                })?
            }
            AzureAuthMode::WorkloadIdentity => {
                WorkloadIdentityCredential::new(None).map_err(|error| TlsError::Provider {
                    provider: "azure",
                    message: error.to_string(),
                })?
            }
            AzureAuthMode::DeveloperTools => {
                DeveloperToolsCredential::new(None).map_err(|error| TlsError::Provider {
                    provider: "azure",
                    message: error.to_string(),
                })?
            }
        };
        let client = SecretClient::new(&self.vault_url, credential, None).map_err(|error| {
            TlsError::Provider {
                provider: "azure",
                message: error.to_string(),
            }
        })?;
        let secret = client
            .get_secret(&self.secret_name, None)
            .await
            .and_then(|response| response.into_model())
            .map_err(|error| TlsError::Provider {
                provider: "azure",
                message: error.to_string(),
            })?;
        let value = secret.value.ok_or_else(|| TlsError::Provider {
            provider: "azure",
            message: format!("secret {} has no value", self.secret_name),
        })?;
        decode_stored_bundle(
            value.as_bytes(),
            secret.id.as_deref().and_then(secret_version_from_id),
        )
    }
}

struct VaultCertificateProvider {
    address: String,
    namespace: Option<String>,
    auth: VaultAuth,
    source: VaultSource,
}

#[async_trait]
impl CertificateProvider for VaultCertificateProvider {
    async fn load(&self, _service_name: &str) -> Result<TlsMaterial, TlsError> {
        let client =
            authenticated_vault_client(&self.address, self.namespace.as_deref(), &self.auth)
                .await?;
        match &self.source {
            VaultSource::Kv { mount, path } => {
                let bundle: StoredTlsBundle = vaultrs::kv2::read(&client, mount, path)
                    .await
                    .map_err(|error| TlsError::Provider {
                        provider: "vault",
                        message: error.to_string(),
                    })?;
                bundle.into_material(Some(format!("vault-kv:{mount}/{path}")))
            }
            VaultSource::Pki {
                mount,
                role,
                common_name,
                alt_names,
                ip_sans,
                ttl,
            } => {
                let mut request = GenerateCertificateRequest::builder();
                request
                    .common_name(common_name)
                    .alt_names(alt_names)
                    .ttl(ttl)
                    .private_key_format("pkcs8");
                if let Some(ip_sans) = ip_sans {
                    request.ip_sans(ip_sans);
                }
                let issued = vaultrs::pki::cert::generate(&client, mount, role, Some(&mut request))
                    .await
                    .map_err(|error| TlsError::Provider {
                        provider: "vault",
                        message: error.to_string(),
                    })?;
                let mut chain = issued.certificate.clone();
                if let Some(ca_chain) = &issued.ca_chain {
                    for certificate in ca_chain {
                        append_pem(&mut chain, certificate);
                    }
                }
                let ca_bundle = issued
                    .ca_chain
                    .filter(|chain| !chain.is_empty())
                    .map(|chain| chain.join("\n"))
                    .unwrap_or(issued.issuing_ca);
                let expires_at = issued
                    .expiration
                    .and_then(|value| Utc.timestamp_opt(value as i64, 0).single())
                    .unwrap_or(certificate_expiry(issued.certificate.as_bytes())?);
                Ok(TlsMaterial {
                    certificate_chain_pem: chain.into_bytes(),
                    private_key_pem: issued.private_key.into_bytes(),
                    ca_bundle_pem: ca_bundle.into_bytes(),
                    expires_at,
                    version: issued.serial_number,
                })
            }
        }
    }
}

fn provider_from_config(config: &ProviderConfig) -> Box<dyn CertificateProvider> {
    match config {
        ProviderConfig::Local { directory } => Box::new(LocalCertificateProvider {
            directory: directory.clone(),
        }),
        ProviderConfig::Aws { secret_id } => Box::new(AwsCertificateProvider {
            secret_id: secret_id.clone(),
        }),
        ProviderConfig::Azure {
            vault_url,
            secret_name,
            auth_mode,
            managed_identity_client_id,
        } => Box::new(AzureCertificateProvider {
            vault_url: vault_url.clone(),
            secret_name: secret_name.clone(),
            auth_mode: *auth_mode,
            managed_identity_client_id: managed_identity_client_id.clone(),
        }),
        ProviderConfig::Vault {
            address,
            namespace,
            auth,
            source,
        } => Box::new(VaultCertificateProvider {
            address: address.clone(),
            namespace: namespace.clone(),
            auth: auth.clone(),
            source: source.clone(),
        }),
    }
}

pub async fn load_server_tls(
    config: &ServiceTlsConfig,
) -> Result<tonic::transport::ServerTlsConfig, TlsError> {
    ensure_process_crypto_provider();
    tracing::info!(
        service = config.service_name(),
        dependency = "service_tls",
        direction = "server",
        mode = %config.mode,
        provider = %config.provider,
        "loading TLS material"
    );
    let material = match config.material().await {
        Ok(material) => material,
        Err(error) => {
            tracing::error!(
                service = config.service_name(),
                dependency = "service_tls",
                direction = "server",
                mode = %config.mode,
                provider = %config.provider,
                error = %error,
                "TLS material load failed"
            );
            return Err(error);
        }
    };
    tracing::info!(
        service = config.service_name(),
        dependency = "service_tls",
        direction = "server",
        mode = %config.mode,
        provider = %config.provider,
        certificate_expires_at = %material.expires_at,
        "TLS material loaded and validated"
    );
    let identity = tonic::transport::Identity::from_pem(
        &material.certificate_chain_pem,
        &material.private_key_pem,
    );
    let mut tls = tonic::transport::ServerTlsConfig::new().identity(identity);
    if config.is_mtls() {
        tls = tls.client_ca_root(tonic::transport::Certificate::from_pem(
            &material.ca_bundle_pem,
        ));
    }
    Ok(tls)
}

pub async fn load_client_tls(
    config: &ServiceTlsConfig,
) -> Result<tonic::transport::ClientTlsConfig, TlsError> {
    ensure_process_crypto_provider();
    tracing::info!(
        service = config.service_name(),
        dependency = "service_tls",
        direction = "client",
        mode = %config.mode,
        provider = %config.provider,
        "loading TLS material"
    );
    let material = match config.material().await {
        Ok(material) => material,
        Err(error) => {
            tracing::error!(
                service = config.service_name(),
                dependency = "service_tls",
                direction = "client",
                mode = %config.mode,
                provider = %config.provider,
                error = %error,
                "TLS material load failed"
            );
            return Err(error);
        }
    };
    tracing::info!(
        service = config.service_name(),
        dependency = "service_tls",
        direction = "client",
        mode = %config.mode,
        provider = %config.provider,
        certificate_expires_at = %material.expires_at,
        "TLS material loaded and validated"
    );
    let mut tls = tonic::transport::ClientTlsConfig::new().ca_certificate(
        tonic::transport::Certificate::from_pem(&material.ca_bundle_pem),
    );
    if config.is_mtls() {
        tls = tls.identity(tonic::transport::Identity::from_pem(
            &material.certificate_chain_pem,
            &material.private_key_pem,
        ));
    }
    Ok(tls)
}

pub async fn configure_client_endpoint(
    endpoint: tonic::transport::Endpoint,
    endpoint_uri: &str,
    config: &ServiceTlsConfig,
) -> Result<tonic::transport::Endpoint, TlsError> {
    if !config.is_enabled() {
        tracing::warn!(
            service = config.service_name(),
            dependency = "service_tls",
            direction = "client",
            endpoint = %crate::startup::sanitize_endpoint(endpoint_uri),
            "TLS is disabled for internal gRPC client"
        );
        return Ok(endpoint);
    }
    require_secure_transport_url("gRPC endpoint", endpoint_uri, "https")?;
    tracing::info!(
        service = config.service_name(),
        dependency = "internal_grpc",
        endpoint = %crate::startup::sanitize_endpoint(endpoint_uri),
        tls_mode = %config.mode,
        "configuring secure internal gRPC client"
    );
    let tls = load_client_tls(config).await?;
    let endpoint = endpoint
        .tls_config(tls)
        .map_err(|error| TlsError::Transport(error.to_string()))?;
    tracing::info!(
        service = config.service_name(),
        dependency = "internal_grpc",
        endpoint = %crate::startup::sanitize_endpoint(endpoint_uri),
        tls_mode = %config.mode,
        "secure internal gRPC client configured"
    );
    Ok(endpoint)
}

#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    #[error("invalid TLS configuration: {0}")]
    Configuration(String),
    #[error("{provider} certificate provider failed: {message}")]
    Provider {
        provider: &'static str,
        message: String,
    },
    #[error("failed to read TLS {kind} at {path}: {source}")]
    FileRead {
        kind: &'static str,
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid TLS material: {0}")]
    InvalidMaterial(String),
    #[error(
        "TLS certificate expires at {expires_at}; at least {min_validity_seconds} seconds are required"
    )]
    CertificateExpiring {
        expires_at: DateTime<Utc>,
        min_validity_seconds: u64,
    },
    #[error("TLS transport configuration failed: {0}")]
    Transport(String),
}

async fn authenticated_vault_client(
    address: &str,
    namespace: Option<&str>,
    auth: &VaultAuth,
) -> Result<VaultClient, TlsError> {
    let mut settings = VaultClientSettingsBuilder::default();
    settings.address(address).token("");
    if let Some(namespace) = namespace {
        settings.set_namespace(namespace.to_string());
    }
    let settings = settings.build().map_err(|error| TlsError::Provider {
        provider: "vault",
        message: error.to_string(),
    })?;
    let mut client = VaultClient::new(settings).map_err(|error| TlsError::Provider {
        provider: "vault",
        message: error.to_string(),
    })?;

    let token = match auth {
        VaultAuth::TokenFile { path } => read_trimmed_secret_file(path, "Vault token").await?,
        VaultAuth::Kubernetes {
            mount,
            role,
            jwt_path,
        } => {
            let jwt = read_trimmed_secret_file(jwt_path, "Kubernetes service account JWT").await?;
            vaultrs::auth::kubernetes::login(&client, mount, role, &jwt)
                .await
                .map_err(|error| TlsError::Provider {
                    provider: "vault",
                    message: error.to_string(),
                })?
                .client_token
        }
        VaultAuth::AppRole {
            mount,
            role_id_path,
            secret_id_path,
        } => {
            let role_id = read_trimmed_secret_file(role_id_path, "Vault AppRole role ID").await?;
            let secret_id =
                read_trimmed_secret_file(secret_id_path, "Vault AppRole secret ID").await?;
            vaultrs::auth::approle::login(&client, mount, &role_id, &secret_id)
                .await
                .map_err(|error| TlsError::Provider {
                    provider: "vault",
                    message: error.to_string(),
                })?
                .client_token
        }
    };
    client.set_token(&token);
    Ok(client)
}

async fn read_tls_file(path: &Path, kind: &'static str) -> Result<Vec<u8>, TlsError> {
    tokio::fs::read(path)
        .await
        .map_err(|source| TlsError::FileRead {
            kind,
            path: path.display().to_string(),
            source,
        })
}

async fn read_trimmed_secret_file(path: &Path, kind: &'static str) -> Result<String, TlsError> {
    let bytes = read_tls_file(path, kind).await?;
    let value = String::from_utf8(bytes)
        .map_err(|error| TlsError::InvalidMaterial(format!("{kind} is not UTF-8: {error}")))?;
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(TlsError::InvalidMaterial(format!("{kind} is empty")));
    }
    Ok(value)
}

fn decode_stored_bundle(
    bytes: &[u8],
    source_version: Option<String>,
) -> Result<TlsMaterial, TlsError> {
    serde_json::from_slice::<StoredTlsBundle>(bytes)
        .map_err(|error| {
            TlsError::InvalidMaterial(format!(
                "certificate secret must be a TLS bundle JSON object: {error}"
            ))
        })?
        .into_material(source_version)
}

fn certificate_expiry(certificate_chain_pem: &[u8]) -> Result<DateTime<Utc>, TlsError> {
    let (_, pem) = x509_parser::pem::parse_x509_pem(certificate_chain_pem)
        .map_err(|error| TlsError::InvalidMaterial(format!("invalid certificate PEM: {error}")))?;
    let (_, certificate) = X509Certificate::from_der(&pem.contents).map_err(|error| {
        TlsError::InvalidMaterial(format!("invalid X.509 certificate: {error}"))
    })?;
    Utc.timestamp_opt(certificate.validity().not_after.timestamp(), 0)
        .single()
        .ok_or_else(|| TlsError::InvalidMaterial("certificate expiration is out of range".into()))
}

fn parse_certificates(
    pem: &[u8],
    kind: &str,
) -> Result<Vec<rustls::pki_types::CertificateDer<'static>>, TlsError> {
    rustls_pemfile::certs(&mut Cursor::new(pem))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| TlsError::InvalidMaterial(format!("invalid {kind}: {error}")))
}

fn parse_private_key(pem: &[u8]) -> Result<rustls::pki_types::PrivateKeyDer<'static>, TlsError> {
    rustls_pemfile::private_key(&mut Cursor::new(pem))
        .map_err(|error| TlsError::InvalidMaterial(format!("invalid private key: {error}")))?
        .ok_or_else(|| TlsError::InvalidMaterial("private key PEM is empty".into()))
}

fn ensure_process_crypto_provider() {
    if let Err(error) = rustls::crypto::ring::default_provider().install_default() {
        tracing::debug!(
            ?error,
            "rustls process crypto provider was already installed"
        );
    }
}

fn secret_version_from_id(id: &str) -> Option<String> {
    id.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn append_pem(target: &mut String, value: &str) {
    if !target.ends_with('\n') {
        target.push('\n');
    }
    target.push_str(value);
    if !target.ends_with('\n') {
        target.push('\n');
    }
}

fn optional_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn required_env(key: &str) -> Result<String, TlsError> {
    optional_env(key)
        .ok_or_else(|| TlsError::Configuration(format!("{key} is required and cannot be empty")))
}

fn service_env(prefix: &str, suffix: &str) -> Option<String> {
    optional_env(&format!("{prefix}_{suffix}"))
}

fn u64_env(key: &str, default: u64) -> Result<u64, TlsError> {
    match optional_env(key) {
        None => Ok(default),
        Some(value) => value
            .parse::<u64>()
            .map_err(|_| TlsError::Configuration(format!("{key} must be an unsigned integer"))),
    }
}

fn require_https_provider_url(key: &str, value: &str) -> Result<(), TlsError> {
    let parsed = url::Url::parse(value)
        .map_err(|error| TlsError::Configuration(format!("{key} is invalid: {error}")))?;
    if parsed.scheme() != "https" {
        return Err(TlsError::Configuration(format!("{key} must use https://")));
    }
    Ok(())
}

fn require_secure_transport_url(
    name: &str,
    value: &str,
    expected_scheme: &str,
) -> Result<(), TlsError> {
    let parsed = url::Url::parse(value)
        .map_err(|error| TlsError::Configuration(format!("{name} is invalid: {error}")))?;
    if parsed.scheme() != expected_scheme {
        return Err(TlsError::Configuration(format!(
            "{name} must use {expected_scheme}:// when TLS is enabled"
        )));
    }
    Ok(())
}

fn azure_secret_name(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' {
                character
            } else {
                '-'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcgen::{
        BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
        KeyUsagePurpose,
    };

    fn test_material(dns_name: &str) -> TlsMaterial {
        let mut ca_params = CertificateParams::new(Vec::new()).unwrap();
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca_params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
        ];
        let ca_key = KeyPair::generate().unwrap();
        let ca = ca_params.self_signed(&ca_key).unwrap();
        let issuer = Issuer::new(ca_params, ca_key);

        let mut leaf_params = CertificateParams::new(vec![dns_name.to_string()]).unwrap();
        leaf_params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        leaf_params.extended_key_usages = vec![
            ExtendedKeyUsagePurpose::ServerAuth,
            ExtendedKeyUsagePurpose::ClientAuth,
        ];
        let leaf_key = KeyPair::generate().unwrap();
        let leaf = leaf_params.signed_by(&leaf_key, &issuer).unwrap();
        let certificate_chain_pem = leaf.pem().into_bytes();
        let expires_at = certificate_expiry(&certificate_chain_pem).unwrap();

        TlsMaterial {
            certificate_chain_pem,
            private_key_pem: leaf_key.serialize_pem().into_bytes(),
            ca_bundle_pem: ca.pem().into_bytes(),
            expires_at,
            version: "test".into(),
        }
    }

    #[test]
    fn rejects_unknown_tls_mode() {
        assert!(TlsMode::parse("required").is_err());
    }

    #[test]
    fn parses_transport_modes_without_conflating_provider() {
        assert_eq!(TlsMode::parse("disabled").unwrap(), TlsMode::Disabled);
        assert_eq!(TlsMode::parse("tls").unwrap(), TlsMode::Tls);
        assert_eq!(TlsMode::parse("mtls").unwrap(), TlsMode::Mtls);
        assert_eq!(TlsProvider::parse("aws").unwrap(), TlsProvider::Aws);
    }

    #[test]
    fn normalizes_azure_secret_names() {
        assert_eq!(
            azure_secret_name("prod/nexus.billing-service"),
            "prod-nexus-billing-service"
        );
    }

    #[test]
    fn extracts_azure_secret_version() {
        assert_eq!(
            secret_version_from_id("https://vault.vault.azure.net/secrets/nexus-billing/abc123")
                .as_deref(),
            Some("abc123")
        );
    }

    #[test]
    fn validates_matching_certificate_key_ca_and_san() {
        test_material("billing-service.svc.internal")
            .validate(
                Duration::from_secs(60),
                Some("billing-service.svc.internal"),
            )
            .unwrap();
    }

    #[test]
    fn rejects_unexpected_certificate_identity() {
        let error = test_material("billing-service.svc.internal")
            .validate(
                Duration::from_secs(60),
                Some("payment-services.svc.internal"),
            )
            .unwrap_err();
        assert!(error.to_string().contains("certificate SAN"));
    }

    #[test]
    fn rejects_private_key_from_another_identity() {
        let mut material = test_material("billing-service.svc.internal");
        material.private_key_pem = KeyPair::generate().unwrap().serialize_pem().into_bytes();
        let error = material
            .validate(Duration::from_secs(60), None)
            .unwrap_err();
        assert!(error.to_string().contains("incompatible"));
    }

    #[test]
    fn builds_tls_and_mtls_server_configurations() {
        let material = test_material("billing-service.svc.internal");
        assert!(material.server_config(false).is_ok());
        assert!(material.server_config(true).is_ok());
    }
}
