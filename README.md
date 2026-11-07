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
cargo test              # Tests (headless, ohne Display)
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
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
