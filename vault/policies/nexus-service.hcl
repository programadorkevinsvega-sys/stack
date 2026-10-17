path "pki-internal/issue/nexus-service-mtls" {
  capabilities = ["create", "update"]
}

path "pki-internal/cert/ca" {
  capabilities = ["read"]
}

path "secret/data/dev/nexus/runtime/*" {
  capabilities = ["read"]
}

path "secret/metadata/dev/nexus/runtime/*" {
  capabilities = ["read", "list"]
}
