#!/bin/sh
# Builds MTG Manager for the iOS Simulator without a signing certificate.
#
# `fyne package -os iossimulator` builds through Xcode for devices first and so needs
# an Apple development certificate; the Simulator only needs an ad-hoc signature, so
# this script cross-compiles with Go and assembles the .app itself.
#
#   scripts/build-ios-simulator.sh [output-dir]      (default: dist/ios-simulator)
#   xcrun simctl install booted "dist/ios-simulator/MTG Manager.app"
set -eu

cd "$(dirname "$0")/.."
OUT=${1:-dist/ios-simulator}
APP="$OUT/MTG Manager.app"
BUNDLE_ID=io.github.jrhorner1.mtgmanager
VERSION=$(sed -n 's/^ *Version *= *"\(.*\)"/\1/p' cmd/mtgmanager/FyneApp.toml)
BUILD=$(sed -n 's/^ *Build *= *\([0-9]*\)/\1/p' cmd/mtgmanager/FyneApp.toml)
MIN_IOS=13.0

SDK=$(xcrun --sdk iphonesimulator --show-sdk-path)
CC=$(xcrun --sdk iphonesimulator --find clang)
case "$(uname -m)" in arm64) ARCH=arm64 ;; *) ARCH=amd64 ;; esac
CLANG_ARCH=$ARCH; [ "$ARCH" = amd64 ] && CLANG_ARCH=x86_64
FLAGS="-isysroot $SDK -mios-simulator-version-min=$MIN_IOS -arch $CLANG_ARCH -DGLES_SILENCE_DEPRECATION"

rm -rf "$APP"
mkdir -p "$APP"
echo "Compiling for the iOS Simulator ($ARCH)…"
GOOS=ios GOARCH=$ARCH CGO_ENABLED=1 CC="$CC" CXX="$CC++" \
  CGO_CFLAGS="$FLAGS" CGO_CXXFLAGS="$FLAGS" CGO_LDFLAGS="$FLAGS" \
  go build -tags ios -ldflags=-w -o "$APP/main" ./cmd/mtgmanager

# App icons (iPhone 60pt at 2x and 3x, iPad 76pt at 2x) from the 1024px icon.
for spec in AppIcon60x60@2x:120 AppIcon60x60@3x:180 AppIcon76x76@2x~ipad:152; do
  sips -z "${spec#*:}" "${spec#*:}" cmd/mtgmanager/Icon.png --out "$APP/${spec%:*}.png" >/dev/null
done

cat > "$APP/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleExecutable</key><string>main</string>
  <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>MTG Manager</string>
  <key>CFBundleDisplayName</key><string>MTG Manager</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$BUILD</string>
  <key>CFBundleSupportedPlatforms</key><array><string>iPhoneSimulator</string></array>
  <key>CFBundleIcons</key>
  <dict><key>CFBundlePrimaryIcon</key><dict>
    <key>CFBundleIconFiles</key><array><string>AppIcon60x60</string></array>
  </dict></dict>
  <key>CFBundleIcons~ipad</key>
  <dict><key>CFBundlePrimaryIcon</key><dict>
    <key>CFBundleIconFiles</key><array><string>AppIcon60x60</string><string>AppIcon76x76</string></array>
  </dict></dict>
  <key>MinimumOSVersion</key><string>$MIN_IOS</string>
  <key>LSRequiresIPhoneOS</key><true/>
  <key>UILaunchScreen</key><dict/>
  <key>UIDeviceFamily</key><array><integer>1</integer><integer>2</integer></array>
  <key>UIRequiresFullScreen</key><true/>
  <key>UISupportedInterfaceOrientations</key>
  <array>
    <string>UIInterfaceOrientationPortrait</string>
    <string>UIInterfaceOrientationLandscapeLeft</string>
    <string>UIInterfaceOrientationLandscapeRight</string>
  </array>
</dict>
</plist>
PLIST
printf 'APPL????' > "$APP/PkgInfo"

# Apple's codesign explicitly: other tools (e.g. Anaconda) install a "codesign" earlier
# on PATH that can't sign app bundles.
/usr/bin/codesign --force --sign - "$APP" >/dev/null
echo "Built $APP"
