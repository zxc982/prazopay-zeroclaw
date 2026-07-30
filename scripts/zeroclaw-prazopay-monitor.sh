#!/usr/bin/env bash
set -euo pipefail

action="${1:-}"
milestone="${2:-}"
channel_id="${3:-}"
interval_minutes="${4:-5}"
alert_before_secs="${5:-300}"
config_dir="${6:-$HOME/.config/zeroclaw-entrega/creator}"
notification_window_secs="${7:-$((interval_minutes * 60 + 120))}"
relay_port="${8:-42620}"
webhook_port="${9:-42619}"
state_root="${10:-$config_dir/prazopay-monitor-state}"

zeroclaw_bin="${ZEROCLAW_BIN:-$HOME/.local/bin/zeroclaw}"
agent_alias="creator"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
relay_script="$script_dir/zeroclaw-prazopay-relay.sh"

usage() {
  cat >&2 <<'EOF'
Usage:
  zeroclaw-prazopay-monitor.sh install MILESTONE CHANNEL_ID [INTERVAL_MINUTES] [ALERT_BEFORE_SECS] [CONFIG_DIR] [NOTIFICATION_WINDOW_SECS] [RELAY_PORT] [WEBHOOK_PORT] [STATE_ROOT]
  zeroclaw-prazopay-monitor.sh enable  MILESTONE CHANNEL_ID [INTERVAL_MINUTES] [ALERT_BEFORE_SECS] [CONFIG_DIR] [NOTIFICATION_WINDOW_SECS] [RELAY_PORT] [WEBHOOK_PORT] [STATE_ROOT]
  zeroclaw-prazopay-monitor.sh disable MILESTONE CHANNEL_ID [INTERVAL_MINUTES] [ALERT_BEFORE_SECS] [CONFIG_DIR] [NOTIFICATION_WINDOW_SECS] [RELAY_PORT] [WEBHOOK_PORT] [STATE_ROOT]
  zeroclaw-prazopay-monitor.sh status  MILESTONE CHANNEL_ID [INTERVAL_MINUTES] [ALERT_BEFORE_SECS] [CONFIG_DIR] [NOTIFICATION_WINDOW_SECS] [RELAY_PORT] [WEBHOOK_PORT] [STATE_ROOT]

The monitor keeps ZeroClaw's native heartbeat and read-only prazopay_status
tool. Outbound cards pass through a loopback-only durable relay before Discord:
event IDs are committed only after successful delivery, duplicate stages are
suppressed, and terminal outcomes remain retryable until acknowledged.
EOF
}

require_zeroclaw() {
  [[ -x "$zeroclaw_bin" ]] || {
    echo "ZeroClaw binary not found: $zeroclaw_bin" >&2
    exit 1
  }
}

set_config() {
  local path="$1"
  local value="$2"
  "$zeroclaw_bin" config set "$path" "$value" \
    --no-interactive \
    --config-dir "$config_dir" >/dev/null
}

show_status() {
  "$zeroclaw_bin" config list \
    --filter heartbeat \
    --config-dir "$config_dir"
  echo
  "$zeroclaw_bin" config get \
    "risk_profiles.locked_down.allowed_tools" \
    --config-dir "$config_dir"
  echo
  "$relay_script" status \
    "$milestone" "$channel_id" "$config_dir" "$state_root" "$relay_port" || true
}

require_zeroclaw

