#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
fixture_dir="$repo_dir/Packages/TillerACP/Tests/TillerACPTests/Fixtures/opencode-init-turn"
scratch_dir="$(mktemp -d "${TMPDIR:-/tmp}/opencode-fixture.XXXXXX")"
server_log="$scratch_dir/server.log"
server_pid=""
password="$(openssl rand -hex 24)"

cleanup() {
    if [[ -n "$server_pid" ]] && kill -0 "$server_pid" 2>/dev/null; then
        kill "$server_pid" 2>/dev/null || true
        wait "$server_pid" 2>/dev/null || true
    fi
    rm -rf "$scratch_dir"
}
trap cleanup EXIT INT TERM

mkdir -p "$fixture_dir"

(
    cd "$scratch_dir"
    OPENCODE_SERVER_PASSWORD="$password" opencode serve --port 0 --hostname 127.0.0.1
) >"$server_log" 2>&1 &
server_pid=$!

base_url=""
for _ in $(seq 1 100); do
    base_url="$(sed -nE 's/.*(http:\/\/127\.0\.0\.1:[0-9]+).*/\1/p' "$server_log" | tail -1)"
    if [[ -n "$base_url" ]]; then
        break
    fi
    sleep 0.1
done

if [[ -z "$base_url" ]]; then
    printf 'Unable to discover opencode serve URL\n' >&2
    sed -n '1,120p' "$server_log" >&2
    exit 1
fi

auth_args=()
auth_mechanism=""
bearer_status=""
basic_username_status=""
basic_empty_status=""

bearer_status="$(curl --silent --show-error --output /dev/null --write-out '%{http_code}' \
    --header "Authorization: Bearer $password" "$base_url/config/providers")"
if [[ "$bearer_status" == 2* ]]; then
    auth_args=(--header "Authorization: Bearer $password")
    auth_mechanism="Authorization: Bearer"
else
    basic_username_status="$(curl --silent --show-error --output /dev/null --write-out '%{http_code}' \
        --user "opencode:$password" "$base_url/config/providers")"
    if [[ "$basic_username_status" == 2* ]]; then
        auth_args=(--user "opencode:$password")
        auth_mechanism="HTTP Basic (opencode username)"
    else
        basic_empty_status="$(curl --silent --show-error --output /dev/null --write-out '%{http_code}' \
            --user ":$password" "$base_url/config/providers")"
        if [[ "$basic_empty_status" == 2* ]]; then
            auth_args=(--user ":$password")
            auth_mechanism="HTTP Basic (empty username)"
        else
            printf 'Unable to authenticate: Bearer=%s Basic(opencode)=%s Basic(empty)=%s\n' \
                "$bearer_status" "$basic_username_status" "$basic_empty_status" >&2
            exit 1
        fi
    fi
fi

request() {
    local method="$1"
    local path="$2"
    local output="$3"
    shift 3
    curl --fail-with-body --silent --show-error --request "$method" \
        "${auth_args[@]}" \
        --header 'Content-Type: application/json' \
        "$base_url$path" "$@" >"$output"
}

request POST /session "$fixture_dir/session-create.json" \
    --data '{"title":"Task 9 fixture"}'
session_id="$(/usr/bin/python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["id"])' "$fixture_dir/session-create.json")"

request GET /config/providers "$fixture_dir/providers.json"

curl --fail-with-body --silent --show-error --no-buffer --max-time 20 \
    "${auth_args[@]}" \
    --header 'Accept: text/event-stream' \
    "$base_url/event" >"$fixture_dir/events.sse" &
event_pid=$!

sleep 1
request POST "/session/$session_id/message" "$fixture_dir/message-response.json" \
    --data '{"parts":[{"type":"text","text":"Reply with the single word: done"}]}' || true

sleep 8
request POST "/session/$session_id/abort" "$fixture_dir/abort-response.json" || true
wait "$event_pid" 2>/dev/null || true

# Provider responses can contain API keys or bearer tokens. Preserve provider/model
# ids while replacing all obvious secret-looking values before the fixture is saved.
/usr/bin/python3 - "$fixture_dir/providers.json" <<'PY'
import json
import sys

path = sys.argv[1]
with open(path) as handle:
    value = json.load(handle)

secret_names = {"apiKey", "api_key", "apikey", "key", "token", "accessToken", "access_token", "password", "secret"}

def redact(item):
    if isinstance(item, dict):
        return {key: ("<REDACTED>" if key in secret_names else redact(child)) for key, child in item.items()}
    if isinstance(item, list):
        return [redact(child) for child in item]
    return item

with open(path, "w") as handle:
    json.dump(redact(value), handle, indent=2, sort_keys=True)
    handle.write("\n")
PY

printf 'Captured OpenCode fixture in %s\n' "$fixture_dir"
printf 'Session id: %s\n' "$session_id"
printf 'Auth mechanism: %s\n' "$auth_mechanism"
printf 'Server log:\n'
sed -n '1,40p' "$server_log"
