# freemapper-core

[English version](README.en.md)

<!-- claude-cost:start -->
[![KI-Aufwand (Claude API): 0 $](https://img.shields.io/badge/KI--Aufwand%20%28Claude%20API%29-0%20%24-8A2BE2)](docs/ai-cost/history.csv)
<!-- claude-cost:end -->

Die Entstehung der Bibliotheken ist im KI-Aufwand der Anwendungen erfasst, vor allem in freemapper und freemapper-courseplan; das Badge zählt nur Sessions, die direkt in freemapper-core gestartet wurden.

Gemeinsame Bibliotheken der freemapper-Suite. Was mehr als eine der Anwendungen braucht, steht hier einmal statt
als Kopie in jeder:

- [freemapper](https://github.com/HenryJobst/freemapper) – Karten aus offenen Daten,
- [freemapper-courseplan](https://github.com/HenryJobst/freemapper-courseplan) – Bahnlegung,
- [freemapper-print](https://github.com/HenryJobst/freemapper-print) – Druck von Karten und Bahnen.

## Stand

Das Projekt beginnt gerade. Enthalten sind bisher `freemapper-fs` und `freemapper-text`, die anderen Bibliotheken sind
geplant:

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

Wer eine Anwendung und eine Bibliothek zugleich ändert, legt freemapper-core neben die Anwendung
(`../freemapper-core`) und trägt in der `Cargo.toml` der Anwendung vorübergehend den lokalen Ordner ein:

```toml
[patch."https://github.com/HenryJobst/freemapper-core"]
freemapper-text = { path = "../freemapper-core/crates/freemapper-text" }
```

Der Eintrag gilt nur zum Entwickeln. Vor dem Merge der Anwendung kommt er wieder heraus, nachdem die Änderung in
freemapper-core veröffentlicht ist und die Anwendung die neue Version einbindet (Entscheidung 0006).

Entscheidungen stehen in [`docs/adr/`](docs/adr/README.md), Änderungen im [Changelog](CHANGELOG.md).

## Lizenz

GPL-3.0-or-later, siehe [LICENSE](LICENSE).
