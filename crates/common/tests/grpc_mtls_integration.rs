use common::ServiceTlsConfig;
use rcgen::{
    BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use std::fs;
use tokio::sync::oneshot;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Endpoint, Server};

fn write_identity(directory: &std::path::Path) {
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

    let mut leaf_params =
        CertificateParams::new(vec!["localhost".into(), "tls-test-service".into()]).unwrap();
    leaf_params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    leaf_params.extended_key_usages = vec![
        ExtendedKeyUsagePurpose::ServerAuth,
        ExtendedKeyUsagePurpose::ClientAuth,
    ];
    let leaf_key = KeyPair::generate().unwrap();
    let leaf = leaf_params.signed_by(&leaf_key, &issuer).unwrap();

    fs::create_dir_all(directory).unwrap();
    fs::write(directory.join("server.crt"), leaf.pem()).unwrap();
    fs::write(directory.join("server.key"), leaf_key.serialize_pem()).unwrap();
    fs::write(directory.join("ca.crt"), ca.pem()).unwrap();
}

#[tokio::test]
async fn establishes_grpc_mutual_tls_with_shared_configuration() {
    let temp = tempfile::tempdir().unwrap();
    write_identity(&temp.path().join("tls-test-service"));

    std::env::set_var("TLS_MODE", "mtls");
    std::env::set_var("TLS_CERT_PROVIDER", "local");
    std::env::set_var("TLS_DIR", temp.path());
    std::env::set_var("TLS_MIN_VALIDITY_SECONDS", "60");

    let config = ServiceTlsConfig::from_env("tls-test-service").unwrap();
    let server_tls = common::tls::load_server_tls(&config).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let incoming = TcpListenerStream::new(listener);
    let (_reporter, health_service) = tonic_health::server::health_reporter();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    let server = tokio::spawn(async move {
        Server::builder()
            .tls_config(server_tls)
            .unwrap()
            .add_service(health_service)
            .serve_with_incoming_shutdown(incoming, async {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });

    let endpoint_uri = format!("https://localhost:{}", address.port());
    let endpoint = Endpoint::from_shared(endpoint_uri.clone()).unwrap();
    let endpoint = common::tls::configure_client_endpoint(endpoint, &endpoint_uri, &config)
        .await
        .unwrap();
    let channel = endpoint.connect().await.unwrap();
    let mut client = tonic_health::pb::health_client::HealthClient::new(channel);
    client
        .check(tonic_health::pb::HealthCheckRequest {
            service: String::new(),
        })
        .await
        .unwrap();

    let _ = shutdown_tx.send(());
    server.await.unwrap();
}
