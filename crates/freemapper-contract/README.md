# freemapper-contract

Der Vertrag zwischen freemapper und den Programmen, die seine Karten lesen, freemapper-print und
freemapper-courseplan. freemapper schreibt eine Karte (`<Name>.omap`) und eine Begleitdatei (`freemapper.toml`).
Diese Bibliothek hält fest, worauf sich ein Leser dabei verlassen darf, und prüft es. Eigentümer ist
freemapper (Entscheidung 0005), der Vertrag steht in Entscheidung 0018.

Die Bibliothek ordnet jedes Kartenobjekt nach seinem Tag `freemapper:source` einer Objektklasse zu: Randobjekt
(`sheet`), Grundfläche (`base`), Nordlinie (`north-lines`) oder Karteninhalt (alles andere). Daraus leitet sie
drei Dinge ab:

- `census` zählt die Objekte je Klasse und liefert die Rahmen von Karteninhalt und Randobjekten in
  Kartenkoordinaten.
- `Vocabulary::of` sammelt die Wörter der Datei ohne ihren Inhalt: Namen der Kartenteile, Tags in den Teilen
  „peripheral information“ und „base“, Form des KRS-Texts, Vertragsschlüssel der Begleitdatei.
- `check` prüft zwölf Regeln für die Ausgabe von freemapper und nennt jeden Verstoß mit Fundstelle.

Karte und Begleitdatei gehen als Text hinein. Kein Typ von freemapper-omap steht in der Schnittstelle, deshalb
kann jede Anwendung denselben Text mit dem Leser der core-Version lesen, die sie selbst einbindet.

## Gemeinsame Typen

Die Wörter des Vertrags gibt es einmal, hier (Entscheidung 0027). Die drei Anwendungen benutzen sie, statt sie
selbst zu schreiben:

- `MapKind`: die Kartenart (`sprint`, `schoolyard`, `forest`) mit `name`, `from_name`, `base_scale` und
  `drawing_scales`, den Maßstäben, in denen freemapper sie schreibt. Druckmaßstäbe stehen in
  freemapper-printrules.
- `Sidecar::parse` liest die Vertragsschlüssel aus dem Text einer `freemapper.toml`, `Sidecar::next_to` die
  Datei neben einer Karte. Eine unbekannte Kartenart ist `SidecarKind::Unknown` mit ihrem Namen und kein
  Fehler; `Sidecar::title` gibt den Titel ohne Leerzeichen am Rand oder nichts.
- `ObjectClass::of` bestimmt die Objektklasse aus den Tags eines Objekts, `is_map_content` sagt, ob sie zum
  Karteninhalt zählt.
- `tags`: `SOURCE_KEY`, `ROLE_KEY`, die reservierten Quellen, `SHEET_PART`, `BASE_PART`, `SIDECAR_FILE` und
  die Vertragsschlüssel.

Diese Typen brauchen keinen Kartenleser. Wer nur sie ins Programm einbindet, schaltet die Standard-Features ab;
`census`, `Vocabulary` und `check` liegen hinter dem Feature `check`:

```toml
[dependencies]
freemapper-contract = { git = "https://github.com/HenryJobst/freemapper-core", tag = "v0.18.0", default-features = false }
```

## Fixtures

Mit dem Feature `fixtures` enthält die Bibliothek zwei Karten, die freemapper 0.9.2 geschrieben hat:
`schoolyard-1000` und `sprint-4000`. Zu jeder gehören `expected.toml` (was ein Leser ableiten muss) und
`vocabulary.toml` (was freemapper einhalten muss). Die Karten nennen Kartenvorlagen, die nicht mitgeliefert
werden.

Zwei weitere Fixtures sind nur für die Leser da und verstoßen absichtlich gegen Regeln; `breaks` in ihrer
`expected.toml` nennt sie. `legacy-0.9.0-scale-bar` ist eine Karte von freemapper 0.9.0: Die Maßstabsleiste
trägt `freemapper:source=scale-bar`, und die Begleitdatei hat keinen `title`. `future-kind` hat
`map_kind = "gym"`, einen unbekannten Schlüssel und eine unbekannte Rolle eines Randobjekts. Ein Leser nimmt
dann keine Kartenart aus der Begleitdatei, meldet keinen Fehler und liest den Titel trotzdem. Woher jedes
Fixture stammt, steht in `fixtures/README.md`.

