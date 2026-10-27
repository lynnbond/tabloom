#!/bin/sh
# Build a native .app; optionally install it for Finder/Spotlight launch.
set -eu
project_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
cd "$project_root"
if [ "$(uname -s)" != Darwin ]; then
    echo 'This bundle script requires macOS. Use cargo run on other platforms.' >&2
    exit 1
fi
profile=${1:-release}
install_mode=${2:---install}
if [ "$#" -gt 2 ] || { [ "$install_mode" != --install ] && [ "$install_mode" != --local ]; }; then
    echo 'Usage: scripts/macos-app.sh [debug|release] [--install|--local]' >&2
    exit 1
fi
case "$profile" in
    debug) ./scripts/cargo.sh build ;;
    release) ./scripts/cargo.sh build --release ;;
    *) echo 'Usage: scripts/macos-app.sh [debug|release] [--install|--local]' >&2; exit 1 ;;
esac
bundle="$HOME/Applications/Tabloom.app"
app_name=Tabloom
bundle_identifier=org.tabloom.Tabloom
if [ "$install_mode" = --local ]; then
    app_name="Tabloom Dev"
    bundle_identifier=org.tabloom.Tabloom.dev
    bundle="$project_root/target/$profile/$app_name.app"
fi
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
bundle_binary_tmp=$(mktemp "$bundle/Contents/MacOS/.tabloom.XXXXXX")
trap 'rm -f "$bundle_binary_tmp"' EXIT
cp "target/$profile/tabloom" "$bundle_binary_tmp"
chmod 755 "$bundle_binary_tmp"
# Replace the inode atomically so a running older build keeps its executable intact.
mv -f "$bundle_binary_tmp" "$bundle/Contents/MacOS/tabloom"
cat > "$bundle/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>tabloom</string>
<key>CFBundleIdentifier</key><string>$bundle_identifier</string>
<key>CFBundleName</key><string>$app_name</string>
<key>CFBundleDisplayName</key><string>$app_name</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>CFBundleIconFile</key><string>Tabloom.icns</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
cp assets/icon/Tabloom.icns "$bundle/Contents/Resources/"
cp THIRD_PARTY_NOTICES.md "$bundle/Contents/Resources/"
mkdir -p "$bundle/Contents/Resources/assets"
cp assets/syntax-acknowledgements.md "$bundle/Contents/Resources/assets/"
/usr/bin/plutil -lint "$bundle/Contents/Info.plist" >/dev/null
if [ "$install_mode" = --install ]; then
    launch_services=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
    # Retire the former duplicate bundles without deleting builds or workspace data.
    for old_profile in debug release; do
        old_bundle="$project_root/target/$old_profile/Tabloom.app"
        if [ -d "$old_bundle" ]; then
            "$launch_services" -u "$old_bundle"
            mkdir -p "$project_root/target/retired-apps"
            retired=$(mktemp -d "$project_root/target/retired-apps/XXXXXX")
            mv "$old_bundle" "$retired/Tabloom.app.disabled"
        fi
    done
    touch "$bundle"
    "$launch_services" -f "$bundle"
fi
echo "$bundle"
