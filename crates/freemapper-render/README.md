# freemapper-render

Die Darstellung von Karten von OpenOrienteering Mapper: OOM-Symbole so zeichnen, wie OOM sie zeichnet, das
Seitenmodell, das alle Ausgaben zeichnen (`page::RenderedPage`), die PDF-Ausgabe (`pdf`), die SVG-Ausgabe
für die Editoren im Browser (`svg`) und die Schrift Arimo mit ihren Textmaßen (`fonts`). Eigentümer ist
freemapper-print (Entscheidung 0005), die Entscheidung steht in 0019.

Der Code stammt aus freemapper 743ce4b. freemapper-print hat ihn dort kopiert (freemapper-print, Entscheidung
0014), weiterentwickelt und hierher verschoben, ohne das Verhalten zu ändern. Die Pfade unterhalb der
Bibliothek sind die alten Pfade unterhalb von `render`, zum Beispiel `freemapper_render::pdf::write_pdf`.
Einzige Abhängigkeit aus der Suite ist `freemapper-omap`.

Für die Editoren (Entscheidung 0022):

- `RenderedPage::mapping()` liefert die Seitenabbildung (`PageMapping`). `to_page` und `to_map` rechnen
  zwischen der Karte in Millimetern beim Druckmaßstab und der Seite in Millimetern um, beide mit y nach unten.
- `RenderedPage::hits_at(p, tolerance_mm)` nennt die Kartenobjekte unter einem Punkt der Seite, das oberste
  zuerst (`Hit`). Bahnaufdruck, Blattebene, Grundfläche und Nordlinien zählen nicht; Clip-Rechteck und
  Abdeckungen gelten wie beim Zeichnen.
- `svg::write_svg(&page, &SvgView::default())` schreibt die Seite als SVG, in derselben Reihenfolge wie das
  PDF. Die viewBox ist in Seitenmillimetern, `SvgView` wählt Ausschnitt und Pixel je Millimeter. Texte sind
  Glyphenumrisse, der Browser braucht keine Schrift. Farben werden mit einer einfachen Formel von CMYK nach
  sRGB umgerechnet, das PDF bleibt CMYK.

`PdfMeta::new` trägt freemapper-print und dessen GitHub-Adresse als Ersteller ein (`CREDIT`).

Das Feature `test-support` macht `pdf::content_of`, `draw::work_units` und `draw::BUDGET_SLACK` für die Tests
einer Anwendung zugänglich. Eine Anwendung trägt es nur unter `[dev-dependencies]` ein.

Die Schriften liegen in [`assets/fonts/`](assets/fonts/README.md), dort stehen Quelle und Lizenz. Die
Testkarte steht in [`tests/fixtures/maps/`](tests/fixtures/maps/README.md), die erwarteten SVGs in
[`tests/golden/`](tests/golden/README.md).

---

Rendering of OpenOrienteering Mapper maps: OOM symbols drawn the way OOM draws them, the page model every
backend draws (`page::RenderedPage`), the PDF backend (`pdf`), the SVG backend for the browser editors
(`svg`) and the Arimo fonts with their text metrics (`fonts`). The owner is freemapper-print (decision 0005);
the decision is 0019.

The code comes from freemapper 743ce4b. freemapper-print copied it from there (freemapper-print decision
0014), developed it further and moved it here without changing its behaviour. Paths below the crate root are
the old paths below `render`, for example `freemapper_render::pdf::write_pdf`. The only suite dependency is
`freemapper-omap`.

For the editors (decision 0022):

- `RenderedPage::mapping()` returns the page mapping (`PageMapping`). `to_page` and `to_map` convert between
  the map in mm at the print scale and the page in mm, both with y down.
- `RenderedPage::hits_at(p, tolerance_mm)` lists the map objects under a page point, topmost first (`Hit`).
  Course overprint, sheet layer, base area and north lines do not count; the clip rectangle and the masks
  apply as when drawing.
- `svg::write_svg(&page, &SvgView::default())` writes the page as SVG, in the same order as the PDF. The
  viewBox is in page mm, and `SvgView` picks the part of the page and the pixels per mm. Texts are glyph
  outlines, so the browser needs no font. Colours go from CMYK to sRGB by a simple formula; the PDF stays
  CMYK.

`PdfMeta::new` names freemapper-print and its GitHub address as creator (`CREDIT`).

The feature `test-support` exposes `pdf::content_of`, `draw::work_units` and `draw::BUDGET_SLACK` to an app's
tests. An app enables it under `[dev-dependencies]` only.

The fonts are in [`assets/fonts/`](assets/fonts/README.md) with their source and licence. The test map is in
[`tests/fixtures/maps/`](tests/fixtures/maps/README.md), the expected SVGs in
[`tests/golden/`](tests/golden/README.md).
