# freemapper-georef

Koordinaten und Koordinatensysteme der freemapper-Suite: geografische und projizierte Punkte (`LatLon`,
`ProjectedPoint`), Begrenzungsrechtecke (`BoundingBox`), die Kachelraster zu 1 km und 2 km (`TileId`) und die
UTM-Zonen 32 und 33 auf ETRS89 (`Crs`, EPSG:25832 und 25833). Eigentümer ist freemapper (Entscheidung 0005).

Die Bibliothek enthält die einzige UTM-Rechnung der Suite. `tm::TransverseMercator` rechnet mit der
Krüger-Reihe bis n^6 auf jedem Ellipsoid und liefert Rechtswert, Hochwert, Meridiankonvergenz und
Punktmaßstab. `Crs` rechnet darüber, und `tm::utm_from_crs` erkennt UTM-Zonen im Koordinatensystemtext einer
Karte: `EPSG:258NN` (ETRS89, also auch 25831), `EPSG:326NN` und `EPSG:327NN` (WGS84) sowie
`+proj=utm +zone=NN`. Die Bibliothek braucht keine anderen Bibliotheken.

`Crs`, `LatLon`, `ProjectedPoint`, `BoundingBox` und `TileId` stammen mit Namen, Verhalten und Tests aus
freemapper `crs.rs` und `tiles.rs` (freemapper v0.9.0, Commit `0f25841`). Die Projektion stammt aus
freemapper-print (`course/tm.rs`, Version 0.4.2 von freemapper-course); die Reihe bis n^6, Konvergenz und
Maßstab kommen aus `crs.rs`. Beide Testsätze laufen gegen dieselbe Rechnung. Die magnetische Deklination
bleibt in freemapper, sie braucht dessen Datumstyp und das Weltmagnetmodell (Entscheidung 0015).

---

Coordinates and coordinate systems of the freemapper suite: geographic and projected points (`LatLon`,
`ProjectedPoint`), bounding boxes (`BoundingBox`), the 1 km and 2 km tile grids (`TileId`) and UTM zones 32
and 33 on ETRS89 (`Crs`, EPSG:25832 and 25833). The owner is freemapper (decision 0005).

The library holds the suite's only UTM computation. `tm::TransverseMercator` uses the Krüger series to n^6 on
any ellipsoid and returns easting, northing, grid convergence and point scale factor. `Crs` computes through
it, and `tm::utm_from_crs` recognises UTM zones in a map's CRS text: `EPSG:258NN` (ETRS89, so 25831 too),
`EPSG:326NN` and `EPSG:327NN` (WGS84), and `+proj=utm +zone=NN`. The library needs no other crates.

`Crs`, `LatLon`, `ProjectedPoint`, `BoundingBox` and `TileId` come with names, behaviour and tests from
freemapper's `crs.rs` and `tiles.rs` (freemapper v0.9.0, commit `0f25841`). The projection comes from
freemapper-print (`course/tm.rs`, version 0.4.2 of freemapper-course); the series to n^6, convergence and
scale factor come from `crs.rs`. Both test sets run against the same computation. Magnetic declination stays
in freemapper because it needs freemapper's date type and the World Magnetic Model (decision 0015).