case "$action" in
  install)
    if [[ ! "$milestone" =~ ^[1-9A-HJ-NP-Za-km-z]{32,44}$ ]]; then
      echo "MILESTONE must be a 32-44 character base58 address." >&2
      exit 2
    fi
    if [[ ! "$channel_id" =~ ^[0-9]{17,20}$ ]]; then
      echo "CHANNEL_ID must be a 17-20 digit Discord snowflake." >&2
      exit 2
    fi
    if [[ ! "$interval_minutes" =~ ^[0-9]+$ ]] \
      || (( interval_minutes < 1 || interval_minutes > 60 )); then
      echo "INTERVAL_MINUTES must be between 1 and 60." >&2
      exit 2
    fi
    if [[ ! "$alert_before_secs" =~ ^[0-9]+$ ]] \
      || (( alert_before_secs < 30 || alert_before_secs > 86400 )); then
      echo "ALERT_BEFORE_SECS must be between 30 and 86400." >&2
      exit 2
    fi
    if [[ ! "$notification_window_secs" =~ ^[0-9]+$ ]] \
      || (( notification_window_secs < 60 || notification_window_secs > 3600 )); then
      echo "NOTIFICATION_WINDOW_SECS must be between 60 and 3600." >&2
      exit 2
    fi
    for port in "$relay_port" "$webhook_port"; do
      if [[ ! "$port" =~ ^[0-9]+$ ]] || (( port < 1024 || port > 65535 )); then
        echo "Relay and webhook ports must be between 1024 and 65535." >&2
        exit 2
      fi
    done
    if [[ "$relay_port" == "$webhook_port" ]]; then
      echo "RELAY_PORT and WEBHOOK_PORT must differ." >&2
      exit 2
    fi
    [[ -x "$relay_script" || -f "$relay_script" ]] || {
      echo "Reliable delivery relay script is missing: $relay_script" >&2
      exit 1
    }
    command -v python3 >/dev/null 2>&1 || {
      echo "python3 is required for reliable delivery." >&2
      exit 1
    }

    skill_bundles="$(
      "$zeroclaw_bin" config get "agents.$agent_alias.skill_bundles" \
        --config-dir "$config_dir"
    )"
    if ! grep -Fq '"prazopay"' <<<"$skill_bundles"; then
      echo "PrazoPay skill is not enabled for creator." >&2
      echo "Run: ./scripts/zeroclaw-prazopay-skill.sh enable \"$config_dir\"" >&2
      exit 1
    fi

    auto_approve="$(
      "$zeroclaw_bin" config get "risk_profiles.locked_down.auto_approve" \
        --config-dir "$config_dir"
    )"
    if ! grep -Fq '"prazopay_status"' <<<"$auto_approve"; then
      echo "prazopay_status is not approved for unattended read-only calls." >&2
      echo "Run: ./scripts/zeroclaw-prazopay-approval.sh enable \"$config_dir\"" >&2
      exit 1
    fi

    prompt="Act as the PrazoPay Active Monitor. Copy this milestone exactly, then call prazopay_status exactly once with cluster devnet, milestone $milestone, alert_before_secs $alert_before_secs, and poll_interval_secs $notification_window_secs. The tool JSON is the sole source of truth. If the tool call fails, reply exactly NO_REPLY[FAIL]: PRAZOPAY_STATUS_UNAVAILABLE. If monitor.should_notify is false, reply exactly NO_REPLY. If monitor.should_notify is true, produce one compact English Discord card. Do not use a Markdown table; use a compact bullet list followed by a Next action section. Use heading 'PrazoPay Final Outcome' for SETTLEMENT_SUCCESS or MILESTONE_FAILED, 'PrazoPay Delay Alert' when monitor.event_code contains DELAYED, and 'PrazoPay Active Alert' otherwise. Always include protocol version, acceptance policy, shortened milestone, status, monitor.event_code, severity, responsible role, monitor.reminder_stage, seconds_to_boundary, currently allowed action names, the full monitor.event_id on a line beginning exactly 'Event ID:', and the Solana Explorer account URL. A final outcome card must also include outcome, amount_lamports, shortened funder, shortened worker, terminal_at, and the sentence 'This is the final notification; monitoring for this milestone is closed.' A delay card must identify the overdue obligation exactly: worker delivery or funder review. Funder review reminders continue on their own sparse schedule while the milestone remains unresolved. For PERMISSIONLESS_SETTLEMENT_READY or FUNDER_REVIEW_DELAYED_SETTLEMENT_READY, state that the silence policy is complete, any trigger may finalize the transaction, funds can go only to the immutable worker, and the worker is not overdue. If funder review and settlement readiness hit in one poll, produce one combined card, not two messages. State that each overdue schedule sends Discord reminders only at first delay, 30 minutes, 2 hours, then daily. State that ZeroClaw checks every $interval_minutes minute(s), uses a $notification_window_secs-second observation window to tolerate scheduler and model latency, and alerts only at state-entry, deadline, sparse escalation, permissionless settlement readiness, or the single final outcome. Never describe the polling interval as a fixed reminder interval. Never expose complete funder or worker addresses. Never request wallet material, sign, simulate, submit a transaction, or call any other tool. Respond only in English."

    state_dir="$state_root/$milestone"
    token_file="$state_dir/relay-token"
    mkdir -p "$state_dir"
    chmod 700 "$state_dir"
    if [[ ! -s "$token_file" ]]; then
      umask 077
      python3 -c 'import secrets; print(secrets.token_hex(32))' >"$token_file"
    fi
    relay_token="$(<"$token_file")"
    if [[ ${#relay_token} -lt 32 ]]; then
      echo "Could not create a valid local relay token." >&2
      exit 1
    fi

    # Keep heartbeat disabled until every required field has been validated and
    # written and the durable relay is healthy. A partially configured worker
    # can therefore never run or deliver directly to Discord.
    set_config heartbeat.enabled false
    set_config heartbeat.agent "$agent_alias"
    set_config heartbeat.interval_minutes "$interval_minutes"
    set_config heartbeat.two_phase false
    set_config heartbeat.message "$prompt"
    # ZeroClaw 0.8.3 validates heartbeat targets by bare channel type. With
    # exactly one enabled webhook alias, its live registry exposes both
    # `webhook.default` and the required compatibility key `webhook`.
    set_config heartbeat.target webhook
    set_config heartbeat.to "$channel_id"
    set_config heartbeat.adaptive false
    set_config heartbeat.load_session_context false
    set_config heartbeat.task_timeout_secs 120

    # This alias must be enabled so ZeroClaw registers the bare `webhook` key.
    # The otherwise-unused inbound listener is protected by a random HMAC
    # secret; outbound delivery is additionally authenticated to the relay.
    set_config channels.webhook.default.enabled true
    set_config channels.webhook.default.port "$webhook_port"
    set_config channels.webhook.default.listen_path "/prazopay-unused"
    set_config channels.webhook.default.send_url "http://127.0.0.1:$relay_port/heartbeat"
    set_config channels.webhook.default.send_method POST
    set_config channels.webhook.default.auth_header "Bearer $relay_token"
    set_config channels.webhook.default.secret "$relay_token"
    set_config channels.webhook.default.max_retries 3
    set_config channels.webhook.default.retry_base_delay_ms 500
    set_config channels.webhook.default.retry_max_delay_ms 5000

    # The heartbeat API has no per-task allowlist in ZeroClaw 0.8.3. Narrow
    # this dedicated Creator agent's risk profile to the one required tool.
    set_config risk_profiles.locked_down.allowed_tools '["prazopay_status"]'
    set_config risk_profiles.locked_down.auto_approve '["prazopay_status"]'

    "$relay_script" start \
      "$milestone" "$channel_id" "$config_dir" "$state_root" "$relay_port"
    set_config heartbeat.enabled true

    echo "PrazoPay heartbeat monitor and durable Discord relay installed."
    echo "Restart the creator daemon to load it; the first tick runs after one configured interval."
    show_status
    ;;
  enable|disable)
    if [[ ! "$milestone" =~ ^[1-9A-HJ-NP-Za-km-z]{32,44}$ ]] \
      || [[ ! "$channel_id" =~ ^[0-9]{17,20}$ ]]; then
      echo "enable/disable requires the installed MILESTONE and CHANNEL_ID." >&2
      exit 2
    fi
    enabled=false
    if [[ "$action" == "enable" ]]; then
      "$relay_script" start \
        "$milestone" "$channel_id" "$config_dir" "$state_root" "$relay_port"
      enabled=true
    else
      "$relay_script" stop \
        "$milestone" "$channel_id" "$config_dir" "$state_root" "$relay_port"
    fi
    set_config heartbeat.enabled "$enabled"
    echo "PrazoPay heartbeat monitor $action completed."
    echo "Restart the creator daemon to apply it."
    show_status
    ;;
  status)
    if [[ ! "$milestone" =~ ^[1-9A-HJ-NP-Za-km-z]{32,44}$ ]] \
      || [[ ! "$channel_id" =~ ^[0-9]{17,20}$ ]]; then
      echo "status requires the installed MILESTONE and CHANNEL_ID." >&2
      exit 2
    fi
    show_status
    ;;
  *)
    usage
    exit 2
    ;;
esac
