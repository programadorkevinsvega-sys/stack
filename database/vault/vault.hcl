listener "tcp" {
  address = "0.0.0.0:8500"
  tls_disable = false
  tls_cert_file = "/vault/config/tls/server.crt"
  tls_key_file = "/vault/config/tls/server.key"
  tls_client_ca_file = "/vault/config/tls/root.crt"
  # SOLO TLS 1.3 - deshabilitar SSL y versiones antiguas
  tls_min_version = "tls13"
  tls_max_version = "tls13"
}

storage "file" {
  path = "/vault/data"
}

ui = true
disable_mlock = true