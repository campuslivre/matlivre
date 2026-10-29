# matlivre — Live Script behavioral specification (clean room)

This document is the **only** input the implementation is written from. It records
observed *behavior* and *on-disk interchange formats*; it contains no MathWorks source
code, and none was read. Evidence came from:

| Source | What was observed | Evidence |
|---|---|---|
| `toolbox/matlab/demos/SolarPanelEstimator.mlx` (R2023b install) | Package parts, `document.xml` / `output.xml` vocabulary | REA `extract_artifact`, `ev_4699290bf7d2a87577b6f1b586b3e8a9571c355539a96722a49c683bdf7bf510` (sha256 `229fae42…bc40`) |
| `RealTimeDashboard.mlx`, `publishInterfaceWorkflowTemplate.mlx`, `LiveScriptBasedTest.mlx` | Same vocabulary, section breaks, headings, hyperlinks, TOC | ZIP inventory + element census (4 files) |
| Public product behavior of the Live Editor | Run section / run all, inline output, variable display | User-visible behavior only |

Observations are marked **(O)**, inferences **(I)**, and our own design choices **(D)**.

---

## 1. Package format (`.mlx`)

1. **(O)** A `.mlx` file is a ZIP container following Open Packaging Conventions (OPC):

   | Part | Content type |
   |---|---|
   | `[Content_Types].xml` | OPC content-type map. Default `xml` → `application/vnd.mathworks.matlab.code.document+xml`; `rels` → relationships |
   | `_rels/.rels` | Root relationships `rId1..rId5` |
   | `matlab/document.xml` | The document (rel type `http://schemas.mathworks.com/matlab/code/2013/relationships/document`) |
   | `matlab/output.xml` | Saved outputs (rel type `…/2013/relationships/output`), overridden content type `text/xml` |
   | `metadata/coreProperties.xml` | OPC core properties (`dcterms:created`, `dcterms:modified`, W3CDTF) |
   | `metadata/mwcoreProperties.xml` | `contentType` = `application/vnd.mathworks.matlab.code`, `contentTypeFriendlyName`, `matlabRelease` |
   | `metadata/mwcorePropertiesExtension.xml` | `matlabVersion` |

2. **(O)** `document.xml` reuses the WordprocessingML namespace
   `http://schemas.openxmlformats.org/wordprocessingml/2006/main` (prefix `w`):
   `w:document/w:body/w:p*`.

3. **(O)** Each `w:p` is one *paragraph*; its role is `w:pPr/w:pStyle/@w:val`. Observed styles
   (census over 4 files): `text` (24), `code` (16), `heading` (13), `heading2` (8), `title` (3),
   `CodeExampleLine`, `LiveAppLine`, `TOC1`, `TOCHeading`.

4. **(O)** Text paragraphs contain runs `w:r/w:t`; a run may carry `w:rPr` with `w:b` (bold),
   `w:i` (italic), `w:rFonts`. Paragraph alignment is `w:jc/@w:val` (`left`, …).
   Hyperlinks appear as `w:hyperlink/@w:anchor` (e.g. `internal:H_4044D1AE`) pointing at
   `w:bookmarkStart` targets.

5. **(O)** A `code` paragraph holds the **whole code block** as one run whose `w:t` is a
   CDATA section with embedded newlines. Consecutive code lines are *not* split into
   separate paragraphs.

6. **(O)** A **section break** is an empty paragraph whose properties contain an empty
   `w:sectPr`: `<w:p><w:pPr><w:sectPr/></w:pPr></w:p>`.

7. **(O)** Interactive *live controls* are attached to a code paragraph after its run, inside
   `mc:AlternateContent/mc:Choice[@Requires]/w:customXml[@w:element="livecontrol"]`. Their
   `w:customXmlPr/w:attr` items give `context` (JSON: `type` = `slider` | `comboBox` …,
   `data` with `min/max/step/value/items`), `text` (the literal currently in the code), and the
   replaced span `startOffsetLine/startColumn/endOffsetLine/endColumn` (1-based, relative to
   the code block). The code text already contains the control's current value.

8. **(O)** `output.xml` root is `embeddedOutputs` with:
   - `metaData/evaluationState` (`manual`), `layoutState` (`code` / `hidecode`, wrapped in
     `mc:AlternateContent`), `outputStatus` (`ready`), `splitterState/proportion` (e.g. `0.4`
     — the code/output split when outputs are shown on the right).
   - `outputArray/element*`, each with `type` (`text`, `figure`, …), `outputData`, and
     `lineNumbers` (document-relative line numbers of the producing statement).
     - `text`: `outputData/text` plus `truncationInfo`.
     - `figure`: `figureId` (UUID), `figureUri` (`data:image/png;base64,…`), `figureSize`
       (`[560 420]`), `regionNumbers`, `originalRegionNumber`.
   - `regionArray/element*`: one *region* per executable statement — `code/regionString`,
     `regionNumber`, `sectionBreak` (true for the first statement after a break),
     `endOfSection`, `startLine`, `endLine`, `outputIndexes`.

