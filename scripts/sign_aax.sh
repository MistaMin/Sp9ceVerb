#!/bin/sh
# Signs dist/Sp9ceVerb.aaxplugin for Pro Tools: Apple Developer ID, then the
# PACE "Signing Only" wrap, then the Apple signature again over the result.
# Writes dist/signed/Sp9ceVerb.aaxplugin and leaves the unsigned bundle alone.
#   scripts/sign_aax.sh             sign and verify
#   scripts/sign_aax.sh --install   also copy to the system AAX folder (asks for sudo)
# No credentials or account details are stored in the repository. Set these in
# the environment or in scripts/signing.local.env (untracked):
#   SP9CE_PACE_GUID       the wrap configuration GUID from PACE Central
#   SP9CE_PACE_ACCOUNT    the iLok account that owns the wrap configuration
#   SP9CE_SIGN_IDENTITY   the "Developer ID Application: ..." identity
# wraptool asks for the account password itself unless it has one stored.
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
[ -f "$ROOT/scripts/signing.local.env" ] && . "$ROOT/scripts/signing.local.env"
NAME=Sp9ceVerb
WRAPTOOL="${PACE_WRAPTOOL:-/Applications/PACEAntiPiracy/Eden/Fusion/Current/bin/wraptool}"
IN="$ROOT/dist/$NAME.aaxplugin"
OUT="$ROOT/dist/signed"
for v in SP9CE_PACE_GUID SP9CE_PACE_ACCOUNT SP9CE_SIGN_IDENTITY; do
  eval "[ -n \"\${$v}\" ]" || { echo "$v is not set (see the top of this script)" >&2; exit 1; }
done
[ -d "$IN" ] || { echo "$IN not found; run scripts/build_shells.sh first" >&2; exit 1; }
[ -x "$WRAPTOOL" ] || { echo "wraptool not found; set PACE_WRAPTOOL" >&2; exit 1; }

STAGE="$(mktemp -d "$ROOT/dist/.sign.XXXXXX")"
trap 'rm -rf "$STAGE"' EXIT
mkdir -p "$STAGE/in" "$STAGE/out"
ditto "$IN" "$STAGE/in/$NAME.aaxplugin"
codesign --force --deep --timestamp --options runtime --sign "$SP9CE_SIGN_IDENTITY" "$STAGE/in/$NAME.aaxplugin"
"$WRAPTOOL" sign --verbose --account "$SP9CE_PACE_ACCOUNT" --wcguid "$SP9CE_PACE_GUID" \
  --signid "$SP9CE_SIGN_IDENTITY" --in "$STAGE/in/$NAME.aaxplugin" \
  --out "$STAGE/out/$NAME.aaxplugin" --autoinstall on
# Seal the PACE metadata with the Apple signature, then check both.
codesign --force --deep --timestamp --options runtime --sign "$SP9CE_SIGN_IDENTITY" "$STAGE/out/$NAME.aaxplugin"
codesign --verify --deep --strict "$STAGE/out/$NAME.aaxplugin"
"$WRAPTOOL" verify --verbose --in "$STAGE/out/$NAME.aaxplugin"
mkdir -p "$OUT"
rm -rf "$OUT/$NAME.aaxplugin"
ditto "$STAGE/out/$NAME.aaxplugin" "$OUT/$NAME.aaxplugin"
echo "signed $OUT/$NAME.aaxplugin"

if [ "$1" = "--install" ]; then
  DEST="/Library/Application Support/Avid/Audio/Plug-Ins"
  sudo rm -rf "$DEST/$NAME.aaxplugin"
  sudo ditto "$OUT/$NAME.aaxplugin" "$DEST/$NAME.aaxplugin"
  echo "installed $DEST/$NAME.aaxplugin"
fi
