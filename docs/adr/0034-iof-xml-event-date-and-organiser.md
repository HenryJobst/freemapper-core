# 0034. IOF XML event date and organiser

Date: 2026-11-14
Status: Accepted

## Context
freemapper-courseplan 0.17.0 writes the date and the organiser of an event into its IOF XML export, as
`Event/StartTime/Date` and `Event/Organiser/Name`. freemapper-print prints both in the title block (`{date}` and
`{organiser}`) but takes them only from its own project file. The course reader read only `Event/Name`, so a person
who sets the event date once in the course-setting program had to type it again for the print.

## Decision
- `CourseSource` gets two fields, `event_date: Option<String>` and `organiser: Option<String>`. The date stays text,
  as the file writes it (`YYYY-MM-DD`), so the course crate needs no date type and does not judge a malformed
  value; the program that shows it parses it.
- `read_iof_xml` fills them from `Event/StartTime/Date` and `Event/Organiser/Name`. An empty element gives `None`,
  and a value longer than the code or name limit is refused as for other texts.
- The Purple Pen reader sets both to `None`: a `.ppen` file has no field for either.

## Consequences
- Adding fields to a public struct with public fields breaks code that builds it by hand. The literals in
  freemapper-print's tests need the two new lines; this is a minor-version change.
- The date is not validated here. A file with `StartTime/Date` in another form reaches the consumer as is.
