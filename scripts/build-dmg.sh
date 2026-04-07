#!/usr/bin/env bash
set -euo pipefail

# ── Config ──────────────────────────────────────────────────────────
APP_NAME="Rivet"
BUNDLE_ID="tech.artelproject.rivet"
TEAM_ID="6DR98YW3PY"
VERSION="0.1.0"
BUILD_NUMBER="1"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BUILD_DIR="$ROOT/build"
APP_BUNDLE="$BUILD_DIR/$APP_NAME.app"
DMG_PATH="$BUILD_DIR/$APP_NAME.dmg"

SWIFT_APP_DIR="$ROOT/RivetApp"
RESOURCES_DIR="$SWIFT_APP_DIR/Resources"
BRANDING_DIR="$ROOT/branding"

# Signing identity (override with SIGNING_IDENTITY env var)
SIGNING_IDENTITY="${SIGNING_IDENTITY:-}"

# Notarization keychain profile (override with NOTARY_PROFILE env var)
NOTARY_PROFILE="${NOTARY_PROFILE:-notarytool}"

# ── Helpers ─────────────────────────────────────────────────────────
log()  { printf "\033[1;34m==>\033[0m \033[1m%s\033[0m\n" "$*"; }
warn() { printf "\033[1;33m⚠\033[0m  %s\n" "$*"; }
err()  { printf "\033[1;31m✖\033[0m  %s\n" "$*" >&2; exit 1; }

render_png_from_svg() {
    local svg="$1"
    local size="$2"
    local out="$3"

    if command -v rsvg-convert &>/dev/null; then
        rsvg-convert -w "$size" -h "$size" "$svg" -o "$out"
        return
    fi

    if command -v qlmanage &>/dev/null; then
        local thumb_dir
        thumb_dir="$(mktemp -d "$BUILD_DIR/qlthumb.XXXXXX")"
        qlmanage -t -s "$size" -o "$thumb_dir" "$svg" >/dev/null 2>&1

        local thumb_path="$thumb_dir/$(basename "$svg").png"
        if [[ ! -f "$thumb_path" ]]; then
            rm -rf "$thumb_dir"
            err "Quick Look failed to render $svg"
        fi

        mv "$thumb_path" "$out"
        rm -rf "$thumb_dir"
        return
    fi

    err "Neither rsvg-convert nor qlmanage is available to render $svg"
}

# ── Auto-detect signing identity ────────────────────────────────────
detect_signing_identity() {
    if [[ -n "$SIGNING_IDENTITY" ]]; then
        return
    fi

    # Prefer "Developer ID Application" for distribution
    local devid
    devid=$(security find-identity -v -p codesigning | grep "Developer ID Application" | head -1 | awk -F'"' '{print $2}') || true
    if [[ -n "$devid" ]]; then
        SIGNING_IDENTITY="$devid"
        log "Using Developer ID: $SIGNING_IDENTITY"
        return
    fi

    # Fall back to "Apple Development" (works locally, won't notarize)
    local dev
    dev=$(security find-identity -v -p codesigning | grep "Apple Development" | head -1 | awk -F'"' '{print $2}') || true
    if [[ -n "$dev" ]]; then
        SIGNING_IDENTITY="$dev"
        warn "Using Apple Development cert — DMG will work locally but won't pass notarization"
        warn "For notarization, install a 'Developer ID Application' certificate"
        return
    fi

    err "No codesigning identity found. Install a certificate via Xcode → Settings → Accounts."
}

# ── Step 1: Build Rust daemon ───────────────────────────────────────
build_rust() {
    log "Building rivetd (Rust, release)..."
    (cd "$ROOT" && cargo build --release -p rivet-daemon --bin rivetd)
    log "Building rivet CLI (Rust, release)..."
    (cd "$ROOT" && cargo build --release -p rivet-cli --bin rivet)
}

# ── Step 2: Build SwiftUI app ───────────────────────────────────────
build_swift() {
    log "Building RivetApp (Swift, release)..."
    (cd "$SWIFT_APP_DIR" && swift build -c release)
}

# ── Step 3: Generate app icon ───────────────────────────────────────
generate_icon() {
    local icns_path="$BUILD_DIR/AppIcon.icns"
    local svg="$BRANDING_DIR/rivet-icon.svg"

    if [[ ! -f "$svg" ]]; then
        warn "No SVG icon found at $svg — skipping icon generation"
        return
    fi

    if command -v rsvg-convert &>/dev/null; then
        log "Generating AppIcon.icns from SVG..."
    else
        warn "rsvg-convert not found — falling back to Quick Look thumbnails for icon rendering"
        log "Generating AppIcon.icns from SVG via Quick Look..."
    fi

    local iconset="$BUILD_DIR/AppIcon.iconset"
    mkdir -p "$iconset"

    local sizes=(16 32 64 128 256 512 1024)
    for size in "${sizes[@]}"; do
        render_png_from_svg "$svg" "$size" "$iconset/icon_${size}x${size}.png"
    done

    # macOS iconset naming convention
    cp "$iconset/icon_32x32.png"    "$iconset/icon_16x16@2x.png"
    cp "$iconset/icon_64x64.png"    "$iconset/icon_32x32@2x.png"
    cp "$iconset/icon_256x256.png"  "$iconset/icon_128x128@2x.png"
    cp "$iconset/icon_512x512.png"  "$iconset/icon_256x256@2x.png"
    cp "$iconset/icon_1024x1024.png" "$iconset/icon_512x512@2x.png"

    # Remove non-standard names
    rm -f "$iconset/icon_64x64.png" "$iconset/icon_1024x1024.png"

    iconutil -c icns "$iconset" -o "$icns_path"
    rm -rf "$iconset"
    log "Icon generated: $icns_path"
}

