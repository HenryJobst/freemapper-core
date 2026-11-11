# Druckregeln / Print rules

[Deutsch](#deutsch) | [English](#english)

## Deutsch

Ein Regelprofil ist eine TOML-Datei in diesem Ordner. Sie enthält die Werte, nach denen freemapper-print
druckt: die Tabelle der Symbolgrößen je Maßstab, den Abstand der Nordlinien, die Größen der Bahnsymbole, das
Violett, die Kästchengröße der Postenbeschreibung und die Gewichte für die Lage der Postennummern. Jeder
Wert nennt seine Quelle. Das Programm bettet die Dateien beim Bauen ein; ein neues oder geändertes Profil
braucht einen neuen Build. ADR 0031 legt das fest, `src/rules.rs` liest die Dateien.

Das Format ist das von freemapper-courseplan (dort ADR 0025), mit den Unterschieden aus ADR 0031. Gelesen
werden die Dateien mit der gemeinsamen Bibliothek `freemapper-rules` aus freemapper-core (dort ADR 0011).
`src/rules.rs` beschreibt in `SCHEMA`, was ein Druckprofil enthalten darf.

Es gibt zwei Arten von Regelprofilen:

- Ein **Basisprofil** enthält alle Werte. Zurzeit gibt es genau eines: `iof-print.toml` mit den Werten aus
  ISSprOM 2019-2, ISOM 2017-2 und den übrigen IOF-Dokumenten.
- Ein **Aufsatz** ergänzt ein Basisprofil. Er fügt Regeln hinzu, ersetzt Regeln des Basisprofils oder
  schaltet sie ab. M3 bettet noch keinen Aufsatz ein; die Tests prüfen das Verfahren mit eigenen Aufsätzen.

Die Projektdatei nennt das Basisprofil in `[print] rule_profile` (Vorgabe `iof-print`) und die Aufsätze in
`rule_overlays` (Vorgabe: keine). Die Aufsätze gelten in der angegebenen Reihenfolge. Eigene Profildateien
neben der Projektdatei kann das Programm noch nicht lesen.

Was ein Aufsatz zurzeit ändern kann: die Größen der Bahnsymbole, die Kästchengröße der Postenbeschreibung,
die Platzierungsgewichte, das Violett und den Abstand der Nordlinien auf den Blättern. Was vorerst nur aus
`iof-print` kommt: die Tabelle der Symbolgrößen je Maßstab (erlaubte Maßstäbe, Symbolfaktor), der
Nordlinienabstand beim Befehl `render` ohne Projekt und die Äquidistanz, die `init` vorschlägt. Diese Werte
liest `scale.rs`, und das nimmt immer das Basisprofil.

### Ein Regelprofil hinzufügen

1. `iof-print.toml` kopieren und umbenennen. Der Dateiname ohne `.toml` ist die Kennung des Profils, etwa
   `dosv-print`. Erlaubt sind Kleinbuchstaben, Ziffern und Bindestriche.
2. `[profile]`, `[sources]` und die Regeln ausfüllen, wie unten beschrieben.
3. Die Datei in `EMBEDDED` in `src/rules.rs` eintragen.
4. `cargo test -p freemapper-print rules` ausführen. Die Tests lesen jede eingebettete Datei und lehnen
   unbekannte Schlüssel, Werte ohne Einheit, Regeln ohne Quelle und Tippfehler in Feldnamen ab.

### Aufbau der Datei

```toml
format = 1

[profile]
id = "iof-print"           # der Dateiname ohne .toml
kind = "base"              # "base" (Basisprofil) oder "overlay" (Aufsatz)
# base = "iof-print"       # nur bei Aufsätzen: das Basisprofil, das sie ergänzen
name = { de = "IOF-Druckregeln", en = "IOF print rules" }
valid_from = 2026-03-01    # optional

[sources.ISSprOM]
title = "ISSprOM 2019-2, International Specification for Sprint Orienteering Maps"
url = "https://..."        # optional

[[rules]]
id = "iofprint.course.control_sprint"
key = "course_symbol"
severity = "warning"
enabled = true                       # optional, Vorgabe true
map_kinds = ["sprint"]               # optionaler Filter
print_scales = []                    # optionaler Filter
values = { circle_diameter_mm = 6.0 }
source = { doc = "ISSprOM", section = "4.7, symbol 703 Control point" }
note = { de = "...", en = "..." }    # optional
```

Eine Datei ohne `format = 1` oder mit einem höheren Format lehnt das Programm ab, ebenso jeden Feldnamen,
den das Format nicht kennt. Ein vertipptes `map_kind` ist also ein Fehler und heißt nicht still „alle
Kartenarten“. Den Filter `disciplines` von freemapper-courseplan gibt es hier nicht.

### Felder einer Regel

`id` ist stabil, mit Punkten gegliedert und kleingeschrieben: `<profil>.<familie>.<name>`, die
Profilkennung ohne Bindestriche (`iofprint.scale.sprint`). Ein Aufsatz ersetzt oder schaltet eine Regel über
ihre `id` ab.

`key` sagt, was die Regel festlegt. Das Programm kennt diese Schlüssel und Werte:

| Schlüssel | Werte | Was sie festlegen |
|---|---|---|
| `scale_table` | `rows` | Maßstäbe einer Kartenart und die Symbolgröße relativ zum Basismaßstab (ADR 0018) |
| `north_lines` | `spacing_metres` | Abstand der Nordlinien im Gelände (ADR 0027) |
| `contour_interval` | `proposed_metres` | Äquidistanz, die `init` vorschlägt; ohne Regel kein Vorschlag |
| `course_symbol` | `start_side_mm`, `circle_diameter_mm`, `number_height_mm`, `line_width_mm`, `finish_inner_mm`, `finish_outer_mm`, `marked_route_dash_mm`, `marked_route_gap_mm`, `out_of_bounds_line_mm`, `crossing_point_mm`, `out_of_bounds_hatch_angle_degrees`, `out_of_bounds_hatch_second_angle_degrees` (optional), `out_of_bounds_hatch_spacing_mm`, `out_of_bounds_hatch_line_mm`, `first_aid_mm`, `first_aid_bar_mm`, `refreshment_mm`, `refreshment_line_mm` | Größen der Bahnsymbole 701 bis 713; Kreisdurchmesser von Linienmitte zu Linienmitte; die Schraffur des Sperrgebiets (709) mit Richtung gegen den Uhrzeigersinn ab Osten, zweiter Richtung für eine Kreuzschraffur, Abstand von Linienmitte zu Linienmitte und Linienbreite; Erste Hilfe (Balkenlänge und -breite) und Getränkeposten (Außenmaß und Linienbreite) |
| `description_box` | `cell_mm` | Kästchengröße der Postenbeschreibung auf der Karte |
| `course_colour` | `cyan_percent`, `magenta_percent`, `yellow_percent`, `black_percent` | Farbe des Bahnaufdrucks |
| `placement_weights` | `black_factor`, `brown_factor`, `blue_factor`, `other_factor` | wie stark Kartendetails je Farbe unter einer Postennummer zählen |
| `number_gap` | `min_gap_mm`, `max_gap_mm` | kleinster und größter Abstand zwischen Postenkreis und Nummer (ADR 0038) |

Eine Regel mit einem anderen Schlüssel oder einem Wert, den ihr Schlüssel nicht liest, lehnt das Programm
ab. Ein neuer Schlüssel braucht Code und einen Eintrag in `SCHEMA` in `src/rules.rs`.

Die Bahnsymbole verteilen sich auf mehrere Regeln, eine je Symbol und Norm. Jede nennt so ihren eigenen
Abschnitt.

`severity` ist `error`, `warning` oder `hint` und sagt, wie ernst eine Abweichung ist. M3 liest den Wert
noch nicht; er steht da, weil das Format von freemapper-courseplan ihn verlangt.

`note` bitte deutsch und englisch schreiben; der englische Text ist Pflicht.

### Filter und Basismaßstab

`map_kinds` (`sprint`, `schoolyard`, `forest`) und `print_scales` (Maßstabszahlen wie `4000`) schränken
ein, wo eine Regel gilt. Ein leerer oder fehlender Filter heißt: überall.

Jeder Wert gilt im Basismaßstab seiner Kartenart: Sprintkarte 1:4000, Waldkarte 1:15000, Schulhofkarte
1:1000. Das Programm vergrößert die Bahnsymbole mit dem Größenfaktor des Druckmaßstabs aus der Tabelle der
Symbolgrößen, dem Bahnfaktor. Ein Filter `print_scales` wird deshalb im Basismaßstab gelesen.

Hat ein Wert für verschiedene Kartenarten verschiedene Größen, steht er in einer Regel je Kartenart, jede mit
eigener `id` und eigenem Filter. Beim Laden prüft das Programm, dass jede Kartenart jeden Wert genau einmal
bekommt. Fehlt einer, oder geben zwei Regeln denselben Wert, ist der Regelsatz unbrauchbar und das Programm
sagt, welcher Wert und welche Regeln es sind. `course_colour`, `placement_weights` und `number_gap` gelten für
alle Kartenarten und nehmen keinen Filter. Bei `number_gap` dürfen mehrere Regeln gelten: Dann gilt der größte
Mindestabstand und der kleinste Höchstabstand. Ohne Regel setzt das Programm Nummern mit 0,5 mm Abstand.

### Werte und Einheiten

Eine Zahl trägt ihre Einheit im Namen: Der Name ist eine Einheit oder endet auf `_<einheit>`. Die Einheiten
sind `metres`, `mm`, `degrees`, `percent` und `factor`. Werte in `mm` und `metres` sind größer als 0,
`factor` ist 0 oder mehr, `percent` liegt zwischen 0 und 100. Jeder Wert außer `rows` ist eine einzelne
Zahl. Eine Liste wie `spacing_metres = [100, 120]` lehnt das Programm ab.

Die einzige Tabelle ist `rows` von `scale_table`, beschrieben in `SCHEMA` in `src/rules.rs`:

```toml
values = { rows = [
    { scale = 3000, size_factor = 1.3333333333333333 },
    { scale = 4000, size_factor = 1.0 },
    { scale = 5000, size_factor = 1.0, standard = false },
] }
```

`scale` ist die Maßstabszahl, `size_factor` die Symbolgröße relativ zum Basismaßstab. `standard = false`
markiert eine Verkleinerung, die die Norm nicht vorsieht. Die Zeilen stehen aufsteigend nach Maßstab. Jede
Tabelle braucht eine Zeile für den Basismaßstab ihrer Kartenart mit `size_factor = 1.0`, ohne
`standard = false`.

### Quellen

Jede Regel nennt ein Dokument aus der `[sources]`-Tabelle ihrer Datei: `source = { doc, section }`.
`iof-print.toml` nutzt ISSprOM, ISOM, IOF-SCHOOL, IOF-PCD (Printing and Colour Definitions), IOF-CD
(Control Descriptions) und `fmp`. `fmp` steht für eine Entscheidung von freemapper-print, wo keine Norm eine
Zahl nennt. Eine solche Regel braucht eine `note`, sonst lehnt das Programm die Datei ab, und sie nennt als
Abschnitt das ADR. Auch die Annahme, dass Schulhofkarten die Sprintwerte nehmen, steht unter `fmp`.

Die Größen der Bahnsymbole stammen aus dem Text von ISSprOM 2019-2 §4.7 und ISOM 2017-2 §3.7. Purple Pen
nutzt dieselben Werte (`NormalCourseAppearance`). Für Schulhofkarten nimmt das Profil die Sprintwerte an;
die Notiz der Regel sagt das.

### Aufsätze

Ein Aufsatz nennt sein Basisprofil in `[profile] base`. Eine Projektdatei kann nur Aufsätze für ihr eigenes
Basisprofil nutzen. Jede Regel eines Aufsatzes bewirkt eines von drei Dingen:

- Eine Regel mit neuer `id` kommt nach den Regeln des Basisprofils dazu.
- Eine Regel mit der `id` einer Basisregel ersetzt diese an ihrer Stelle.
- Eine Regel mit der `id` einer Basisregel und `enabled = false` schaltet die Basisregel ab. Der Regelsatz
  merkt sich, welches Profil sie abgeschaltet hat. Eine `id` abzuschalten, die das Basisprofil nicht hat, ist
  ein Fehler. Wer eine Regel abschaltet, muss ihren Wert mit einer neuen Regel geben, sonst fehlt er.

Regeln eines Aufsatzes verweisen auf die `[sources]` des Aufsatzes.

## English

A rule profile is a TOML file in this folder. It holds the values freemapper-print prints by: the symbol size
table per scale, the north-line spacing, the course symbol sizes, the purple, the control description cell
size and the weights for placing control numbers. Every value names its source. The program embeds the files
at build time, so a new or changed profile needs a new build. ADR 0031 sets this, and `src/rules.rs` reads the
files.

The format is freemapper-courseplan's (its ADR 0025), with the differences listed in ADR 0031. The shared
library `freemapper-rules` from freemapper-core (its ADR 0011) reads the files. `SCHEMA` in `src/rules.rs`
says what a print profile may contain.

There are two kinds of rule profile:

- A **base profile** holds every value. There is one so far: `iof-print.toml`, with the values from ISSprOM
  2019-2, ISOM 2017-2 and the other IOF documents.
- An **overlay** extends one base profile. It adds rules, replaces base rules or switches them off. M3 embeds no
  overlay yet; the tests check the mechanism with overlays of their own.

The project file names the base profile in `[print] rule_profile` (default `iof-print`) and the overlays in
`rule_overlays` (default none). Overlays apply in the order given. The program cannot read profile files next
to the project file yet.

What an overlay can change for now: the course symbol sizes, the control description cell size, the placement
weights, the purple and the north-line spacing on sheets. What comes from `iof-print` only for now: the symbol
size table per scale (allowed scales, symbol factor), the north-line spacing of `render` without a project, and
the contour interval `init` proposes. `scale.rs` reads these values, and it always takes the base profile.

### Adding a rule profile

1. Copy `iof-print.toml` and rename it. The file name without `.toml` is the profile id, for example
   `dosv-print`. Use lower-case letters, digits and hyphens.
2. Fill in `[profile]`, `[sources]` and the rules as described below.
3. Add the file to `EMBEDDED` in `src/rules.rs`.
4. Run `cargo test -p freemapper-print rules`. The tests read every embedded file and reject unknown keys,
   values without a unit, rules without a source and misspelt field names.

### File layout

The example in the German section above applies unchanged. A file without `format = 1`, or with a higher
format, is rejected, and so is every field name the format does not know. A misspelt `map_kind` is an error
and does not silently mean "all map kinds". freemapper-courseplan's `disciplines` filter does not exist here.

### Fields of a rule

`id` is stable, dotted and lower case: `<profile>.<family>.<name>`, with the profile id written without
hyphens (`iofprint.scale.sprint`). An overlay replaces or switches off a rule by its `id`.

`key` says what the rule sets. The program knows these keys and values:

| Key | Values | What they set |
|---|---|---|
| `scale_table` | `rows` | scales of a map kind and the symbol size relative to the base scale (ADR 0018) |
| `north_lines` | `spacing_metres` | ground distance between north lines (ADR 0027) |
| `contour_interval` | `proposed_metres` | contour interval `init` proposes; no rule means no proposal |
| `course_symbol` | `start_side_mm`, `circle_diameter_mm`, `number_height_mm`, `line_width_mm`, `finish_inner_mm`, `finish_outer_mm`, `marked_route_dash_mm`, `marked_route_gap_mm`, `out_of_bounds_line_mm`, `crossing_point_mm`, `out_of_bounds_hatch_angle_degrees`, `out_of_bounds_hatch_second_angle_degrees` (optional), `out_of_bounds_hatch_spacing_mm`, `out_of_bounds_hatch_line_mm`, `first_aid_mm`, `first_aid_bar_mm`, `refreshment_mm`, `refreshment_line_mm` | sizes of course symbols 701 to 713; circle diameters from line centre to line centre; the hatch of an out-of-bounds area (709): direction counter-clockwise from east, a second direction for a cross hatch, spacing from line centre to line centre and line width; first aid (bar length and width) and refreshment point (outside measure and line width) |
| `description_box` | `cell_mm` | cell size of control descriptions on the map |
| `course_colour` | `cyan_percent`, `magenta_percent`, `yellow_percent`, `black_percent` | colour of the course overprint |
| `placement_weights` | `black_factor`, `brown_factor`, `blue_factor`, `other_factor` | how much map detail of each colour under a control number counts |
| `number_gap` | `min_gap_mm`, `max_gap_mm` | smallest and largest gap between control circle and number (ADR 0038) |

A rule with another key, or with a value its key does not read, is rejected. A new key needs code and an entry
in `SCHEMA` in `src/rules.rs`.

The course symbols are split over several rules, one per symbol and standard, so each names its own section.

`severity` is `error`, `warning` or `hint` and says how serious a deviation is. M3 does not read it yet; it is
there because freemapper-courseplan's format requires it.

Please write `note` in German and English; the English text is required.

### Filters and the base scale

`map_kinds` (`sprint`, `schoolyard`, `forest`) and `print_scales` (scale denominators such as `4000`) limit
where a rule applies. An empty or missing filter means everywhere.

Every value holds at the base scale of its map kind: sprint 1:4000, forest 1:15000, schoolyard 1:1000. The
program enlarges course symbols with the size factor of the print scale from the symbol size table, the course
factor, so a `print_scales` filter is read at the base scale.

When a value differs between map kinds, it goes into one rule per map kind, each with its own `id` and filter.
On loading, the program checks that each map kind gets each value exactly once. If a value is missing, or two
rules give the same value, the rule set cannot be used, and the program names the value and the rules.
`course_colour`, `placement_weights` and `number_gap` hold for every map kind and take no filter. Several
`number_gap` rules may apply; then the largest minimum and the smallest maximum count. Without a rule the program
places numbers at a gap of 0.5 mm.

### Values and units

A number carries its unit in its name: the name is a unit or ends in `_<unit>`. The units are `metres`, `mm`,
`degrees`, `percent` and `factor`. Values in `mm` and `metres` are above 0, a `factor` is 0 or more, and a
`percent` lies between 0 and 100. Every value but `rows` is one number. A list such as
`spacing_metres = [100, 120]` is rejected.

The only table is `rows` of `scale_table`, described in `SCHEMA` in `src/rules.rs` and shown in the
German section above. `scale` is the scale denominator and `size_factor` the symbol size relative to the base
scale. `standard = false` marks a reduction the standard does not foresee. Rows are in ascending order of
scale. Every table needs a row for the base scale of its kind with `size_factor = 1.0` and without
`standard = false`.

### Sources

Every rule names a document from its file's `[sources]` table: `source = { doc, section }`. `iof-print.toml`
uses ISSprOM, ISOM, IOF-SCHOOL, IOF-PCD (Printing and Colour Definitions), IOF-CD (Control Descriptions) and
`fmp`. `fmp` stands for a decision of freemapper-print where no standard gives a number. Such a rule needs a
`note`, or the file is rejected, and names the ADR as its section. The assumption that schoolyard maps take
the sprint values is cited under `fmp` too.

The course symbol sizes come from the text of ISSprOM 2019-2 §4.7 and ISOM 2017-2 §3.7. Purple Pen uses the
same values (`NormalCourseAppearance`). For schoolyard maps the profile assumes the sprint values, and the
rule's note says so.

### Overlays

An overlay names its base profile in `[profile] base`. A project file can only use overlays for its own base
profile. Each rule of an overlay does one of three things:

- A rule with a new `id` is added after the base rules.
- A rule with the `id` of a base rule replaces it in place.
- A rule with the `id` of a base rule and `enabled = false` switches the base rule off. The rule set records
  which profile switched it off. Switching off an `id` the base profile does not have is an error. An overlay
  that switches a rule off has to give its value in a new rule, or the value is missing.

Rules of an overlay cite the overlay's own `[sources]`.
