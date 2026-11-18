#!/bin/sh
# Toki statusLine hook: caches stdin JSON atomically and chains any original command.
TOOL="$1"
if [ -z "$TOOL" ]; then
  cat >/dev/null
  exit 0
fi
shift || true

HOOK_DIR="$HOME/.toki/hooks"
mkdir -p "$HOOK_DIR"
TMP_FILE="$HOOK_DIR/.${TOOL}.json.tmp.$$"
DEST_FILE="$HOOK_DIR/${TOOL}.json"

PAYLOAD=$(cat)
printf '%s' "$PAYLOAD" > "$TMP_FILE" && mv "$TMP_FILE" "$DEST_FILE"

if [ "$#" -gt 0 ]; then
  printf '%s' "$PAYLOAD" | "$@"
fi
