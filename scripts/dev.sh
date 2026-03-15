#!/bin/bash
set -euo pipefail

APP="target/liment.app"

# Build
cargo build

# Kill any running dev instance
pkill -f "$APP/Contents/MacOS/liment" 2>/dev/null || true

# Clean previous .app
rm -rf "$APP"

# Create .app structure
mkdir -p "$APP/Contents/MacOS"
mkdir -p "$APP/Contents/Resources"

cp target/debug/liment "$APP/Contents/MacOS/liment"

cat > "$APP/Contents/Info.plist" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>
  <string>liment</string>
  <key>CFBundleDisplayName</key>
  <string>liment</string>
  <key>CFBundleIdentifier</key>
  <string>fish.stupid.liment.dev</string>
  <key>CFBundleVersion</key>
  <string>0.0.0-dev</string>
  <key>CFBundleShortVersionString</key>
  <string>0.0.0-dev</string>
  <key>CFBundleExecutable</key>
  <string>liment</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>LSUIElement</key>
  <true/>
</dict>
</plist>
EOF

# Ad-hoc codesign
codesign --force --sign - "$APP"

# Run
open "$APP"
