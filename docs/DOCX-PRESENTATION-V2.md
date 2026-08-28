# DOCX presentation-v2 boundary

Status: implemented experimental renderer boundary. V1 remains the default
production path. V2 is available through an internal/test selector and is not a
user-facing replacement or a new document model.

Scope: focused DOCX presentation reference. This document is not the canonical
cross-product architecture; use [9PROFS-ARCHITECTURE.md](9PROFS-ARCHITECTURE.md)
for current 9Profs component ownership, authority boundaries, and roadmap.

This document records current source status and protected contracts. The
canonical cross-product architecture is [9PROFS-ARCHITECTURE.md](9PROFS-ARCHITECTURE.md).

## Current implementation status

| Capability                                 | Status                   | Source evidence                                                                                                                                                                                                                                     |
| ------------------------------------------ | ------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Renderer selection                         | Implemented experimental | `apps/docs/src/renderer/presentation-v2/index.ts` defines `PresentationRenderer`, defaults to `v1`, and resolves an internal global/query override.                                                                                                 |
| V1 pagination                              | Fallback/legacy baseline | `renderPresentationV1` uses existing `sliceWithLineSplit` and current `apps/docs/src/renderer/pagination.ts` behavior.                                                                                                                              |
| V2 page flow                               | Implemented experimental | `presentation-v2/page-slicer.ts` performs V2 orchestration, bounded refinement, section normalization, and performance recording while reusing GenOffice `BlockBox`, `PageSlice`, and `computeSectionedSlicesF2` primitives.                        |
| V2 measurement/invalidation                | Implemented experimental | `measurement.ts`, `measurement-context.ts`, and `measurement-invalidation.ts` provide refinement windows, font/zoom invalidation, transaction classification, and conservative fallback.                                                            |
| V2 geometry and post-render readback       | Implemented experimental | `geometry.ts`, `geometry-probes.ts`, `post-render.ts`, and App `__pageDebug` capture page, gap, header/footer, float, caret, selection, and position geometry.                                                                                      |
| Parity diagnostics                         | Implemented experimental | `diagnostics.ts` normalizes and compares presentation/model values with explicit geometry tolerance and diagnostic categories.                                                                                                                      |
| Automated proof                            | Partial                  | V2 unit tests, geometry tests, Docs fixture tests, and `e2e/docs-presentation-parity.spec.ts` cover selection, sections, measurement, dirty/save/reopen preservation, and browser geometry. This is not a claim that V2 is ready to become default. |
| Independent replacement of all V1 behavior | Future                   | Requires broader fidelity proof and a deliberate decision to replace individual components. Keep V1 available.                                                                                                                                      |

V2 is therefore more than a design document and more than a delegated page
array. It has its own orchestration and diagnostics, but it remains compatible
with and dependent on established GenOffice presentation primitives. It does
not own persistence or editor state.

## Protected contracts

| Protected area            | Repository contract                                                                                                                                                                         |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| OOXML parse               | `packages/docx-engine/src/parse.ts`, `types.ts`, block construction, raw XML fragments, `docxIndex`, sections, styles, numbering, comments, notes, headers/footers, and unsupported content |
| OOXML patch/generate/save | `packages/docx-engine/src/patch.ts`, `generate.ts`, `zip-load.ts`; surgical part updates and untouched-entry preservation                                                                   |
| Editor/model conversion   | `apps/docs/src/renderer/editor/convert.ts`; `blocksToPmDoc`, `pmDocToSavePlan`, model identity, anchors, SDT shells, and save-plan semantics                                                |
| Tiptap/ProseMirror state  | Docs `App.tsx` and `editor/*`; schema, transactions, nested editors, undo/redo, selections, and editor positions                                                                            |
| Revisions/comments        | Docs `editor/revisions.ts` and `comments.ts`; revision mapping, comment ranges, and OOXML emission                                                                                          |
| Dirty state               | `apps/docs/src/renderer/doc-dirty.ts`; editor and document metadata dirty flags                                                                                                             |
| Save/reparse              | `apps/docs/src/renderer/file-actions.ts`, `save-until-persisted.ts`; snapshot, atomic save, race handling, reparse, recovery, undo/caret/scroll preservation                                |
| Main/preload IPC          | `apps/docs/src/main/docs-main.ts` and `apps/docs/src/preload/index.ts`; filesystem, encryption, recovery, save, and export boundaries                                                       |

