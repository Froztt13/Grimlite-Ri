#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
PROFILE="release"
ABI="arm64-v8a"
SO_ONLY=0

for arg in "$@"; do
    case "$arg" in
        --release) PROFILE="release" ;;
        --debug) PROFILE="debug" ;;
        --so-only) SO_ONLY=1 ;;
        --abi=*) ABI="${arg#*=}" ;;
        *)
            echo "Usage: $0 [--debug|--release] [--abi=arm64-v8a] [--so-only]" >&2
            exit 1
            ;;
    esac
done

export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
if [[ -z "${ANDROID_NDK_HOME:-}" && -d "$ANDROID_HOME/ndk" ]]; then
    LATEST_NDK="$(ls -1 "$ANDROID_HOME/ndk" | sort -V | tail -n 1)"
    export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/$LATEST_NDK"
fi

echo "==> Building libgrimlite_android.so ($ABI, $PROFILE)..."
echo "    ANDROID_NDK_HOME=${ANDROID_NDK_HOME:-<unset>}"

NDK_ARGS=(
    ndk
    --platform 26
    -t "$ABI"
    -o "$ROOT_DIR/android/app/src/main/jniLibs"
    build -p grimlite_android
    --manifest-path "$ROOT_DIR/Cargo.toml"
)
if [[ "$PROFILE" == "release" ]]; then
    NDK_ARGS+=(--release)
fi

cargo "${NDK_ARGS[@]}"

echo "==> Built: $ROOT_DIR/android/app/src/main/jniLibs/$ABI/libgrimlite_android.so"

if [[ "$SO_ONLY" -eq 0 ]]; then
    if [[ -x "$ROOT_DIR/android/gradlew" ]]; then
        TASK="assembleDebug"
        [[ "$PROFILE" == "release" ]] && TASK="assembleRelease"
        echo "==> Packaging Android APK ($TASK)..."
        (cd "$ROOT_DIR/android" && ./gradlew "$TASK")
    else
        echo "==> Note: Open $ROOT_DIR/android in Android Studio (or add Gradle wrapper) to package the APK."
    fi
fi
