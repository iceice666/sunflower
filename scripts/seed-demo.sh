#!/usr/bin/env bash
# scripts/seed-demo.sh — Deterministic demo seed for Sunflower smoke testing.
#
# Idempotent: re-running against an already-seeded DB is safe (ON CONFLICT DO NOTHING).
# On success writes .seed-env at the repo root with SUNFLOWER_DEMO_URL, _TOKEN, _DEVICE_ID.
#
# Prerequisites:
#   curl, python3 (adb/flutter are used by `just smoke-android`)
#
# Environment overrides:
#   SERVER_URL     — default http://localhost:8080   (sunflowerd must be running)
#   MEDIA_DIR      — where demo MP3 stubs are written; default /tmp/sunflower-demo-media
#   SEED_ENV       — output env file;                 default .seed-env (repo root)
#   SUNFLOWER_SETUP_TOKEN     — required if owner setup is not complete yet
#   SUNFLOWER_OWNER_PASSWORD  — owner/admin password; default local demo password

set -euo pipefail

SERVER_URL="${SERVER_URL:-http://localhost:8080}"
MEDIA_DIR="${MEDIA_DIR:-/tmp/sunflower-demo-media}"
SEED_ENV="${SEED_ENV:-.seed-env}"
OWNER_PASSWORD="${SUNFLOWER_OWNER_PASSWORD:-sunflower demo owner password}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# ── 0. Sanity checks ─────────────────────────────────────────────────────────

for cmd in curl python3; do
  command -v "$cmd" >/dev/null 2>&1 || { echo "FATAL: $cmd not found on PATH"; exit 1; }
done

echo "==> [seed] SERVER_URL   = $SERVER_URL"
echo "==> [seed] MEDIA_DIR    = $MEDIA_DIR"

# ── 1. Migration boundary ────────────────────────────────────────────────────

echo "==> [seed] Using migrations applied by the running sunflowerd."

# ── 2. Demo media stubs ──────────────────────────────────────────────────────
# Minimal ID3v2.3-tagged MP3 stubs (ID3 header + one silent MPEG frame).
# dhowden/tag reads the ID3 metadata; the audio payload is intentionally silent.

mkdir -p "$MEDIA_DIR"

python3 - "$MEDIA_DIR" <<'PYEOF'
import sys, struct, os

def syncsafe4(n: int) -> bytes:
    out = bytearray(4)
    for i in range(3, -1, -1):
        out[i] = n & 0x7F
        n >>= 7
    return bytes(out)

def id3_text_frame(fid: str, text: str) -> bytes:
    body = b"\x00" + text.encode("latin-1", errors="replace")
    return fid.encode() + struct.pack(">I", len(body)) + b"\x00\x00" + body

def write_stub_mp3(path: str, title: str, artist: str, album: str, track: int, year: int) -> None:
    frames  = id3_text_frame("TIT2", title)
    frames += id3_text_frame("TPE1", artist)
    frames += id3_text_frame("TALB", album)
    frames += id3_text_frame("TRCK", str(track))
    frames += id3_text_frame("TYER", str(year))
    # ID3v2.3 header: magic + version(2.3.0) + flags(0) + syncsafe size
    tag = b"ID3\x03\x00\x00" + syncsafe4(len(frames)) + frames
    # One MPEG1 Layer3 128kbps 44100Hz Joint-Stereo frame (417 bytes, silent payload)
    mp3_frame = bytes([0xFF, 0xFB, 0x90, 0x64]) + bytes(413)
    with open(path, "wb") as f:
        f.write(tag + mp3_frame)

media_dir = sys.argv[1]

