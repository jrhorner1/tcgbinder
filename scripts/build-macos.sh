#!/bin/sh
# Builds TCG Binder for macOS: one universal binary (Apple silicon and Intel) and a
# "TCG Binder.app" bundle around it, ad-hoc signed (not notarized).
#
#   scripts/build-macos.sh [version] [output-dir]     (defaults: FyneApp.toml's version, dist/macos)
#
# Writes <output-dir>/tcgbinder (the universal binary) and <output-dir>/TCG Binder.app.
set -eu

cd "$(dirname "$0")/.."
VERSION=${1:-$(sed -n 's/^ *Version *= *"\(.*\)"/\1/p' cmd/tcgbinder/FyneApp.toml)}
OUT=${2:-dist/macos}
APP="$OUT/TCG Binder.app"
BUNDLE_ID=io.github.jrhorner1.tcgbinder
MIN_MACOS=11.0
LDFLAGS="-s -w -X github.com/jrhorner1/tcgbinder/internal/version.Version=$VERSION"

rm -rf "$OUT"
mkdir -p "$OUT/arch"
for arch in arm64 amd64; do
  clang_arch=$arch; [ "$arch" = amd64 ] && clang_arch=x86_64
  echo "Compiling for macOS $arch…"
  flags="-arch $clang_arch -mmacosx-version-min=$MIN_MACOS"
  GOOS=darwin GOARCH=$arch CGO_ENABLED=1 CC="clang $flags" \
    CGO_CFLAGS="$flags" CGO_LDFLAGS="$flags" MACOSX_DEPLOYMENT_TARGET=$MIN_MACOS \
    go build -trimpath -ldflags "$LDFLAGS" -o "$OUT/arch/tcgbinder-$arch" ./cmd/tcgbinder
done
lipo -create -output "$OUT/tcgbinder" "$OUT/arch/tcgbinder-arm64" "$OUT/arch/tcgbinder-amd64"
rm -rf "$OUT/arch"

# The app bundle: binary, icon (.icns from the 1024px PNG) and Info.plist.
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$OUT/tcgbinder" "$APP/Contents/MacOS/tcgbinder"
ICONSET="$OUT/icon.iconset"
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
  sips -z $size $size cmd/tcgbinder/Icon.png --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  sips -z $((size * 2)) $((size * 2)) cmd/tcgbinder/Icon.png --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/icon.icns"
rm -rf "$ICONSET"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleExecutable</key><string>tcgbinder</string>
  <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
  <key>CFBundleIconFile</key><string>icon.icns</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>TCG Binder</string>
  <key>CFBundleDisplayName</key><string>TCG Binder</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>$MIN_MACOS</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
</dict>
</plist>
PLIST

# Ad-hoc signature: required for Apple silicon to run it. Apple's codesign explicitly
# (others on PATH, e.g. Anaconda's, can't sign apps).
/usr/bin/codesign --force --sign - "$OUT/tcgbinder"
/usr/bin/codesign --force --deep --sign - "$APP"
echo "Built $OUT/tcgbinder and $APP ($VERSION, $(lipo -archs "$OUT/tcgbinder"))"
