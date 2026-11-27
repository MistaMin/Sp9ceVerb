#!/bin/sh
# The release installer: signs the built bundles with Developer ID, builds
# dist/release/Sp9ceVerb-<Version>-macos.pkg (one choice per format, plus the
# manual and licences) and a disk image holding it, notarizes and staples both,
# and writes SHA256SUMS. Run after scripts/build.sh, scripts/build_shells.sh,
# scripts/sign_aax.sh and python3 tools/build_user_manual.py.
#   scripts/package.sh 1.1-Beta
# Settings in the environment or scripts/signing.local.env (untracked):
#   SP9CE_SIGN_IDENTITY     "Developer ID Application: ..."
#   SP9CE_NOTARY_PROFILE    notarytool keychain profile (default AC_NOTARY)
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
[ -f "$ROOT/scripts/signing.local.env" ] && . "$ROOT/scripts/signing.local.env"
VERSION_NAME="$1"
[ -n "$VERSION_NAME" ] || { echo "usage: scripts/package.sh <version as in the file names, e.g. 1.1-Beta>" >&2; exit 2; }
[ -n "$SP9CE_SIGN_IDENTITY" ] || { echo "SP9CE_SIGN_IDENTITY not set" >&2; exit 1; }
INSTALLER_IDENTITY="$(echo "$SP9CE_SIGN_IDENTITY" | sed 's/Developer ID Application/Developer ID Installer/')"
PROFILE="${SP9CE_NOTARY_PROFILE:-AC_NOTARY}"
NAME=Sp9ceVerb
DIST="$ROOT/dist"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/plugin/Cargo.toml" | head -1)"
VERSION="${VERSION%%-*}"
OUT="$DIST/release"
WORK="$DIST/package-work"
PKG="$OUT/$NAME-$VERSION_NAME-macos.pkg"
DMG="$OUT/$NAME-$VERSION_NAME-macos.dmg"
MANUAL="$ROOT/output/pdf/$NAME-User-Manual.pdf"
[ -f "$MANUAL" ] || { echo "build the manual first: python3 tools/build_user_manual.py" >&2; exit 1; }
[ -d "$DIST/signed/$NAME.aaxplugin" ] || { echo "sign the AAX first: scripts/sign_aax.sh" >&2; exit 1; }
rm -rf "$WORK" "$OUT"
mkdir -p "$WORK/comp" "$WORK/res" "$OUT"
export COPYFILE_DISABLE=1

# One root per format. The AAX keeps its PACE signature, so it is not signed again.
stage() { # format, bundle source, install location
  mkdir -p "$WORK/root-$1"
  ditto --norsrc --noextattr "$2" "$WORK/root-$1/$(basename "$2")"
  echo "$3" > "$WORK/loc-$1"
}
stage clap "$DIST/$NAME.clap" /Library/Audio/Plug-Ins/CLAP
stage vst3 "$DIST/$NAME.vst3" /Library/Audio/Plug-Ins/VST3
stage au "$DIST/$NAME.component" /Library/Audio/Plug-Ins/Components
stage lv2 "$DIST/$NAME.lv2" /Library/Audio/Plug-Ins/LV2
stage aax "$DIST/signed/$NAME.aaxplugin" "/Library/Application Support/Avid/Audio/Plug-Ins"
for b in "$WORK/root-clap/$NAME.clap" "$WORK/root-vst3/$NAME.vst3" "$WORK/root-au/$NAME.component" "$WORK/root-lv2/$NAME.lv2/$NAME.dylib"; do
  codesign --force --timestamp --options runtime --sign "$SP9CE_SIGN_IDENTITY" "$b"
  codesign --verify --strict "$b"
done
codesign --verify --deep --strict "$WORK/root-aax/$NAME.aaxplugin"