# ── Step 4: Assemble .app bundle ────────────────────────────────────
assemble_app() {
    log "Assembling $APP_NAME.app bundle..."

    rm -rf "$APP_BUNDLE"
    mkdir -p "$APP_BUNDLE/Contents/MacOS"
    mkdir -p "$APP_BUNDLE/Contents/Resources"

    # Copy Info.plist
    cp "$RESOURCES_DIR/Info.plist" "$APP_BUNDLE/Contents/"

    # Copy executables
    local swift_bin
    swift_bin=$(find "$SWIFT_APP_DIR/.build/release" -name "RivetApp" -type f -perm +111 | head -1)
    if [[ -z "$swift_bin" ]]; then
        swift_bin="$SWIFT_APP_DIR/.build/arm64-apple-macosx/release/RivetApp"
    fi
    cp "$swift_bin" "$APP_BUNDLE/Contents/MacOS/RivetApp"
    cp "$ROOT/target/release/rivetd" "$APP_BUNDLE/Contents/MacOS/rivetd"

    # Copy icon if generated
    if [[ -f "$BUILD_DIR/AppIcon.icns" ]]; then
        cp "$BUILD_DIR/AppIcon.icns" "$APP_BUNDLE/Contents/Resources/"
    fi

    # Set executable permissions
    chmod +x "$APP_BUNDLE/Contents/MacOS/RivetApp"
    chmod +x "$APP_BUNDLE/Contents/MacOS/rivetd"

    log "App bundle assembled: $APP_BUNDLE"
}

# ── Step 5: Code sign ───────────────────────────────────────────────
sign_app() {
    detect_signing_identity
    local entitlements="$RESOURCES_DIR/Rivet.entitlements"

    log "Signing with: $SIGNING_IDENTITY"

    # Sign rivetd first (nested binary)
    codesign --force --options runtime --timestamp \
        --sign "$SIGNING_IDENTITY" \
        "$APP_BUNDLE/Contents/MacOS/rivetd"

    # Sign the main app bundle
    codesign --force --options runtime --timestamp \
        --entitlements "$entitlements" \
        --sign "$SIGNING_IDENTITY" \
        "$APP_BUNDLE"

    # Verify
    codesign --verify --deep --strict --verbose=2 "$APP_BUNDLE" 2>&1
    log "Signature verified ✓"
}

# ── Step 6: Create DMG ──────────────────────────────────────────────
create_dmg() {
    log "Creating DMG..."
    rm -f "$DMG_PATH"

    local dmg_staging="$BUILD_DIR/dmg-staging"
    rm -rf "$dmg_staging"
    mkdir -p "$dmg_staging"

    # Copy app
    cp -R "$APP_BUNDLE" "$dmg_staging/"

    # Create Applications symlink
    ln -s /Applications "$dmg_staging/Applications"

    # Create DMG
    hdiutil create \
        -volname "$APP_NAME" \
        -srcfolder "$dmg_staging" \
        -ov \
        -format UDZO \
        "$DMG_PATH"

    rm -rf "$dmg_staging"

    # Sign the DMG
    detect_signing_identity
    codesign --force --sign "$SIGNING_IDENTITY" --timestamp "$DMG_PATH"

    log "DMG created: $DMG_PATH"
}

# ── Step 7: Notarize ───────────────────────────────────────────────
notarize() {
    log "Submitting for notarization..."

    # Check if credentials are configured
    if ! xcrun notarytool history --keychain-profile "$NOTARY_PROFILE" &>/dev/null; then
        warn "Notarization credentials not configured."
        warn "Run: xcrun notarytool store-credentials \"$NOTARY_PROFILE\" --apple-id YOUR_ID --team-id YOUR_TEAM --password APP_SPECIFIC_PASSWORD"
        warn "Skipping notarization."
        return
    fi

    xcrun notarytool submit "$DMG_PATH" \
        --keychain-profile "$NOTARY_PROFILE" \
        --wait

    # Staple the ticket
    log "Stapling notarization ticket..."
    xcrun stapler staple "$DMG_PATH"

    log "Notarization complete ✓"
}

# ── Main ────────────────────────────────────────────────────────────
main() {
    log "Building $APP_NAME v$VERSION (build $BUILD_NUMBER)"
    echo

    mkdir -p "$BUILD_DIR"

    build_rust
    build_swift
    generate_icon
    assemble_app
    sign_app
    create_dmg

    if [[ "${SKIP_NOTARIZE:-}" != "1" ]]; then
        notarize
    else
        warn "Skipping notarization (SKIP_NOTARIZE=1)"
    fi

    echo
    log "Done! Artifacts:"
    echo "   App:  $APP_BUNDLE"
    echo "   DMG:  $DMG_PATH"
    if [[ -f "$ROOT/target/release/rivet" ]]; then
        echo "   CLI:  $ROOT/target/release/rivet"
    fi
}

main "$@"
