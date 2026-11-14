#!/bin/sh
# Builds Sp9ceVerb and bundles it.
#   scripts/build.sh                build dist/Sp9ceVerb.clap, dist/Sp9ceVerb.lv2 and dist/Sp9ceVerb.app
#   scripts/build.sh --install       also copy the CLAP and the LV2 to the user plug-in folders
# Universal (arm64 and x86_64) unless SP9CE_ARCHS names one architecture.
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
. "$ROOT/scripts/sdk_env.sh"
NAME=Sp9ceVerb
BUNDLE_ID=com.opengrid.sp9ceverb
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/plugin/Cargo.toml" | head -1)"
# Bundle versions are numbers only; the plugin itself reports the full version.
VERSION="${VERSION%%-*}"

# No machine paths in the binaries.
SEP="$(printf '\037')"
CARGO_ENCODED_RUSTFLAGS="--remap-path-prefix=$ROOT=.${SEP}--remap-path-prefix=$SDK_DIR=sdk${SEP}--remap-path-prefix=$DRI_DIR=dri${SEP}--remap-path-prefix=$HOME=home"
export CARGO_ENCODED_RUSTFLAGS

cd "$ROOT/plugin"
cargo_universal --features standalone
join_archs libsp9ceverb.dylib target/universal/libsp9ceverb.dylib
join_archs sp9ceverb-app target/universal/sp9ceverb-app

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
LV2="$DIST/$NAME.lv2"
APP="$DIST/$NAME.app"
rm -rf "$CLAP" "$LV2" "$APP"
mkdir -p "$CLAP/Contents/MacOS" "$LV2" "$APP/Contents/MacOS"
cp target/universal/libsp9ceverb.dylib "$CLAP/Contents/MacOS/$NAME"
# The library records its build path as its install name; replace it.
install_name_tool -id "@loader_path/$NAME" "$CLAP/Contents/MacOS/$NAME"
plist "$NAME" BNDL "" > "$CLAP/Contents/Info.plist"
printf 'BNDL????' > "$CLAP/Contents/PkgInfo"
# The LV2 is the same library in a folder with its two description files,
# which the plugin writes from its own parameter table.
cp target/universal/libsp9ceverb.dylib "$LV2/$NAME.dylib"
install_name_tool -id "@loader_path/$NAME.dylib" "$LV2/$NAME.dylib"
"target/$(rust_target "$(uname -m)")/release/sp9ceverb-lv2-ttl" "$LV2"
cp target/universal/sp9ceverb-app "$APP/Contents/MacOS/$NAME"
plist "$NAME" APPL ".app" > "$APP/Contents/Info.plist"
printf 'APPL????' > "$APP/Contents/PkgInfo"
codesign --force --sign - "$CLAP" >/dev/null 2>&1
codesign --force --sign - "$LV2/$NAME.dylib" >/dev/null 2>&1
codesign --force --sign - "$APP" >/dev/null 2>&1
echo "built $CLAP"
echo "built $LV2"
echo "built $APP"

if [ "$1" = "--install" ]; then
  DEST="$HOME/Library/Audio/Plug-Ins/CLAP"
  mkdir -p "$DEST"
  rm -rf "$DEST/$NAME.clap"
  cp -R "$CLAP" "$DEST/"
  echo "installed $DEST/$NAME.clap"
  DEST="$HOME/Library/Audio/Plug-Ins/LV2"
  mkdir -p "$DEST"
  rm -rf "$DEST/$NAME.lv2"
  cp -R "$LV2" "$DEST/"
  echo "installed $DEST/$NAME.lv2"
fi
