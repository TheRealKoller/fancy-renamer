# fancy-renamer

Datei-Umbenennungstool mit grafischer Oberfläche für **Fedora Linux**: Dateien und Datei-Paare
(z. B. `abc.jpg` + `abc.raw`) per Regelkette umbenennen – mit Live-Vorschau und Kollisionsschutz.

Technik: Rust + GTK4/libadwaita. Lizenz: GPL-3.0.

> Status: in Entwicklung, Version 1.0 siehe [Milestone](https://github.com/TheRealKoller/fancy-renamer/milestone/1).

## Voraussetzungen

- Fedora mit GTK4 und libadwaita:

  ```sh
  sudo dnf install gtk4-devel libadwaita-devel gcc pkgconf-pkg-config
  ```

- Rust (Edition 2024, mindestens 1.85) – z. B. per [rustup](https://rustup.rs).

## Bauen, starten, testen

```sh
cargo build --release   # Binär: target/release/fancy-renamer
cargo run               # App starten
cargo test              # Tests; UI-Tests brauchen ein Display (siehe unten)
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

## Bedienung

Dateien über **Dateien hinzufügen** (`Ctrl+O`), **Ordner hinzufügen** (oberste Ebene, ohne
versteckte Dateien) oder per Drag & Drop ins Fenster holen – oder die App direkt mit Pfaden
starten:

```sh
fancy-renamer ~/Bilder/*.jpg
```

Die Liste bleibt natürlich sortiert (`foto2` vor `foto10`), doppelte Pfade werden nicht erneut
aufgenommen, nicht lesbare Pfade meldet ein Hinweis.

Im Regelbereich lassen sich die vier Karten kombinieren, per Hoch/Runter umsortieren und einzeln
abschalten; die Reihenfolge der Karten ist die Reihenfolge der Regeln. Jede Änderung berechnet die
Vorschau neu (entprellt), große Listen im Hintergrund. Zeilen mit Problemen (Kollision, doppelter
Name, ungültiger Name) blockieren **Umbenennen** und nennen den Grund im Tooltip.

Tastenkürzel: `Ctrl+O` Dateien, `Ctrl+Enter` umbenennen, `Ctrl+Entf` Liste leeren. Einzelne Zeilen
entfernt der Knopf am Zeilenende.

## Paare

Dateien mit gleichem Stamminhalt und unterschiedlicher Erweiterung (`abc.jpg` + `abc.raw`,
`abc.CR2`, `abc.xmp`) bilden eine **Gruppe**. Die Vorschau zeigt sie als Block mit Kopfzeile
„Gruppe abc → abc“; Regeln mit Gruppenbezug (z. B. Nummerierung) vergeben innerhalb der Gruppe
denselben Wert, sodass der Zusammenhang im Dateisystem erhalten bleibt. Gruppen entstehen nur bei
mindestens zwei Dateien mit identischem Stamm, Einzeldateien erscheinen ohne Kopfzeile.

## Tests

`cargo test` läuft ohne Display; die UI-Tests (`tests/ui.rs`) brauchen eines und überspringen sich
sonst. Mit virtuellem X-Server:

```sh
xvfb-run -a cargo test
```

## Projektstruktur

| Pfad | Inhalt |
|---|---|
| `src/engine/` | Domänenlogik – reine Datenverarbeitung, keine GTK-Abhängigkeiten, ohne Display testbar |
| `src/ui/` | GTK4/libadwaita-Oberfläche |
| `src/main.rs` | Einstiegspunkt |

## Entwicklung

Das Projekt wird vollständig KI-gestützt entwickelt. Arbeitsweise, Issue-Lebenszyklus und
Leitplanken stehen in [AGENTS.md](AGENTS.md).

## Lizenz

[GPL-3.0](LICENSE)
