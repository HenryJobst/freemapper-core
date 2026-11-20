# 0041. UTM zone 34 and the Polish grid

Date: 2026-11-20
Status: Accepted

## Context

freemapper is to make maps anywhere in Poland. `Crs` knows the ETRS89 zones 32 and 33 only. Poland east of about
18° E lies in zone 34, and the national data (GUGiK) comes on the grid PL-1992 (EPSG:2180), which is rotated about
3° against the UTM grid at the German border and more further east. The programs of the suite (freemapper,
freemapper-print, freemapper-courseplan) read the zone from the map and choose no zone themselves.

## Decision

`freemapper-georef` gets:

- `Crs::Utm34` (EPSG:25834, zone 34), with `from_epsg(25834)`.
- `Crs::for_position(LatLon)`: below 12° E zone 32, from 12° to below 18° zone 33, from 18° zone 34. A program
  that must pick one zone for a map area asks it with the centre of the area.
- `Pl1992` with `forward` (the inverse comes with the raster warp that needs it) (transverse Mercator on GRS80, central meridian 19° E, scale 0.9993,
  false easting 500 000 m, false northing -5 300 000 m). It is a source grid for rasters and vectors, never a
  `Crs` a project is made in: a map file names a UTM zone.

## Consequences

- Every `match` on `Crs` in the programs gets a third arm; the compiler lists them.
- A map area over the zone border takes the zone of its centre and stays in that one zone; no program splits a
  map in two zones.
- `utm_from_crs` already read EPSG:25834, so freemapper-course (`fit`, `MapGrid`) needs no change; a test pins it.
- The reference values of the tests come from an independent Redfearn series, not from PROJ.
