#!/usr/bin/env bash
set -euo pipefail

trace_id="${1:?usage: $0 <trace-id> [runtime-trace.jsonl]}"
source_trace="${2:-$HOME/.config/zeroclaw-entrega/creator/data/state/runtime-trace.jsonl}"
output="target/devnet/zeroclaw-trace.jsonl"

mkdir -p "$(dirname "$output")"
grep -F "$trace_id" "$source_trace" >"$output"

line_count="$(wc -l <"$output")"
grep -Fq '"message":"tool_call_start"' "$output"
grep -Fq '"message":"tool_call_result"' "$output"
grep -Fq '"message":"turn_final_response"' "$output"

echo "ZEROCLAW_TRACE_CAPTURE=PASS"
echo "TRACE_ID=$trace_id"
echo "TRACE_LINES=$line_count"
echo "OUTPUT=$output"
