#!/bin/sh
# Installs delve for the current user, with an entry in the applications menu. Works
# from the source tree, which it builds, and from a release archive, already built.
#   ./install.sh            install
#   ./install.sh --remove   uninstall (saves and gold are kept)
set -eu

cd "$(dirname "$0")"
data="${XDG_DATA_HOME:-$HOME/.local/share}"
bin="$HOME/.local/bin"
apps="$data/applications"
icons="$data/icons/hicolor/256x256/apps"

refresh() {
    command -v update-desktop-database >/dev/null && update-desktop-database -q "$apps" || true
    command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$data/icons/hicolor" || true
}

if [ "${1:-}" = "--remove" ]; then
    rm -f "$bin/delve" "$bin/delve-term" "$apps/delve.desktop" "$icons/delve.png"
    refresh
    echo "delve removed. Saves and gold stay in $data/delve."
    exit 0
fi

if [ -f Cargo.toml ]; then
    cargo build --release --workspace
    built=target/release
    icon=crates/gui/assets/delve.png
else
    built=.
    icon=delve.png
fi
mkdir -p "$bin" "$apps" "$icons"
install -m 755 "$built/delve" "$built/delve-term" "$bin/"
install -m 644 "$icon" "$icons/delve.png"
cat > "$apps/delve.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=delve
GenericName=Roguelike
Comment=Go down the dungeon and seize the Amulet
Exec=$bin/delve
Icon=delve
Terminal=false
Categories=Game;RolePlaying;
Keywords=roguelike;dungeon;rpg;
StartupWMClass=delve
EOF
refresh
echo "delve installed: look for it in your applications (Super key, then type delve)."
