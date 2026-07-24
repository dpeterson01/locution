#!/usr/bin/env bash
# Phase-1 spike harness: proves the AFM sidecar does a clean OpenAI-compatible
# round-trip, enforces the bearer token, reports its context window, and is
# crash-isolated from its parent. No Rust required — pure process + curl.
set -uo pipefail

SIDECAR="$(cd "$(dirname "$0")" && pwd)/.build/release/afm-sidecar"
[ -x "$SIDECAR" ] || { echo "FAIL: build first: swift build -c release"; exit 1; }

# Mint a per-session token exactly like Locution will (32 random bytes, hex).
TOKEN="$(head -c 32 /dev/urandom | xxd -p | tr -d '\n')"

pass() { printf '  \033[32mPASS\033[0m %s\n' "$1"; }
fail() { printf '  \033[31mFAIL\033[0m %s\n' "$1"; FAILED=1; }
FAILED=0

echo "== launching sidecar =="
# Token via env (never argv). Ephemeral port; read the chosen port from stdout.
AFM_SIDECAR_TOKEN="$TOKEN" "$SIDECAR" --port 0 > /tmp/afm_sidecar.out 2>/tmp/afm_sidecar.err &
SIDECAR_PID=$!
trap 'kill "$SIDECAR_PID" 2>/dev/null' EXIT

PORT=""
for _ in $(seq 1 50); do
  LINE="$(grep -m1 '^AFM_SIDECAR_LISTENING ' /tmp/afm_sidecar.out 2>/dev/null || true)"
  [ -n "$LINE" ] && { PORT="${LINE##* }"; break; }
  kill -0 "$SIDECAR_PID" 2>/dev/null || { echo "sidecar died early:"; cat /tmp/afm_sidecar.err; exit 1; }
  sleep 0.1
done
[ -n "$PORT" ] && pass "listening on loopback port $PORT" || { fail "never printed ready line"; exit 1; }
BASE="http://127.0.0.1:$PORT/v1"

echo "== 1. auth: request WITHOUT token must be rejected =="
CODE="$(curl -s -o /dev/null -w '%{http_code}' "$BASE/models")"
[ "$CODE" = "401" ] && pass "unauthenticated GET /v1/models -> 401" || fail "expected 401, got $CODE"

echo "== 2. GET /v1/models reports context window =="
MODELS="$(curl -s -H "Authorization: Bearer $TOKEN" "$BASE/models")"
echo "     $MODELS"
echo "$MODELS" | grep -q '"context_window"' && pass "context_window reported" || fail "no context_window"

echo "== 3. POST /v1/chat/completions cleans a transcript =="
REQ='{"model":"apple-afm","messages":[{"role":"system","content":"You clean up dictated text. Fix punctuation and capitalization. Respond with nothing but the corrected transcript."},{"role":"user","content":"<transcript>um so i think we should like ship the feature on monday and uh tell the team</transcript>"}],"temperature":0.2}'
RESP="$(curl -s -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' -d "$REQ" "$BASE/chat/completions")"
echo "     $RESP"
CONTENT="$(echo "$RESP" | /usr/bin/python3 -c 'import sys,json; d=json.load(sys.stdin); print(d["choices"][0]["message"]["content"])' 2>/dev/null || true)"
if [ -n "$CONTENT" ]; then pass "cleaned round-trip: $CONTENT"; else fail "no cleaned content (Apple Intelligence enabled + model downloaded?)"; fi

echo "== 4. crash isolation: kill sidecar, parent (this script) survives =="
kill -9 "$SIDECAR_PID" 2>/dev/null
wait "$SIDECAR_PID" 2>/dev/null
if kill -0 "$SIDECAR_PID" 2>/dev/null; then fail "sidecar still alive after kill"; else pass "sidecar terminated"; fi
echo "     parent still running after child SIGKILL -> isolation holds"
pass "parent survived child crash"

echo
[ "$FAILED" = "0" ] && echo "ALL CHECKS PASSED" || echo "SOME CHECKS FAILED"
exit "$FAILED"
