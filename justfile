default: dev

dev:
    cargo run --release

# Build release binary and package as a standalone AppImage
appimage:
    #!/usr/bin/env bash
    set -euo pipefail

    echo "==> Building release binary..."
    cargo build --release

    APP_DIR="target/appimage/AppDir"
    OUTPUT_APPIMAGE="target/Ratlas-x86_64.AppImage"

    echo "==> Preparing AppDir at ${APP_DIR}..."
    rm -rf "${APP_DIR}"
    mkdir -p "${APP_DIR}/usr/bin"
    mkdir -p "${APP_DIR}/usr/share/applications"
    mkdir -p "${APP_DIR}/usr/share/icons/hicolor/256x256/apps"
    mkdir -p "${APP_DIR}/usr/share/ratlas"

    cp "target/release/ratlas" "${APP_DIR}/usr/bin/ratlas"

    if [ -d "assets" ]; then
        cp -r "assets" "${APP_DIR}/"
        cp -r "assets" "${APP_DIR}/usr/share/ratlas/"
    fi

    if [ -f "assets/ratlas.desktop" ]; then
        cp "assets/ratlas.desktop" "${APP_DIR}/ratlas.desktop"
    else
        printf '%s\n' \
            "[Desktop Entry]" \
            "Type=Application" \
            "Name=Ratlas" \
            "Comment=3D Radio Atlas & Streaming Player" \
            "Exec=ratlas" \
            "Icon=ratlas" \
            "Categories=AudioVideo;Audio;Player;" \
            "Terminal=false" \
            "StartupWMClass=ratlas" > "${APP_DIR}/ratlas.desktop"
    fi
    cp "${APP_DIR}/ratlas.desktop" "${APP_DIR}/usr/share/applications/ratlas.desktop"

    if [ -f "assets/ratlas.png" ]; then
        cp "assets/ratlas.png" "${APP_DIR}/ratlas.png"
        cp "assets/ratlas.png" "${APP_DIR}/.DirIcon"
        cp "assets/ratlas.png" "${APP_DIR}/usr/share/icons/hicolor/256x256/apps/ratlas.png"
    fi

    printf '%s\n' \
        "#!/bin/sh" \
        "set -e" \
        'HERE="$(dirname "$(readlink -f "$0")")"' \
        'export PATH="${HERE}/usr/bin:${PATH}"' \
        'export LD_LIBRARY_PATH="${HERE}/usr/lib:${LD_LIBRARY_PATH}"' \
        'export XDG_DATA_DIRS="${HERE}/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"' \
        'cd "${HERE}"' \
        'exec "${HERE}/usr/bin/ratlas" "$@"' > "${APP_DIR}/AppRun"
    chmod +x "${APP_DIR}/AppRun"

    # Locate or download appimagetool
    TOOL="appimagetool"
    if ! command -v appimagetool >/dev/null 2>&1; then
        TOOL_DIR="target/appimage"
        TOOL="${TOOL_DIR}/appimagetool"
        if [ ! -f "${TOOL}" ]; then
            echo "==> Downloading appimagetool..."
            mkdir -p "${TOOL_DIR}"
            curl -fsSL -o "${TOOL}" "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage"
            chmod +x "${TOOL}"
        fi
    fi

    echo "==> Generating ${OUTPUT_APPIMAGE}..."
    ARCH=x86_64 "${TOOL}" --appimage-extract-and-run "${APP_DIR}" "${OUTPUT_APPIMAGE}"
    echo "==> Successfully packaged AppImage: ${OUTPUT_APPIMAGE}"

# Run the packaged AppImage
run-appimage: appimage
    ./target/Ratlas-x86_64.AppImage

# Build release Windows binary and package into a zip archive
windows-zip:
    #!/usr/bin/env bash
    set -euo pipefail

    echo "==> Building Windows release binary (x86_64-pc-windows-gnu)..."
    cargo build --release --target x86_64-pc-windows-gnu

    DIST_DIR="target/windows-package/ratlas"
    OUTPUT_ZIP="$(pwd)/target/Ratlas-Windows-x86_64.zip"

    echo "==> Preparing Windows distribution folder at ${DIST_DIR}..."
    rm -rf "${DIST_DIR}"
    mkdir -p "${DIST_DIR}"

    cp "target/x86_64-pc-windows-gnu/release/ratlas.exe" "${DIST_DIR}/ratlas.exe"

    if [ -d "assets" ]; then
        cp -r "assets" "${DIST_DIR}/"
    fi

    echo "==> Creating ${OUTPUT_ZIP}..."
    rm -f "${OUTPUT_ZIP}"
    (
        cd target/windows-package
        if command -v bsdtar >/dev/null 2>&1; then
            bsdtar -a -cf "${OUTPUT_ZIP}" ratlas
        elif command -v zip >/dev/null 2>&1; then
            zip -r "${OUTPUT_ZIP}" ratlas
        else
            python3 -c "import shutil; shutil.make_archive('${OUTPUT_ZIP%.zip}', 'zip', '.', 'ratlas')"
        fi
    )

    echo "==> Successfully packaged Windows zip: ${OUTPUT_ZIP}"

# Alias for windows-zip
package-windows: windows-zip
