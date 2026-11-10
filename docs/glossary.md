# Glossar / Glossary

Verbindliche Begriffe für Code und Dokumentation. Neue Fachbegriffe werden zuerst hier ergänzt, dann verwendet.
Code-Bezeichner sind Englisch. Begriffe, die die Glossare von
[freemapper](https://github.com/HenryJobst/freemapper/blob/main/docs/glossary.md),
[freemapper-courseplan](https://github.com/HenryJobst/freemapper-courseplan/blob/main/docs/glossary.md) oder
[freemapper-print](https://github.com/HenryJobst/freemapper-print/blob/main/docs/glossary.md) festlegen, gelten
hier unverändert, mit denselben Code-Bezeichnern. Eine Bibliothek, die einen Begriff aus einer Anwendung
übernimmt, übernimmt auch seine Zeile hierher.

| Begriff | Term | Bedeutung | Code |
|---|---|---|---|
| Kernbibliothek | core crate | eine Bibliothek dieses Arbeitsbereichs, die mehrere Anwendungen nutzen | `crates/freemapper-*` |
| Eigentümer | owner | die Session, die eine Bibliothek eingebracht hat und pflegt (ADR 0005) | – |
| Projektordner | project folder | Ordner eines Projekts; `freemapper-fs` schreibt, kopiert und löscht nur darin (ADR 0009) | `root` |
| Verknüpfung | link | symbolischer Link, unter Windows auch Junction; Schreiben folgt ihr nie, sondern ersetzt sie (ADR 0009) | `is_symlink`, `is_reparse_point` |
| Teildatei | partial file | neue Datei `<Datei>.<Prozess-ID>.<Zähler>.part` neben dem Ziel; erst wenn sie vollständig auf dem Datenträger steht, wird sie über das Ziel umbenannt | `part_name`, `create_part` |
| veraltete Teildatei | stale partial file | Teildatei eines abgebrochenen Laufs, älter als das vom Aufrufer genannte Alter; wird gelöscht | `remove_stale_parts_inside`, `remove_stale_cache_parts` |
| schlichter Dateiname | plain name | ein einzelner Dateiname ohne Pfadtrenner, `..`, Laufwerk oder Steuerzeichen; nur solche Namen aus Dateien oder von Servern kommen in einen Pfad (ADR 0008) | `is_plain_name` |
| Zwischenspeicher | cache | lokaler, projektübergreifender Speicher einer Anwendung; Ordner darin dürfen Verknüpfungen sein | `write_cache_file`, `rename_cache_file` |
| Textkatalog | catalog | die Fluent-Texte einer Anwendung in allen Sprachen, Englisch zuerst; die Bibliothek enthält selbst keine Texte (ADR 0010) | `Catalog` |
| Sprache | language | Sprache der Programmtexte, bestimmt durch ihren Sprachteil (`en`, `de`) | `Lang` |
| Regelprofil | rule profile | TOML-Datei mit Prüf- oder Druckregeln aus einem Regelwerk, jede Regel mit Quelle; ein Basisprofil enthält einen vollständigen Satz, ein Aufsatz nennt sein Basisprofil und ergänzt, ersetzt oder schaltet Regeln ab; die Anwendung bettet ihre Profile ein (ADR 0011) | `RuleProfile`, `ProfileKind` |
| Regelsatz | rule set | ein Basisprofil mit seinen Aufsätzen, zusammengeführt; er merkt sich, aus welchem Profil jede Regel kommt und welches Profil eine Regel abgeschaltet hat | `RuleSet`, `load_rule_set`, `SwitchedOff` |
| Schema | schema | was die Regelprofile einer Anwendung enthalten dürfen: Prüfregel-Schlüssel mit ihren Wertnamen, Einheiten, Tabellenwerte, Filter und eigene Prüfungen; meist ein `static` der Anwendung (ADR 0011) | `Schema`, `KeySchema`, `TableSchema` |
| Filter | filter | Liste in einer Regel, die sie auf bestimmte Fälle beschränkt, etwa `map_kinds` oder `print_scales`; leer oder fehlend heißt, sie gilt für alle; das Schema legt fest, welche Filter es gibt und welche Werte sie enthalten dürfen | `FilterSchema`, `FilterKind`, `Rule::applies` |
| OOM | OOM | OpenOrienteering Mapper, das Kartenprogramm, dessen Dateiformat `freemapper-omap` liest (`.omap`, `.xmap`, XML, Version 9) | – |
| Kartendokument | map document | eine eingelesene OOM-Karte: Kartennotizen, Georeferenzierung, Symbolsatz, Kartenteile und Kartenvorlagen (ADR 0012) | `MapDocument`, `read_map`, `read_map_file` |
| Kartenkoordinate | map coordinate | Lage auf dem Papier, in OOM 1/1000 mm, y nach Süden | `MapCoord` |
| Kartenobjekt | map object | einzelnes gezeichnetes Element mit Symbol und Geometrie | `MapObject`, `MapObjectKind` |
| Kartenteil | map part | Gruppe von Kartenobjekten in OOM (`<part>`), etwa „peripheral information“ | `MapPart` |
| Kartenvorlage | template | Hintergrunddatei (Raster oder Vektor) in OOM, die nicht Teil der Karte ist und zum Abzeichnen dient; das Kartendokument hält nur den Verweis darauf | `TemplateRef`, `TemplateKind` |
| Kartennotizen | map notes | der Text im Element `<notes>` der `.omap`; freemapper schreibt dort die Datenquellen hinein, eine je Zeile | `MapDocument::notes` |
| Georeferenzierung | georeferencing | Verknüpfung von Kartenkoordinaten mit realen Koordinaten, so wie die Datei sie angibt: Maßstab, Deklination, Grivation, Bezugspunkt, Koordinatensystem | `MapGeoref` |
| Symbolsatz | symbol set | Sammlung aller Symbole einer Norm in einem Maßstab, etwa „ISSprOM 2019“ in OOM; ISSprOM 2019 und der IOF-Schulsymbolsatz 2019 sind eingebettet | `SymbolSet`, `SymbolSet::issprom_2019`, `SymbolSet::school_2019` |
| Symbolnummer | symbol code | Nummer des Symbols in der Norm, etwa „521“ (Gebäude) oder „501.5“ | `SymbolCode` |
| Symbolvergrößerung | symbol enlargement | Faktor, um den alle Symbolmaße wachsen (Schulkarte: 1,25), nach denselben Regeln wie OOMs `Symbol::scale` | `enlarge_symbols`, `SCHOOL_SYMBOL_ENLARGEMENT` |
| verstecktes Symbol | hidden symbol | Symbol, das die Kartendatei mit `is_hidden` ausblendet; seine Objekte werden nicht gedruckt | `SymbolSet::is_hidden` |
| Randsymbole | sheet symbols | die freemapper-Symbole 991–994 für die Randangaben (Titel, Kartentext, kleiner Text, Maßstabsleiste), ohne Symbolvergrößerung angehängt | `SymbolSet::with_sheet_symbols` |
| Objekte unbekannter Art | unknown-type objects | Objekte, deren Objektart (`type`) der Leser nicht kennt (nicht Punkt, Linie/Fläche oder Text); sie werden samt Inhalt übersprungen und gezählt | `MapDocument::unknown_objects` |
| Textfeld | box text | Kartentext in einem Rechteck: Der Anker ist die Mitte des Rechtecks, die Zeilen brechen an seiner Breite um und richten sich nach seinen Kanten aus, wie in OOM | `MapObjectKind::Text::box_size` |
| Verschachtelungsgrenze | nesting limit | tiefste Verschachtelung von XML-Elementen, die die Leser annehmen (256); eine tiefere Datei ist kaputt oder feindlich und ergibt einen Fehler statt eines Stapelüberlaufs | `MAX_XML_DEPTH`, `too_deep` |
| Bahn | course | Folge von Start, Posten und Ziel für eine Gruppe von Läufern | `Course` |
| Bahnlegungsprogramm | course-setting program | Purple Pen, Condes oder OCAD-Bahnlegung | – |
| Bahndatei | course file | Datei einer Bahnlegung: IOF-XML 3.0 `CourseData` oder `.ppen`; ein Leser macht daraus ein `RawEvent` (ADR 0013) | `CourseEvent`, `read_iof_xml`, `read_ppen` |
| Purple-Pen-Projekt | Purple Pen project | native XML-Datei von Purple Pen (`.ppen`) mit Posten, Bahnen, Strecken, Kreislücken je Maßstab, Postenbeschreibungen und Sonderobjekten; Posten ohne eigenen Code bekommen die Codes aus Purple Pens IOF-XML-Export (`STA1`, `FIN1`, `XCHG1`, `CROSS1`, `ISSUE1`) | `read_ppen` |
| Posten | control | Punkt, den der Läufer anlaufen muss; hat einen Code | `Control` |
| stabile ID | stable id | die ID, unter der eine `.ppen` einen Posten oder eine Bahn führt (`control/@id`, `course/@id`); ein Schreiber findet damit dieselben Objekte wieder; IOF-XML hat keine (ADR 0013) | `Control::id`, `Course::id` |
| Basisdatei | base file | die vorhandene `.ppen`, in die ein Patch eine Veranstaltung schreibt; was die Veranstaltung nicht erwähnt, bleibt darin stehen, unveränderte Objekte Byte für Byte | `write_ppen_patch`, `PatchOptions`, `Deletions` |
| Stopp | stop | ein Besuch eines Postens in einer Bahn; derselbe Posten kann mehrfach vorkommen; trägt Nummernlage oder Nummernversatz, Kreislücken und, falls die Bahn sie ändert, die Postenart | `Stop` |
| Strecke | leg | Verbindung zwischen zwei aufeinanderfolgenden Posten | `Leg` |
| Knickpunkt | bend | Punkt, an dem eine Strecke abknickt, etwa um ein Sperrgebiet herum | `Leg::bends` |
| Streckenlücke | leg gap | Abschnitt einer Strecke, der nicht gezeichnet wird; Abstand vom Mittelpunkt des ersten Postens entlang der Strecke in mm der Bahndatei | `Leg::gaps` |
| Kreislücke | circle gap | Winkelbereich, in dem ein Postenkreis nicht gezeichnet wird; Grad gegen den Uhrzeigersinn ab Osten in der Achsrichtung der Bahndatei; Purple Pen speichert sie je Posten und Maßstab | `Stop::circle_gaps` |
| Teilabflaggung | partial flagging | Purple Pen flaggt eine Strecke nur ab einem ihrer Knickpunkte oder bis zu einem (`flagging kind="beginning-part"`, `"end-part"`, `"map-issue-middle"` mit dem Punkt); der Leser liest das nicht als Pflichtstrecke | `WriteNote::PartialFlaggingDropped` |
| Pflichtstrecke | marked route | abgeflaggter Weg, dem der Läufer folgen muss, etwa vom letzten Posten zum Ziel | `Leg::marked_route`, `Special::MarkedRoute` |
| Sonderobjekt | special object | Element der Bahnlegung außer Posten und Strecken: Sperrgebiet, Gefahrenstelle, verbotene Strecke, Grenze, Erste Hilfe, Wasser, Text, Linie, Abdeckung | `Special` |
| Postenbeschreibung | control descriptions | IOF-Symbole der Spalten C bis H eines Postens, wie die `.ppen` sie speichert, mit dem Beschreibungsstandard (2004, 2018, 2024); IOF-XML trägt keine | `Description` |
| Gabelung | forking | Varianten einer Bahn, etwa in Staffeln; in IOF-XML eine eigene Bahn je Variante mit derselben Bahnfamilie; eine `.ppen`-Bahn mit Gabeln oder Schleifen wird ausgelassen und gemeldet | `Course::family`, `CourseNote::Forked`, `CourseNote::Variations` |
| Bahnhinweis | course note | Meldung, dass ein Leser etwas gefunden, aber nicht übernommen hat (unbekanntes Element, Gabelung, fehlende Beschreibungen) | `CourseNote` |
| Lesegrenzen | reader limits | was die Bahnleser höchstens annehmen, etwa 50 MiB je Datei, 10 000 Posten, 1000 Bahnen und 64 Ebenen Verschachtelung; darüber ist die Datei ein Fehler (freemapper-print ADR 0019) | `MAX_COURSE_FILE_BYTES`, `MAX_CONTROLS`, `model::MAX_XML_DEPTH`, `CourseError::Limit` |
| Bahneinpassung | course fitting | Umrechnung der Punkte einer Bahndatei in Kartenkoordinaten (µm, y nach unten); prüft Maßstab und Lage, bei einer UTM-Karte auch Breite und Länge der Posten (freemapper-print ADR 0032); hängt am Cargo-Feature `fit` (ADR 0013) | `fit`, `Fit`, `FitNote`, `FitError`, `FitOptions` |
| y nach oben | y-up | Achsrichtung der Kartenpositionen, wie Purple Pen, Condes und OCAD sie schreiben; der IOF-Standard und OOM zählen y nach unten | `fit::YAxis` |
| Nummernlage | number placement | Richtung und Abstand der Postennummer vom Kreis: Richtung in Grad gegen den Uhrzeigersinn ab Osten, Abstand als kürzeste Entfernung vom Außenrand der Kreislinie zum Kasten der Nummer, der mittig auf dem Strahl sitzt (freemapper-print ADR 0038) | `NumberPlacement`, `placement::Placement`, `placement::number_box` |
| Nummernversatz | number offset | wohin eine Bahndatei die Mitte der Postennummer setzt, relativ zur Kreismitte in mm der Bahndatei (`.ppen` `number-location`, IOF-XML `MapTextPosition`); wird mit dem Bahnobjektfaktor in eine Nummernlage umgerechnet und zurück (ADR 0013) | `Offset`, `Stop::number_text_at`, `placement_from_offset`, `offset_from_placement` |
| Bahnobjektfaktor | course object ratio | Kartenmillimeter, die ein Millimeter Nenngröße eines Bahnsymbols in einer `.ppen` einnimmt (Purple Pens `CourseObjRatio`); ohne Angabe 1 | `Course::course_obj_ratio` |
| Symbolgrößenskalierung | item scaling | Einstellung einer `.ppen`, wie Purple Pen die Größe der Bahnsymbole skaliert (`course-appearance/@scale-sizes`): keine, relativ zur Karte oder relativ zu 1:15000; bei IOF-XML ohne Angabe | `ItemScaling`, `CourseSource::item_scaling` |
| Nummernabstand | number gap | Spanne, in der der Abstand einer Postennummer liegen soll; ohne Regel 0,5 mm, die Grenzen auf 0,01 mm nach innen gerundet und auf 0,01 bis 10 mm gehalten | `placement::GapRange` |
| automatische Platzierung | automatic placement | Wahl der Nummernlage durch das Platzierungsmodul: bevorzugt „hinter dem Posten“, in Verlängerung der hereinführenden Strecke; liegt dort die hinausführende Strecke, die äußere Winkelhalbierende | `placement::best` |
| äußere Winkelhalbierende | outer bisector | Halbierende des größeren Winkels zwischen hereinführender und hinausführender Strecke an einem Posten | `placement::outer_bisector` |
| Platzierungsmodul | placement module | Modul für Kasten, Bewertung und Suche der Postennummern, das nichts sonst aus der Bibliothek nutzt; freemapper-print und freemapper-courseplan platzieren damit nach denselben Regeln | `placement` |
| Hindernis | obstacle | was eine Postennummer nicht verdecken soll, mit seiner Stufe: Bahnaufdruck, Punkt- und Liniensymbole der Karte, weiche Hindernisse je mm² gewichtet | `placement::Obstacle`, `placement::Tier` |
