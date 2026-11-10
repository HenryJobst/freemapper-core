# freemapper-course

Bahnen des Orientierungslaufs: das Bahnmodell, Leser für Purple Pen (`.ppen`) und IOF-XML 3.0, die
Einpassung einer Bahndatei in eine OOM-Karte und die Platzierung der Postennummern. Eigentümer ist
freemapper-print (Entscheidung 0005).

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

Die Testdateien liegen in [`tests/fixtures/courses/`](tests/fixtures/courses/README.md), dort stehen
Quelle und Lizenz.

---

Orienteering courses: the course model, readers for Purple Pen (`.ppen`) and IOF XML 3.0, the fit of a course
file to an OOM map, and the placement of control numbers. The owner is freemapper-print (decision 0005).

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

The test files are in [`tests/fixtures/courses/`](tests/fixtures/courses/README.md), with their source and
licence.
