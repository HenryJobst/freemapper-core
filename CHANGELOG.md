# Änderungen

[English version](CHANGELOG.en.md)

Hier stehen alle wichtigen Änderungen an freemapper-core, die neueste zuerst, jeweils mit der betroffenen
Bibliothek. Das Format folgt [Keep a Changelog](https://keepachangelog.com/de/1.1.0/).

## [Unveröffentlicht]

### Geändert

- freemapper-course: Jede Variante von `Special` hat ein neues erstes Feld `id: Option<u32>` und
  `CourseSource` das Feld `item_scaling: Option<ItemScaling>`. Anwendungen, die `Special`-Varianten oder
  `CourseSource` aufbauen oder ohne `..` zerlegen, ergänzen `id: None` und `item_scaling: None`; wer
  `Special` mit `..` zerlegt, ändert nichts. Für freemapper-print heißt das: Literale bekommen `id: None` und
  `item_scaling: None`. Der Leser lehnt außerdem eine `special-object/@id` mit ungültiger Entität jetzt ab, wo
  er sie früher übersah. Die Bibliothek geht deshalb auf 0.4.0.

### Hinzugefügt

- freemapper-course: `Special::id` liefert die ID aus `special-object/@id` der `.ppen` (`None` bei einer ID
  außerhalb von `u32`), `Special::courses` den Bahnfilter. `ItemScaling` (bisher intern im Leser) ist
  öffentlich, `CourseSource::item_scaling` hält die Einstellung der Datei (`None` bei IOF-XML), und
  `course_obj_ratio` rechnet den Bahnobjektfaktor aus Skalierung, Karten- und Druckmaßstab aus. Der Leser
  nutzt dieselbe Funktion. Das braucht der `.ppen`-Schreiber von freemapper-courseplan.
- freemapper-course: `write_ppen` schreibt eine Veranstaltung als neue `.ppen`-Datei, so formatiert, wie
  Purple Pen sie schreibt (Modul `ppen::write`). `ScratchOptions` legt die Karte (`PpenMap`), Karten- und
  Beschreibungsstandard (`MapStandard`, `DescriptionStandard`), die Sprache der Beschreibungen und das
  Zeilenende (`LineEnding`, Vorgabe CRLF wie Purple Pen unter Windows) fest. Das Ergebnis `Written` enthält den
  Dateitext, die ID jedes Objekts (`AssignedIds`) und Hinweise (`WriteNote`). Objekte behalten ihre IDs; eine
  fehlende oder doppelte ID wird zur größten ID ihrer Art plus eins. Der Schreiber prüft die Veranstaltung
  zuerst gegen die Grenzen des Lesers und lehnt mit `PpenWriteError` ab, was Purple Pen nicht speichern kann,
  etwa zwei Bahnen, die dieselbe Strecke verschieden zeichnen. Entscheidung 0014.
- freemapper-course: `write_ppen_patch` schreibt eine Veranstaltung in eine vorhandene `.ppen`, die
  Basisdatei. Der Patch ordnet Posten und Bahnen über Code oder Namen zu. Die IDs der Veranstaltung nutzt er
  nur, wenn `PatchOptions::ids_from_base` sagt, dass sie aus dieser Basisdatei stammen, wie bei der Datei, mit
  der eine Bahnplanung verknüpft ist; dann bleibt eine umbenannte Bahn oder ein Posten mit neuem Code dasselbe
  Objekt. Als Vorgabe übergeht er jede ID, damit eine Veranstaltung aus einer anderen Datei keine Objekte über
  zufällig gleiche IDs übernimmt. Ein Objekt ohne geänderte Felder behält seine Bytes, ein Patch mit der
  Veranstaltung, die der Leser aus einer Datei gelesen hat, ergibt also genau diese Datei. Geänderte Felder
  schreibt er neu, Attribute und Elemente, die der Schreiber nicht kennt, bleiben stehen. Gelöscht wird nur,
  was `Deletions` in `PatchOptions` per ID nennt; `Written::kept` listet die Objekte der Basisdatei, die die
  Veranstaltung nicht erwähnt. Eine Strecke oder Kreislücken, die auch eine Bahn außerhalb der Veranstaltung
  zeichnet, ändert der Patch nicht, er lehnt ab. Eine Teilabflaggung, deren Punkt kein Knickpunkt der Strecke
  mehr ist, entfällt und steht als `WriteNote::PartialFlaggingDropped` im Ergebnis. Bekannte Grenzen: Die
  Zweige einer behaltenen Gabelungsbahn prüft der Patch nicht, Übergänge bekommen immer die Ausrichtung 0, und
  `options` entfällt ohne Druckmaßstab, Höhenmeter und Länge. Entscheidung 0014.

## [0.3.0] - 2026-11-10

### Hinzugefügt

- freemapper-omap: Kartenmodell und Leser für Karten von OpenOrienteering Mapper (`.omap`, `.xmap`) mit den
  eingebetteten Symbolsätzen ISSprOM 2019 und Schulkarte 2019 und den Randsymbolen 991–994. Übernommen aus
  freemapper-print (Commit `918cec2`) mit gleichen Pfaden, gleichem Verhalten und gleichen `OmapError`-Texten,
  samt der Verschachtelungsgrenze `MAX_XML_DEPTH`. Neu öffentlich sind `too_deep`, `symbol_set::attribute`
  und `SymbolSet::without_codes`, weil der Renderer und die Tests von freemapper-print sie brauchen.
  Entscheidung 0012.
- freemapper-course: Bahnmodell, Leser für Purple Pen (`.ppen`) und IOF-XML 3.0, Einpassung einer Bahndatei in
  die Karte und Platzierung der Postennummern. Übernommen aus freemapper-print (Commit `918cec2`, Module
  `course/` und `placement/`) mit gleichen Pfaden, gleichem Verhalten, gleichen Lesegrenzen und gleichen
  `CourseError`- und `FitError`-Texten. Neu sind `Control::id` und `Course::id` mit den IDs aus der `.ppen`
  (`None` bei IOF-XML) und `placement_from_offset` und `offset_from_placement`, die einen Nummernversatz in eine
  Nummernlage umrechnen und zurück. Neu öffentlich sind die Teile von `xml`, die der Leser der
  Beschreibungssymbole in freemapper-print braucht. Wer `Control` oder `Course` selbst baut, ergänzt `id: None`.
  Die Einpassung hängt am Feature `fit` (standardmäßig an); ohne es baut die Bibliothek ohne `freemapper-omap`.
  Entscheidung 0013.

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
