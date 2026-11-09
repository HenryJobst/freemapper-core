# freemapper-omap

Karten von OpenOrienteering Mapper (OOM): das Kartenmodell, ein Leser für `.omap` und `.xmap` und die
eingebetteten Symbolsätze ISSprOM 2019 und Schulkarte 2019. Eigentümer ist freemapper-print (Entscheidung
0005).

Der Code stammt aus freemapper 743ce4b. freemapper-print hat ihn dort kopiert und weiterentwickelt
(freemapper-print, Entscheidung 0014) und mit Commit `918cec2` hierher verschoben (Entscheidung 0012). Die
Pfade sind dieselben wie in freemapper-print: `freemapper_omap::MapDocument`,
`freemapper_omap::coord::MapCoord`, `freemapper_omap::read::read_map_file`,
`freemapper_omap::SymbolSet::issprom_2019()`.

Der Leser lehnt Dateien ab, die tiefer als `MAX_XML_DEPTH` (256) verschachtelt sind oder Zahlen enthalten,
die nicht endlich sind (freemapper-print, Entscheidung 0019). Fehlertexte sind Englisch, die Anwendung
übersetzt sie.

Die Symbolsätze liegen in [`assets/symbol-sets/`](assets/symbol-sets/README.md), dort stehen Quelle und
Lizenz.

---

Maps of OpenOrienteering Mapper (OOM): the map model, a reader for `.omap` and `.xmap`, and the embedded
symbol sets ISSprOM 2019 and school map 2019. The owner is freemapper-print (decision 0005).

The code comes from freemapper 743ce4b. freemapper-print copied it from there and developed it further
(freemapper-print decision 0014), then moved it here at commit `918cec2` (decision 0012). The paths are the
same as in freemapper-print: `freemapper_omap::MapDocument`, `freemapper_omap::coord::MapCoord`,
`freemapper_omap::read::read_map_file`, `freemapper_omap::SymbolSet::issprom_2019()`.

The reader refuses files nested deeper than `MAX_XML_DEPTH` (256) and numbers that are not finite
(freemapper-print decision 0019). Error texts are English; the app translates them.

The symbol sets are in [`assets/symbol-sets/`](assets/symbol-sets/README.md), with their source and
licence.
