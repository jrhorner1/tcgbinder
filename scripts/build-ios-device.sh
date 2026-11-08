#!/bin/sh
# Builds TCG Binder for an iPhone or iPad and adds what `fyne package` can't: the
# camera permission text card scanning needs (iOS ends any app that opens the camera
# without it). Then re-signs the app with the identity fyne package used.
#
# Needs Xcode signed in to an Apple account with a development team (Xcode →
# Settings → Accounts); a free account works for your own devices.
#
#   scripts/build-ios-device.sh                       (writes cmd/tcgbinder/TCG Binder.app)
#   xcrun devicectl device install app --device <name or id> "cmd/tcgbinder/TCG Binder.app"
set -eu

cd "$(dirname "$0")/../cmd/tcgbinder"
APP="TCG Binder.app"
CAMERA_TEXT="TCG Binder uses the camera to scan your cards and identify them."

rm -rf "$APP"
fyne package -os ios

PLIST="$APP/Info.plist"
/usr/libexec/PlistBuddy -c "Delete :NSCameraUsageDescription" "$PLIST" 2>/dev/null || true
/usr/libexec/PlistBuddy -c "Add :NSCameraUsageDescription string $CAMERA_TEXT" "$PLIST"

# Re-sign with the same certificate and entitlements; the embedded provisioning
# profile is unchanged. Apple's codesign explicitly (others on PATH can't sign apps).
IDENTITY=$(/usr/bin/codesign -dvv "$APP" 2>&1 | sed -n 's/^Authority=//p' | head -1)
if [ -z "$IDENTITY" ]; then
  echo "error: the packaged app isn't signed; set up signing in Xcode first" >&2
  exit 1
fi
ENT=$(mktemp)
trap 'rm -f "$ENT"' EXIT
/usr/bin/codesign -d --entitlements - --xml "$APP" > "$ENT" 2>/dev/null
/usr/bin/codesign --force --sign "$IDENTITY" --entitlements "$ENT" "$APP"
echo "Built cmd/tcgbinder/$APP (signed by $IDENTITY)"
