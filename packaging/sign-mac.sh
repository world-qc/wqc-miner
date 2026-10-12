#!/usr/bin/env bash
# Sign the universal macOS binaries, then notarize and staple the disk image.
# Local keychain: Developer ID Application must already be installed.
# CI: import the p12 into a temporary keychain before `sign`.
set -euo pipefail

IDENTITY="${APPLE_SIGN_IDENTITY:-Developer ID Application: Yoshitaka Sato (ZV84T86GTH)}"

usage() {
  echo "usage: sign-mac.sh sign <staging-dir> | notarize <dmg>" >&2
  exit 2
}

sign_one() {
  local path="$1"
  codesign --force --options runtime --timestamp --sign "$IDENTITY" "$path"
  codesign --verify --strict --verbose=2 "$path"
}

cmd_sign() {
  local staging="${1:-}"
  if [[ -z "$staging" || ! -d "$staging" ]]; then
    echo "staging directory not found: ${staging:-}" >&2
    usage
  fi
  sign_one "$staging/wqc-miner"
  sign_one "$staging/bin/wqc-core"
  sign_one "$staging/bin/wqc-node"
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
    sign) cmd_sign "$@" ;;
    notarize) cmd_notarize "$@" ;;
    *) usage ;;
  esac
}

main "$@"
