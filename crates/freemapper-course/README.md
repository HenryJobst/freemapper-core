# freemapper-course

Bahnen des Orientierungslaufs: das Bahnmodell, Leser für Purple Pen (`.ppen`) und IOF-XML 3.0, die
Einpassung einer Bahndatei in eine OOM-Karte, die Platzierung der Postennummern und die Symbole der
Postenbeschreibung. Eigentümer ist freemapper-print (Entscheidung 0005).

Der Code kommt aus freemapper-print, Commit `918cec2` (Module `course/` und `placement/`, dort die
Entscheidungen 0032, 0033, 0038 und 0039; hier Entscheidung 0013). Die Pfade sind dieselben wie dort:
`freemapper_course::model::Control`, `freemapper_course::read_ppen`, `freemapper_course::fit::fit`,
`freemapper_course::placement::best`.

Neu gegenüber freemapper-print:

- `Control::id` und `Course::id` tragen die IDs aus der `.ppen`, damit ein Schreiber dieselben Posten und
  Bahnen wiederfindet. IOF-XML hat keine, dort bleiben sie `None`.
- `placement_from_offset` und `offset_from_placement` rechnen den Nummernversatz einer Bahndatei in eine
  Nummernlage um und zurück. Bezugspunkt, Achsen und Einheiten stehen im Modul `number_offset`. Die
  `.ppen` speichert `number-location` mit y nach oben, ein Schreiber kehrt also das Vorzeichen von `dy_mm` um.

Von der Suite braucht die Bibliothek nur `freemapper-omap` und `freemapper-georef`, und die nur für die Einpassung. Sie hängt am
Cargo-Feature `fit`, das standardmäßig an ist. Wer nur Modell, Leser und Nummernplatzierung braucht, schreibt
`default-features = false` und baut ohne `freemapper-omap` und `freemapper-georef`. Die Leser lehnen Dateien jenseits der Grenzen in
`model` ab, etwa über 50 MiB, mit mehr als 10 000 Posten oder tiefer als 64 Ebenen verschachtelt
(freemapper-print, Entscheidung 0019). Fehlertexte sind Englisch, die Anwendung übersetzt sie.

`placement::place_course_numbers` setzt alle Nummern einer Bahn in Laufreihenfolge. Es übernimmt die Suche
`best` von freemapper-print und `auto_place` von freemapper-courseplan; gehaltene Nummern bleiben stehen, und
jede gesetzte Nummer ist Hindernis für die nächsten (Entscheidung 0016).

Das Modul `description_symbols` enthält die Symbole der IOF-Postenbeschreibung (Zeichnung, deutscher und
englischer Text) aus Purple Pens `symbols.xml`. Es bildet auch zwischen Purple-Pen-Kennungen und IOF-2024-Nummern
ab, in beide Richtungen (Entscheidung 0017). Die Datei liegt unverändert in [`data/`](data/README.md). Purple
Pen steht unter der BSD-Lizenz, Copyright (c) 2006-2008, Peter Golde; der volle Lizenztext steht dort. Er muss
jeder Weitergabe der Bibliothek, auch als Binärdatei, beiliegen.

Die Testdateien liegen in [`tests/fixtures/courses/`](tests/fixtures/courses/README.md), dort stehen
Quelle und Lizenz.

---

Orienteering courses: the course model, readers for Purple Pen (`.ppen`) and IOF XML 3.0, the fit of a course
file to an OOM map, the placement of control numbers and the control description symbols. The owner is
freemapper-print (decision 0005).

The code comes from freemapper-print at commit `918cec2` (modules `course/` and `placement/`, decisions 0032,
0033, 0038 and 0039 there; decision 0013 here). The paths are the same as there:
`freemapper_course::model::Control`, `freemapper_course::read_ppen`, `freemapper_course::fit::fit`,
`freemapper_course::placement::best`.

New compared with freemapper-print:

- `Control::id` and `Course::id` carry the ids from the `.ppen`, so a writer can find the same controls and
  courses again. IOF XML has none, so they stay `None` there.
- `placement_from_offset` and `offset_from_placement` turn a course file's number offset into a number
  placement and back. The module `number_offset` documents the reference point, the axes and the units. A
  `.ppen` stores `number-location` with y up, so a writer flips the sign of `dy_mm`.

Of the suite, the crate needs only `freemapper-omap` and `freemapper-georef`, and only for the fit. The fit sits behind the cargo
feature `fit`, which is on by default. Code that needs only the model, the readers and the number placement
sets `default-features = false` and builds without `freemapper-omap` and `freemapper-georef`. The readers refuse files beyond the limits in `model`,
such as more than 50 MiB, more than 10 000 controls or nesting deeper than 64 levels (freemapper-print
decision 0019). Error texts are English; the app translates them.

`placement::place_course_numbers` places all numbers of a course in running order. It takes the search `best`
from freemapper-print and `auto_place` from freemapper-courseplan; kept numbers stay where they are, and each
placed number blocks the next ones (decision 0016).

The module `description_symbols` holds the IOF control description symbols (drawing, German and English text)
from Purple Pen's `symbols.xml`. It also maps between Purple Pen ids and IOF 2024 numbers, in both directions
(decision 0017). The file sits unchanged in [`data/`](data/README.md). Purple Pen is published
under the BSD licence, Copyright (c) 2006-2008, Peter Golde; the full licence text is there. It must accompany
every redistribution of the library, binary form included.

The test files are in [`tests/fixtures/courses/`](tests/fixtures/courses/README.md), with their source and
licence.
