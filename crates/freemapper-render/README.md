# freemapper-render

Die Darstellung von Karten von OpenOrienteering Mapper: OOM-Symbole so zeichnen, wie OOM sie zeichnet, das
Seitenmodell, das alle Ausgaben zeichnen (`page::RenderedPage`), die PDF-Ausgabe (`pdf`) und die Schrift Arimo
mit ihren Textmaßen (`fonts`). Eigentümer ist freemapper-print (Entscheidung 0005), die Entscheidung steht in
0019.

Der Code stammt aus freemapper 743ce4b. freemapper-print hat ihn dort kopiert (freemapper-print, Entscheidung
0014), weiterentwickelt und hierher verschoben, ohne das Verhalten zu ändern. Die Pfade unterhalb der
Bibliothek sind die alten Pfade unterhalb von `render`, zum Beispiel `freemapper_render::pdf::write_pdf`.
Einzige Abhängigkeit aus der Suite ist `freemapper-omap`.

`PdfMeta::new` trägt freemapper-print und dessen GitHub-Adresse als Ersteller ein (`CREDIT`).

Das Feature `test-support` macht `pdf::content_of`, `draw::work_units` und `draw::BUDGET_SLACK` für die Tests
einer Anwendung zugänglich. Eine Anwendung trägt es nur unter `[dev-dependencies]` ein.

Die Schriften liegen in [`assets/fonts/`](assets/fonts/README.md), dort stehen Quelle und Lizenz. Die
Testkarte steht in [`tests/fixtures/maps/`](tests/fixtures/maps/README.md).
