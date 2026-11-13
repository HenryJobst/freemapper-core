# Änderungen

[English version](CHANGELOG.en.md)

Hier stehen alle wichtigen Änderungen an freemapper-core, die neueste zuerst, jeweils mit der betroffenen
Bibliothek. Das Format folgt [Keep a Changelog](https://keepachangelog.com/de/1.1.0/).

## [Unveröffentlicht]

## [0.23.0] - 2026-11-13

### Geändert

- freemapper-contract: Die Kartenart `forest` hat Zeichenmaßstäbe, 1:7500, 1:10 000 und 1:15 000 (ISOM 2017-2), und freemapper schreibt sie damit (`is_written_by_freemapper`). Ein Leser nimmt Waldkarten in diesen Maßstäben jetzt an; vorher meldete `check` sie als unbekannte Art. Für freemapper-print und freemapper-courseplan ändert sich am Code nichts.

## [0.22.0] - 2026-11-12

### Geändert

- freemapper-render: `write_pdf` und `MultiPagePdf::add_page` schreiben Kartenobjekte und Bahnaufdruck nicht mehr ins PDF, wenn sie ganz außerhalb des sichtbaren Rahmens liegen (Entscheidung 0031). Blätter, deren Rahmen nur einen Teil der Karte zeigt, werden dadurch kleiner. Die Beispielblätter von freemapper-print aus einer Sprintkarte von 1,4 MB in 1:4000 schrumpfen um 15 bis 30 %, von 3,6 oder 3,7 MB auf 2,6 bis 3,2 MB; ein Blatt, das nur eine Ecke der Karte zeigt, von 252 KB auf 38 KB. Gerastert gleichen die PDFs den bisherigen Pixel für Pixel (pdftoppm, 6 bis 300 dpi), die Bytes ändern sich. Ob eine Form ins PDF kommt, entscheidet ihr Farbbereich: ihre Punkte mit halber Linienbreite, Gehrungsspitzen, eckigen Linienenden und Glyphenumrissen, dazu 5 mm Spielraum, weil die Kantenglättung am Rahmen auch Formen knapp außerhalb in die Randpixel mischt. `RenderedPage`, `BuildReport` und die Treffer bleiben gleich. Fehlende Glyphen und Zahlen, die ein PDF nicht fassen kann, werden auch für ausgelassene Formen gemeldet. Die SVG-Ausgabe lässt nach derselben Regel aus und behält dadurch am Rand der Ansicht einige Formen mehr. Am Code der Anwendungen ändert sich nichts; wer PDFs mit eingecheckten vergleicht, schreibt sie neu.

## [0.21.1] - 2026-11-11

### Hinzugefügt

- freemapper-web: `suite.css` hat den Farbwert `--on-accent` für Text auf einer mit `--accent` gefüllten Fläche (hell `#ffffff`, dunkel `#1c1c1e`) und die Klasse `button.primary` für die Hauptaktion einer Ansicht (Entscheidung 0030, Nachtrag).

## [0.21.0] - 2026-11-11

### Hinzugefügt

- freemapper-web: die Suite-Oberfläche (Entscheidung 0030). `serve` liefert unter `/{token}/suite/` fünf gemeinsame Dateien aus, hinter denselben Prüfungen und Headern wie die Routen der Anwendung: `suite.css` mit den Farbwerten hell und dunkel (Akzent `#0b6bcb`, dunkel `#6cb0ff`) und den Grundelementen, dazu die Module `api.js`, `texts.js`, `status.js` und `tree.js` aus dem Editor von freemapper-courseplan. Ein anderer Name unter `suite/` ergibt 404. `revision` berechnet die Revision einer Datei (FNV-1a 64, 16 Hexziffern), `REVISION_HEADER` ist `x-freemapper-revision`. `stale` antwortet 409 mit `{ "error", "revision" }`, `refused` 422 mit einem Satz als Text. `texts_language` nimmt für die Seitentexte die festgelegte Sprache, sonst `de` oder `en` aus `Accept-Language` nach q-Wert, sonst Englisch. Mit dem Feature `page-rules` prüft `check_page_rules` die Dateien einer Seite auf eingebettete Skripte, Inline-Handler, `javascript:`-Adressen, Quellen aus dem Netz und die Größe; `suite_assets` liefert die Suite-Dateien für diese Prüfung.

## [0.20.0] - 2026-11-11

### Hinzugefügt

- freemapper-render: `PageSpec::page_mm` setzt eine Seite beliebiger Größe von 10 mm bis 5080 mm je Seite statt A4 oder A3 (Entscheidung 0029). Eine Größe außerhalb dieses Bereichs ist `BuildError::PageSize`; es wird nichts begrenzt. `BuildReport::source_objects` gibt jeder Objektnummer, die ein Treffer trägt, ihre Position in der Karte; sie weicht von der Nummer ab, sobald ein Objekt übersprungen wird.

## [0.19.0] - 2026-11-11

### Geändert

- freemapper-printrules: `scale::MapKind` ist jetzt der Typ aus `freemapper-contract` (Entscheidung 0028), ein Typ für die ganze Suite. `MapKind::allowed_scales()` und `MapKind::proposed_contour_interval_m()` werden zu den Funktionen `scale::allowed_scales(kind)` und `scale::proposed_contour_interval_m(kind)`. Ein Test prüft, dass jeder Maßstab, in dem freemapper zeichnet, in der Tabelle des Profils steht.

## [0.18.0] - 2026-11-11

### Hinzugefügt

- freemapper-contract: gemeinsame Typen für die drei Anwendungen (Entscheidung 0027, Stufe 4 des Vertragstests). `MapKind` ist die eine Definition der Kartenart (`sprint`, `schoolyard`, `forest`) mit `name`, `from_name`, `base_scale` und `drawing_scales`, den Maßstäben, in denen freemapper eine Kartenart schreibt. `Sidecar::parse` und `Sidecar::next_to` lesen die Vertragsschlüssel der `freemapper.toml`; eine unbekannte Kartenart kommt als `SidecarKind::Unknown` mit ihrem Namen zurück und ist kein Fehler. `ObjectClass::of` ordnet die Tags eines Objekts einer Objektklasse zu. Das Modul `tags` enthält die reservierten Wörter: Tag-Schlüssel, Quellen, die Rolle `scale-bar`, die Namen der beiden Kartenteile, den Dateinamen der Begleitdatei und ihre Vertragsschlüssel.

### Geändert

- freemapper-contract: Bestandsaufnahme, Vokabular und Regeln liegen hinter dem Feature `check`, das standardmäßig an ist; `fixtures` schaltet es ein. Mit `default-features = false` enthält die Bibliothek nur die gemeinsamen Typen und braucht `freemapper-omap` nicht. Die Regel `ScaleAllowed` liest die Maßstäbe aus `MapKind::drawing_scales`. Eine Zahl in der Begleitdatei, die nicht in `u32` passt, etwa ein negativer Maßstab, gilt als fehlend.

## [0.17.0] - 2026-11-11

### Hinzugefügt

- freemapper-update: neue Bibliothek für die Selbst-Aktualisierung der Anwendungen aus signierten Veröffentlichungen (Entscheidung 0026). Sie enthält das Manifest, die minisign-Prüfung gegen eine Schlüsselliste, Versionen, die Hostregel `check_url`, Prüfvermerk und Modus, den Tausch der laufenden Programmdatei mit Rückweg und Aufräumen sowie `install` mit Prüfung vor dem Tausch. Eine Installationssperre mit dem Kennzeichen ihres Halters hält zwei Installationen auseinander; wer sie nach 15 Minuten an eine zweite verliert, bricht vor dem Tausch ab. Das Ergebnis einer Prüfung nennt Version, Veröffentlichungsdatum, Link zu den Versionshinweisen und Downloadgröße. `ProcessRunner::new(app.name)` erwartet bei `--version` den Namen der Anwendung, auch wenn die Programmdatei umbenannt wurde. Netzzugriff hat sie nicht: die Anwendungen liefern ihn über `UpdateSource`. Die Testschlüssel unter `tests/keys` nimmt kein Release an.

## [0.16.1] - 2026-11-11

### Geändert

- freemapper-course: Die gemeinsamen Platzierungsfälle (`tests/fixtures/placement/cases.toml`) gehen jetzt von der Nummernbox aus, die der Renderer zeichnet: 0,8083747 × Höhe je Ziffer (die Arimo-Schrittweite, in freemapper-render per Test geprüft) statt 0,6 × Höhe. Die Erwartungswerte von `numbers_avoid_each_other` und `mixed_course_4` ändern sich. `place_course_numbers` selbst bleibt gleich; es bekommt die Boxgröße vom Aufrufer.

## [0.16.0] - 2026-11-11

### Hinzugefügt

- freemapper-printrules: die Druckregeln aus freemapper-print, im Verhalten unverändert (Entscheidung 0025): das Profil `iof-print`, der Regelsatz, die Kartenarten und die Symbolgrößentabelle je Druckmaßstab. `scale::course_symbols(kind, print_scale)` liefert die Kurssymbolgrößen und den Kursfaktor für `freemapper_render::overprint::overprint`. Mit dem Feature `test-support` sind `rules::load_for_tests` und `rules::embedded_text` öffentlich.

## [0.15.0] - 2026-11-11

### Hinzugefügt

- freemapper-render: die Kursüberdruckung aus freemapper-print, im Verhalten unverändert (Entscheidung 0024). `overprint::overprint` zeichnet einen Kurs (Start, Posten, Ziel, Wechselpunkte, Strecken, Sonderobjekte, Postennummern) als unteres und oberes Purpur einer `RenderedPage`, in der Größe, die der Aufrufer vorgibt; `overprint::on_map` ordnet die untere Ebene zwischen den Kartenfarben ein. `CourseSymbolSizes` zieht mit um. Die Bibliothek hängt jetzt von `freemapper-course` ab.

## [0.14.1] - 2026-11-11

### Behoben

- freemapper-render: Bézier-Kurven werden pro Pfad nur noch bis zu einer Obergrenze von einer Million Stützpunkten geglättet. Eine Kurve kann bis zu 65.536 Punkte brauchen, eine manipulierte Karte also die Arbeit eines Seitenaufbaus vervielfachen, bevor das Seitenbudget greift. Ist die Grenze erreicht, werden die übrigen Kurven des Pfads zu je einer Sehne. Normale Karten bleiben unverändert.

## [0.14.0] - 2026-11-11

### Hinzugefügt

- freemapper-course: `description_symbols::ids(standard)` zählt alle Symbol-IDs eines Beschreibungsstandards auf, in der Reihenfolge der Datei und jede einmal. `symbol_kind` nennt dazu die Spalte.

## [0.13.0] - 2026-11-11

### Hinzugefügt

- freemapper-render: der Seitenaufbau aus freemapper-print, unverändert im Verhalten (Entscheidung 0023). `build::build_page` macht aus einer `MapDocument` die `RenderedPage`, die auch das PDF bekommt, und einen `BuildReport` mit den Seitenfeldern des `PrintReport` von freemapper-print unter denselben Namen. `PageSpec` nimmt Symbolfaktor und Geometriefaktor als Zahlen; die Maßstabsregeln (Kartenart, erlaubte Maßstäbe, Faktoren) bleiben in print. Mit dem Seitenaufbau ziehen `Paper`, `Orientation`, `PAGE_MARGIN_MM`, `MAX_UNITS_PER_PAGE`, `frame_box`, die Tags `SHEET_TAG`, `SCALE_BAR_TAG`, `BASE_TAG`, der Kartenteil `BASE_PART` und `is_sheet_object`, `is_base_object`, `is_north_line` um. `build_page_within` nimmt ein anderes Seitenbudget, für Tests. Die Bibliothek hängt jetzt von `serde` ab.

## [0.12.0] - 2026-11-11

### Hinzugefügt

- freemapper-render: SVG-Ausgabe für die Editoren im Browser (Entscheidung 0022). `svg::write_svg` schreibt eine `RenderedPage` als SVG, in derselben Reihenfolge wie das PDF, mit Abdeckungen als Clip-Pfade und dem Bahnaufdruck mit `mix-blend-mode:multiply`. Die viewBox ist in Seitenmillimetern; `SvgView` wählt den Ausschnitt und die Pixel je Millimeter, Formen außerhalb fallen weg. Texte sind Glyphenumrisse aus Arimo, Bilder PNG-Daten-URIs, Farben werden mit r = 255 · (1 − c) · (1 − k) von CMYK nach sRGB umgerechnet. Eine Seite über `svg::MAX_UNITS` ergibt `SvgError::TooLarge`.
- freemapper-render: `RenderedPage::mapping` liefert die Seitenabbildung `PageMapping` zwischen Karte (mm beim Druckmaßstab) und Seite (mm). `RenderedPage::hits_at` nennt die Kartenobjekte unter einem Punkt der Seite, das oberste zuerst, und beachtet Clip-Rechteck, Abdeckungen und Füllregeln (Entscheidung 0022).
- freemapper-contract: Ändert sich die Bibliothek oder freemapper-omap, führt die CI jetzt die Vertragstests (`contract_*`) von freemapper-print und freemapper-courseplan gegen den neuen Stand aus (Entscheidung 0022). Ein roter Lauf heißt, dass die Änderung einen Leser bricht; dann lernt erst der Leser die neue Form, danach wird core veröffentlicht.

## [0.11.0] - 2026-11-11

### Hinzugefügt

- freemapper-web: neue Bibliothek mit dem Rahmen für lokale Webseiten (Entscheidung 0020), verallgemeinert aus der Gebietsauswahl von freemapper. `serve` bindet 127.0.0.1 (Port 0 wählt einen freien), setzt ein zufälliges 128-Bit-Token vor die Routen der Anwendung und schützt sie: bedient werden nur die Hosts `127.0.0.1:PORT` und `localhost:PORT`, ein gesendeter `Origin` muss die Seite selbst sein, und jede Antwort trägt `Cache-Control: no-store` (außer der Handler setzt eigenen), die Content-Security-Policy der Anwendung, `X-Frame-Options: DENY`, `Referrer-Policy: no-referrer` und `X-Content-Type-Options: nosniff`. Mit `ServeOptions::strict_post` braucht eine Anfrage, die etwas ändert, zusätzlich einen `Origin` und den Inhaltstyp `application/json` oder `application/octet-stream`. `embedded` liefert rust-embed-Dateien, `open_browser` startet den Browser, `LocalServer::shutdown` und `finished` beenden den Server und warten darauf.

## [0.10.0] - 2026-11-10

### Geändert

- freemapper-contract: `Expected::title` ist jetzt `Option<String>`, weil Karten von freemapper 0.9.0 keinen Titel in der Begleitdatei haben. Lesertests vergleichen mit `expected.title.as_deref()`. `Expected::map_kind` kann `unknown` sein; ein Lesertest, der die Kartenart nachschlägt, muss diesen Fall behandeln. Neu ist `Expected::breaks` mit den Regeln, gegen die ein Fixture absichtlich verstößt.

### Hinzugefügt

- freemapper-contract: zwei Fixtures für die Leser (Entscheidung 0018, Nachtrag). `legacy-0.9.0-scale-bar` ist eine Karte von freemapper 0.9.0 mit dem alten Tag an der Maßstabsleiste und ohne Titel. `future-kind` hat eine Kartenart, einen Schlüssel und eine Rolle, die kein Leser kennt. `fixtures/README.md` nennt die Herkunft jedes Fixtures.

## [0.9.0] - 2026-11-10

### Hinzugefügt

- freemapper-render: neue Bibliothek mit der Darstellung aus freemapper-print, unverändert im Verhalten übernommen (Entscheidung 0019). Sie zeichnet OOM-Symbole in das Seitenmodell (`page`), schreibt PDF (`pdf`) und bringt die Schrift Arimo mit ihren Textmaßen mit (`fonts`). Einzige Abhängigkeit aus der Suite ist freemapper-omap. Das Feature `test-support` macht `pdf::content_of`, `draw::work_units` und `draw::BUDGET_SLACK` für die Tests der Anwendungen zugänglich. Der Seitenbau (`print.rs`) bleibt vorerst in freemapper-print.

## [0.8.0] - 2026-11-10

### Hinzugefügt

- freemapper-contract: neue Bibliothek mit dem Vertrag zwischen freemapper und seinen Lesern freemapper-print und freemapper-courseplan (Entscheidung 0018). `census` zählt die Objekte einer Karte je Objektklasse (Randobjekte, Grundfläche, Nordlinien, Karteninhalt) und liefert die Rahmen von Karteninhalt und Randobjekten. `Vocabulary::of` sammelt die Wörter von Karte und Begleitdatei, `check` prüft zwölf Regeln für die Ausgabe von freemapper. Karte und Begleitdatei gehen als Text hinein, kein Typ von freemapper-omap steht in der Schnittstelle.
- freemapper-contract: Das Feature `fixtures` bettet zwei Karten ein, die freemapper 0.9.2 geschrieben hat, `schoolyard-1000` und `sprint-4000`. Jede bringt `expected.toml` und `vocabulary.toml` mit. Eine Anwendung trägt die Bibliothek unter `[dev-dependencies]` mit diesem Feature ein. `Fixture::write_into` legt Karte und Begleitdatei in einen Ordner, `Fixture::expected` nennt, was ein Leser daraus ableiten muss.

## [0.7.0] - 2026-11-10

### Hinzugefügt

- freemapper-course: `MapGrid` (Feature `fit`) rechnet zwischen Kartenposition (mm, x rechts, y unten) und Breite/Länge um: `MapGrid::of(&MapGeoref)`, `latlon_of`, `map_mm_of`. Die Einpassung nutzt dieselbe Umrechnung.

## [0.6.1] - 2026-11-10

### Behoben

- freemapper-course: `placement::candidate` wickelt Richtungen über 360° vor dem Runden um und nicht danach. Neu platzierte Richtungen tragen kein Rundungsrauschen mehr (102,4 statt 102,39999999999998).

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