# The manual and licence texts.
D="$WORK/root-docs"
mkdir -p "$D/Licenses/third-party"
cp "$MANUAL" "$ROOT/CHANGELOG.md" "$D/"
cp "$ROOT/LICENSE" "$D/Licenses/LICENSE-MIT.txt"
cp "$ROOT/BINARY_LICENSE.txt" "$ROOT/THIRD_PARTY_LICENSES.md" "$D/Licenses/"
cp "$DIST/$NAME.component/Contents/Resources/AudioUnitSDK-LICENSE.txt" \
   "$DIST/$NAME.vst3/Contents/Resources/VST3-SDK-LICENSE.txt" \
   "$DIST/signed/$NAME.aaxplugin/Contents/Resources/AAX-SDK-LICENSE.txt" "$D/Licenses/third-party/"
cat > "$D/INSTALL.txt" <<EOF
$NAME $VERSION_NAME (macOS, Apple silicon and Intel, macOS 11.0 or later)

Run $NAME-$VERSION_NAME-macos.pkg and choose the formats to install:
  CLAP       /Library/Audio/Plug-Ins/CLAP
  VST3       /Library/Audio/Plug-Ins/VST3
  Audio Unit /Library/Audio/Plug-Ins/Components
  LV2        /Library/Audio/Plug-Ins/LV2
  AAX        /Library/Application Support/Avid/Audio/Plug-Ins
The manual and licence texts are installed to
/Library/Application Support/$NAME. Rescan plugins in your host afterwards.
EOF
echo "/Library/Application Support/$NAME" > "$WORK/loc-docs"
xattr -cr "$WORK"

# Component packages; bundles install where they are put, never relocated.
for f in clap vst3 au lv2 aax docs; do
  plist="$WORK/comp-$f.plist"
  pkgbuild --analyze --root "$WORK/root-$f" "$plist" >/dev/null
  n=0
  while /usr/libexec/PlistBuddy -c "Print :$n" "$plist" >/dev/null 2>&1; do
    # The LV2 folder is no macOS bundle and has no relocation key.
    /usr/libexec/PlistBuddy -c "Set :$n:BundleIsRelocatable false" "$plist" 2>/dev/null || true
    n=$((n + 1))
  done
  pkgbuild --quiet --root "$WORK/root-$f" --component-plist "$plist" --identifier "com.opengrid.sp9ceverb.$f" \
    --version "$VERSION" --install-location "$(cat "$WORK/loc-$f")" "$WORK/raw-$f.pkg"
  # macOS gives every new file a com.apple.provenance attribute that cannot be
  # removed, and pkgbuild stores it as ._ files. Rebuild the payload and its
  # bill of materials from the root without them.
  e="$WORK/expand-$f"
  pkgutil --expand "$WORK/raw-$f.pkg" "$e"
  (cd "$WORK/root-$f" && tar --format cpio --no-mac-metadata --uid 0 --gid 0 -cf - .) | gzip -9 > "$e/Payload"
  lsbom "$e/Bom" | grep -v '/\._' > "$e.list"
  rm "$e/Bom" && mkbom -i "$e.list" "$e/Bom"
  files="$(lsbom -s "$e/Bom" | wc -l | tr -d ' ')"
  sed -i '' "s/numberOfFiles=\"[0-9]*\"/numberOfFiles=\"$files\"/" "$e/PackageInfo"
  pkgutil --flatten "$e" "$WORK/comp/$NAME-$f.pkg"
done

cat > "$WORK/res/welcome.txt" <<EOF
$NAME $VERSION_NAME

A reverb plugin with nine spaces on one engine, by OpenGrid.
For Macs with Apple silicon or an Intel processor, running macOS 11.0 or later.

Choose the plugin formats to install on the next screens: CLAP, VST3, Audio Unit, LV2 and AAX (Pro Tools).
The manual and licence texts are installed to /Library/Application Support/$NAME.

