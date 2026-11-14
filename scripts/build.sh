#!/bin/sh
# Builds Sp9ceVerb and bundles it.
#   scripts/build.sh                build dist/Sp9ceVerb.clap and dist/Sp9ceVerb.app
#   scripts/build.sh --install       also copy the CLAP to the user plug-in folder
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
. "$ROOT/scripts/sdk_env.sh"
NAME=Sp9ceVerb
BUNDLE_ID=com.opengrid.sp9ceverb
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/plugin/Cargo.toml" | head -1)"

# No machine paths in the binaries.
SEP="$(printf '\037')"
CARGO_ENCODED_RUSTFLAGS="--remap-path-prefix=$ROOT=.${SEP}--remap-path-prefix=$SDK_DIR=sdk${SEP}--remap-path-prefix=$DRI_DIR=dri${SEP}--remap-path-prefix=$HOME=home"
export CARGO_ENCODED_RUSTFLAGS

cd "$ROOT/plugin"
cargo_sdk build --release --features standalone

plist() { # executable, package type, extra keys
  cat <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleExecutable</key><string>$1</string>
  <key>CFBundleIdentifier</key><string>$BUNDLE_ID$3</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>$NAME</string>
  <key>CFBundlePackageType</key><string>$2</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSMicrophoneUsageDescription</key><string>$NAME can process the audio input when started with --input.</string>
</dict>
</plist>
PLIST
}

DIST="$ROOT/dist"
CLAP="$DIST/$NAME.clap"
APP="$DIST/$NAME.app"
rm -rf "$CLAP" "$APP"
mkdir -p "$CLAP/Contents/MacOS" "$APP/Contents/MacOS"
cp target/release/libsp9ceverb.dylib "$CLAP/Contents/MacOS/$NAME"
# The library records its build path as its install name; replace it.
install_name_tool -id "@loader_path/$NAME" "$CLAP/Contents/MacOS/$NAME"
plist "$NAME" BNDL "" > "$CLAP/Contents/Info.plist"
printf 'BNDL????' > "$CLAP/Contents/PkgInfo"
cp target/release/sp9ceverb-app "$APP/Contents/MacOS/$NAME"
plist "$NAME" APPL ".app" > "$APP/Contents/Info.plist"
printf 'APPL????' > "$APP/Contents/PkgInfo"
codesign --force --sign - "$CLAP" >/dev/null 2>&1
codesign --force --sign - "$APP" >/dev/null 2>&1
echo "built $CLAP"
echo "built $APP"

if [ "$1" = "--install" ]; then
  DEST="$HOME/Library/Audio/Plug-Ins/CLAP"
  mkdir -p "$DEST"
  rm -rf "$DEST/$NAME.clap"
  cp -R "$CLAP" "$DEST/"
  echo "installed $DEST/$NAME.clap"
fi
