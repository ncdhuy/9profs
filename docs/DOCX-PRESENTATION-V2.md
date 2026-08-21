# DOCX presentation-v2 boundary

Status: future design boundary only. No presentation-v2 implementation is authorized by this document.

Evidence base: `docs/9PROFS-ARCHITECTURE-AUDIT.md`.

## Purpose

Improve DOCX visual presentation/layout independently of the existing DOCX persistence and editing-state engine. The first implementation is a reversible seam/proof, not a new layout algorithm.

## Protected contracts

The following remain protected and are not presentation-v2 ownership:

| Protected area | Repository contract |
|---|---|
| OOXML parse | `packages/docx-engine/src/parse.ts:184-422 (parseDocx)`; block construction, raw XML fragments, `docxIndex`, original bytes, sections, styles, numbering, comments, notes, headers/footers, and unsupported content |
| OOXML patch/generate/save | `packages/docx-engine/src/patch.ts:380 (saveDocx)`, `generate.ts`, `types.ts`, `zip-load.ts`; surgical part updates and untouched-entry preservation |
| Editor/model conversion | `apps/docs/src/renderer/editor/convert.ts:62-81 (blocksToPmDoc)` and `:1114 (pmDocToSavePlan)`; model identity, anchors, SDT shells, and save plan semantics |
| Tiptap/ProseMirror state | `apps/docs/src/renderer/App.tsx:641-772`; `editor/extensions.ts:3477+`, `marks.ts`, nested editors, transactions, undo/redo, and editor positions |
| Revisions/comments | `apps/docs/src/renderer/editor/revisions.ts:574 (TrackChangesExtension)` and `comments.ts`; revision mapping, acceptance/rejection, comment ranges, and OOXML emission |
| Dirty state | `apps/docs/src/renderer/doc-dirty.ts:6 (DocDirtyState)` and `:32 (isDocDirty)`; editor and document metadata dirty flags |
| Save/reparse | `apps/docs/src/renderer/file-actions.ts:466 (buildDocBytes)`, `:662 (saveOnce)`, `save-until-persisted.ts:28/60`; snapshot, atomic save, race handling, reparse, rebase, undo/caret/scroll preservation |
| Main/preload IPC | `apps/docs/src/main/docs-main.ts:2307 (loadDocx)`, `:2927 (registerDocsIpc)`, `apps/docs/src/preload/index.ts:63`; filesystem, encryption, recovery, save, and export boundaries |

Presentation-v2 may consume these contracts as inputs. It must not redefine them.

## Presentation-v2 owns

Presentation-v2 owns only visual layout and geometry:

- layout adapter and renderer selection;
- layout representation derived from the live editor/model/DOM;
- font/style measurement inputs and line metrics;
- pagination and page/section geometry;
- page-gap visual presentation and navigation geometry;
- visual header/footer placement and reserved space;
- columns and column balancing presentation;
- float/text-box visual placement;
- line/table cut geometry;
- position-to-rectangle and rectangle-to-position mapping needed for cursor/selection display.

The current implementation points are `apps/docs/src/renderer/pagination.ts`, `line-metrics.ts`, `doc-style-css.ts`, `editor/pagination-gaps.ts`, `page-gap-nav.ts`, `column-layout.ts`, `hf-dom.ts`, and the App measurement/placement orchestration. The existing types/functions to reuse as a seam include `PageSlice`, `BlockBox`, `SectionGeom`, `computePageSlices`, `measureBlocks`, `fillLineBoxes`, `lineBreakBoundaries`, and `sectionGeoms`.

## Explicit non-ownership

Presentation-v2 does not own:

- OOXML parsing or serialization;
- block identity, `docxIndex`, raw XML, relationships, or ZIP parts;
- Tiptap schema, ProseMirror transactions, or editor commands;
- comments, revisions, fields, protection, notes, or their persistence semantics;
- dirty tracking, save ordering, save races, reparse, recovery, or Save As;
- AI tool semantics or document mutation commands;
- other Office Core products.

## Input/output seam

The initial contract should be conceptually equivalent to:

```text
PresentationInput
  = current PM/DOM view
  + BlockBox/LineBox/table measurement inputs
  + section geometry and header/footer metadata
  + style/theme/font inputs
  + current editor position/selection context

PresentationOutput
  = PageSlice[]
  + SectionGeom[]
  + line/table cut geometry
  + page-gap/column/header-footer/float decorations
  + position ↔ rectangle mapping
```

This is an adapter boundary, not a requirement to add a new document model. V2 should either produce the current pagination output types or provide a narrow adapter that makes the same output available to existing UI consumers.

## First implementation strategy: parallel V1/V2 renderer

### Feature flag

Introduce one renderer-selection feature flag at the Docs App/pagination orchestration boundary. The flag selects the current V1 pipeline or the V2 seam/proof as one presentation unit.

The flag must cover together:

1. DOM measurement and line metrics;
2. page/section/column slice calculation;
3. page-gap decorations and overlays;
4. header/footer visual placement;
5. float shifts and phantom row-span presentation;
6. pagination preview;
7. position/geometry mapping for cursor and selection.

There is no confirmed existing renderer feature flag. The flag is new wiring and must not be placed inside `packages/docx-engine`.

### V2 seam/proof scope

The initial V2 must do the smallest useful thing:

- accept the existing presentation inputs;
- return the existing page/geometry output shape;
- render one or more representative documents through the parallel path;
- expose diagnostics comparing V1 and V2 output;
- leave layout decisions equivalent to V1 or use a deliberately minimal adapter;
- prove that switching renderers does not modify the editor model, dirty state, save plan, or saved bytes.

