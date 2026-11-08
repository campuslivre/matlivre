#!/usr/bin/env bash
# Builds MatLivre on macOS and packs it as dist/MatLivre.app.
# Usage: ./build-mac.sh [fast] [--universal] [--dmg]
#   fast         quicker build with a slightly slower binary (default: release)
#   --universal  one app for Apple Silicon and Intel (needs both Rust targets)
#   --dmg        also create dist/MatLivre-<version>-macOS.dmg (drag MatLivre to Applications)
set -euo pipefail
cd "$(dirname "$0")"
command -v cargo >/dev/null || { echo "cargo not found. Install Rust from https://rustup.rs"; exit 1; }
profile=release; flag=--release; universal=0; dmg=0
for a in "$@"; do
  case "$a" in
    fast) profile=fast; flag="--profile fast" ;;
    --universal) universal=1 ;;
    --dmg) dmg=1 ;;
    *) echo "unknown option $a"; exit 1 ;;
  esac
done
if [ $universal = 1 ]; then
  rustup target add aarch64-apple-darwin x86_64-apple-darwin
  for t in aarch64-apple-darwin x86_64-apple-darwin; do cargo build $flag --target $t -p livre-app -p livre-lang; done
  out=target/universal/$profile; mkdir -p "$out"
  for b in matlivre matlivre-cli; do
    lipo -create -output "$out/$b" target/aarch64-apple-darwin/$profile/$b target/x86_64-apple-darwin/$profile/$b
  done
else
  cargo build $flag -p livre-app -p livre-lang
  out=target/$profile
fi
app=dist/MatLivre.app
rm -rf "$app" && mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$out/matlivre" "$app/Contents/MacOS/matlivre"
[ -f "$out/matlivre-cli" ] && cp "$out/matlivre-cli" "$app/Contents/MacOS/matlivre-cli"
version=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
# App icon from the bundled PNG.
icons=$(mktemp -d)/matlivre.iconset; mkdir -p "$icons"
for s in 16 32 128 256 512; do
  sips -z $s $s crates/livre-app/assets/matlivre.png --out "$icons/icon_${s}x${s}.png" >/dev/null
  sips -z $((s*2)) $((s*2)) crates/livre-app/assets/matlivre.png --out "$icons/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$icons" -o "$app/Contents/Resources/matlivre.icns"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>MatLivre</string>
  <key>CFBundleDisplayName</key><string>MatLivre</string>
  <key>CFBundleIdentifier</key><string>org.campuslivre.matlivre</string>
  <key>CFBundleExecutable</key><string>matlivre</string>
  <key>CFBundleIconFile</key><string>matlivre</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>CFBundleDocumentTypes</key><array>
    <dict><key>CFBundleTypeName</key><string>MATLAB Live Script</string>
      <key>CFBundleTypeExtensions</key><array><string>mlx</string></array>
      <key>CFBundleTypeRole</key><string>Editor</string></dict>
    <dict><key>CFBundleTypeName</key><string>MATLAB Script</string>
      <key>CFBundleTypeExtensions</key><array><string>m</string></array>
      <key>CFBundleTypeRole</key><string>Editor</string></dict>
  </array>
</dict></plist>
PLIST
codesign --force --deep --sign - "$app" 2>/dev/null || true
echo "Command line: $app/Contents/MacOS/matlivre-cli script.m"
echo "Built $app  (run: open $app, or $app/Contents/MacOS/matlivre file.mlx)"
if [ $dmg = 1 ]; then
  d=dist/MatLivre-$version-macOS.dmg
  stage=$(mktemp -d)
  cp -R "$app" "$stage/"; ln -s /Applications "$stage/Applications"
  rm -f "$d"
  hdiutil create -volname "MatLivre" -srcfolder "$stage" -ov -format UDZO "$d" >/dev/null
  echo "Built $d"
fi
