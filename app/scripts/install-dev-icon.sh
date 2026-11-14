#!/usr/bin/env sh
# Shows Hedy's icon on the window while developing on Linux (Wayland).
# Wayland desktops take the icon from a .desktop file named after the window's
# app_id ("hedy-app", the binary name); installed packages already ship one.
# The entry is hidden (NoDisplay), so it adds nothing to the app menu. A user
# entry shadows the system one with the same name, so it is skipped (and an old
# one removed) when the Hedy package is installed: that one would vanish from the menu.
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
data=${XDG_DATA_HOME:-$HOME/.local/share}
for size in 32 64 128; do
  dir="$data/icons/hicolor/${size}x${size}/apps"
  mkdir -p "$dir"
  cp "$here/src-tauri/icons/${size}x${size}.png" "$dir/hedy-app.png"
done
mkdir -p "$data/icons/hicolor/scalable/apps" "$data/applications"
cp "$here/src-tauri/icons/logo.svg" "$data/icons/hicolor/scalable/apps/hedy-app.svg"
for system in /usr/share/applications /usr/local/share/applications; do
  if [ -f "$system/hedy-app.desktop" ]; then
    rm -f "$data/applications/hedy-app.desktop"
    echo "Icono de Hedy instalado en $data; el paquete ya trae hedy-app.desktop."
    exit 0
  fi
done
cat > "$data/applications/hedy-app.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Hedy
Icon=hedy-app
Exec=hedy-app
StartupWMClass=hedy-app
NoDisplay=true
DESKTOP
command -v update-desktop-database >/dev/null && update-desktop-database "$data/applications" 2>/dev/null || true
echo "Icono de Hedy instalado en $data (entrada oculta hedy-app.desktop)."
