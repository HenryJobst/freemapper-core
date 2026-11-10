# 0017. Description symbols in core

Date: 2026-11-10
Status: Accepted

## Context
Three programs need the symbols of the IOF control descriptions. freemapper-print draws them from Purple Pen's
`symbols.xml`. freemapper-courseplan maps Purple Pen ids to IOF 2024 numbers when it imports a `.ppen` file,
and back when it exports one. The writer in this crate refuses ids that Purple Pen does not know in a standard
or does not offer in a column. Each program kept its own table, and the export table could write ids the
writer then refused.

## Decision
- The database moves to `freemapper-course::description_symbols`: the XML byte for byte (about 670 KB, embedded in
  the binary), the parser, `symbol`, `symbol_text`, `symbol_kind`, `exists` and `Standard`.
  The file is BSD-licensed; `data/README.md` carries the notice and its provenance (freemapper-print ADR 0036).
- The forward mapping `iof_2024` moves unchanged from courseplan (`ppen_import.rs`; courseplan ADR 0034). Purple Pen's English
  names match the IOF 2024 text row by row, so the mapping needs no standard parameter: an id means the same in
  the 2004 and 2018 definitions. Only 1.10 (drawing) and 11.7
  (column G in 2004, F in 2018) differ between the two, and neither changes the number.
- The inverse is `ppen_symbol(id, standard)` for one number and `ppen_columns` for columns C to H. It is built
  from the database, not from a guessed list of ids. Where several Purple Pen ids give one number, the plain
  id wins, then the direction forms, then `high`, `deep`, `single`, `control`. Every written id passes `exists`
  and has a `symbol_kind` that Purple Pen offers in the box (D takes D; E takes E and D; C also takes the
  instruction rows V to Z, which a finish or crossing point keeps in its one box).
- Crossing-point handling stays in courseplan. The writer's combination rule for columns C and G
  (`COMBINATION_SYMBOLS`) is not repeated.

## Rulings
*(Claude's call)* unless marked otherwise.

| 2024 id (column) | Ruling | Verified |
|---|---|---|
| 0.1NE/NW/SE/SW (C) | Written as 0.2NE/NW/SE/SW | 2024 has 0.1 "Northern" and 0.2 "South Eastern"; Purple Pen has 0.1 only for N/E/S/W and 0.2 only for the diagonals |
| 0.2N/E/S/W (C) | Written as 0.1N/E/S/W | same |
| plain 9.1, 9.3 (F) | `9.1high`, `9.3high`, `chosen = true` (decided by the controller; the plan default) | 2024 9.1 is one symbol "Height or Depth" and 9.3 "Height on slope"; Purple Pen splits both into `high` and `deep`, with the same drawing |
| 2.10, 15.6, 5.23, 5.24 in a 2004 file | `NotInStandard(Iof2004)` | `symbols.xml` has 2.10, 15.6, 5.25 and 5.26 for 2018 only. 2024 5.23 and 5.24 are Purple Pen's 5.25 and 5.26 |
| 2024 5.21, 5.22 | Written as 5.23, 5.24 in both standards | `symbols.xml` has 5.23 and 5.24 without a standard. 2024 5.21 is "Canopy", a covered passage, Purple Pen's "Building pass-through" |
| 2024 5.25, 5.26, 14.1, 13.9, unknown numbers | `NoPurplePenId` | the database has no such number |
| bend 11.1 in F, 2004 file | G when G is empty (as before) | `symbols.xml` has 11.7 as kind G in 2004 and kind F in 2018 |
| bend 11.1 in F, 2004 file, G taken | Deviation from the proposal: `WrongColumn`, not `NotInStandard(Iof2004)` | 11.7 exists in 2004; only box F does not offer it |
| D equal to E with 10.1 or 10.2 in F | D keeps the feature, E empty, F gets `10.1single` or `10.2single` | `10.1single` reads "{0} crossing" and has kind F |
| D equal to E with 12.14 in G | Deviation from the proposal: D and E both stay, G gets the plain `11.15` | `11.15single` reads "between {1}" and `write_ppen` refuses it in column G (`COMBINATION_SYMBOLS`; Purple Pen throws on it). 2024 names both features in D and E for 12.14, so the plain symbol is correct. There is no `12.14single` |
| a symbol outside the kinds a box offers | Addition: `Refusal::WrongColumn` (the proposal had two variants) | `purple_pen_strict.rs` `COLUMN_KINDS` |
| any other number | the first Purple Pen id in the order above | |

The old export table looked for `single` on the id in column D (`10.1` in D), which no description has. The new
rule tests column F.

## Consequences
- Both apps read and write ids through one table. courseplan replaces its copy in a later change; its
  results change only where the table above says so: diagonal and cardinal zeros are swapped, plain 9.1 and 9.3
  become `high` (with a note), 2018-only symbols are refused in 2004 files, and `single` moves to column F.
- The crate grows by the embedded file. A program that only reads `.ppen` files carries it too.
- `ppen_columns` does not know the control kind. A caller that writes a normal control must still keep the
  instruction rows out of column C, as `write_ppen` does.
- If Purple Pen adds an id or changes a standard, the database file changes and the tests that walk it show
  which numbers moved.

Source: IOF International Specification for Control Descriptions 2024; Purple Pen `symbols.xml` at
petergolde/PurplePen `201417f`; freemapper-print ADR 0036; freemapper-courseplan ADR 0034; Task 6 of the plan
`core-placement-and-symbols`.
