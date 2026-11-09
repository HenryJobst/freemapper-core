# Änderungen

[English version](CHANGELOG.en.md)

Hier stehen alle wichtigen Änderungen an freemapper-core, die neueste zuerst, jeweils mit der betroffenen
Bibliothek. Das Format folgt [Keep a Changelog](https://keepachangelog.com/de/1.1.0/).

## [0.2.0] - 2026-11-09

### Hinzugefügt

- freemapper-rules: Regelprofile für Prüfungen als TOML-Dateien, gelesen und zu einem Regelsatz zusammengeführt.
  Die Anwendung beschreibt in einem `Schema`, welche Prüfregel-Schlüssel, Wertnamen, Einheiten, Tabellen und
  Filter es gibt. Übernommen aus freemapper-courseplan 0.4.0 mit gleichem Dateiformat, gleichem Zusammenführen
  und gleichen `RulesError`-Texten. Aus freemapper-print kommen dazu: Wertnamen je Schlüssel, Bereichsprüfung der Zahlen
  nach Einheit, Schlüssel ohne Filter, eigene Prüfungen der Anwendung für Tabellen und Werte, und
  `RuleSet::pick`, das bei mehr als einer passenden Regel mit dem neuen Fehler `Ambiguous` abbricht. Filter heißen jetzt
  nach ihrem Feld (`map_kinds`, `disciplines`, `print_scales`) statt fester Felder der Regel. Eine Regel, die
  die eigenen Vorschläge des Programms zitiert, braucht eine Notiz. Entscheidung 0011.

### Geändert

- freemapper-text 0.2.0 hat den Trait `Localize` nicht mehr. Für Nutzer von 0.1.0 ist das eine inkompatible
  Änderung: Eine Anwendung, die ihn importiert, definiert ihn selbst, wie es freemapper-courseplan und
  freemapper-print schon tun.

### Entfernt

- freemapper-text: der Trait `Localize`. Keine Anwendung konnte ihn nutzen, weil die Orphan-Regel verbietet,
  ihn für Fehlertypen aus einer dritten Bibliothek zu implementieren. Anwendungen behalten ihren eigenen Trait;
  wer `freemapper_text::Localize` importiert, definiert ihn selbst (Nachtrag zu Entscheidung 0010).

## [0.1.0] - 2026-11-09

### Hinzugefügt

- Grundgerüst: Arbeitsbereich, CI auf Linux, macOS und Windows, Entscheidungen 0001 bis 0007.
- freemapper-text: Programmtexte aus Fluent-Katalogen. Die Anwendung bringt ihre `.ftl`-Dateien mit, die
  Bibliothek erkennt die Sprache (`--lang`, `FREEMAPPER_LANG`, `LC_ALL`, `LC_MESSAGES`, `LANG`), nimmt Englisch,
  wenn ein Text fehlt, und prüft Kataloge mit `Catalog::problems`. Übernommen aus freemapper-courseplan 0.4.0
  mit gleichem Verhalten; Entscheidung 0010.
- freemapper-fs: Schreiben, Kopieren und Löschen im Projektordner, das keiner Verknüpfung folgt. Jede Datei
  entsteht als neue Teildatei und wird dann über das Ziel umbenannt; eine Verknüpfung am Ziel wird ersetzt, ihr
  Ziel bleibt unberührt. Ein Pfad, dessen Ordner über eine Verknüpfung aus dem Projektordner führt, wird
  abgelehnt. Kopien lesen nur Dateien des Projekts selbst. Übernommen aus freemapper 0.6.2 mit gleichem
  Verhalten, nur nennen die Fehlertexte keine Anwendung mehr, und `remove_stale_parts_inside` gibt einen
  abgelehnten Ordner als Fehler zurück, statt selbst eine Warnung auszugeben. Dazu kommen die Entscheidungen
  0008 und 0009 (vorher freemapper 0059 und 0060).
