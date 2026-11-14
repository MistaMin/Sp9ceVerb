#!/bin/sh
# Builds the VST3, Audio Unit and AAX host shells around the plugin and
# copies them into dist/. The shells come from OpenPluginSDK (native/) and
# call the ABI in plugin/src/shell_host.rs: op_shell_* for audio and
# op_editor_* for the editor (OP_EDITOR=ON, OP_TAIL=ON).
#   scripts/build_shells.sh            build dist/Sp9ceVerb.{vst3,component,aaxplugin}
#   scripts/build_shells.sh --install  also copy VST3 and AU to ~/Library/Audio/Plug-Ins
#   scripts/build_shells.sh --check    also run the editor checks in tools/ (with --install for the AU)
#   scripts/build_shells.sh --check-only   run the editor checks on what is already built and installed;
#                                      builds nothing, so the bundles under test stay as they are
# Third-party SDKs are located like the other dependencies (a Dependencies or
# SDKs folder next to or above this one); override with VST3_SDK, AU_SDK,
# AAX_SDK. The AAX bundle links the Avid SDK and is unsigned.
# Universal (arm64 and x86_64) unless SP9CE_ARCHS names one architecture.
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
. "$ROOT/scripts/sdk_env.sh"
NAME=Sp9ceVerb
BUNDLE_ID=com.opengrid.sp9ceverb
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/plugin/Cargo.toml" | head -1)"
# Bundle versions are numbers only; the plugin itself reports the full version.
VERSION="${VERSION%%-*}"
# Four-character codes. Placeholders until registered with Apple.
AU_SUBTYPE=Sp9v
AU_MANUFACTURER=OpGd

find_sdk() { # env override, then names to look for under any Dependencies or SDKs folder
  if [ -n "$1" ] && [ -d "$1" ]; then echo "$1"; return 0; fi
  shift
  here="$ROOT"
  for base in "$here/.." "$here/../.." "$here/../Dependencies" "$here/../../Dependencies" "$here/../SDKs" "$here/../../SDKs" /Volumes/*/Dependencies /Volumes/*/SDKs "$HOME/Dependencies" "$HOME/SDKs"; do
    for rel in "$@"; do
      [ -d "$base/$rel" ] && { (cd "$base/$rel" && pwd); return 0; }
    done
  done
  return 1
}
VST3_SDK="$(find_sdk "$VST3_SDK" vst3sdk clap-wrapper/build/cpm/vst3sdk)" || { echo "VST3 SDK not found; set VST3_SDK" >&2; exit 1; }
AU_SDK="$(find_sdk "$AU_SDK" AudioUnitSDK)" || { echo "AudioUnitSDK not found; set AU_SDK" >&2; exit 1; }
AAX_SDK="$(find_sdk "$AAX_SDK" aax-sdk-2-8-1)" || { echo "AAX SDK not found; set AAX_SDK" >&2; exit 1; }

# No machine paths in the binaries.
SEP="$(printf '\037')"
CARGO_ENCODED_RUSTFLAGS="--remap-path-prefix=$ROOT=.${SEP}--remap-path-prefix=$SDK_DIR=sdk${SEP}--remap-path-prefix=$DRI_DIR=dri${SEP}--remap-path-prefix=$HOME=home"
export CARGO_ENCODED_RUSTFLAGS
# Quoted: SDK folders can have spaces in their path.
MAP="-ffile-prefix-map='$ROOT'=. -ffile-prefix-map='$SDK_DIR'=sdk -ffile-prefix-map='$VST3_SDK'=vst3sdk -ffile-prefix-map='$AU_SDK'=ausdk -ffile-prefix-map='$AAX_SDK'=aaxsdk -ffile-prefix-map='$HOME'=home"
# The editor's Cocoa and Core Text code is in the same archive.
FW="-framework CoreText -framework CoreGraphics -framework CoreFoundation -framework AppKit -framework Foundation -framework QuartzCore"

INSTALL=0
CHECK=0
BUILD_IT=1
for arg in "$@"; do
  case "$arg" in
    --install) INSTALL=1 ;;
    --check) CHECK=1 ;;
    --check-only) CHECK=1; BUILD_IT=0 ;;
  esac
done
BUILD="$ROOT/plugin/target/shells"
DIST="$ROOT/dist"

