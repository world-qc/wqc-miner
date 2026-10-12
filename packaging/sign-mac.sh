#!/usr/bin/env bash
# Assemble the macOS app, sign it inside-out, then sign, notarize, and staple the disk image.
# Local keychain: Developer ID Application must already be installed.
# CI: import the p12 into a temporary keychain before `sign`.
set -euo pipefail

IDENTITY="${APPLE_SIGN_IDENTITY:-Developer ID Application: Yoshitaka Sato (ZV84T86GTH)}"
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

usage() {
  echo "usage: sign-mac.sh assemble <staging-dir> <version> | sign <app> | notarize <dmg>" >&2
  exit 2
}

sign_one() {
  local path="$1"
  codesign --force --options runtime --timestamp --sign "$IDENTITY" "$path"
  codesign --verify --strict --verbose=2 "$path"
}

cmd_assemble() {
  local staging="${1:-}"
  local version="${2:-}"
  if [[ -z "$staging" || ! -d "$staging" ]]; then
    echo "staging directory not found: ${staging:-}" >&2
    usage
  fi
  if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "version must be MAJOR.MINOR.PATCH: ${version:-}" >&2
    usage
  fi
  local app="${staging}/WQC Miner.app"
  local macos="${app}/Contents/MacOS"
  mkdir -p "${macos}/bin"
  mv "${staging}/wqc-miner" "${macos}/wqc-miner"
  mv "${staging}/bin/wqc-core" "${macos}/bin/wqc-core"
  mv "${staging}/bin/wqc-node" "${macos}/bin/wqc-node"
  rmdir "${staging}/bin"
  sed "s/__VERSION__/${version}/g" "${SCRIPT_DIR}/macos/Info.plist" > "${app}/Contents/Info.plist"
  printf 'APPL????' > "${app}/Contents/PkgInfo"
  plutil -lint "${app}/Contents/Info.plist"
  file "${macos}/wqc-miner" "${macos}/bin/wqc-core" "${macos}/bin/wqc-node"
}

cmd_sign() {
  local app="${1:-}"
  if [[ -z "$app" || ! -d "$app" ]]; then
    echo "app bundle not found: ${app:-}" >&2
    usage
  fi
  # Nested Mach-Os first, then the bundle so Info.plist is sealed over them.
  sign_one "${app}/Contents/MacOS/bin/wqc-core"
  sign_one "${app}/Contents/MacOS/bin/wqc-node"
  codesign --force --options runtime --timestamp --sign "$IDENTITY" "$app"
  codesign --verify --strict --verbose=2 "$app"
  codesign --verify --strict --verbose=2 "${app}/Contents/MacOS/bin/wqc-core"
  codesign --verify --strict --verbose=2 "${app}/Contents/MacOS/bin/wqc-node"
}

cmd_notarize() {
  local dmg="${1:-}"
  if [[ -z "$dmg" || ! -f "$dmg" ]]; then
    echo "disk image not found: ${dmg:-}" >&2
    usage
  fi
  : "${APPLE_ID:?APPLE_ID is required}"
  : "${APPLE_TEAM_ID:?APPLE_TEAM_ID is required}"
  : "${APPLE_APP_PASSWORD:?APPLE_APP_PASSWORD is required}"
  # Hardened runtime applies to executables, not to the disk image.
  codesign --force --timestamp --sign "$IDENTITY" "$dmg"
  codesign --verify --verbose=2 "$dmg"
  # notarytool 1.1.2 and 1.1.3 do not expand @env:NAME; that literal
  # is sent as the password and Apple returns 401.
  xcrun notarytool submit "$dmg" \
    --apple-id "$APPLE_ID" \
    --team-id "$APPLE_TEAM_ID" \
    --password "$APPLE_APP_PASSWORD" \
    --wait
  xcrun stapler staple "$dmg"
  xcrun stapler validate "$dmg"
}

main() {
  local cmd="${1:-}"
  shift || true
  case "$cmd" in
    assemble) cmd_assemble "$@" ;;
    sign) cmd_sign "$@" ;;
    notarize) cmd_notarize "$@" ;;
    *) usage ;;
  esac
}

main "$@"
