#!/bin/sh
# Fetch from the server: curl -fsSL https://host:8080/enroll.sh | sh -s -- --token T --ca-fingerprint HEX
# The fingerprint MUST come from the server log ("CA SHA-256"), not from an untrusted page.
set -eu
SERVER="${OSFM_SERVER:-https://localhost:8080}"
FINGERPRINT="${OSFM_CA_FINGERPRINT:-}"
TOKEN=""
while [ $# -gt 0 ]; do
  case "$1" in
    --token) TOKEN="$2"; shift 2 ;;
    --server) SERVER="$2"; shift 2 ;;
    --ca-fingerprint) FINGERPRINT="$2"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done
if [ -z "$TOKEN" ] || [ -z "$FINGERPRINT" ]; then
  echo "usage: enroll.sh --token <token> --ca-fingerprint <hex-from-server-log> [--server URL]" >&2
  exit 2
fi
if ! command -v osfm-edm-agent >/dev/null 2>&1; then
  echo "osfm-edm-agent not on PATH" >&2
  exit 1
fi
exec osfm-edm-agent --server "$SERVER" --token "$TOKEN" --ca-fingerprint "$FINGERPRINT"