9. **(I)** Outputs are associated with code by statement region and document line, so they can
   be re-anchored after load without re-running.

## 2. Document model

- **(O/D)** A live script is an ordered list of *blocks*: `Title`, `Heading`, `Heading2`, `Text`,
  `Code`, and `SectionBreak`. Sections are the maximal runs of blocks between breaks; the
  document start and end are implicit breaks.
- **(D)** Unknown paragraph styles load as `Text`, preserving their text. Live controls are
  preserved as plain code (their current value is already in the code, see 1.7).
- **(D)** Plain `.m` files are also accepted: `%%` starts a new section; text after `%%` becomes a
  `Heading`; following leading `%` comment lines become a `Text` block.

## 3. Execution model

- **(O)** *Run All* evaluates every code block top to bottom in **one shared workspace**
  (the base workspace); variables persist between runs until cleared.
- **(O)** *Run Section* evaluates only the code blocks of the section containing the caret.
- **(O)** Before evaluating, the outputs of the blocks being run are cleared; outputs from
  other sections remain.
- **(O)** A statement not terminated by `;` displays its result. Assignment shows
  `name = value`; a bare expression assigns to and shows `ans`. Scalars print on one line
  (`x = 5`); arrays print a header with the size (`x = 1×3`) followed by the formatted rows.
- **(O)** `disp`/`fprintf` output appears as text output anchored to the statement's line.
- **(O)** Graphics calls produce a **figure output** shown inline below the code that created
  it; subsequent plotting in the same section updates that figure (with `hold on`) or
  replaces its content.
- **(O)** An error stops execution of the run, shows a red error output at the failing line, and
  leaves earlier outputs intact.
- **(O)** Functions may be defined at the end of a script (local functions) and called from
  any code above.
- **(D)** Execution happens on an engine thread, never the UI thread; the UI streams outputs as
  they are produced and can request cancellation between statements.

## 4. Editor UI behavior

- **(O)** Code and formatted text are interleaved in a single scrolling document. Code blocks
  have a tinted background; the current section is marked by a vertical bar in the left gutter.
- **(O)** Outputs can be shown *inline* (below each code block) or *on the right* (split at
  `splitterState/proportion`).
- **(O)** Toolbar: insert Text / Code / Section Break / headings; Run Section; Run All;
  Run-and-Advance. Shortcuts: `Ctrl+Enter` run section, `F5` run all.
- **(D)** Visual design follows ModernWinForms (Windows 11 Fluent): custom 32 px title bar
  carrying the menu, Segoe UI Variable 10.5 pt, 4 px rounded controls with the elevation
  stroke, accent `#005FB8` (light) / `#4CC2FF` (dark), window `#F3F3F3` / `#202020`, field
  `#FBFBFB`, light and dark skins.

## 5. Language subset for the proof of concept (D)

Numeric double matrices (column-major), logical, char row vectors, `"string"` scalars, cell
arrays, structs, anonymous function handles. Operators: `+ - * / \ ^ .* ./ .^ ' .'`,
comparisons, `& | && || ~`, ranges `a:b:c`, `end` in indexing. Statements: assignment
(indexed, field, multi-output `[a,b] = f(...)`), `if/elseif/else`, `for`, `while`, `switch`,
`try/catch`, `break`, `continue`, `return`, local `function`s, command syntax
(`hold on`, `close all`, `clc`, `format long`). A core library of math, array, string, and
plotting functions (`plot`, `figure`, `hold`, `title`, `xlabel`, `ylabel`, `legend`, `grid`,
`scatter`, `bar`).

### 5.1 Compatibility rule

MATLAB compatibility is a requirement, not a goal: syntax, semantics, function names and
signatures, error conditions, value display and file interchange follow MATLAB's public,
documented behavior. Where matlivre does not match yet, the gap is listed in section 6 as a
limitation — never redefined as a design choice.

### 5.2 Extended language (public MATLAB behavior)

- **N-D arrays.** Every array type has a shape of ≥ 2 dimensions with trailing singleton
  dimensions dropped. Indexing with k subscripts collapses the trailing dimensions into the
  k-th; linear indexing follows the source-vector orientation rule; `end` is the size of the
  dimension it indexes (or of the collapsed tail). Implicit expansion applies to every
  dimension. Pages display as `A(:,:,k) =`.