if [ "$BUILD_IT" = 1 ]; then
  cd "$ROOT/plugin"
  # cargo does not notice a changed deployment target, so the library is rebuilt each time.
  touch src/lib.rs
  cargo_universal --lib
  join_archs libsp9ceverb.a target/universal/libsp9ceverb.a

  BUILD="$ROOT/plugin/target/shells"
  cmake -S "$SDK_DIR/native" -B "$BUILD" -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_OSX_DEPLOYMENT_TARGET=11.0 -DCMAKE_OSX_ARCHITECTURES="$(echo $SP9CE_ARCHS | tr ' ' ';')" -DCMAKE_C_FLAGS="$MAP" -DCMAKE_CXX_FLAGS="$MAP" -DCMAKE_OBJCXX_FLAGS="$MAP" \
    -DCMAKE_SHARED_LINKER_FLAGS="$FW" -DCMAKE_MODULE_LINKER_FLAGS="$FW" -DCMAKE_EXE_LINKER_FLAGS="$FW" \
    -DOP_RUST_LIB="$ROOT/plugin/target/universal/libsp9ceverb.a" \
    -DOP_VST3_SDK="$VST3_SDK" -DOP_AU_SDK="$AU_SDK" -DOP_AAX_SDK="$AAX_SDK" \
    -DOP_EDITOR=ON -DOP_TAIL=ON -DOP_AAX_PAGES=ON \
    -DOP_PRODUCT="$NAME" -DOP_BUNDLE_ID="$BUNDLE_ID" -DOP_VERSION="$VERSION" \
    -DOP_MANUFACTURER=OpenGrid -DOP_AU_SUBTYPE="$AU_SUBTYPE" -DOP_AU_MANUFACTURER="$AU_MANUFACTURER" >/dev/null
  # The SDK's own proof programs test its FreEQ example, not this plugin; build only the bundles.
  cmake --build "$BUILD" --config Release -j 8 --target OpenPluginVST3 OpenPluginAU OpenPluginAAX

  DIST="$ROOT/dist"
  mkdir -p "$DIST"
  for ext in vst3 component aaxplugin; do
    rm -rf "$DIST/$NAME.$ext"
    cp -R "$BUILD/$NAME.$ext" "$DIST/"
  done
  codesign --force --sign - "$DIST/$NAME.vst3" "$DIST/$NAME.component" >/dev/null 2>&1
  echo "built $DIST/$NAME.vst3"
  echo "built $DIST/$NAME.component"
  echo "built $DIST/$NAME.aaxplugin (unsigned; needs the Avid wrap)"
fi

if [ "$INSTALL" = 1 ]; then
  mkdir -p "$HOME/Library/Audio/Plug-Ins/VST3" "$HOME/Library/Audio/Plug-Ins/Components"
  rm -rf "$HOME/Library/Audio/Plug-Ins/VST3/$NAME.vst3" "$HOME/Library/Audio/Plug-Ins/Components/$NAME.component"
  cp -R "$DIST/$NAME.vst3" "$HOME/Library/Audio/Plug-Ins/VST3/"
  cp -R "$DIST/$NAME.component" "$HOME/Library/Audio/Plug-Ins/Components/"
  killall -9 AudioComponentRegistrar >/dev/null 2>&1 || true
  echo "installed VST3 and AU"
fi

# Editor checks: each opens the editor in a window that is never shown, clicks
# "next preset" and checks that the host is told. The AU check uses the
# installed Audio Unit, so it wants --install as well.
if [ "$CHECK" = 1 ]; then
  LIBS="$BUILD/lib/Release"
  clang++ -std=c++17 -fno-objc-arc -I"$VST3_SDK" "$ROOT/tools/vst3_editor_check.mm" \
    "$VST3_SDK/public.sdk/source/vst/vstinitiids.cpp" -o "$BUILD/vst3_editor_check" \
    "$LIBS/libsdk_common.a" "$LIBS/libbase.a" "$LIBS/libpluginterfaces.a" -framework Cocoa
  clang++ -std=c++17 -fno-objc-arc "$ROOT/tools/au_editor_check.mm" -o "$BUILD/au_editor_check" \
    -framework Cocoa -framework AudioToolbox -framework AudioUnit
  echo "VST3 editor check:"; "$BUILD/vst3_editor_check" "$DIST/$NAME.vst3"
  echo "AU editor check:"; "$BUILD/au_editor_check"
fi
