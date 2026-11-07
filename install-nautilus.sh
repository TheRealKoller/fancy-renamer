#!/bin/sh
# Legt die Nautilus-Erweiterung im Benutzerverzeichnis ab (idempotent).
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
DATA_HOME=${XDG_DATA_HOME:-${HOME}/.local/share}
TARGET_DIR="$DATA_HOME/nautilus-python/extensions"
TARGET="$TARGET_DIR/fancy-renamer.py"

mkdir -p "$TARGET_DIR"
install -m 644 "$ROOT/nautilus/fancy-renamer.py" "$TARGET"

echo "Erweiterung: $TARGET"
if command -v rpm >/dev/null 2>&1 && ! rpm -q nautilus-python >/dev/null 2>&1; then
    echo "Hinweis: Paket nautilus-python fehlt – sudo dnf install nautilus-python"
fi
echo "Hinweis: Nautilus neu starten, damit das Menü erscheint: nautilus -q"
