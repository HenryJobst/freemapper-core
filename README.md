# freemapper-core

[English version](README.en.md)

Gemeinsame Bibliotheken der freemapper-Suite. Was mehr als eine der Anwendungen braucht, steht hier einmal statt
als Kopie in jeder:

- [freemapper](https://github.com/HenryJobst/freemapper) – Karten aus offenen Daten,
- [freemapper-courseplan](https://github.com/HenryJobst/freemapper-courseplan) – Bahnlegung,
- [freemapper-print](https://github.com/HenryJobst/freemapper-print) – Druck von Karten und Bahnen.

## Stand

Das Projekt beginnt gerade. Enthalten ist bisher `freemapper-fs`, die anderen Bibliotheken sind geplant:

| Bibliothek | Inhalt | kommt aus |
|---|---|---|
| `freemapper-text` | Programmtexte mit Fluent, Spracherkennung | freemapper-courseplan |
| `freemapper-rules` | Format der Regelprofile für Prüfungen | freemapper-courseplan |
| `freemapper-georef` | Georeferenzierung, Deklination, UTM | freemapper |
| `freemapper-fs` | Schreiben, Kopieren und Löschen im Projektordner, ohne einer Verknüpfung zu folgen | freemapper |
| `freemapper-omap` | .omap lesen und schreiben, Symbolsätze | freemapper, freemapper-print |
| `freemapper-render` | Kartendarstellung, Schriften, PDF | freemapper-print |
| `freemapper-course` | Bahnen, .ppen und IOF-XML lesen, Nummernplatzierung | freemapper-print, freemapper-courseplan |

## Nutzung

Die Anwendungen binden eine Bibliothek über Git mit fester Version ein:

```toml
freemapper-text = { git = "https://github.com/HenryJobst/freemapper-core", tag = "v0.1.0" }
```

Entscheidungen stehen in [`docs/adr/`](docs/adr/README.md), Änderungen im [Changelog](CHANGELOG.md).

## Lizenz

GPL-3.0-or-later, siehe [LICENSE](LICENSE).
