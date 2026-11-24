#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

mkdir -p .runtime logs
PID_FILE=".runtime/pids.env"

if [[ -f "$PID_FILE" ]]; then
  echo "[launcher] Existing PID file detected ($PID_FILE). Stop previous instance first."
  exit 1
fi

for cmd in cargo node npm python3; do
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "[launcher] Missing required dependency: $cmd"
    exit 1
  fi
done

if [[ ! -f .env ]]; then
  echo "[launcher] Copy .env.example to .env and configure local secrets first."
  exit 1
fi

set -a
source .env
set +a

POSTGRES_PORT="${POSTGRES_PORT:-14543}"
DOC_INTEL_PORT="${DOC_INTEL_PORT:-14900}"
BACKEND_PORT="${BACKEND_PORT:-14800}"
FRONTEND_PORT="${FRONTEND_PORT:-14300}"

cleanup() {
  echo "[launcher] Stopping ABIFR child processes..."
  for pid in "${FE_PID:-}" "${BE_PID:-}" "${DOC_PID:-}" "${PG_PID:-}"; do
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
    fi
  done
  rm -f "$PID_FILE"
}
trap cleanup EXIT INT TERM

(cd backend && cargo build --bins)

./backend/target/debug/pg-manager > logs/postgres.out.log 2> logs/postgres.err.log &
PG_PID=$!
sleep 4

python3 doc-intel/server.py > logs/doc-intel.out.log 2> logs/doc-intel.err.log &
DOC_PID=$!
sleep 2

./backend/target/debug/abifr-backend > logs/backend.out.log 2> logs/backend.err.log &
BE_PID=$!
sleep 2

(cd frontend && npx next dev -H 127.0.0.1 -p "$FRONTEND_PORT") > logs/frontend.out.log 2> logs/frontend.err.log &
FE_PID=$!

cat > "$PID_FILE" <<EOF
PG_PID=$PG_PID
DOC_PID=$DOC_PID
BE_PID=$BE_PID
FE_PID=$FE_PID
EOF

echo "ABIFR running: UI=http://127.0.0.1:${FRONTEND_PORT} API=http://127.0.0.1:${BACKEND_PORT}"
wait
