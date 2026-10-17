#!/usr/bin/env bash
set -euo pipefail

printf '%s\n' 'This script is retired: AWS Secrets Manager and SSM are the only runtime providers.' >&2
printf '%s\n' 'No Vault fallback or Vault migration is permitted. Use AWS_RUNTIME_SECRET_ID and AWS_PARAMETER_PREFIX.' >&2
exit 1
