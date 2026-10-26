#!/usr/bin/env bash
# bundle-macos-app.sh — package the native host as a double-clickable .app.
#
#   npm run bundle:macos             # release build → dist/Micro OS.app
#   npm run bundle:macos -- --debug  # debug build (faster, same bundle)
#   npm run bundle:macos -- --open   # build and launch it
#   npm run bundle:macos -- --debug --no-build   # re-pack an existing binary
#
# The bundle carries the app icon, Info.plist, the SDL/LVGL host binary and the
# compiled MBCs, so Finder launches the OS shell without any command line. The
# host also paints the same icon at runtime, so `cargo run` shows it in the Dock
# even without this bundle.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

profile="release"
build_host=true
build_apps=true
open_after=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --debug) profile="debug" ;;
    --release) profile="release" ;;
    --no-apps) build_apps=false ;;
    --no-build) build_host=false ;;
    --open) open_after=true ;;
    -h|--help)
      sed -n '2,12p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "bundle-macos-app: unknown option $1" >&2
      exit 2
      ;;
  esac
  shift
done



step() { printf '\n\033[1m[bundle-macos-app] %s\033[0m\n' "$*"; }

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "bundle-macos-app: .app bundles are macOS-only (uname: $(uname -s))" >&2
  exit 1
fi

# Reuse the pinned SDL3/LVGL sources when they were already fetched, the same
# way scripts/test-native.sh does.
if [[ -d "$REPO_ROOT/target/native-deps/sdl3-src" && -d "$REPO_ROOT/target/native-deps/lvgl-src" ]]; then
  export MICRO_NATIVE_SDL3_SOURCE="$REPO_ROOT/target/native-deps/sdl3-src"
  export MICRO_NATIVE_LVGL_SOURCE="$REPO_ROOT/target/native-deps/lvgl-src"
  export MICRO_NATIVE_OFFLINE=1
fi

if "$build_apps"; then
  step "Compile Apps (TS → MBC)"
  npm run build:app
fi

binary="$REPO_ROOT/target/$profile/micro-host-sdl"
if "$build_host"; then
  step "Build native host ($profile)"
  if [[ "$profile" == "release" ]]; then
    cargo build -p micro-host-sdl --features native --release
  else
    cargo build -p micro-host-sdl --features native
  fi
else
  step "Reuse native host ($profile)"
fi
[[ -x "$binary" ]] || { echo "missing native host binary: $binary" >&2; exit 1; }

icon="$REPO_ROOT/assets/app-icon/AppIcon.icns"
[[ -f "$icon" ]] || {
  echo "missing $icon — run: swift scripts/generate-app-icon.swift" >&2
  exit 1
}

# The shell hardcodes the registry order used by its launcher cards
# (0 Counter, 1 Settings, 2 momo). Keep that order here.
apps=(counter settings momo)
for app in "${apps[@]}"; do
  [[ -f "$REPO_ROOT/apps/$app/dist/$app.mbc" ]] || {
    echo "missing apps/$app/dist/$app.mbc — run: npm run build:app" >&2
    exit 1
  }
done

version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$REPO_ROOT/Cargo.toml" | head -1)"
version="${version:-0.1.0}"

bundle="$REPO_ROOT/dist/Micro OS.app"
contents="$bundle/Contents"
step "Assemble ${bundle#"$REPO_ROOT"/}"
rm -rf "$bundle"
mkdir -p "$contents/MacOS" "$contents/Resources/apps"

cp "$binary" "$contents/MacOS/micro-os"
chmod +x "$contents/MacOS/micro-os"
cp "$icon" "$contents/Resources/AppIcon.icns"

# The Rust host links libmicro_native.dylib through @rpath, and Cargo points
# that rpath into target/<profile>/build/<hash>/out/lib. A bundle that only
# copied the executable would break as soon as the build tree is cleaned, so
# ship the dylib and rewrite the install name to the bundle-relative path.
native_lib_dir="$(otool -l "$binary" | awk '/^ *path .*\/out\/lib / {print $2; exit}')"
if [[ -z "$native_lib_dir" || ! -f "$native_lib_dir/libmicro_native.dylib" ]]; then
  native_lib_dir="$(dirname "$(find "$REPO_ROOT/target/$profile/build" -name libmicro_native.dylib -print -quit)")"
fi
[[ -f "$native_lib_dir/libmicro_native.dylib" ]] || {
  echo "cannot locate libmicro_native.dylib beside $binary" >&2
  exit 1
}
mkdir -p "$contents/Frameworks"
cp "$native_lib_dir/libmicro_native.dylib" "$contents/Frameworks/libmicro_native.dylib"
install_name_tool -change \
  "@rpath/libmicro_native.dylib" \
  "@executable_path/../Frameworks/libmicro_native.dylib" \
  "$contents/MacOS/micro-os"
# Drop the build-tree rpath; the rewritten install name needs no search path.
install_name_tool -delete_rpath "$native_lib_dir" "$contents/MacOS/micro-os" 2>/dev/null || true
chmod +x "$contents/Frameworks/libmicro_native.dylib"
cp "$REPO_ROOT/apps/shell/dist/shell.mbc" "$contents/Resources/apps/shell.mbc"
: > "$contents/Resources/apps/registry.txt"
for app in "${apps[@]}"; do
  cp "$REPO_ROOT/apps/$app/dist/$app.mbc" "$contents/Resources/apps/$app.mbc"
  echo "$app.mbc" >> "$contents/Resources/apps/registry.txt"
done

cat > "$contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDevelopmentRegion</key>
	<string>en</string>
	<key>CFBundleDisplayName</key>
	<string>Micro OS</string>
	<key>CFBundleExecutable</key>
	<string>micro-os</string>
	<key>CFBundleIconFile</key>
	<string>AppIcon.icns</string>
	<key>CFBundleIdentifier</key>
	<string>dev.micros.micro-os</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleName</key>
	<string>Micro OS</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>${version}</string>
	<key>CFBundleVersion</key>
	<string>${version}</string>
	<key>LSApplicationCategoryType</key>
	<string>public.app-category.developer-tools</string>
	<key>LSMinimumSystemVersion</key>
	<string>12.0</string>
	<key>NSHighResolutionCapable</key>
	<true/>
	<key>NSSupportsAutomaticGraphicsSwitching</key>
	<true/>
</dict>
</plist>
PLIST

plutil -lint "$contents/Info.plist" >/dev/null

# Ad-hoc signature: keeps LaunchServices happy with a rebuilt, local bundle.
# Not required for a locally built app, so a failure is only a warning.
if command -v codesign >/dev/null 2>&1; then
  codesign --force --sign - --timestamp=none \
    "$contents/Frameworks/libmicro_native.dylib" >/dev/null 2>&1 || true
  codesign --force --deep --sign - --timestamp=none "$bundle" >/dev/null 2>&1 \
    || echo "  warning: ad-hoc codesign failed (the app still runs locally)"
fi

# Refresh the LaunchServices record so Finder picks up a changed icon.
lsregister="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"
[[ -x "$lsregister" ]] && "$lsregister" -f "$bundle" >/dev/null 2>&1 || true

step "Done: ${bundle#"$REPO_ROOT"/}"
du -sh "$bundle" | sed 's/^/  /'
printf '  launch: open "%s"\n' "$bundle"

if "$open_after"; then
  open "$bundle"
fi
