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
