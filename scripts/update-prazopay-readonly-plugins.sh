#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "Usage: $0 ABSOLUTE_ZEROCLAW_CONFIG_DIR UNIQUE_BACKUP_SUFFIX" >&2
  exit 2
fi

config_dir="$1"
backup_suffix="$2"

if [[ "$config_dir" != /* ]]; then
  echo "Config directory must be an absolute path." >&2
  exit 2
fi
if [[ ! "$backup_suffix" =~ ^[A-Za-z0-9._-]+$ ]]; then
  echo "Backup suffix contains unsupported characters." >&2
  exit 2
fi

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "$script_dir/.." && pwd)"

plugins=(prazopay-status prazopay-agreement-status)

for plugin in "${plugins[@]}"; do
  source_wasm="$repo_root/plugins/$plugin/$plugin.wasm"
  destination_wasm="$config_dir/plugins/$plugin/$plugin.wasm"
  backup_wasm="$destination_wasm.$backup_suffix"

  [[ -f "$source_wasm" ]] || {
    echo "Missing source WASM: $source_wasm" >&2
    exit 1
  }
  [[ -f "$destination_wasm" ]] || {
    echo "Missing installed WASM: $destination_wasm" >&2
    exit 1
  }
  [[ ! -e "$backup_wasm" ]] || {
    echo "Refusing to overwrite existing backup: $backup_wasm" >&2
    exit 1
  }

  cp -p -- "$destination_wasm" "$backup_wasm"
  cp -- "$source_wasm" "$destination_wasm"

  source_hash="$(sha256sum -- "$source_wasm" | awk '{print $1}')"
  installed_hash="$(sha256sum -- "$destination_wasm" | awk '{print $1}')"
  [[ "$source_hash" == "$installed_hash" ]] || {
    echo "Hash verification failed for $plugin" >&2
    exit 1
  }

  echo "$plugin"
  echo "  installed: $destination_wasm"
  echo "  backup:    $backup_wasm"
  echo "  sha256:    $installed_hash"
done
