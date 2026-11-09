# Purple Pen fixtures

Three test events from Purple Pen's repository, copied unchanged on 2026-11-09 (freemapper-print M3 Task 4).
They and this README came over from freemapper-print, commit `918cec2`, with the course reader.

Source: https://github.com/petergolde/PurplePen, commit `201417f97bb69302e3e0db123f428870e0914515`
(2026-11-07).

| File here | Path in the Purple Pen repository | SHA-256 | What it covers |
|---|---|---|---|
| `lincoln-park.ppen` | `src/TestFiles/controller/Lincoln Park.ppen` | `686e07e7…c324e8` | One course; a leg with a bend and a gap; circle gaps in the old (bit string) and the new (angle) form; descriptions; no `<standards>` element, so the description standard is the default 2004. |
| `overlapper.ppen` | `src/TestFiles/courserenderer/Overlapper.ppen` | `9f8cd545…4c2f78` | Two normal courses at print scales 7500 and 10000, a score course, a crossing point, a `number-location`, circle gaps in the old form only, water and forbidden-route specials. |
| `sampleevent8.ppen` | `src/TestFiles/descriptions/sampleevent8.ppen` | `d8d4872f…f1fb` | Description standard 2018, full description rows including column F text (`5x5\|3`), a map-exchange control, map exchanges at normal controls (`map-exchange="true"`), a water special and a descriptions special. |

Full hashes: `686e07e774a94a650d5169a268661201ec3dff3186d1dc2c79c2247743c324e8`,
`9f8cd5455c3957f492e2169eef5085933e00bb2de60ff1c9cff77740c4162f78`,
`d8d4872f040f56dc9ec2c8728e70e530d63331ee5cb19cb399e2abbec288f1fb`.

The files carry no licence header of their own. Purple Pen as a whole is published under the BSD licence below:
every source file in the repository carries it (for example `src/PurplePenCore/EventDB.cs`, "Copyright (c)
2006-2008, Peter Golde"), and the user documentation states it as the Purple Pen licence
(`doc/userdocs/WebSite/license.htm`, "Copyright © 2006-2007, Peter Golde"). The repository has no top-level
licence file. The tests change the files only in memory (for example to add unknown elements).

## Licence

```
Copyright (c) 2006-2008, Peter Golde
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

1. Redistributions of source code must retain the above copyright
notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright
notice, this list of conditions and the following disclaimer in the
documentation and/or other materials provided with the distribution.

3. Neither the name of Peter Golde, nor "Purple Pen", nor the names
of its contributors may be used to endorse or promote products
derived from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND
CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT OWNER OR
CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING,
BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY
OF SUCH DAMAGE.
```
