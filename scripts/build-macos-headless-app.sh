#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
APP_NAME="Grimlite Ri Headless"
BIN_NAME="grimlite-headless"

PROFILE="debug"
DO_BUILD=1
DO_RUN=0

for arg in "$@"; do
    case "$arg" in
        --release) PROFILE="release" ;;
        --debug) PROFILE="debug" ;;
        --no-build) DO_BUILD=0 ;;
        --run) DO_RUN=1 ;;
        *)
            echo "Usage: $0 [--debug|--release] [--no-build] [--run]" >&2
            exit 1
            ;;
    esac
done

BUNDLE_DIR="$ROOT_DIR/target/$PROFILE/bundle/${APP_NAME}.app"
CONTENTS_DIR="$BUNDLE_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"

if [[ "$DO_BUILD" -eq 1 ]]; then
    echo "==> Building $BIN_NAME ($PROFILE)..."
    CARGO_ARGS=(
        build -p "$BIN_NAME"
        --manifest-path "$ROOT_DIR/Cargo.toml"
    )
    if [[ "$PROFILE" == "release" ]]; then
        CARGO_ARGS+=(--release)
    fi
    cargo "${CARGO_ARGS[@]}"
fi

BIN_PATH="$ROOT_DIR/target/$PROFILE/$BIN_NAME"
if [[ ! -f "$BIN_PATH" ]]; then
    echo "Error: Binary not found at $BIN_PATH" >&2
    exit 1
fi

echo "==> Assembling ${APP_NAME}.app ($PROFILE)..."
mkdir -p "$MACOS_DIR" "$RESOURCES_DIR"

cp "$BIN_PATH" "$MACOS_DIR/$BIN_NAME"
chmod +x "$MACOS_DIR/$BIN_NAME"
cp "$ROOT_DIR/headless/packages/macOS/Contents/Info.plist" "$CONTENTS_DIR/Info.plist"
printf 'APPL????' > "$CONTENTS_DIR/PkgInfo"

# Reuse pre-generated .icns icon from app/assets
ICNS_SRC="$ROOT_DIR/app/assets/GrimliteRi.icns"
if [[ -f "$ICNS_SRC" ]]; then
    cp "$ICNS_SRC" "$RESOURCES_DIR/GrimliteRi.icns"
fi

# Ad-hoc sign so macOS / Apple Silicon launches cleanly
ENTITLEMENTS="$ROOT_DIR/app/assets/macOSEntitlements.plist"
if command -v codesign >/dev/null 2>&1; then
    if [[ -f "$ENTITLEMENTS" ]]; then
        codesign --force --deep --sign - --entitlements "$ENTITLEMENTS" "$BUNDLE_DIR" 2>/dev/null
    else
        codesign --force --deep --sign - "$BUNDLE_DIR" 2>/dev/null
    fi
fi

# Touch bundle root so macOS Finder/Dock refreshes the icon cache
touch "$BUNDLE_DIR"

echo "==> Ready: $BUNDLE_DIR"

if [[ "$DO_RUN" -eq 1 ]]; then
    open "$BUNDLE_DIR"
fi
