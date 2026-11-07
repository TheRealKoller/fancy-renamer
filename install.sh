#!/bin/sh
# Installiert fancy-renamer aus dem Quellbaum in das Benutzerverzeichnis.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PREFIX=${PREFIX:-${HOME}/.local}
BIN_DIR=${XDG_BIN_HOME:-${PREFIX}/bin}
DATA_HOME=${XDG_DATA_HOME:-${PREFIX}/share}
APP_ID=io.github.therealKoller.FancyRenamer

fail() {
    echo "Fehler: $*" >&2
    exit 1
}

command -v cargo >/dev/null 2>&1 || fail "cargo nicht gefunden – Rust installieren: https://rustup.rs"

if ! pkg-config --exists gtk4 2>/dev/null || ! pkg-config --exists libadwaita-1 2>/dev/null; then
    fail "GTK4/libadwaita-Entwicklungspakete fehlen – sudo dnf install gtk4-devel libadwaita-devel gcc pkgconf-pkg-config"
fi

echo "Baue Release …"
(cd "$ROOT" && cargo build --release --locked)

mkdir -p "$BIN_DIR" "$DATA_HOME/applications" "$DATA_HOME/icons/hicolor/scalable/apps"
install -m 755 "$ROOT/target/release/fancy-renamer" "$BIN_DIR/fancy-renamer"
install -m 644 "$ROOT/data/${APP_ID}.desktop" "$DATA_HOME/applications/${APP_ID}.desktop"
install -m 644 "$ROOT/data/${APP_ID}.svg" "$DATA_HOME/icons/hicolor/scalable/apps/${APP_ID}.svg"

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t "$DATA_HOME/icons/hicolor" 2>/dev/null || true
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q "$DATA_HOME/applications" 2>/dev/null || true
fi

echo "Installiert: $BIN_DIR/fancy-renamer"
echo "Starter:     $DATA_HOME/applications/${APP_ID}.desktop"
case ":${PATH}:" in
    *":${BIN_DIR}:"*) ;;
    *) echo "Hinweis: ${BIN_DIR} liegt nicht im PATH." ;;
esac
