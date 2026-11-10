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

## Fixtures

Mit dem Feature `fixtures` enthält die Bibliothek zwei Karten, die freemapper 0.9.2 geschrieben hat:
`schoolyard-1000` und `sprint-4000`. Zu jeder gehören `expected.toml` (was ein Leser ableiten muss) und
`vocabulary.toml` (was freemapper einhalten muss). Die Karten nennen Kartenvorlagen, die nicht mitgeliefert
werden.

```toml
[dev-dependencies]
freemapper-contract = { git = "https://github.com/HenryJobst/freemapper-core", tag = "v0.8.0", features = ["fixtures"] }
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

## Fixtures

With the feature `fixtures` the crate contains two maps written by freemapper 0.9.2: `schoolyard-1000` and
`sprint-4000`. Each has an `expected.toml` (what a reader must derive) and a `vocabulary.toml` (what freemapper
must keep). The maps name templates that are not shipped.

```toml
[dev-dependencies]
freemapper-contract = { git = "https://github.com/HenryJobst/freemapper-core", tag = "v0.8.0", features = ["fixtures"] }
```

A reader tests itself like this: it puts each fixture into a folder with `Fixture::write_into`, reads it with
its own code and compares map kind, title, object counts and extent with `Fixture::expected`. The tests are
named `contract_*`.

freemapper tests its output in every test run with `check` and compares its vocabulary with the fixture's. A
change to the vocabulary is a change to the contract. The test `contract_write_fixtures` in freemapper then
writes the fixtures again, core gets a release, and the readers follow.