- **Numeric classes.** `double`, `single`, `logical`, `char`, `int8…int64`, `uint8…uint64`.
  Integer arithmetic is computed in double, rounded half away from zero and saturated to the
  class range; an integer combines only with the same integer class or with double/
  logical/char; `single` with `double` yields `single`; relational results are `logical`.
  `intmax`, `intmin`, `realmax`, `class`, `cast`, `isinteger`, `isfloat`.
- **Complex numbers.** Literals `3i`/`2j`, constants `i`, `j`, `1i`. Arithmetic, `'`
  conjugate transpose vs `.'`, `abs`, `angle`, `real`, `imag`, `conj`, `isreal`,
  `complex`, elementary functions on the complex plane (`sqrt(-1)`, `log(-1)`, `(-8)^(1/3)`
  return complex). Results whose imaginary parts are all zero become real. `<`, `>` compare
  real parts; `==` compares both. Display `3.0000 + 4.0000i`.
- **String arrays.** `"…"` creates a string scalar; `[...]` of strings forms a string array;
  `+` concatenates elementwise with implicit expansion and converts numbers; `==` compares
  elementwise; `<missing>`; `strings`, `strlength`, `join`, `split`, `strip`, `replace`,
  `append`, `extractBefore`, `extractAfter`, and the text functions accept string arrays.
- **Struct arrays.** `s(2).f = v` grows the array; `s.f` on an array is a comma-separated
  list (`[s.f]`, `{s.f}`); `struct('a', {1, 2})` builds a 1×2 array; `numel`, `isfield`,
  `fieldnames`, `rmfield`, `arrayfun` over elements; display `1×2 struct array with fields:`.
- **Classes.** `classdef` files on the search path (the script's folder): `properties` with
  defaults and attributes, `methods` (ordinary, `Static`, constructor), single and multiple
  inheritance with `obj@Super(...)`, `< handle` reference semantics vs value semantics,
  `obj.method(...)` and `method(obj, ...)` dispatch, operator overloading through the
  documented method names (`plus`, `minus`, `times`, `mtimes`, `eq`, `lt`, `uminus`, …),
  `disp` overloading, `isa`, `class`, `properties`, `methods`, `isprop`, `ismethod`,
  `isobject`. Function files (`name.m`) on the path are callable too.
- **Date and time.** `datetime` (components, `'now'`, text, `'TimeZone'` UTC offsets,
  `'Format'`), `duration` (`years`, `days`, `hours`, `minutes`, `seconds`, `milliseconds`,
  `duration(h,m,s)`), `calendarDuration` (`calyears`, `calmonths`, `calweeks`, `caldays`,
  `between`), arithmetic and comparisons, component functions and properties (`Year`,
  `Month`, `Day`, `Hour`, `Minute`, `Second`), `NaT`, colon ranges with a duration step,
  default display formats `dd-MMM-yyyy HH:mm:ss`, `hh:mm:ss`, `1.5 hr`, `5d`.
- **Tables.** `table` (variable names from argument names or `'VariableNames'`),
  `T.Var`, `T(rows, vars)`, `T{rows, vars}`, `T.Properties.VariableNames`, adding variables,
  `height`, `width`, `size`, `head`, `tail`, `sortrows`, `table2array`, `array2table`,
  `cell2table`, `readtable`/`writetable` (CSV), vertical/horizontal concatenation, display
  with the variable-name header and underline.
- **Symbolic Math.** `syms`, `sym`, exact rational arithmetic, `subs`, `diff`, `int`
  (indefinite and definite), `simplify`, `expand`, `factor`, `collect`, `solve` (polynomial
  and linear systems), `limit`, `taylor`, `symvar`, `coeffs`, `sym2poly`, `poly2sym`, `numden`,
  `double`, `vpa`, `char`, `latex`, `pretty`, `matlabFunction`, `fplot`, symbolic matrices
  (`det`, `inv`, `*`). Output uses MATLAB's term ordering (descending degree).

## 6. Open questions / unknowns

- Exact numeric display rules for every `format` mode and for mixed-magnitude matrices
  (common scale factor `1.0e+03 *`) — approximated.
- Semantics of `regionArray` for multi-line statements and control-flow blocks — not needed
  for loading; we write a minimal `output.xml`.
- Live control execution models (`executionModel`, `executionOn`) — controls are not
  interactive in the PoC.

### Known compatibility gaps

- `int64`/`uint64` values beyond 2^53 lose precision (computed in double).
- Named IANA time zones other than `UTC`/fixed offsets, and daylight-saving rules.
- Object arrays, `set.`/`get.` property methods beyond `get.` for `Dependent` properties,
  property validation, events, enumerations, packages.
- Symbolic: integration beyond table rules, linearity, linear substitution and parts;
  `solve` beyond polynomials and linear systems; assumptions are accepted but ignored.
