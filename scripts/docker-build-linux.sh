#!/usr/bin/env bash
set -e

cd /app

echo "=== Ensuring frontend dependencies ==="
if [ ! -d "node_modules/@tauri-apps/cli-linux-x64-gnu" ]; then
  npm install --no-save @tauri-apps/cli-linux-x64-gnu
fi

echo "=== Building Tauri Linux bundles (deb) ==="
export DISABLE_COPYRIGHT_FILES_DEPLOYMENT=1
export APPIMAGE_EXTRACT_AND_RUN=1
export RUSTFLAGS="-C linker=clang -C link-arg=-fuse-ld=lld"
export CARGO_BUILD_JOBS=2

npx tauri build --bundles deb

echo "=== Packaging AppImage with appimagetool ==="
mkdir -p /tmp/build/AppDir
ar -p /app/src-tauri/target/release/bundle/deb/ClipCut_0.2.0_amd64.deb data.tar.gz | tar -xz -C /tmp/build/AppDir

cd /tmp/build/AppDir
ln -sf usr/share/applications/ClipCut.desktop ClipCut.desktop
ln -sf usr/share/icons/hicolor/128x128/apps/clipcut.png clipcut.png
ln -sf usr/share/icons/hicolor/128x128/apps/clipcut.png .DirIcon

cat << "EOF" > AppRun
#!/bin/sh
SELF=$(readlink -f "$0")
HERE=${SELF%/*}
export PATH="${HERE}/usr/bin:${PATH}"
exec "${HERE}/usr/bin/clipcut" "$@"
EOF
chmod +x AppRun

export PATH="/opt/appimage-plugin/appimagetool-prefix/usr/bin:$PATH"
mkdir -p /app/src-tauri/target/release/bundle/appimage
ARCH=x86_64 appimagetool -n /tmp/build/AppDir /tmp/ClipCut_0.2.0_amd64.AppImage
cp /tmp/ClipCut_0.2.0_amd64.AppImage /app/src-tauri/target/release/bundle/appimage/ClipCut_0.2.0_amd64.AppImage

echo "=== Linux build complete ==="
if [ -n "$HOST_UID" ] && [ -n "$HOST_GID" ]; then
  chown -R "$HOST_UID:$HOST_GID" /app/src-tauri/target/release/bundle 2>/dev/null || true
fi
