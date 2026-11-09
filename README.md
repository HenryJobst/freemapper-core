# freemapper-core

[English version](README.en.md)

Gemeinsame Bibliotheken der freemapper-Suite. Was mehr als eine der Anwendungen braucht, steht hier einmal statt
als Kopie in jeder:

- [freemapper](https://github.com/HenryJobst/freemapper) – Karten aus offenen Daten,
- [freemapper-courseplan](https://github.com/HenryJobst/freemapper-courseplan) – Bahnlegung,
- [freemapper-print](https://github.com/HenryJobst/freemapper-print) – Druck von Karten und Bahnen.

## Stand

Das Projekt beginnt gerade. Enthalten sind bisher `freemapper-fs`, `freemapper-rules` und `freemapper-text`, die anderen
Bibliotheken sind geplant:

| Bibliothek | Inhalt | kommt aus | Stand |
|---|---|---|---|
| `freemapper-text` | Programmtexte mit Fluent, Spracherkennung | freemapper-courseplan | enthalten |
| `freemapper-rules` | Regelprofile für Prüfungen, gelesen und geprüft nach einem Schema der Anwendung | freemapper-courseplan, freemapper-print | enthalten |
| `freemapper-georef` | Georeferenzierung, Deklination, UTM | freemapper | geplant |
| `freemapper-fs` | Schreiben, Kopieren und Löschen im Projektordner, ohne einer Verknüpfung zu folgen | freemapper | enthalten |
| `freemapper-omap` | .omap lesen und schreiben, Symbolsätze | freemapper, freemapper-print | geplant |
| `freemapper-render` | Kartendarstellung, Schriften, PDF | freemapper-print | geplant |
| `freemapper-course` | Bahnen, .ppen und IOF-XML lesen, Nummernplatzierung | freemapper-print, freemapper-courseplan | geplant |

## Nutzung

Die Anwendungen binden die Bibliotheken, die sie brauchen, über Git mit fester Version ein:

```toml
freemapper-text = { git = "https://github.com/HenryJobst/freemapper-core", tag = "v0.2.0" }
freemapper-rules = { git = "https://github.com/HenryJobst/freemapper-core", tag = "v0.2.0" }
```

Wer eine Anwendung und eine Bibliothek zugleich ändert, legt freemapper-core neben die Anwendung
(`../freemapper-core`) und trägt in der `Cargo.toml` der Anwendung vorübergehend den lokalen Ordner ein:

```toml
[patch."https://github.com/HenryJobst/freemapper-core"]
freemapper-text = { path = "../freemapper-core/crates/freemapper-text" }
freemapper-rules = { path = "../freemapper-core/crates/freemapper-rules" }
```

Der Eintrag gilt nur zum Entwickeln. Vor dem Merge der Anwendung kommt er wieder heraus, nachdem die Änderung in
freemapper-core veröffentlicht ist und die Anwendung die neue Version einbindet (Entscheidung 0006).

Entscheidungen stehen in [`docs/adr/`](docs/adr/README.md), Änderungen im [Changelog](CHANGELOG.md).

## Lizenz

GPL-3.0-or-later, siehe [LICENSE](LICENSE).
