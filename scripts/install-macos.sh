#!/usr/bin/env bash
#
# One-step macOS setup for PlainText.
#
# Run this from the unzipped release folder:
#
#     bash scripts/install-macos.sh
#
# It does the three fiddly things by hand so you don't have to:
#   1. clears the macOS "downloaded from the internet" quarantine flag,
#   2. makes the binary executable,
#   3. copies `plaintext` into /usr/local/bin, which is already on your PATH —
#      so you can just type `plaintext` from anywhere afterwards.

set -euo pipefail

# Find the `plaintext` binary. It sits one level up from this script inside the
# release zip; fall back to the current folder if you moved things around.
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

# 1 + 2: unblock it and make it runnable.
xattr -dr com.apple.quarantine "$BIN" 2>/dev/null || true
chmod +x "$BIN"

# 3: install onto the PATH. /usr/local/bin is on the default macOS PATH.
DEST="/usr/local/bin"
if [ -w "$DEST" ] || { [ ! -e "$DEST" ] && [ -w "$(dirname "$DEST")" ]; }; then
    mkdir -p "$DEST"
    cp "$BIN" "$DEST/plaintext"
    chmod +x "$DEST/plaintext"
else
    echo "Installing to $DEST (macOS may ask for your login password)…"
    sudo mkdir -p "$DEST"
    sudo cp "$BIN" "$DEST/plaintext"
    sudo chmod +x "$DEST/plaintext"
fi

hash -r 2>/dev/null || true

echo
if command -v plaintext >/dev/null 2>&1; then
    echo "Done — installed $(plaintext version)."
    echo "Try it:  plaintext run examples/basics.pt"
else
    echo "Copied to $DEST, but it isn't on your PATH yet."
    echo "Add this line to ~/.zshrc, then open a new terminal:"
    echo "    export PATH=\"$DEST:\$PATH\""
fi
