# DOCX presentation-v2 reference map

Audit date: 2026-08-21

This is a source audit, not an implementation proposal to replace GenOffice. The
external snapshots reviewed were the `main` trees of Casual Docs at
`d11605185698cfc4b16a83a975cfecc8056ac348` and SuperDoc at
`b0ff2221645f79b7094e1c037723fe2a435ffd3c`. Paths below are representative
source anchors, not claims that the repositories are otherwise API-compatible.

## 1. Executive conclusion

The existing 9Profs seam is directionally correct and should remain at the Docs
App/pagination orchestration boundary:

```text
live PM/DOM-derived presentation inputs
        -> presentation-v2 renderer selection
        -> existing PageSlice[] consumer contract
        -> Docs page preview/editor presentation
```

The current seam is a valid first proof because V1 remains the default and V2
delegates to the current `sliceWithLineSplit` behavior. It does not touch
ProseMirror state, `blocksToPmDoc`, `pmDocToSavePlan`, dirty tracking, save,
reparse, or the protected DOCX engine.

The important limitation is equally clear: the current code seam is presently
a page-slice selection seam, not yet the complete presentation boundary described
in the architecture documents. Its `PresentationInput`/`PageSlice[]` contract
does not yet own every side effect listed in the architecture target, including
all line/table readback, page-gap and float decorations, header/footer overlays,
or position-to-rectangle mapping. That is not a blocker for the reversible proof,
but it must be closed before V2 introduces different layout decisions.

Recommended direction:

- Keep GenOffice’s Tiptap/ProseMirror state, DOCX parse/patch/generate path,
  current DOM measurement, pagination rules, and presentation consumers.
- Adapt the seam around existing `BlockBox`, `PageSlice`, `SectionGeom`, line
  geometry, table cut geometry, and page-gap/position mapping contracts.
- Learn from Casual Docs’ explicit PM-to-layout projection, framework-neutral
  layout/painter split, coordinator state, and visual corpus. Do not replace
  GenOffice’s protected PM/document model with Casual’s `FlowBlock` model.
- Learn from SuperDoc’s explicit layout contracts, neutral geometry readback,
  measurement-cache invalidation, deterministic measurement mode, and incremental
  checkpoint/dependency design. Reproduce those ideas independently; do not copy
  SuperDoc source.
- Do not port a complete paginator, painter, or editor subsystem at this stage.

No component is recommended for `PORT_FROM_CASUAL` in this audit. The highest-
value future work is a narrow parity/diagnostics harness that captures the full
presentation result and proves mapping and side-effect equivalence while V2 still
delegates to V1.

## 2. Architecture comparison

### GenOffice / 9Profs

GenOffice keeps the document truth in Tiptap/ProseMirror and the DOCX truth in
the protected engine. Presentation is derived from live editor DOM and document
metadata. The main flow is:

```text
DOCX parse -> blocksToPmDoc -> Tiptap/ProseMirror
                                  |
                                  v
                    DOM measurement + section/style inputs
                                  |
                                  v
       BlockBox / LineBox / table geometry -> PageSlice[] / SectionGeom[]
                                  |
                                  v
              page gaps, columns, header/footer, cursor mapping, painter
```

Primary files are [pagination.ts](../apps/docs/src/renderer/pagination.ts),
[line-metrics.ts](../apps/docs/src/renderer/line-metrics.ts),
[doc-style-css.ts](../apps/docs/src/renderer/doc-style-css.ts),
[pagination-gaps.ts](../apps/docs/src/renderer/editor/pagination-gaps.ts),
[page-gap-nav.ts](../apps/docs/src/renderer/editor/page-gap-nav.ts),
[column-layout.ts](../apps/docs/src/renderer/editor/column-layout.ts),
[hf-dom.ts](../apps/docs/src/renderer/editor/hf-dom.ts),
[App.tsx](../apps/docs/src/renderer/App.tsx), and
[PaginationPreview.tsx](../apps/docs/src/renderer/components/PaginationPreview.tsx).

Strengths are preservation of existing DOCX identity, PM positions, comments,
revisions, save/reparse behavior, and a large set of already-tested pagination
rules. The weakness is that browser measurement, pagination, visual overlays,
and cursor mapping are distributed across several modules, so a renderer flag
must eventually cover all of those effects as one presentation unit.

### Casual Docs

