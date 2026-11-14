#!/bin/sh
# Locates OpenPluginSDK and GoodLookinDri and prints the cargo --config
# arguments that patch them in. Source of truth for every script that builds
# the plugin crate. Override with OPENPLUGIN_SDK / GOODLOOKINDRI.
find_dir() { # name, env override
  if [ -n "$2" ] && [ -d "$2" ]; then echo "$2"; return 0; fi
  here="$(cd "$(dirname "$0")/.." && pwd)"
  for base in "$here/.." "$here/../.." "$here/../Dependencies" "$here/../../Dependencies" /Volumes/*/Dependencies "$HOME/Dependencies"; do
    [ -d "$base/$1" ] && { (cd "$base/$1" && pwd); return 0; }
  done
  return 1
}
SDK_DIR="$(find_dir OpenPluginSDK "$OPENPLUGIN_SDK")" || { echo "OpenPluginSDK not found; set OPENPLUGIN_SDK" >&2; exit 1; }
DRI_DIR="$(find_dir GoodLookinDri "$GOODLOOKINDRI")" || { echo "GoodLookinDri not found; set GOODLOOKINDRI" >&2; exit 1; }
export SDK_DIR DRI_DIR
cargo_sdk() { # usage: cargo_sdk <cargo args...>
  sub="$1"; shift
  cargo "$sub" \
    --config "patch.crates-io.openplugin.path=\"$SDK_DIR/crates/openplugin\"" \
    --config "patch.crates-io.goodlookindri-shell.path=\"$DRI_DIR/shell\"" \
    "$@"
}
