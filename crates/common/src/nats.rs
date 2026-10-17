use crate::startup::sanitize_endpoint;
use async_nats::{Client, ConnectOptions, ServerAddr};
use sha2::{Digest, Sha256};
use std::env;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NatsConnectionError {
    #[error("invalid NATS server URL")]
    InvalidServerUrl(#[source] std::io::Error),
    #[error("NATS_USERNAME and NATS_PASSWORD must be configured together")]
    IncompleteCredentials,
    #[error("failed to connect to NATS")]
    Connect(#[from] async_nats::ConnectError),
}

/// Connects to NATS and explicitly applies user/password authentication.
///
/// `async_nats::connect` accepts URLs containing user info, but the client
/// connector does not copy that user info into the CONNECT payload. Runtime
/// credentials therefore take precedence, with URL credentials retained as a
/// compatibility fallback.
pub async fn connect_nats(server_url: &str) -> Result<Client, NatsConnectionError> {
    let endpoint = sanitize_endpoint(server_url);
    let server = server_url.parse::<ServerAddr>().map_err(|error| {
        tracing::error!(
            dependency = "nats",
            %endpoint,
            error = %error,
            "dependency endpoint is invalid"
        );
        NatsConnectionError::InvalidServerUrl(error)
    })?;

    let env_username = non_empty_env("NATS_USERNAME");
    let env_password = non_empty_env("NATS_PASSWORD");
    let embedded_username = server.username().map(ToOwned::to_owned);
    let embedded_password = server.password().map(ToOwned::to_owned);

    let embedded_credentials = match (embedded_username, embedded_password) {
        (Some(username), Some(password)) => Some((username, password)),
        (None, None) => None,
        _ => {
            tracing::error!(
                dependency = "nats",
                %endpoint,
                "NATS username and password must be configured together"
            );
            return Err(NatsConnectionError::IncompleteCredentials);
        }
    };
    let credentials = match (env_username, env_password) {
        (Some(username), Some(password)) => Some((username, password)),
        (None, None) => embedded_credentials.clone(),
        _ => {
            tracing::error!(
                dependency = "nats",
                %endpoint,
                "NATS username and password must be configured together"
            );
            return Err(NatsConnectionError::IncompleteCredentials);
        }
    };
    let authentication = if credentials.is_some() {
        "username_password"
    } else {
        "none"
    };

    let fallback_credentials = embedded_credentials;
    let options = ConnectOptions::with_auth_callback(move |_| {
        let fallback_credentials = fallback_credentials.clone();
        async move {
            let username = non_empty_env("NATS_USERNAME");
            let password = non_empty_env("NATS_PASSWORD");
            match (username, password) {
                (Some(username), Some(password)) => {
                    let mut auth = async_nats::Auth::new();
                    auth.username = Some(username);
                    auth.password = Some(password);
                    Ok(auth)
                }
                (None, None) => match fallback_credentials {
                    Some((username, password)) => {
                        let mut auth = async_nats::Auth::new();
                        auth.username = Some(username);
                        auth.password = Some(password);
                        Ok(auth)
                    }
                    None => Ok(async_nats::Auth::new()),
                },
                _ => Err(async_nats::AuthError::new(
                    "NATS_USERNAME and NATS_PASSWORD must be configured together",
                )),
            }
        }
    });

    tracing::info!(
        dependency = "nats",
        %endpoint,
        authentication,
        "connecting to dependency"
    );
    match options.connect(server).await {
        Ok(client) => {
            tracing::info!(
                dependency = "nats",
                %endpoint,
                authentication,
                "dependency connected"
            );
            Ok(client)
        }
        Err(error) => {
            tracing::error!(
                dependency = "nats",
                %endpoint,
                error = %error,
                "dependency connection failed"
            );
            Err(NatsConnectionError::Connect(error))
        }
    }
}

/// Supervises a persistent NATS client and forces a reconnect after a
/// validated configuration snapshot changes. `ConnectOptions` invokes the
/// authentication callback for that reconnect, so the new credentials are
/// used without rebuilding every JetStream context or restarting the process.
pub fn supervise_nats_client(
    reload: &crate::ConfigReloadHandle,
    service: &'static str,
    client: Client,
    server_url_key: &'static str,
) {
    let mut receiver = reload.subscribe();
    tokio::spawn(async move {
        let initial = receiver.borrow().clone();
        let mut active_credentials = nats_credential_fingerprint(&initial);
        let mut active_server = initial
            .env_value(server_url_key)
            .map(str::to_owned)
            .unwrap_or_default();
        loop {
            if receiver.changed().await.is_err() {
                return;
            }
            let snapshot = receiver.borrow_and_update().clone();
            let next_credentials = nats_credential_fingerprint(&snapshot);
            let next_server = snapshot
                .env_value(server_url_key)
                .map(str::to_owned)
                .unwrap_or_default();
            if active_credentials == next_credentials && active_server == next_server {
                continue;
            }
            if !next_server.is_empty() && active_server != next_server {
                let parsed = match next_server.parse::<ServerAddr>() {
                    Ok(parsed) => parsed,
                    Err(error) => {
                        tracing::error!(service, server_url_key, error = %error, "reloaded NATS server URL is invalid; retaining current server pool");
                        continue;
                    }
                };
                let server_pool = [parsed];
                if let Err(error) = client.set_server_pool(&server_pool[..]).await {
                    tracing::error!(service, server_url_key, error = %error, "reloaded NATS server pool could not be applied");
                    continue;
                }
                active_server = next_server;
            }
            if active_credentials != next_credentials {
                active_credentials = next_credentials;
            }
            metrics::counter!("nats_client_rotation_total", "service" => service).increment(1);
            match client.force_reconnect().await {
                Ok(()) => tracing::info!(
                    service,
                    "NATS client reconnect requested after credential/configuration rotation"
                ),
                Err(error) => {
                    metrics::counter!("nats_client_rotation_errors_total", "service" => service)
                        .increment(1);
                    tracing::error!(service, error = %error, "NATS client reconnect request failed")
                }
            }
        }
    });
}

fn nats_credential_fingerprint(snapshot: &crate::ConfigSnapshot) -> String {
    let mut hasher = Sha256::new();
    for key in ["NATS_USERNAME", "NATS_PASSWORD"] {
        hasher.update(key.as_bytes());
        hasher.update([0]);
        if let Some(value) = snapshot.env_value(key) {
            hasher.update(value.as_bytes());
        }
        hasher.update([0]);
    }
    hex::encode(hasher.finalize())
}

fn non_empty_env(key: &str) -> Option<String> {
    env::var(key).ok().filter(|value| !value.trim().is_empty())
}
