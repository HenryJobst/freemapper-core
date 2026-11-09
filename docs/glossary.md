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