Casual uses a deliberately visible pipeline:

```text
ProseMirror document -> toFlowBlocks() -> FlowBlock[]
                                      |
                                      v
                           measures -> layoutDocument()
                                      |
                                      v
                          Layout/Page/Fragment objects
                                      |
                                      v
                          layout-painter -> positioned DOM
                                      |
                                      v
                            selection/caret overlays
```

The key source anchors are
[toFlowBlocks.ts](https://github.com/CasualOffice/docs/blob/main/docx-editor/packages/core/src/layout-bridge/toFlowBlocks.ts),
[layout-engine/index.ts](https://github.com/CasualOffice/docs/blob/main/docx-editor/packages/core/src/layout-engine/index.ts),
[layout-engine/types.ts](https://github.com/CasualOffice/docs/blob/main/docx-editor/packages/core/src/layout-engine/types.ts),
[layout-painter/index.ts](https://github.com/CasualOffice/docs/blob/main/docx-editor/packages/core/src/layout-painter/index.ts),
[LayoutCoordinator.ts](https://github.com/CasualOffice/docs/blob/main/docx-editor/packages/core/src/managers/LayoutCoordinator.ts),
and [PagedEditor.tsx](https://github.com/CasualOffice/docs/blob/main/docx-editor/packages/react/src/paged-editor/PagedEditor.tsx).

Strengths are explicit intermediate data, framework-neutral layout/painter
boundaries, PM ranges carried alongside fragments, and a visible test corpus for
pagination and visual regression. Weaknesses are that it is a separate editor
and document representation, its layout API is marked experimental, and its
font strategy leans on bundled metric-compatible fonts rather than being a drop-
in match for GenOffice’s existing font/OXML behavior. Casual’s architecture is
useful as a reference; its model must not replace GenOffice’s protected model.

### SuperDoc

SuperDoc separates layout contracts, DOM measurement, layout execution, and the
editor-facing bridge more aggressively:

```text
editor/document inputs -> @superdoc/contracts
                              |
                              v
             DOM measuring + font/text/table caches
                              |
                              v
             layout engine + paginator + execution phases
                              |
                              v
             layout bridge: PM ranges, neutral geometry, page geometry
                              |
                              v
                 editor/document API selection and rendering
```

The current source anchors are
[layout-engine/index.ts](https://github.com/superdoc/docx-editor/blob/main/packages/layout-engine/layout-engine/src/index.ts),
[measuring/dom/index.ts](https://github.com/superdoc/docx-editor/blob/main/packages/layout-engine/measuring/dom/src/index.ts),
[layout-bridge/index.ts](https://github.com/superdoc/docx-editor/blob/main/packages/layout-engine/layout-bridge/src/index.ts),
[neutral-segment-geometry.ts](https://github.com/superdoc/docx-editor/blob/main/packages/layout-engine/layout-bridge/src/neutral-segment-geometry.ts),
[page-geometry-helper.ts](https://github.com/superdoc/docx-editor/blob/main/packages/layout-engine/layout-bridge/src/page-geometry-helper.ts),
and [incremental-dependency.ts](https://github.com/superdoc/docx-editor/blob/main/packages/layout-engine/contracts/src/incremental-dependency.ts).

Strengths are explicit contracts, validation, cache invalidation, range-limited
layout/checkpoints, and geometry designed for selection consumers. Weaknesses are
substantial complexity, an evolving internal API, and source/license constraints.
SuperDoc is an architectural reference only here. No AGPL source or algorithm
implementation should be copied into 9Profs.

## 3. Component-by-component matrix

The seam-fit column answers whether the reference is useful without changing the
current V1/V2 boundary. Actions are intentionally conservative.

| Area | GenOffice current implementation | Casual Docs source | SuperDoc source | Architecture, strengths, weaknesses, and seam fit | Recommended action |
|---|---|---|---|---|---|
| 1. Presentation adapter / PM projection | `editor/convert.ts` owns `blocksToPmDoc` and `pmDocToSavePlan`; `pagination.ts` derives `BlockBox` from live DOM. | `core/src/layout-bridge/toFlowBlocks.ts`; PM nodes become `FlowBlock[]` with `pmStart`/`pmEnd`, runs, tables, drawings, and text boxes. | `layout-engine/layout-bridge/src/index.ts`; contracts and bridge helpers carry PM ranges into layout geometry. | Casual makes the projection boundary easy to see. SuperDoc makes the bridge contract explicit. Both are compatible as derived presentation adapters, not as replacements for GenOffice conversion or save contracts. | `KEEP_GENOFFICE` |
| 2. Intermediate layout representation | `BlockBox`, line boxes, table row/cell cuts, `PageSlice`, `SectionGeom`, and metadata in `pagination.ts`. | `FlowBlock`, `Measure`, `Layout`, `Page`, `Fragment` in `layout-engine/types.ts`. | `FlowBlock`, `Measure`, `Layout`, `Fragment`, and engine contracts in `contracts/src/index.ts`. | Existing GenOffice types already match the required seam. Casual and SuperDoc show the value of separating derived inputs from positioned output, but adding a second document model would increase conversion and identity risk. | `ADAPT_GENOFFICE` |
| 3. Style resolution | `doc-style-css.ts`, generated document CSS, and style data consumed by DOM measurement. | Style/mark extraction in `toFlowBlocks.ts`, style cascade tests, and `packages/react/src/styles/editor.css`. | Paragraph/style contracts in `contracts/src/engines/paragraph.ts` and shared computed-style inputs. | Casual’s explicit style cascade is useful for tracing; SuperDoc’s contract approach is useful for deterministic inputs. GenOffice must keep its DOCX style/theme and CSS realization. | `KEEP_GENOFFICE` |
| 4. Font measurement and fallback | `line-metrics.ts`; `packages/font-metrics/src/metrics.ts` and `font-locate.ts` provide font lookup/coverage and vertical metrics. | `styles/fonts.css` bundles metric-compatible fonts; `styles/fonts/LICENSE` identifies SIL OFL font licensing. No equivalent broad fallback subsystem was established in the reviewed core sources. | `measuring/dom/fontMetricsCache.ts`, `clearTextMeasurementCaches()`, and DOM measurement configuration address font-dependent metrics and fallback invalidation. | GenOffice already has a font boundary. SuperDoc’s explicit cache clearing after font load is a valuable design lesson; Casual’s bundled fonts may improve deterministic tests but are a separate asset/license decision. | `LEARN_FROM_SUPERDOC` |
| 5. Text measurement | `line-metrics.ts` computes line height, line metrics, simulated lines, header/footer and footnote estimates; browser DOM remains authoritative for rendered blocks. | Layout measurement is consumed by `layout-engine` and painter; `PagedEditor.tsx` runs the measure/layout pipeline. | `measuring/dom/index.ts` uses Canvas text measurement, greedy width-constrained breaks, typography metrics, and a documented deterministic/browser mode. | SuperDoc exposes measurement policy and cache invalidation more clearly. GenOffice’s current behavior is the compatibility baseline; changing measurement changes pagination. | `KEEP_GENOFFICE` |
| 6. Line breaking | `pagination.ts` `fillLineBoxes`, `lineBreakBoundaries`, and `sliceWithLineSplit`; `line-metrics.ts` covers CJK/doc-grid/line-height concerns. | Layout engine and e2e tests include kinsoku/line-breaking coverage; `layout-engine/index.ts` consumes measured lines. | `measuring/dom/index.ts` and `cjk-line-break.ts` implement greedy breaks and CJK boundary rules. | All three use measurement-driven line boundaries. This is a high-risk compatibility surface and explicitly outside the first V2 task. | `KEEP_GENOFFICE` |
| 7. Paragraph layout | `pagination.ts` places paragraph blocks and applies keep-next, widow/orphan, spacing, and line cuts. | `layout-engine/index.ts`, `keep-together.ts`, and `layout-painter/renderParagraph.ts`. | `layout-paragraph.ts`, `paginator.ts`, and keep-next preflight in `layout-engine/index.ts`. | Casual and SuperDoc provide useful isolated rule modules, but GenOffice already encodes representative DOCX behavior and has parity tests. | `KEEP_GENOFFICE` |
| 8. Pagination | `computeSectionedSlicesF2`, `sliceWithLineSplit`, page/column placement, forced breaks, balancing, tables, floats, and section transitions in `pagination.ts`. | `paginator.ts`, `layout-engine/index.ts`, `section-breaks.ts`, `keep-together.ts`. | `paginator.ts`, `layout-engine/index.ts`, execution phases, range/continuation checkpoints. | SuperDoc’s continuation state is architecturally interesting; Casual’s paginator is readable. Neither justifies replacing the current GenOffice algorithm during a seam proof. | `KEEP_GENOFFICE` |
| 9. Sections and margins | `sectionGeoms` in `pagination.ts`; section properties and header/footer metadata come from existing DOCX/editor inputs. | `SectionLayoutConfig` and `section-breaks.ts` normalize page size, margins, columns, and header/footer references. | `document-api/sections/sections.ts`, contracts section types, and active page/column state in `layout-engine/index.ts`. | This is the strongest fit for an adapter because `SectionGeom` already exists. Normalize inputs only at the seam; do not move section persistence or OOXML ownership. | `ADAPT_GENOFFICE` |
| 10. Multi-column layout | `pagination.ts` handles columns/balancing; `editor/column-layout.ts` renders column presentation. | `ColumnLayout` in layout types, paginator state, and section-break handling. | `ColumnLayout`, `ColumnRegion`, and paginator `columnIndex`/continuation state. | Reference designs confirm that columns belong in page-flow geometry plus a presentation decorator. Keep GenOffice’s tested behavior and route all column side effects through the eventual renderer contract. | `KEEP_GENOFFICE` |
| 11. Tables and row splitting | `pagination.ts` `tableRowFlags`, `tableHeaderFlags`, `domTableRows`, `cellCutYs`, line/table cut geometry, and `_placeTable`. | `layout-engine/types.ts`, `renderPage.ts`, integration/table tests, and table width utilities. | `layout-table.ts`, contracts table types, row/column boundaries, and table measurement helpers. | Tables are a high-risk identity and geometry surface. Casual and SuperDoc demonstrate isolated table representations, but current GenOffice cut geometry is already the right seam input/output. | `KEEP_GENOFFICE` |
| 12. Headers and footers | `line-metrics.ts` estimates reserve height; `editor/hf-dom.ts` and section geometry render overlays; OOXML header/footer parts remain protected. | Header/footer references are projected in `toFlowBlocks.ts`; `renderPage.ts` and `PagedEditor.tsx` place page content; `EndnoteSection.tsx` handles endnotes separately. | `normalize-header-footer-fragments.ts`, header/footer contracts, and section resolution. | SuperDoc’s normalization is a useful ownership pattern. GenOffice must retain raw header/footer identity and variant semantics; current seam does not yet carry all overlay outputs. | `KEEP_GENOFFICE` |
| 13. Footnotes and endnotes | `line-metrics.ts` has `estimateFootnoteHeight`; notes are part of protected DOCX/editor semantics and current presentation support. | `EndnoteSection.tsx`; footnote/endnote behavior is also covered by the paged editor and core layout types. | `footnote-anchor-index.ts`, footnote preflight/execution phases, and layout contracts. | SuperDoc exposes the dependency between note demand and page layout most clearly. This is too much new policy for the first seam and requires fixture proof before any change. | `DEFER` |
| 14. Floating images, text boxes, anchored graphics | `pagination.ts` measures float exclusions/anchors; `pagination-gaps.ts`, `protected-render.ts`, and editor extensions provide visual handling. | `toFlowBlocks.ts`, `imageLayout.ts`, `renderImage.ts`, `renderPage.ts`, and text-box block types. | `layout-image.ts`, `layout-drawing.ts`, `layout-textbox.ts`, and anchored/header/footer normalization. | Casual and SuperDoc both isolate floating geometry from flow layout. That is a useful future split, but changing anchor behavior is a major fidelity risk and is not required to prove V2 selection. | `DEFER` |
| 15. Page painter / DOM rendering | `App.tsx`, `PaginationPreview.tsx`, editor DOM, `protected-render.ts`, page-gap and header/footer DOM extensions. | `layout-painter/index.ts`, `renderPage.ts`, `renderFragment.ts`, `renderParagraph.ts`; absolute-positioned fragments and reconciliation. | Layout engine output is consumed through SuperDoc’s editor/rendering layers; reviewed sources emphasize layout/bridge contracts rather than a drop-in painter. | Casual’s painter split is the clearest architectural lesson. The existing seam must eventually cover painter-side effects, but a second painter is not justified while V2 delegates. | `LEARN_FROM_CASUAL` |
| 16. Position ↔ geometry mapping | `pagination.ts` page lookup/start anchors and line/table geometry; `page-gap-nav.ts` and pagination gaps handle visual crossing; PM positions remain editor-owned. | `toFlowBlocks.ts` carries `pmStart`/`pmEnd`; `PagedEditor.tsx` computes selection rectangles/caret from layout and DOM hit testing. | `neutral-segment-geometry.ts`, `page-geometry-helper.ts`, `pm-position-validator.ts`, and layout bridge helpers. | SuperDoc’s neutral per-segment readback is the highest-value mapping lesson. The current V2 seam does not yet expose this as a single output, so parity is incomplete beyond page slices. | `LEARN_FROM_SUPERDOC` |
| 17. Caret and selection | Tiptap/ProseMirror selection is protected; page-gap navigation and editor DOM map visual gaps without changing model semantics. | Hidden PM editor plus selection/caret overlay in `PagedEditor.tsx` and `LayoutCoordinator.ts`. | `document-api/selection/selection.ts` plus layout-bridge geometry and validation. | All references keep selection separate from layout state. SuperDoc’s neutral geometry is useful, but GenOffice selection semantics must remain unchanged. | `KEEP_GENOFFICE` |
| 18. Incremental relayout / invalidation | App remeasurement/pagination effects and presentation extensions remeasure/reapply gaps; there is no single explicit dependency graph at the current seam. | `LayoutCoordinator.ts`, `PagedEditor.tsx`, and incremental scroll/layout coordination. | `incremental-dependency.ts`, measurement caches, continuation checkpoints, and explicit execution phases. | SuperDoc gives the clearest model for future invalidation boundaries. First prove whole-document V1/V2 equivalence; do not introduce partial relayout during the seam proof. | `LEARN_FROM_SUPERDOC` |
| 19. Long-document performance | Current pagination is DOM-driven and broad; `pageAt`/page-start helpers reduce lookup work, but no new virtualized V2 path is proven. | `layout-perf-benchmark.spec.ts`, incremental layout handling in `PagedEditor.tsx`, and painter reconciliation. | `reflow-baseline.test.ts`, range/overscan layout, checkpoints, and page continuation state. | SuperDoc’s range/checkpoint approach is a useful future benchmark target. It is not evidence that GenOffice should adopt virtualization before preserving fidelity. | `LEARN_FROM_SUPERDOC` |
| 20. Visual regression / fidelity testing | Existing Docs pagination/parity/gap tests and DOCX fixture tests cover geometry and preservation; the V2 seam test proves delegated output parity. | e2e `visual-regression.spec.ts` snapshots plus section pagination, kinsoku, table, float, and performance fixtures. | layout-bridge visual/line-cache tests, engine tests, and performance baselines. | Casual has the most directly reusable testing pattern: fixture-driven screenshots plus focused feature fixtures. Add equivalent tests around GenOffice’s existing contracts; do not copy the test harness wholesale. | `LEARN_FROM_CASUAL` |

## 4. GenOffice → Casual → SuperDoc source map

| 9Profs presentation concern | GenOffice source of truth | Casual reference | SuperDoc reference | What the comparison says |
|---|---|---|---|---|
| PM-to-presentation projection | `apps/docs/src/renderer/editor/convert.ts`, `pagination.ts` | `core/src/layout-bridge/toFlowBlocks.ts` | `layout-engine/layout-bridge/src/index.ts` | Derive a presentation view from live PM state; do not create a second persistence model. |
| Layout input/output contracts | `pagination.ts`: `BlockBox`, `PageSlice`, `SectionGeom` | `layout-engine/types.ts` | `contracts/src/index.ts` | Existing GenOffice types are sufficient for the first seam. |
| Page flow | `pagination.ts`: `computeSectionedSlicesF2`, `sliceWithLineSplit` | `layout-engine/index.ts`, `paginator.ts` | `layout-engine/index.ts`, `paginator.ts` | Keep the GenOffice algorithm until an independent replacement has proof. |
| Measurement | `line-metrics.ts`, `packages/font-metrics` | `PagedEditor.tsx` measure/layout pipeline, bundled fonts | `measuring/dom/index.ts`, `fontMetricsCache.ts`, `measurementCache.ts` | Make font state and cache invalidation explicit before changing measurement. |
| Style realization | `doc-style-css.ts`, generated styles | `toFlowBlocks.ts`, `packages/react/src/styles/editor.css` | paragraph/style contracts | Keep GenOffice’s DOCX style/theme inputs and CSS behavior. |
| Sections/columns | `pagination.ts`, `editor/column-layout.ts` | `section-breaks.ts`, `SectionLayoutConfig` | section API, `ColumnLayout`, paginator state | `SectionGeom` is the safe normalization point. |
| Tables | `pagination.ts` table flags/cuts | layout types, `renderPage.ts`, table tests | `layout-table.ts`, table contracts | Keep current row/cell cut behavior. |
| Header/footer | `line-metrics.ts`, `editor/hf-dom.ts`, protected DOCX metadata | `toFlowBlocks.ts`, `renderPage.ts` | `normalize-header-footer-fragments.ts` | Normalize visual placement only; retain OOXML identity. |
| Floating objects | `pagination.ts`, `pagination-gaps.ts`, `protected-render.ts` | `imageLayout.ts`, `renderImage.ts`, text-box blocks | `layout-image.ts`, `layout-drawing.ts`, `layout-textbox.ts` | Defer until the seam includes all anchor effects. |
| Position mapping | `pagination.ts`, `page-gap-nav.ts` | PM ranges and PagedEditor selection overlay | neutral segment/page geometry and validator | Make mapping a measured output, not a ratio-based afterthought. |
| Incremental layout | App effects and presentation extensions | `LayoutCoordinator.ts`, PagedEditor coordination | dependency classes, caches, checkpoints | Benchmark first; implement later behind the same seam. |
| Fidelity tests | Docs pagination/parity/save fixtures | visual regression snapshots and e2e feature corpus | engine/bridge/perf tests | Add a focused GenOffice fixture harness, not a new renderer yet. |

## 5. Recommended target architecture for 9Profs presentation-v2

Keep the existing seam and evolve it in place:

```text
App measurement orchestration
        |
        | existing BlockBox / SectionGeom / DOM-derived inputs
        v
presentation-v2/renderer selection
        |--------------------|
        |                    |
       V1                   V2
 current pagination       adapter/proof
        |                    |
        | same PageSlice / geometry compatibility layer
        |--------------------|
                 v
    shared page-gap, column, header/footer, float,
    painter, caret, selection consumers
                 |
                 v
       unchanged PM/editor/save pipeline
```

The next contract should be an additive internal presentation contract, not a
new document model:

```text
PresentationInput
  = current BlockBox / DOM measurement inputs
  + SectionGeom and style/font inputs
  + line/table cut inputs
  + editor position context

PresentationOutput
  = PageSlice[] / SectionGeom[]
  + line/table cut geometry
  + page-gap/column/header-footer/float decorations
  + position <-> rectangle mapping
```

V1 and V2 should both be able to produce this shape. During the proof phase, V2
may simply return the V1 result and diagnostics may compare normalized output.
Persistence must remain a parallel consumer of PM state and document metadata,
never a consumer of page slices.

The current selection switch is suitable for development/tests because V1 is the
default and V2 is selected internally through the renderer resolver/global test
hook. It should remain out of end-user settings and out of the DOCX engine.

## 6. Exact components to keep

Keep these as implementation foundations and compatibility baselines:

- `packages/docx-engine/**`, including parse, patch, generate, raw XML, anchors,
  `docxIndex`, headers/footers, notes, comments, revisions, and round-trip data.
- `apps/docs/src/renderer/editor/convert.ts`, including `blocksToPmDoc` and
  `pmDocToSavePlan`.
- Tiptap/ProseMirror schema, transactions, editor positions, revisions, comments,
  nested editors, undo/redo, and AI document commands.
- `doc-dirty.ts`, `file-actions.ts`, save serialization, preload/main save IPC,
  save/reparse, and caret/selection preservation.
- `pagination.ts` as the V1 geometry/pagination baseline, especially
  `BlockBox`, `PageSlice`, `SectionGeom`, line/table cut geometry, section flow,
  column behavior, keep-next/widow/orphan behavior, and page lookup helpers.
- `line-metrics.ts`, `doc-style-css.ts`, `pagination-gaps.ts`, `page-gap-nav.ts`,
  `column-layout.ts`, `hf-dom.ts`, and `protected-render.ts` as the current
  presentation behavior and consumer contracts.
- Existing Docs fixture and parity tests. New tests should wrap these contracts,
  not bypass them with a second editor model.

## 7. Exact components worth adapting from Casual Docs

No direct code port is recommended now. The highest-value adaptations are ideas
that can be implemented against GenOffice contracts after license review:

1. **Projection shape:** use the pattern in
   `core/src/layout-bridge/toFlowBlocks.ts`—carry stable PM ranges and derived
   formatting/anchor metadata alongside presentation blocks—without replacing
   `blocksToPmDoc` or changing save mapping.
2. **Framework-neutral coordinator:** use the state separation in
   `core/src/managers/LayoutCoordinator.ts` as a reference for keeping layout
   snapshots, selection rectangles, caret geometry, and interaction state out of
   persistence/editor ownership.
3. **Painter boundary:** use
   `layout-painter/index.ts`/`renderPage.ts` as a reference for treating page
   painting and reconciliation as consumers of positioned fragments. Apply only
   if the existing Docs painter needs a proven boundary; do not add a second DOM
   renderer for the seam proof.
4. **Fidelity corpus:** adapt the testing strategy represented by
   `visual-regression.spec.ts`, `issue-319-section-pagination.spec.ts`,
   `kinsoku-line-breaking.spec.ts`, `tables.spec.ts`, and
   `layout-perf-benchmark.spec.ts` to existing GenOffice fixtures and output
   contracts.
5. **Font determinism as an option:** Casual’s self-hosted metric-compatible
   fonts are useful for deterministic tests, but are separate assets with their
   own licenses and should not be introduced as a dependency or default font
   change through this audit.

The correct action for the first item is an adapter over existing GenOffice
inputs, not `PORT_FROM_CASUAL`; the implementation, copyright, and dependency
review is still required before any code reuse.

## 8. SuperDoc architectural lessons to reproduce independently

Treat SuperDoc as design reference only. The lessons worth reproducing without
copying source are:

- **Explicit contracts:** separate presentation contracts from the editor host,
  measurement implementation, layout engine, and painter. This makes a renderer
  seam testable without moving the document model.
- **Neutral geometry readback:** return per-line/per-segment geometry with PM
  ranges, baseline, caret, and selection data. This is stronger than inferring
  cursor positions from block ratios and should guide a future GenOffice mapping
  adapter.
- **Central page geometry:** use one source of truth for page offsets, page
  heights, and visual gaps so hit testing and selection overlays cannot drift from
  painted pages.
- **Measurement invalidation:** clear text, font, table, and derived block caches
  together when fonts or measurement configuration change. Preserve this as a
  presentation concern, not a DOCX persistence concern.
- **Deterministic measurement mode:** make the measurement environment explicit
  in fixture tests while preserving browser measurement for the real editor.
- **Dependency/checkpoint metadata:** classify why a page must be reflowed and
  retain enough continuation state to resume safely. This is a later performance
  improvement, not a first-seam algorithm change.
- **Phase-level diagnostics and validation:** validate PM range continuity,
  duplicate/missing geometry, section transitions, and layout phase timing before
  optimizing.

## 9. Components explicitly not worth changing

Do not change these merely because Casual Docs or SuperDoc has a cleaner-looking
abstraction:

- `packages/docx-engine/**` or any raw OOXML, anchor, `docxIndex`, ZIP, patch,
  generation, or round-trip contract.
- `blocksToPmDoc`, `pmDocToSavePlan`, Tiptap/ProseMirror schema/extensions,
  revisions, comments, selections, nested editors, or AI document commands.
- Dirty tracking, `saveDocx`, save ordering, save-until-persisted behavior,
  save/reparse, undo/caret preservation, or main/preload IPC.
- The current V1 line breaking, paragraph layout, pagination, table splitting,
  section transition, column balancing, header/footer reserve, or float behavior
  before a complete parity corpus exists.
- Existing GenOffice consumers merely to match Casual’s page/fragment object
  names. `PageSlice`, `BlockBox`, and `SectionGeom` are already adequate seam
  types.
- Sheets, Slides, PDF, Markdown, shell, or dependencies.
- End-user renderer settings UI.

## 10. Proposed implementation order

1. Keep the current V1/V2 selector and delegated V2 proof as-is; record V1 as the
   default and keep the test/development switch internal.
2. Build a read-only parity harness around representative existing fixtures. For
   each renderer capture normalized `PageSlice[]`, `SectionGeom[]`, line/table
   cuts, page gaps, float/header/footer decorations, position mapping, PM JSON,
   dirty state, `pmDocToSavePlan`, saved bytes, and save/reopen state.
3. Expand the narrow internal presentation input/output adapter only where the
   parity harness shows a missing presentation side effect. Keep persistence and
   editor state outside the adapter.
4. Add explicit geometry validation: PM range continuity, page/section order,
   line/table boundary agreement, selection/caret rectangle agreement, and
   deterministic fixture normalization.
5. Add a small visual regression corpus covering simple prose, CJK/doc-grid,
   mixed language, sections, columns, tables, headers/footers, notes, floats,
   comments/revisions, and unsupported-content preservation.
6. Only after the delegated V2 path passes those checks, choose one narrow
   presentation component for independent implementation. The first candidate
   should be measurement/cache diagnostics or geometry readback, not pagination
   or line breaking.
7. Keep V1 available until geometry, position mapping, save/reopen, and
   preservation checks pass across the corpus.

## 11. Risks

- **Incomplete current seam:** the present V2 proof selects page slicing but does
  not yet centralize every presentation side effect. A page-array equality test
  alone is insufficient for selection, page gaps, floats, or overlays.
- **Browser-dependent measurement:** font availability, Canvas/DOM metrics,
  fallback timing, and zoom can change line breaks and page count.
- **High-risk DOCX rules:** keep-next, widow/orphan, columns, sections, tables,
  anchored drawings, notes, and header/footer variants interact across pages.
- **Position drift:** a visually equivalent page can still produce different
  caret or selection rectangles if PM ranges and page offsets are not compared.
- **Performance regressions:** broad remeasurement and DOM reads can dominate
  long documents; incremental layout should be measured, not assumed.
- **License/provenance:** external repositories contain nested licenses,
  third-party assets, and source-level copyright headers. A path that appears
  reusable must still pass provenance and legal review.
- **Unrelated test environment failure:** the required pre-audit rerun still has
  two failures in `tests/ai-panel-collapse.test.ts`; both fail at
  `AiPanel.tsx:288` because `localStorage` is undefined. This is independent of
  presentation-v2 and was not changed by this audit.

## 12. License notes

### Casual Docs

The repository root `LICENSE` is Apache License 2.0. The nested
`docx-editor/LICENSE` is an MIT License dated 2024 and identifies EigenPal. The
reviewed core package is named `@eigenpal/docx-core`; the current source files
also carry Casual Office 2026 copyright headers. Therefore the whole GitHub
repository must not be treated as one undifferentiated license:

- repository-level and nested-project terms must be checked for the exact path;
- source copyright headers and any retained notices must be preserved;
- package/dependency licenses and inherited third-party code must be audited
  before direct reuse;
- the bundled font directory has its own `LICENSE` identifying SIL Open Font
  License terms for Carlito, Caladea, and Liberation families.

This audit recommends no direct Casual code port. If a future task proposes one,
first pin the source commit, identify the exact governing license for each file,
separate Casual-authored code from inherited/third-party code, preserve notices,
and obtain the required legal review.

### SuperDoc

The requested `superdoc/docx-editor` repository resolves to the current
SuperDoc repository and its root `LICENSE` is GNU Affero General Public License
version 3. SuperDoc source is consequently architecture/reference material only
for this project by default. Do not copy its source, tests, or implementation
algorithms into 9Profs. Reproduce only independently reasoned boundaries and
behaviors after a separate license review if a future task requires more.

## 13. Recommended FIRST implementation task after this audit

Add a read-only renderer parity diagnostics harness at the existing
`presentation-v2` seam, with V2 still delegating to V1.

The harness should run the same representative Docs fixtures through both
selectors and compare:

- normalized `PageSlice[]` and `SectionGeom[]`;
- line and table cut geometry;
- page-gap, column, header/footer, and float presentation effects;
- PM position ↔ rectangle mapping, caret, and selection geometry;
- ProseMirror document JSON and dirty state;
- `pmDocToSavePlan` output;
- save bytes and save/reopen state.

This is the smallest next task that resolves the seam’s only material weakness:
the current delegated proof demonstrates equivalent page slices, but not yet the
full presentation output described by the architecture contract. It introduces
no new layout algorithm, keeps V1 as the default, and gives a safe stopping point
before any component is independently rewritten.

