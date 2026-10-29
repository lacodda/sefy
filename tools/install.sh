#!/bin/sh
# sefy installer for macOS and Linux:
#   curl -fsSL https://raw.githubusercontent.com/lacodda/sefy/main/tools/install.sh | sh
set -eu

REPO="lacodda/sefy"

case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) TARGET="x86_64-unknown-linux-gnu" ;;
    Darwin-arm64) TARGET="aarch64-apple-darwin" ;;
    # Git Bash, MSYS2 and Cygwin run this script happily on Windows, where it
    # has no business being: the Windows build is installed by install.ps1.
    # Saying so beats the generic "no prebuilt binary" dead end, since a
    # Windows release does exist - just not for this installer.
    MINGW*|MSYS*|CYGWIN*)
        echo "This is the macOS/Linux installer, but you are on Windows ($(uname -s))." >&2
        echo "Install with PowerShell instead:" >&2
        echo "  irm https://raw.githubusercontent.com/$REPO/main/tools/install.ps1 | iex" >&2
        exit 1
        ;;
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
        echo "Cannot resolve the latest release of $REPO - set SEFY_VERSION to a tag like v0.18.0" >&2
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

# Transports go where sefy looks for them, not beside the binary: a plugins
# directory next to the vault would annotate a file that gives nothing away,
# and one on PATH would be found but is not where sefy documents them.
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
case "$(uname -s)" in
    Darwin) DATA_DIR="$HOME/Library/Application Support" ;;
esac
PLUGIN_DIR="$DATA_DIR/sefy/plugins"

for PLUGIN in $(find "$TMP" -type f -name 'sefy-plugin-*' -perm -u+x); do
    mkdir -p "$PLUGIN_DIR"
    install -m 755 "$PLUGIN" "$PLUGIN_DIR/$(basename "$PLUGIN")"
    echo "Installed $(basename "$PLUGIN") to $PLUGIN_DIR"
done

case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) echo "Note: add $BIN_DIR to your PATH." ;;
esac

# The desktop app. A window that is open holds an unlocked vault, and
# replacing the program under it is the person's to do, so a running one is
# left as it is and the note says how to finish.
VERSION="${TAG#v}"
RELEASE="https://github.com/$REPO/releases/download/$TAG"
if pgrep -x sefy-gui >/dev/null 2>&1 || pgrep -f 'sefy-gui\.AppImage' >/dev/null 2>&1; then
    echo "Note: the desktop app is open - lock it, close it and run the installer again to update it."
    exit 0
fi

case "$(uname -s)" in
    Darwin)
        APPS="${SEFY_APPLICATIONS:-$HOME/Applications}"
        URL="$RELEASE/sefy_${VERSION}_aarch64.app.tar.gz"
        echo "Downloading $URL"
        curl -fsSL "$URL" | tar xz -C "$TMP"
        [ -d "$TMP/sefy.app" ] || { echo "The archive did not contain sefy.app" >&2; exit 1; }
        mkdir -p "$APPS"
        rm -rf "$APPS/sefy.app"
        mv "$TMP/sefy.app" "$APPS/sefy.app"
        # Downloaded by curl it carries no quarantine flag; taken from a
        # browser it would, and an unsigned app is then refused outright.
        xattr -dr com.apple.quarantine "$APPS/sefy.app" 2>/dev/null || true
        INSTALLED=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APPS/sefy.app/Contents/Info.plist")
        WHERE="$APPS/sefy.app"
        ;;
    *)
        URL="$RELEASE/sefy_${VERSION}_amd64.AppImage"
        echo "Downloading $URL"
        curl -fsSL -o "$TMP/sefy-gui.AppImage" "$URL"
        mkdir -p "$DATA_DIR/sefy"
        install -m 755 "$TMP/sefy-gui.AppImage" "$DATA_DIR/sefy/sefy-gui.AppImage"
        INSTALLED="$VERSION"
        WHERE="$DATA_DIR/sefy/sefy-gui.AppImage"
        ln -sf "$WHERE" "$BIN_DIR/sefy-gui"

        # The menu entry. It names the program and nothing else: no file type
        # is claimed, since a vault has none.
        mkdir -p "$DATA_DIR/applications" "$DATA_DIR/icons/hicolor/512x512/apps"
        curl -fsSL -o "$DATA_DIR/icons/hicolor/512x512/apps/sefy.png" \
            "https://raw.githubusercontent.com/$REPO/$TAG/assets/logo-512.png" || true
        cat > "$DATA_DIR/applications/sefy.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=sefy
Comment=An inconspicuous encrypted vault.
Exec="$WHERE" %f
Icon=sefy
Terminal=false
Categories=Utility;Security;
DESKTOP
        if command -v update-desktop-database >/dev/null 2>&1; then
            update-desktop-database "$DATA_DIR/applications" >/dev/null 2>&1 || true
        fi
        FUSE=""
        for lib in /lib/libfuse.so.2* /lib64/libfuse.so.2* /usr/lib/libfuse.so.2* /usr/lib64/libfuse.so.2* \
            /lib/*-linux-gnu/libfuse.so.2* /usr/lib/*-linux-gnu/libfuse.so.2*; do
            [ -e "$lib" ] && FUSE="$lib" && break
        done
        if [ -z "$FUSE" ]; then
            echo "Note: the desktop app needs FUSE 2 to start (Debian/Ubuntu: sudo apt install libfuse2)."
        fi
        ;;
esac

if [ "$INSTALLED" != "$VERSION" ]; then
    echo "Installed $WHERE, but it is version $INSTALLED rather than $VERSION" >&2
    exit 1
fi
echo "Installed the sefy desktop app $TAG to $WHERE"
