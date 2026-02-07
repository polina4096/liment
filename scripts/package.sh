#!/bin/bash
set -euo pipefail

VERSION="${1:?Usage: $0 <version>}"
TARGET="aarch64-apple-darwin"
APP="target/liment.app"

# Build
cargo build --release --target "$TARGET"

# Clean previous .app
rm -rf "$APP" target/liment.app.zip

# Create .app structure
mkdir -p "$APP/Contents/MacOS"
mkdir -p "$APP/Contents/Resources"

cp "target/$TARGET/release/liment" "$APP/Contents/MacOS/liment"

cat > "$APP/Contents/Info.plist" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>
  <string>liment</string>
  <key>CFBundleDisplayName</key>
  <string>liment</string>
  <key>CFBundleIdentifier</key>
  <string>fish.stupid.liment</string>
  <key>CFBundleVersion</key>
  <string>${VERSION}</string>
  <key>CFBundleShortVersionString</key>
  <string>${VERSION}</string>
  <key>CFBundleExecutable</key>
  <string>liment</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>LSUIElement</key>
  <true/>
</dict>
</plist>
EOF

cd target
zip -r liment.app.zip liment.app
echo "Packaged: target/liment.app.zip"
