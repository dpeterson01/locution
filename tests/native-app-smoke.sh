#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: the native app smoke test requires macOS" >&2
  exit 2
fi

for command_name in bun codesign ditto sqlite3; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "error: required command not found: $command_name" >&2
    exit 2
  fi
done

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
build_app="$repo_root/src-tauri/target/release/bundle/macos/Locution Acceptance.app"
normal_data_dir="$HOME/Library/Application Support/com.locution.acceptance"
normal_log_dir="$HOME/Library/Logs/com.locution.acceptance"
test_root="$(mktemp -d "${TMPDIR:-/tmp}/locution-native-smoke.XXXXXX")"
test_app="$test_root/Locution Acceptance.app"
app_pid=""

cleanup() {
  if [[ -n "$app_pid" ]] && kill -0 "$app_pid" 2>/dev/null; then
    kill "$app_pid" 2>/dev/null || true
    wait "$app_pid" 2>/dev/null || true
  fi
  rm -rf "$test_root"
}
trap cleanup EXIT

if [[ -e "$normal_data_dir" || -e "$normal_log_dir" ]]; then
  echo "error: stale acceptance data exists outside portable mode" >&2
  echo "remove these test-only paths before retrying:" >&2
  echo "  $normal_data_dir" >&2
  echo "  $normal_log_dir" >&2
  exit 1
fi

if [[ "${LOCUTION_ACCEPTANCE_SKIP_BUILD:-0}" != "1" ]]; then
  cd "$repo_root"
  CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri build --bundles app \
    --config src-tauri/tauri.acceptance.conf.json
fi

if [[ ! -d "$build_app" ]]; then
  echo "error: acceptance app bundle not found at $build_app" >&2
  exit 1
fi

ditto "$build_app" "$test_app"
binary="$test_app/Contents/MacOS/Locution"
data_dir="$test_app/Contents/MacOS/Data"
database="$data_dir/history.db"
printf '%s\n' "Handy Portable Mode" > "$test_app/Contents/MacOS/portable"
codesign --force --deep --sign - "$test_app" >/dev/null

run_smoke() {
  local run_number="$1"
  local log_file="$test_root/run-$run_number.log"

  RUST_LOG=info "$binary" --start-hidden --native-smoke-test >"$log_file" 2>&1 &
  app_pid="$!"
  if ! wait "$app_pid"; then
    app_pid=""
    echo "error: native smoke run $run_number failed" >&2
    cat "$log_file" >&2
    exit 1
  fi
  app_pid=""

  grep -Fq "[portable] data dir: $data_dir" "$log_file" || {
    echo "error: run $run_number did not activate portable mode" >&2
    cat "$log_file" >&2
    exit 1
  }
  grep -Fq "[native-smoke] startup complete" "$log_file" || {
    echo "error: run $run_number did not complete native setup" >&2
    cat "$log_file" >&2
    exit 1
  }
}

run_smoke 1

if [[ ! -f "$database" ]]; then
  echo "error: native startup did not create the history database" >&2
  exit 1
fi

feedback_columns="$(sqlite3 -noheader "$database" \
  "SELECT name FROM pragma_table_info('transcription_history') WHERE name IN ('feedback', 'feedback_updated_at') ORDER BY cid;")"
if [[ "$feedback_columns" != $'feedback\nfeedback_updated_at' ]]; then
  echo "error: feedback migrations were not applied" >&2
  exit 1
fi

sqlite3 "$database" <<'SQL'
INSERT INTO transcription_history (
  file_name,
  timestamp,
  saved,
  title,
  transcription_text,
  post_processed_text,
  post_process_prompt,
  post_process_requested,
  cleanup_mode_id,
  cleanup_mode_name,
  cleanup_model,
  cleanup_tier,
  cleanup_error,
  feedback,
  feedback_updated_at
) VALUES (
  'native-smoke.wav',
  1700000000,
  0,
  'Native smoke',
  'raw smoke text',
  'cleaned smoke text',
  'Clean up the transcript',
  1,
  'clean_up',
  'Clean up',
  'test-model',
  'short',
  NULL,
  'up',
  1700000001
);
SQL

run_smoke 2

persisted_record="$(sqlite3 -noheader "$database" \
  "SELECT feedback || '|' || transcription_text || '|' || post_processed_text FROM transcription_history WHERE file_name = 'native-smoke.wav';")"
if [[ "$persisted_record" != "up|raw smoke text|cleaned smoke text" ]]; then
  echo "error: feedback history did not survive native restart" >&2
  exit 1
fi

if [[ -e "$normal_data_dir" || -e "$normal_log_dir" ]]; then
  echo "error: native smoke test wrote outside its portable data directory" >&2
  exit 1
fi

echo "Native app smoke test passed: isolated launch, migration, and restart persistence."