```toml
[dev-dependencies]
freemapper-contract = { git = "https://github.com/HenryJobst/freemapper-core", tag = "v0.10.0", features = ["fixtures"] }
```

Ein Leser testet sich so: Er legt jedes Fixture mit `Fixture::write_into` in einen Ordner, liest es mit eigenem
Code und vergleicht Kartenart, Titel, Objektzahlen und Ausdehnung mit `Fixture::expected`. Die Tests heißen
`contract_*`.

freemapper prüft seine Ausgabe in jedem Testlauf mit `check` und vergleicht ihr Vokabular mit dem des Fixtures.
Ändert sich das Vokabular, ändert sich der Vertrag. Dann schreibt der Test `contract_write_fixtures` in
freemapper die Fixtures neu, core wird veröffentlicht, und die Leser ziehen nach.

---

The contract between freemapper and the programs that read its maps, freemapper-print and
freemapper-courseplan. freemapper writes a map (`<name>.omap`) and a sidecar file (`freemapper.toml`). This
crate records what a reader may rely on in them, and tests it. The owner is freemapper (decision 0005); the
contract is in decision 0018.

The crate sorts every map object into an object class by its tag `freemapper:source`: sheet object (`sheet`),
base area (`base`), north line (`north-lines`) or content (everything else). It derives three things from
that:

- `census` counts the objects per class and returns the boxes of content and of sheet objects in map
  coordinates.
- `Vocabulary::of` collects the words of the file without its content: part names, tags in the parts
  "peripheral information" and "base", the form of the CRS text, the contract keys of the sidecar file.
- `check` tests twelve rules for freemapper's output and reports every violation with its place.

Map and sidecar file go in as text. No type of freemapper-omap is part of the API, so each app can read the
same text with the reader of the core version it pins.

## Shared types

The words of the contract exist once, here (decision 0027). The three apps use them instead of writing their
own:

- `MapKind`: the map kind (`sprint`, `schoolyard`, `forest`) with `name`, `from_name`, `base_scale` and
  `drawing_scales`, the scales freemapper writes it at. Print scales are in freemapper-printrules.
- `Sidecar::parse` reads the contract keys from the text of a `freemapper.toml`, `Sidecar::next_to` the file
  beside a map. An unknown map kind is `SidecarKind::Unknown` with its name and no error; `Sidecar::title`
  gives the title without blanks around it, or nothing.
- `ObjectClass::of` derives the object class from the tags of an object; `is_map_content` tells whether it
  counts as map content.
- `tags`: `SOURCE_KEY`, `ROLE_KEY`, the reserved sources, `SHEET_PART`, `BASE_PART`, `SIDECAR_FILE` and the
  contract keys.

These types need no map reader. An app that links only them turns the default features off; `census`,
`Vocabulary` and `check` sit behind the feature `check`:

```toml
[dependencies]
freemapper-contract = { git = "https://github.com/HenryJobst/freemapper-core", tag = "v0.18.0", default-features = false }
```

## Fixtures

With the feature `fixtures` the crate contains two maps written by freemapper 0.9.2: `schoolyard-1000` and
`sprint-4000`. Each has an `expected.toml` (what a reader must derive) and a `vocabulary.toml` (what freemapper
must keep). The maps name templates that are not shipped.

Two more fixtures are for the readers only and break rules on purpose; `breaks` in their `expected.toml` names
them. `legacy-0.9.0-scale-bar` is a map from freemapper 0.9.0: the scale bar carries
`freemapper:source=scale-bar`, and the sidecar file has no `title`. `future-kind` has `map_kind = "gym"`, an
unknown key and an unknown role on a sheet object. A reader then takes no kind from the sidecar file, reports
no error and still reads the title. `fixtures/README.md` says where each fixture comes from.

```toml
[dev-dependencies]
freemapper-contract = { git = "https://github.com/HenryJobst/freemapper-core", tag = "v0.10.0", features = ["fixtures"] }
```

A reader tests itself like this: it puts each fixture into a folder with `Fixture::write_into`, reads it with
its own code and compares map kind, title, object counts and extent with `Fixture::expected`. The tests are
named `contract_*`.

freemapper tests its output in every test run with `check` and compares its vocabulary with the fixture's. A
change to the vocabulary is a change to the contract. The test `contract_write_fixtures` in freemapper then
writes the fixtures again, core gets a release, and the readers follow.
