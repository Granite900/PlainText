#!/usr/bin/env bash
#
# One-step Linux setup for PlainText.
#
# Run this from the unzipped release folder:
#
#     bash scripts/install-linux.sh
#
# It makes the binary executable and copies `plaintext` into ~/.local/bin
# (creating that folder if needed), then tells you how to put it on PATH.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN="$SCRIPT_DIR/../plaintext"
if [ ! -f "$BIN" ]; then
    if [ -f "./plaintext" ]; then
        BIN="$(pwd)/plaintext"
    else
        echo "Couldn't find the 'plaintext' binary." >&2
        echo "Run this from the unzipped release folder (the one containing 'plaintext')." >&2
        exit 1
    fi
fi

echo "Found PlainText at: $BIN"
chmod +x "$BIN"

DEST="${HOME}/.local/bin"
mkdir -p "$DEST"
cp "$BIN" "$DEST/plaintext"
chmod +x "$DEST/plaintext"

hash -r 2>/dev/null || true

echo
if command -v plaintext >/dev/null 2>&1; then
    echo "Done â€” installed $(plaintext version)."
    echo "Try it:  plaintext run examples/basics.pt"
else
    echo "Copied to $DEST, but it isn't on your PATH yet."
    echo "Add this line to ~/.bashrc or ~/.profile, then open a new terminal:"
    echo "    export PATH=\"\$HOME/.local/bin:\$PATH\""
fi