SONGS = [
    # (filename, title, artist, album, track, year)
    ("demo-001.mp3", "Sunflower Dawn",   "Demo Artist One", "Demo Album Alpha", 1, 2024),
    ("demo-002.mp3", "Petal Drift",       "Demo Artist One", "Demo Album Alpha", 2, 2024),
    ("demo-003.mp3", "Afternoon Light",   "Demo Artist One", "Demo Album Alpha", 3, 2024),
    ("demo-004.mp3", "Golden Hour",       "Demo Artist One", "Demo Album Alpha", 4, 2024),
    ("demo-005.mp3", "Midnight Garden",   "Demo Artist Two", "Demo Album Beta",  1, 2025),
    ("demo-006.mp3", "Rain on Leaves",    "Demo Artist Two", "Demo Album Beta",  2, 2025),
    ("demo-007.mp3", "Wind Chime",        "Demo Artist Two", "Demo Album Beta",  3, 2025),
    ("demo-008.mp3", "Harvest Moon",      "Demo Artist Two", "Demo Album Beta",  4, 2025),
]

for fname, title, artist, album, track, year in SONGS:
    path = os.path.join(media_dir, fname)
    write_stub_mp3(path, title, artist, album, track, year)
    print(f"  wrote {path}")
PYEOF

echo "==> [seed] Demo MP3 stubs written to $MEDIA_DIR"

# ── 3. Admin setup/login and pairing-code creation ──────────────────────────

echo "==> [seed] Ensuring owner setup and admin login …"

SETUP_STATUS=$(curl -sf "$SERVER_URL/api/v1/setup/status")
CONFIGURED=$(python3 -c "import sys,json; print(str(json.load(sys.stdin).get('configured', False)).lower())" <<< "$SETUP_STATUS")

if [[ "$CONFIGURED" != "true" ]]; then
  if [[ -z "${SUNFLOWER_SETUP_TOKEN:-}" ]]; then
    echo "FATAL: server owner is not configured; set SUNFLOWER_SETUP_TOKEN to the token printed by sunflowerd."
    exit 1
  fi
  curl -sf \
    -X POST "$SERVER_URL/api/v1/setup/owner" \
    -H "Content-Type: application/json" \
    -d "$(python3 - <<PYEOF
import json, os
print(json.dumps({
  "setup_token": os.environ["SUNFLOWER_SETUP_TOKEN"],
  "display_name": "Owner",
  "password": os.environ.get("SUNFLOWER_OWNER_PASSWORD", "sunflower demo owner password"),
}))
PYEOF
)"
  echo "    owner setup completed"
fi

COOKIE_JAR="$(mktemp)"
trap 'rm -f "$COOKIE_JAR"' EXIT

LOGIN_RESP=$(curl -sf \
  -c "$COOKIE_JAR" \
  -X POST "$SERVER_URL/api/v1/admin/auth/login" \
  -H "Content-Type: application/json" \
  -d "$(python3 - <<PYEOF
import json, os
print(json.dumps({"password": os.environ.get("SUNFLOWER_OWNER_PASSWORD", "sunflower demo owner password")}))
PYEOF
)")
CSRF_TOKEN=$(python3 -c "import sys,json; print(json.load(sys.stdin)['csrf_token'])" <<< "$LOGIN_RESP")

create_pairing_code() {
  local label="$1"
  local ttl="$2"
  curl -sf \
    -b "$COOKIE_JAR" \
    -X POST "$SERVER_URL/api/v1/admin/pairing-codes" \
    -H "Content-Type: application/json" \
    -H "X-CSRF-Token: $CSRF_TOKEN" \
    -d "$(python3 - <<PYEOF
import json
print(json.dumps({"label": "$label", "ttl_seconds": int("$ttl")}))
PYEOF
)"
}

PAIR_RESP=$(create_pairing_code "seed-device" 600)
PAIRING_CODE=$(python3 -c "import sys,json; print(json.load(sys.stdin)['pairing_code'])" <<< "$PAIR_RESP")

SMOKE_PAIR_RESP=$(create_pairing_code "visual-smoke" 3600)
SMOKE_PAIRING_CODE=$(python3 -c "import sys,json; print(json.load(sys.stdin)['pairing_code'])" <<< "$SMOKE_PAIR_RESP")

# ── 4. Register a seed device (mints a token) ────────────────────────────────