The initial V2 must not attempt to replace `computeSectionedSlicesF2`, invent a new line-breaking algorithm, redesign the ProseMirror schema, or move persistence into the renderer.

## File ownership map

### Likely V2 files

- `apps/docs/src/renderer/pagination.ts`: adapter, page/section geometry, measurements, and output compatibility.
- `apps/docs/src/renderer/line-metrics.ts`: measurement adapter and deterministic font inputs.
- `apps/docs/src/renderer/doc-style-css.ts`: presentation CSS/input adaptation only.
- `apps/docs/src/renderer/editor/pagination-gaps.ts`: V2 page-gap decoration adapter.
- `apps/docs/src/renderer/editor/page-gap-nav.ts`: V2 position mapping/navigation adapter.
- `apps/docs/src/renderer/editor/column-layout.ts`: V2 column presentation adapter.
- `apps/docs/src/renderer/editor/hf-dom.ts`: V2 header/footer visual placement adapter.
- `apps/docs/src/renderer/App.tsx`: renderer selection and orchestration only.
- `apps/docs/tests/*`: parity, fixture, geometry, cursor/selection, and feature-flag tests.

### Read-only for the first V2 proof

- `packages/docx-engine/src/parse.ts`
- `packages/docx-engine/src/patch.ts`
- `packages/docx-engine/src/generate.ts`
- `packages/docx-engine/src/types.ts`
- `packages/docx-engine/src/zip-load.ts`
- `packages/docx-engine/src/section.ts`
- `packages/docx-engine/src/theme.ts`
- `apps/docs/src/renderer/editor/convert.ts`
- `apps/docs/src/renderer/editor/extensions.ts`
- `apps/docs/src/renderer/editor/marks.ts`
- `apps/docs/src/renderer/editor/revisions.ts`
- `apps/docs/src/renderer/editor/comments.ts`
- `apps/docs/src/renderer/doc-dirty.ts`
- `apps/docs/src/renderer/file-actions.ts` save/reparse logic
- `apps/docs/src/preload/index.ts`
- `apps/docs/src/main/docs-main.ts` open/save IPC

“Read-only” means no semantic changes in the first presentation experiment. An additive adapter may later require a separately reviewed contract change.

## Invariants

Every V1/V2 path must preserve:

1. The same ProseMirror document and JSON for the same user/AI edit.
2. The same `pmDocToSavePlan` result for the same editor state.
3. The same `DocDirtyState` result for the same editor/document metadata.
4. The same `saveDocx` inputs and round-trip behavior.
5. `docxIndex`, raw XML, SDT shells, relationships, media, headers/footers, comments, revisions, styles, numbering, notes, and unsupported-content preservation.
6. Editor positions, selections, revision/comment anchors, and nested text-box state.
7. Save race behavior: edits arriving during `saveOnce` remain live and are not lost.
8. AI behavior: tools call existing document/editor commands and never mutate presentation DOM as a persistence shortcut.

## Proof and acceptance gates

Before V2 becomes the default, compare V1/V2 on the existing corpus, including:

- `apps/docs/tests/pagination-corpus/docx/*` English, CJK/doc-grid, mixed language, sections, columns, breaks, footnotes, headings/keep-next, tables, repeated headers, and oversized tables;
- headers/footers and variants;
- nested tables, drawings/floats, text boxes, comments, revisions, fields, protection, and unsupported-content fixtures;
- missing/fallback fonts, CJK, RTL/complex scripts, and mixed runs.

Required checks:

- page count, page/section geometry, line breaks, table row cuts, column placement, header/footer placement, and float placement;
- position/rectangle mapping, cursor crossing page gaps, selection, comment/revision anchors, and nested-editor coordinates;
- dirty state before/after renderer switching;
- `pmDocToSavePlan` equality;
- save → reparse → editor reconciliation;
- unchanged OOXML part/relationship/media preservation;
- print/export and pagination preview behavior.

Use the existing Word/LibreOffice references and tools (`apps/docs/tests/pagination-corpus/baseline-word.json`, `baseline-lo.json`, `scripts/pagination-baseline*.mjs`, `scripts/docs-word-fidelity.mjs`, `tools/fidelity-compare.mjs`) as external references. They do not replace V1/V2 parity tests.

## AI and external references

AI continues to use the existing Docs protocol and tools: `apps/docs/src/renderer/ai/protocol.ts`, `tools.ts`, `docs-skill.ts`, and `transport.ts`. Presentation-v2 must not expose DOM mutation as an AI editing API.

SuperDoc is an architecture/design reference only by default. Do not copy AGPL source into 9Profs; any direct code reuse requires separately approved commercial/license review.

Casual Docs is an architecture and implementation reference. Its implementation may be studied, adapted, or ported only when the relevant source/license permits it. Preserve required copyright notices, attribution, and license obligations. Do not blindly copy whole subsystems; prefer adapting narrow presentation components to GenOffice contracts.

GenOffice remains the primary implementation base. Casual Docs and SuperDoc must not replace `packages/docx-engine`, Tiptap/ProseMirror document state, save/round-trip, comments/revisions, or dirty tracking.

## Stop conditions

Pause the V2 work if any of the following occurs:

- a proposed change requires modifying `docx-engine` persistence to make the renderer work;
- the editor schema or save plan must change before the seam is proven;
- V1/V2 parity cannot be measured deterministically;
- a renderer change alters saved OOXML without an explicitly approved persistence task;
- the change begins touching Sheets, Slides, PDF, Markdown, shell, dependencies, or unrelated refactors.
