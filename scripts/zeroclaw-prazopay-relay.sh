#!/usr/bin/env bash
set -euo pipefail

action="${1:-}"
milestone="${2:-}"
channel_id="${3:-}"
config_dir="${4:-$HOME/.config/zeroclaw-entrega/creator}"
state_root="${5:-$config_dir/prazopay-monitor-state}"
listen_port="${6:-42620}"

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
relay_program="$script_dir/prazopay-monitor-relay.py"
zeroclaw_bin="${ZEROCLAW_BIN:-$HOME/.local/bin/zeroclaw}"
state_dir="$state_root/$milestone"
token_file="$state_dir/relay-token"
pid_file="$state_dir/relay.pid"
log_file="$state_dir/relay.log"

usage() {
  cat >&2 <<'EOF'
Usage:
  zeroclaw-prazopay-relay.sh start  MILESTONE CHANNEL_ID [CONFIG_DIR] [STATE_ROOT] [LISTEN_PORT]
  zeroclaw-prazopay-relay.sh stop   MILESTONE CHANNEL_ID [CONFIG_DIR] [STATE_ROOT] [LISTEN_PORT]
  zeroclaw-prazopay-relay.sh status MILESTONE CHANNEL_ID [CONFIG_DIR] [STATE_ROOT] [LISTEN_PORT]
EOF
}

validate() {
  [[ "$milestone" =~ ^[1-9A-HJ-NP-Za-km-z]{32,44}$ ]] || {
    echo "MILESTONE must be a 32-44 character base58 address." >&2
    exit 2
  }
  [[ "$channel_id" =~ ^[0-9]{17,20}$ ]] || {
    echo "CHANNEL_ID must be a 17-20 digit Discord snowflake." >&2
    exit 2
  }
  [[ "$listen_port" =~ ^[0-9]+$ ]] \
    && (( listen_port >= 1024 && listen_port <= 65535 )) || {
    echo "LISTEN_PORT must be between 1024 and 65535." >&2
    exit 2
  }
  [[ -f "$relay_program" ]] || {
    echo "Relay program not found: $relay_program" >&2
    exit 1
  }
  [[ -x "$zeroclaw_bin" ]] || {
    echo "ZeroClaw binary not found: $zeroclaw_bin" >&2
    exit 1
  }
  command -v python3 >/dev/null 2>&1 || {
    echo "python3 is required for the local delivery relay." >&2
    exit 1
  }
}

running_pid() {
  [[ -f "$pid_file" ]] || return 1
  local pid
  pid="$(<"$pid_file")"
  [[ "$pid" =~ ^[0-9]+$ ]] || return 1
  kill -0 "$pid" 2>/dev/null || return 1
  local command_line
  command_line="$(ps -p "$pid" -o args= 2>/dev/null || true)"
  [[ "$command_line" == *"prazopay-monitor-relay.py"* ]] || return 1
  printf '%s\n' "$pid"
}

validate

case "$action" in
  start)
    [[ -s "$token_file" ]] || {
      echo "Relay authorization token is missing: $token_file" >&2
      echo "Run zeroclaw-prazopay-monitor.sh install first." >&2
      exit 1
    }
    if pid="$(running_pid)"; then
      echo "PRAZOPAY_RELAY=running"
      echo "PID=$pid"
      exit 0
    fi

    mkdir -p "$state_dir"
    chmod 700 "$state_dir"
    nohup python3 "$relay_program" \
      --listen-host 127.0.0.1 \
      --listen-port "$listen_port" \
      --state-dir "$state_dir" \
      --milestone "$milestone" \
      --discord-channel "$channel_id" \
      --auth-token-file "$token_file" \
      --zeroclaw-bin "$zeroclaw_bin" \
      --config-dir "$config_dir" \
      >>"$log_file" 2>&1 </dev/null &
    pid="$!"
    printf '%s\n' "$pid" >"$pid_file"
    chmod 600 "$pid_file" "$log_file"

    for _ in 1 2 3 4 5; do
      if python3 -c \
        "import urllib.request; urllib.request.urlopen('http://127.0.0.1:$listen_port/health', timeout=1).read()" \
        >/dev/null 2>&1; then
        echo "PRAZOPAY_RELAY=running"
        echo "PID=$pid"
        exit 0
      fi
      sleep 1
    done
    if kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
    fi
    echo "PrazoPay relay did not become healthy. Check $log_file" >&2
    exit 1
    ;;
  stop)
    if pid="$(running_pid)"; then
      kill "$pid"
      for _ in 1 2 3 4 5; do
        kill -0 "$pid" 2>/dev/null || break
        sleep 1
      done
      if kill -0 "$pid" 2>/dev/null; then
        echo "Relay process $pid did not stop cleanly." >&2
        exit 1
      fi
    fi
    rm -f -- "$pid_file"
    echo "PRAZOPAY_RELAY=stopped"
    ;;
  status)
    if pid="$(running_pid)"; then
      echo "PRAZOPAY_RELAY=running"
      echo "PID=$pid"
      python3 -c \
        "import urllib.request; print(urllib.request.urlopen('http://127.0.0.1:$listen_port/health', timeout=2).read().decode())"
    else
      echo "PRAZOPAY_RELAY=stopped"
      exit 1
    fi
    ;;
  *)
    usage
    exit 2
    ;;
esac
