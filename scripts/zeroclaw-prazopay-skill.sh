#!/usr/bin/env bash
set -euo pipefail

mode="${1:-}"
config_dir="${2:-$HOME/.config/zeroclaw-entrega/creator}"
zeroclaw_bin="${ZEROCLAW_BIN:-$HOME/.local/bin/zeroclaw}"
path="agents.creator.skill_bundles"

case "$mode" in
  enable)
    value='["prazopay"]'
    ;;
  disable)
    value='[]'
    ;;
  *)
    echo "usage: $0 <enable|disable> [config-dir]" >&2
    exit 2
    ;;
esac

"$zeroclaw_bin" config set \
  "$path" \
  "$value" \
  --no-interactive \
  --config-dir "$config_dir"

"$zeroclaw_bin" config get "$path" --config-dir "$config_dir"
