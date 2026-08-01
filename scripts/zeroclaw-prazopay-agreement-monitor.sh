#!/usr/bin/env bash
set -euo pipefail

action="${1:-}"
agreement="${2:-}"
channel_id="${3:-}"
shift_count=3

if (( $# < shift_count )); then
  cat >&2 <<'EOF'
Usage:
  zeroclaw-prazopay-agreement-monitor.sh <install|enable|disable|status> AGREEMENT CHANNEL_ID [INTERVAL_MINUTES] [ALERT_BEFORE_SECS] [CONFIG_DIR] [NOTIFICATION_WINDOW_SECS] [RELAY_PORT] [WEBHOOK_PORT] [STATE_ROOT]
EOF
  exit 2
fi

shift "$shift_count"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PRAZOPAY_MONITOR_KIND=agreement \
  bash "$script_dir/zeroclaw-prazopay-monitor.sh" \
  "$action" "$agreement" "$channel_id" "$@"