AAX and Pro Tools are trademarks of Avid Technology, Inc.
EOF
cp "$ROOT/BINARY_LICENSE.txt" "$WORK/res/license.txt"
choice() { # id, title, package suffix, description
  echo "    <choice id=\"$1\" title=\"$2\" description=\"$4\"><pkg-ref id=\"com.opengrid.sp9ceverb.$3\"/></choice>"
  echo "    <pkg-ref id=\"com.opengrid.sp9ceverb.$3\" version=\"$VERSION\" auth=\"Root\">#$NAME-$3.pkg</pkg-ref>"
}
{
  cat <<EOF
<?xml version="1.0" encoding="utf-8" standalone="yes"?>
<installer-gui-script minSpecVersion="2">
    <title>$NAME $VERSION_NAME</title>
    <welcome file="welcome.txt" mime-type="text/plain"/>
    <license file="license.txt" mime-type="text/plain"/>
    <options customize="always" require-scripts="false" hostArchitectures="arm64,x86_64"/>
    <volume-check><allowed-os-versions><os-version min="11.0"/></allowed-os-versions></volume-check>
    <domains enable_anywhere="false" enable_currentUserHome="false" enable_localSystem="true"/>
    <choices-outline>
        <line choice="clap"/><line choice="vst3"/><line choice="au"/><line choice="lv2"/><line choice="aax"/><line choice="docs"/>
    </choices-outline>
EOF
  choice clap CLAP clap "Installs $NAME.clap to /Library/Audio/Plug-Ins/CLAP"
  choice vst3 VST3 vst3 "Installs $NAME.vst3 to /Library/Audio/Plug-Ins/VST3"
  choice au "Audio Unit" au "Installs $NAME.component to /Library/Audio/Plug-Ins/Components"
  choice lv2 LV2 lv2 "Installs $NAME.lv2 to /Library/Audio/Plug-Ins/LV2"
  choice aax "AAX (Pro Tools)" aax "Installs $NAME.aaxplugin to /Library/Application Support/Avid/Audio/Plug-Ins"
  choice docs "Manual and licences" docs "Installs the manual, changelog and licence texts to /Library/Application Support/$NAME"
  echo "</installer-gui-script>"
} > "$WORK/Distribution.xml"
productbuild --quiet --distribution "$WORK/Distribution.xml" --resources "$WORK/res" --package-path "$WORK/comp" "$WORK/unsigned.pkg"
productsign --sign "$INSTALLER_IDENTITY" --timestamp "$WORK/unsigned.pkg" "$PKG"

# No AppleDouble files in any payload.
rm -rf "$WORK/check" && pkgutil --expand "$PKG" "$WORK/check"
for p in "$WORK"/check/*.pkg; do
  if lsbom -s "$p/Bom" | grep -q '/\._'; then echo "AppleDouble files in $p" >&2; exit 1; fi
done

xcrun notarytool submit "$PKG" --keychain-profile "$PROFILE" --wait
xcrun stapler staple "$PKG"
spctl --assess --type install -v "$PKG"

# The disk image: the installer, INSTALL.txt, the manual and the licences.
S="$WORK/dmg"
mkdir -p "$S/Documentation"
cp "$PKG" "$D/INSTALL.txt" "$S/"
cp "$MANUAL" "$S/Documentation/"
ditto --norsrc --noextattr "$D/Licenses" "$S/Licenses"
xattr -cr "$S"
hdiutil create -quiet -volname "$NAME $VERSION_NAME" -srcfolder "$S" -fs HFS+ -format UDZO -ov "$DMG"
codesign --force --timestamp --sign "$SP9CE_SIGN_IDENTITY" "$DMG"
xcrun notarytool submit "$DMG" --keychain-profile "$PROFILE" --wait
xcrun stapler staple "$DMG"
spctl --assess --type open --context context:primary-signature -v "$DMG"

cp "$MANUAL" "$OUT/"
(cd "$OUT" && shasum -a 256 "$(basename "$PKG")" "$(basename "$DMG")" > SHA256SUMS)
rm -rf "$WORK"
echo "release files in dist/release:"; ls -l "$OUT"
