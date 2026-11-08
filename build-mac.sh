#!/usr/bin/env bash
# Builds MatLivre on macOS and packs it as dist/MatLivre.app.
# Usage: ./build-mac.sh [fast] [--universal] [--dmg]
#   fast         quicker build with a slightly slower binary (default: release)
#   --universal  one app for Apple Silicon and Intel (needs both Rust targets)
#   --dmg        also create dist/MatLivre-<version>-macOS.dmg (drag MatLivre to Applications)
#   --velopack   pack the app with Velopack (needs vpk: dotnet tool install -g vpk) so installed copies
#                update themselves from the GitHub releases; the update files go to dist/velopack
set -euo pipefail
cd "$(dirname "$0")"
command -v cargo >/dev/null || { echo "cargo not found. Install Rust from https://rustup.rs"; exit 1; }
profile=release; flag=--release; universal=0; dmg=0; velopack=0
for a in "$@"; do
  case "$a" in
    fast) profile=fast; flag="--profile fast" ;;
    --universal) universal=1 ;;
    --dmg) dmg=1 ;;
    --velopack) velopack=1 ;;
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
# The file types MatLivre opens (Finder's Open With, and their own icons) come from file-types under
# [package.metadata.release-manager] in crates/livre-app/Cargo.toml, the list Windows registers too.
doc_types=""
while IFS='|' read -r ext desc ico; do
  [ -n "$ext" ] || continue
  desc=$(printf '%s' "$desc" | sed 's/&/\&amp;/g; s/</\&lt;/g; s/>/\&gt;/g')
  icon_xml=""
  if [ -n "$ico" ] && [ -f "crates/livre-app/$ico" ]; then
    # The type's .ico becomes an .icns of its own; without one the type keeps the generic icon.
    tmp=$(mktemp -d); set_dir="$tmp/type.iconset"; mkdir -p "$set_dir"
    if sips -s format png "crates/livre-app/$ico" --out "$tmp/type.png" >/dev/null 2>&1; then
      for s in 16 32 128 256; do
        sips -z $s $s "$tmp/type.png" --out "$set_dir/icon_${s}x${s}.png" >/dev/null 2>&1 || true
        sips -z $((s*2)) $((s*2)) "$tmp/type.png" --out "$set_dir/icon_${s}x${s}@2x.png" >/dev/null 2>&1 || true
      done
      if iconutil -c icns "$set_dir" -o "$app/Contents/Resources/$ext.icns" 2>/dev/null; then
        icon_xml="<key>CFBundleTypeIconFile</key><string>$ext</string>"
      fi
    fi
    rm -rf "$tmp"
  fi
  doc_types="$doc_types
    <dict><key>CFBundleTypeName</key><string>$desc</string>
      <key>CFBundleTypeExtensions</key><array><string>$ext</string></array>
      $icon_xml
      <key>CFBundleTypeRole</key><string>Editor</string></dict>"
done <<TYPES
$(sed -n 's/^ *{ extension = "\([^"]*\)", description = "\([^"]*\)"\(, icon = "\([^"]*\)"\)\{0,1\} *},\{0,1\} *$/\1|\2|\4/p' crates/livre-app/Cargo.toml)
TYPES
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
  <key>CFBundleDocumentTypes</key><array>$doc_types
  </array>
</dict></plist>
PLIST
if [ $velopack = 1 ]; then
  command -v vpk >/dev/null || { echo "vpk not found. Install it: dotnet tool install -g vpk"; exit 1; }
  # Velopack adds its updater to the bundle and signs it (ad hoc: "-", no Apple account), then
  # writes the update package and releases.osx.json that installed copies read from GitHub.
  rm -rf dist/velopack
  vpk pack --packId matlivre --packVersion "$version" --packDir "$app" --mainExe matlivre --packTitle MatLivre \
    --icon "$app/Contents/Resources/matlivre.icns" --signAppIdentity - --noInst --delta None --outputDir dist/velopack
  # The app to ship is the one Velopack made: unpack its portable zip in place of ours.
  unpacked=$(mktemp -d)
  ditto -x -k dist/velopack/matlivre-osx-Portable.zip "$unpacked"
  made=$(find "$unpacked" -maxdepth 1 -name '*.app' | head -1)
  [ -n "$made" ] || { echo "Velopack's portable zip holds no .app"; exit 1; }
  rm -rf "$app" && mv "$made" "$app"
else
  codesign --force --deep --sign - "$app" 2>/dev/null || true
fi
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