Presentation V2 may consume these contracts as inputs. It must not redefine or
persist them.

## Presentation ownership

V2 owns derived visual layout and geometry:

- renderer selection and presentation input normalization;
- font/style measurement inputs and line metrics;
- pagination and page/section/column geometry;
- page-gap visual presentation and navigation geometry;
- header/footer visual placement and reserved space;
- float/text-box placement and line/table cut geometry;
- position-to-rectangle and rectangle-to-position diagnostics for caret and
  selection display.

The current V2 boundary is `apps/docs/src/renderer/presentation-v2/*`, with
App measurement/placement orchestration and existing consumers in
`pagination.ts`, `line-metrics.ts`, `doc-style-css.ts`,
`editor/pagination-gaps.ts`, `page-gap-nav.ts`, `column-layout.ts`, and
`editor/hf-dom.ts`.

V2 does not own:

- OOXML parsing or serialization, block identity, `docxIndex`, raw XML,
  relationships, ZIP parts, or save bytes;
- Tiptap schema, ProseMirror transactions, editor commands, comments,
  revisions, fields, protection, notes, or their persistence semantics;
- dirty tracking, save ordering, save races, reparse, recovery, or Save As;
- AI tool semantics, document mutation commands, or external tool writes;
- Sheets, Slides, PDF, Markdown, or shell persistence.

## Runtime seam

```text
live PM/DOM/document metadata
        |
        v
presentation-v2 renderer selector
        |-----------------------|
       V1                      V2
 current pagination       bounded V2 refinement
        |                      |
        +------ PageSlice[] / geometry consumers
                               |
                               v
                     derived Docs presentation

unchanged PM/editor state -> pmDocToSavePlan -> docx-engine save/reparse
```

The current selector is an internal development/test hook. It is not a product
setting and must remain outside `packages/docx-engine`.

## Required proof before V2 becomes default

Keep V1 available until representative fixtures prove all of the following:

- normalized pages, sections, columns, line cuts, and table cuts;
- page gaps, floats, header/footer placement, and reserved space;
- position mapping, caret, selection, and page-boundary hit testing;
- ProseMirror JSON, dirty state, `pmDocToSavePlan`, saved bytes, and save/reopen;
- comments, revisions, anchors, notes, unsupported-content preservation, and
  Word/LibreOffice comparison where applicable;
- deterministic behavior across font/zoom/measurement environments and long
  documents.

Existing proof surfaces include:

- `apps/docs/tests/presentation-v2-seam.test.ts`;
- `presentation-v2-sections.test.ts`, `presentation-v2-measurement.test.ts`,
  `presentation-v2-dirty-range.test.ts`, and `presentation-v2-diagnostics.test.ts`;
- `presentation-geometry.test.ts` and `presentation-geometry-probes.test.ts`;
- `e2e/docs-presentation-parity.spec.ts` and the Docs pagination corpus.

## AI and external references

Docs AI continues to use `apps/docs/src/renderer/ai/protocol.ts`, `tools.ts`,
`docs-skill.ts`, and `transport.ts`. V2 must never expose presentation DOM
mutation as an AI editing API.

Casual Docs and SuperDoc remain architecture/design references only. Do not
copy their source into 9Profs. Any future adaptation requires separate source,
license, and compatibility review.

## Stop conditions

Pause V2 expansion if a proposed change requires modifying
`packages/docx-engine` persistence, a second document/editor model, direct DOM
mutation for AI, a competing active-file writer, or parity proof that cannot be
made deterministic.
