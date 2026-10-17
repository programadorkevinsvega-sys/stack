#!/bin/sh
set -eu

DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
cd "$DIR"

CA_KEY="${CA_KEY:-$DIR/root.key}"
CA_CERT="${CA_CERT:-$DIR/root.crt}"
SERVER_KEY="${SERVER_KEY:-$DIR/server.key}"
SERVER_CRT="${SERVER_CRT:-$DIR/server.crt}"
SERVER_CSR="${SERVER_CSR:-$DIR/server.csr}"
SERVER_EXT="${SERVER_EXT:-$DIR/server.ext}"

if [ ! -f "$CA_KEY" ] || [ ! -f "$CA_CERT" ]; then
  echo "[nats-renew] missing CA files. Run ./01-generate-ca.sh first." >&2
  exit 1
fi

rm -f "$SERVER_KEY" "$SERVER_CRT" "$SERVER_CSR" "$SERVER_EXT"

"$DIR/02-generate-server-cert.sh" "$@"
