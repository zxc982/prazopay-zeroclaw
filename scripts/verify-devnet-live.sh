#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

rpc_url="${SOLANA_RPC_URL:-https://api.devnet.solana.com}"
exec python3 scripts/verify_devnet_live.py --rpc-url "$rpc_url" "$@"