echo "==> [seed] Pairing seed device …"

REGISTER_RESP=$(curl -sf \
  -X POST "$SERVER_URL/api/v1/auth/register-device" \
  -H "Content-Type: application/json" \
  -d "$(python3 - <<PYEOF
import json
print(json.dumps({
  "device_name": "seed-device",
  "platform": "cli",
  "client_version": "0.3.0",
  "pairing_code": "$PAIRING_CODE",
}))
PYEOF
)")

DEMO_TOKEN=$(python3 -c "import sys,json; d=json.load(sys.stdin); print(d['token'])" <<< "$REGISTER_RESP")
DEMO_DEVICE_ID=$(python3 -c "import sys,json; d=json.load(sys.stdin); print(d['device_id'])" <<< "$REGISTER_RESP")

echo "    device_id = $DEMO_DEVICE_ID"
echo "    token     = ${DEMO_TOKEN:0:14}…  (truncated)"

# ── 5. Trigger library scan ──────────────────────────────────────────────────

echo "==> [seed] Triggering library scan of $MEDIA_DIR …"

SCAN_RESP=$(curl -sf \
  -X POST "$SERVER_URL/api/v1/library/scan" \
  -H "Authorization: Bearer $DEMO_TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"roots\":[\"$MEDIA_DIR\"]}")

JOB_ID=$(python3 -c "import sys,json; d=json.load(sys.stdin); print(d['job_id'])" <<< "$SCAN_RESP")
echo "    scan job_id = $JOB_ID"

# Poll for scan completion (max 30 s)
echo -n "    waiting for scan"
for i in $(seq 1 30); do
  STATUS=$(curl -sf \
    -H "Authorization: Bearer $DEMO_TOKEN" \
    "$SERVER_URL/api/v1/jobs/$JOB_ID" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('status','unknown'))")
  if [[ "$STATUS" == "completed" ]]; then
    echo " done."
    break
  elif [[ "$STATUS" == "failed" ]]; then
    echo " ERROR — scan job failed; check sunflowerd logs."
    exit 1
  fi
  echo -n "."
  sleep 1
done

# ── 6. Verify songs in DB ────────────────────────────────────────────────────

SONG_COUNT=$(curl -sf \
  -H "Authorization: Bearer $DEMO_TOKEN" \
  "$SERVER_URL/api/v1/library/songs" | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d['songs'] if isinstance(d,dict) else d))")
echo "==> [seed] Songs in library: $SONG_COUNT"
if [[ "$SONG_COUNT" -eq 0 ]]; then
  echo "WARNING: no songs found after scan — check that sunflowerd DataDir is set and scan completed."
fi

# ── 7. Write .seed-env ───────────────────────────────────────────────────────

SEED_ENV_PATH="$REPO_ROOT/$SEED_ENV"
cat > "$SEED_ENV_PATH" <<EOF
# Generated by scripts/seed-demo.sh — do not commit.
SUNFLOWER_DEMO_URL=$SERVER_URL
SUNFLOWER_DEMO_TOKEN=$DEMO_TOKEN
SUNFLOWER_DEMO_DEVICE_ID=$DEMO_DEVICE_ID
SUNFLOWER_DEMO_PAIRING_CODE=$SMOKE_PAIRING_CODE
SUNFLOWER_DEMO_ADMIN_PASSWORD=$(printf '%q' "${SUNFLOWER_OWNER_PASSWORD:-sunflower demo owner password}")
SUNFLOWER_DEMO_SCAN_JOB=$JOB_ID
SUNFLOWER_DEMO_SONG_COUNT=$SONG_COUNT
SUNFLOWER_DEMO_MEDIA_DIR=$MEDIA_DIR
EOF

echo "==> [seed] Wrote $SEED_ENV_PATH"
echo ""
echo "    To run the Android smoke test:"
echo "      just smoke-android"
echo "    (requires Pixel_10 AVD running and sunflowerd on localhost:8080)"
