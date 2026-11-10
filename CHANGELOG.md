# Änderungen

[English version](CHANGELOG.en.md)

Hier stehen alle wichtigen Änderungen an freemapper-core, die neueste zuerst, jeweils mit der betroffenen
Bibliothek. Das Format folgt [Keep a Changelog](https://keepachangelog.com/de/1.1.0/).

## [Unveröffentlicht]

## [0.6.0] - 2026-11-10

### Hinzugefügt

- freemapper-course: `placement::place_course_numbers` platziert alle Nummern einer Bahn in Laufreihenfolge. Die
  Suche stammt aus `auto_place` von freemapper-courseplan (dort die Entscheidungen 0030 bis 0032). Gehaltene
  Nummern (`NumberStop::kept`) bleiben, wo sie sind, und die anderen weichen ihnen aus. Jede gesetzte Nummer
  ist Hindernis für die folgenden, Kreise und Strecken der Bahn zählen als Bahnaufdruck. Die Kartenhindernisse
  kommen einmal je Bahn in ein Raster. Dazu kommen `CourseNumbers`, `NumberStop`, `NO_LEGS_DIRECTION_DEG`
  (45 Grad an einem Posten ohne Strecken), `Rect::UNBOUNDED` und `GapRange::strictest`. Letzteres fasst
  mehrere `number_gap`-Regeln zusammen, mit dem größten Minimum und dem kleinsten Maximum. Die bisherigen
  Einträge von `placement` bleiben, wie sie sind. Entscheidung 0016.
- freemapper-course: Das Modul `description_symbols` enthält die Symbole der IOF-Postenbeschreibung mit
  Zeichnung sowie deutschem und englischem Text aus Purple Pens `symbols.xml` (BSD-Lizenz). Es stammt aus
  freemapper-print (dort `describe::symbols`), die Datei liegt unverändert in `data/`. Gegenüber dort gibt es
  `TextLang` statt `SheetLang`, `symbol_kind` (die Spalte, in die ein Symbol gehört) und `exists`. Dazu bildet
  das Modul zwischen Purple-Pen-Kennungen und IOF-2024-Nummern ab. `iof_2024` ist die Vorwärtsabbildung von
  courseplan. `ppen_symbol` und `ppen_columns` gehen zurück und liefern nur Kennungen, die der Standard hat und
  die Purple Pen in dieser Spalte anbietet. Sie tauschen die Himmelsrichtungs- und Diagonalformen von 0.1 und
  0.2 und schreiben schlichtes 9.1 und 9.3 als `high` mit Hinweis. Symbole, die eine 2004-Datei nicht kennt
  (2.10, 15.6, 5.23 und 5.24 von 2024), weisen sie ab. `single` setzen sie in Spalte F (`10.1single`).
  "Zwischen" (12.14) behält D und E und bekommt das schlichte `11.15`. Entscheidung 0017.

## [0.5.2] - 2026-11-10

### Hinzugefügt

- freemapper-georef: `TransverseMercator::inverse` rechnet Rechts- und Hochwert in Breite und Länge zurück, für jede UTM-Zone, beide Halbkugeln und jedes Ellipsoid (Krüger-Reihe bis n^6). Hin und zurück weicht es um weniger als 1e-9 Grad (etwa 0,1 mm) ab.

### Geändert

- freemapper-georef: `Crs::inverse` nutzt die neue Rückrechnung statt der Newton-Schleife über `forward`. Die Ergebnisse ändern sich um weniger als 1e-9 Grad; Punkte weit außerhalb der Zone liefern weiter `None`.

## [0.5.1] - 2026-11-10

### Geändert

- freemapper-course: Die Einpassung nutzt die UTM-Umrechnung aus freemapper-georef; das private Modul `tm` entfällt. Die Ergebnisse bleiben gleich.

## [0.5.0] - 2026-11-10

### Hinzugefügt

- freemapper-georef: neue Bibliothek (Entscheidung 0015). `Crs`, `LatLon`, `ProjectedPoint`, `BoundingBox` und
  `TileId` aus freemapper, unverändert in Namen, Verhalten und Tests. Dazu `tm` mit der einzigen UTM-Rechnung
  der Suite: Krüger-Reihe bis n^6 auf jedem Ellipsoid mit Konvergenz und Maßstab, `utm_from_crs` für
  EPSG-Codes und PROJ-Texte. `Crs::utm_zone` und `Crs::transverse_mercator` verbinden beide. freemapper-course
  bleibt unverändert; freemapper-print stellt `fit` auf diese Bibliothek um und löscht dann sein `tm`.

## [0.4.2] - 2026-11-10

### Behoben

- freemapper-course: Purple Pen 4.0 lud eine `.ppen` des Schreibers und stürzte gleich danach ab, weil das
  Ziel kein Beschreibungssymbol hatte. Purple Pen öffnet nach dem Laden die Ansicht aller Posten und schlägt
  dort für jedes Ziel, jede Querungsstelle und jeden Kartenausgabepunkt das Symbol nach. Fehlt es, wirft
  `SymbolDB` eine `ArgumentNullException`. `write_ppen` schreibt jetzt für ein Ziel ohne Symbol `14.3`, für
  eine Querungsstelle `13.3` und für einen Kartenausgabepunkt `13.6`, also die Symbole, die Purple Pen
  einem neuen Posten gibt. Ein Symbol aus dem Ereignis bleibt immer stehen. `write_ppen_patch` behält das
  Symbol der Basisdatei, wenn das Ereignis keines nennt, und ergänzt das fehlende Symbol auch bei Posten, die
  das Ereignis nicht erwähnt. Eine mit 0.4.1 geschriebene Datei öffnet Purple Pen also nach dem nächsten
  Patch wieder. Der Schreiber lehnt außerdem ab, woran Purple Pen nach dem Laden scheitert: ein Zielsymbol
  außer `14.1` bis `14.3`, an einer Querungsstelle ein anderes Symbol als `13.3` und `13.4`, am
  Kartenausgabepunkt ein anderes als `13.6` und ein Kombinationssymbol wie `10.1` in Spalte C oder G. Der
  Test gegen Purple Pens
  Lader prüft jetzt auch die Symbole gegen Purple Pens Symboltabelle, je Spalte und Beschreibungsstandard,
  und ob alle Verweise zwischen Posten, Bahnposten, Teilstrecken, Bahnen und Sonderobjekten auflösen.

## [0.4.1] - 2026-11-10

### Behoben

- freemapper-course: Purple Pen 4.0 lehnte eine `.ppen` des Schreibers ab, sobald ein Kurs Steigung, Länge
  oder Druckmaßstab hatte. Dem `<options>` des Kurses fehlte `description-kind`, und Purple Pen liest dieses
  Attribut ohne Vorgabewert. `write_ppen` schreibt es jetzt in jedes `<options>`, mit `symbols` wie Purple
  Pen für einen neuen Kurs. `write_ppen_patch` behält den Wert der Basisdatei und ergänzt ihn, wo er fehlt,
  auch wenn am Kurs sonst nichts geändert ist. Eine mit 0.4.0 geschriebene Datei lädt Purple Pen also nach dem
  nächsten Patch wieder. Ein neuer Test prüft die Ausgabe des Schreibers gegen die Attribute, Kindelemente
  und Werte, ohne die `EventDB.Load` von Purple Pen eine Datei ablehnt.

## [0.4.0] - 2026-11-10

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
