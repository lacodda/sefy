#!/bin/sh
# sefy installer for macOS and Linux:
#   curl -fsSL https://raw.githubusercontent.com/lacodda/sefy/main/tools/install.sh | sh
set -eu

REPO="lacodda/sefy"

case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) TARGET="x86_64-unknown-linux-gnu" ;;
    Darwin-arm64) TARGET="aarch64-apple-darwin" ;;
    *)
        echo "No prebuilt binary for $(uname -s)/$(uname -m); install with: cargo install sefy" >&2
        exit 1
        ;;
esac

# The tag comes from the /releases/latest redirect rather than the REST API:
# unauthenticated API calls are capped at 60 per hour per IP, and an installer
# that fails because someone else on the same address ran it is no installer.
# SEFY_VERSION pins a specific release.
TAG="${SEFY_VERSION:-}"
if [ -z "$TAG" ]; then
    LOCATION=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest" || true)
    TAG="${LOCATION##*/}"
fi
case "$TAG" in
    v[0-9]*) ;;
    *)
        echo "Cannot resolve the latest release of $REPO - set SEFY_VERSION to a tag like v0.1.2" >&2
        exit 1
        ;;
esac

NAME="sefy-$TAG-$TARGET"
URL="https://github.com/$REPO/releases/download/$TAG/$NAME.tar.gz"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

echo "Downloading $URL"
curl -fsSL "$URL" | tar xz -C "$TMP"

BIN_DIR="${SEFY_INSTALL_DIR:-$HOME/.local/bin}"
mkdir -p "$BIN_DIR"
# The archive may or may not carry a top-level directory; take the binary from
# wherever it landed rather than assuming a layout.
BIN=$(find "$TMP" -type f -name sefy -perm -u+x | head -n 1)
[ -n "$BIN" ] || { echo "The archive did not contain a sefy binary" >&2; exit 1; }
install -m 755 "$BIN" "$BIN_DIR/sefy"
echo "Installed sefy $TAG to $BIN_DIR/sefy"

case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) echo "Note: add $BIN_DIR to your PATH." ;;
esac
