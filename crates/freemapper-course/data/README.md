# Embedded data

## `purple-pen-symbols.xml`

The IOF control description symbols with their texts, embedded into the library by `description_symbols`.
Copied unchanged (CRLF line ends and the UTF-8 BOM included, see `.gitattributes`) on 2026-11-09 into
freemapper-print (M3 Task 8, ADR 0036 there) and moved here from freemapper-print `211976b` on 2026-11-10 (byte
copy, same SHA-256).

| | |
|---|---|
| Source | https://github.com/petergolde/PurplePen, `src/PurplePenCore/symbols.xml` |
| Commit | `201417f97bb69302e3e0db123f428870e0914515` (2026-11-07); the file last changed in `af2b6c41ef1d8ad718ddd4fa3c7a15a1db508014` (2026-03-24) |
| Git blob | `7294545581fa4a7b2e9ea992c9790ae263cbdbb7` |
| SHA-256 | `84b51153333961675cf2de5555e540d2e6360f3735a2ac3e61cdf248b724ad05` |
| Licence | BSD, below |

The file carries no licence header of its own; its header comment only describes the format. Purple Pen as a
whole is published under the BSD licence below. The code that reads the file, `src/PurplePenCore/SymbolDB.cs`,
carries it with "Copyright (c) 2006-2008, Peter Golde", as every source file in the repository does, and the
user documentation states it as the Purple Pen licence (`doc/userdocs/WebSite/license.htm`, "Copyright ©
2006-2007, Peter Golde"). The repository has no top-level licence file. The release archives carry this notice
in `THIRD_PARTY.md`.

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
