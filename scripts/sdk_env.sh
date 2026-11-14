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

# A release is universal. SP9CE_ARCHS="arm64" (or "x86_64") builds one
# architecture, which is quicker while developing.
SP9CE_ARCHS="${SP9CE_ARCHS:-arm64 x86_64}"
# The release floor, the same for every format and both architectures.
MACOSX_DEPLOYMENT_TARGET=11.0
export MACOSX_DEPLOYMENT_TARGET
rust_target() { case "$1" in arm64) echo aarch64-apple-darwin ;; x86_64) echo x86_64-apple-darwin ;; *) echo "unknown architecture $1" >&2; return 1 ;; esac; }
# usage: cargo_universal <cargo build args...>; run it in plugin/. Builds each
# architecture of SP9CE_ARCHS.
cargo_universal() {
  for arch in $SP9CE_ARCHS; do
    cargo_sdk build --release --target "$(rust_target "$arch")" "$@" || return 1
  done
}
# usage: join_archs <file name under target/<triple>/release> <output file>
join_archs() {
  set --  "$1" "$2"
  inputs=""
  for arch in $SP9CE_ARCHS; do inputs="$inputs target/$(rust_target "$arch")/release/$1"; done
  mkdir -p "$(dirname "$2")"
  # shellcheck disable=SC2086
  lipo -create $inputs -output "$2"
}
