# 9Profs Architecture Audit

Audit target: the repository currently named and packaged as GenOffice, evaluated as the proposed 9Profs fork.

Audit scope: repository structure, runtime boundaries, document lifecycles, AI infrastructure, tests, and the blast radius of a future DOCX presentation/layout refactor.

Evidence convention:

- **Confirmed** means directly observed in the repository source, manifests, tests, or fixtures. File paths and function/class/interface names are included so the claim can be checked.
- **Inference** means an architectural recommendation or a conclusion drawn from the confirmed structure. It is labelled explicitly.

No application source code was changed for this audit. The only intended artifact is this Markdown report.

## Executive summary

The repository is a TypeScript/Electron monorepo with five document applications (`docs`, `sheets`, `slides`, `pdf`, `markdown`) and a unified Electron shell. Shared workspace packages provide document engines, rendering, AI/agent loops, provider adapters, project/chat storage, file parsing, font metrics, UI, and Electron utilities. The root workspace is still branded `genoffice` and uses `@genoffice/*` package names (`package.json`, `apps/*/package.json`, `packages/*/package.json`).

The conservative 9Profs strategy is viable:

1. **DOCX should remain the primary preserved core.** `packages/docx-engine` already separates OOXML parsing/modeling from surgical patch/save logic. The editor uses a ProseMirror model whose block attributes retain `docxIndex` anchors. The current pagination and visual presentation implementation is overwhelmingly in `apps/docs/src/renderer`, especially `pagination.ts`, `line-metrics.ts`, `doc-style-css.ts`, and pagination-related editor extensions.
2. **A DOCX presentation-v2 layer can be introduced at the renderer boundary.** It should consume the existing ProseMirror/DOM/layout inputs and return page slices, line boxes, geometry, and decorations. It should not own OOXML parsing, `docxIndex` identity, dirty-state semantics, or save/round-trip behavior. A parallel renderer behind a feature flag is practical, but it requires isolation of page-gap decorations, header/footer overlays, float shifts, and cursor mapping.
3. **Sheets and Slides are independent engines with preservation-oriented save paths.** Sheets uses Univer plus an XLSX gateway and Rust sidecar. Slides uses `pptx-engine` plus `pptx-render`, with element-level OOXML patching. Both should be KEEP by default.
4. **PDF is already split cleanly between PDF.js viewing, renderer-side edit state, PDFium/PDF-lib main-process operations, and AI tools.** The future Dify integration point is an external 9Profs backend request behind the existing PDF AI/app-main boundary. Dify should not be coupled to PDF.js, PDFium text-editing, or the PDF save engine.
5. **The generic AI foundation is reusable, but product/auth plumbing is Genspark-specific.** `agent-core` exposes skill/tool/transport contracts; `ai-provider` exposes provider routing and streaming protocols. Genspark login, `gsk` search/image services, cloud-generation toggles, and product-branded prompts should sit behind a future 9Profs AI Gateway boundary.
6. **Testing is stronger than a superficial inventory suggests.** There are 586 test/spec files across the repository at audit time, including 95 Docs tests, 77 DOCX-engine tests, pagination corpora with Word/LibreOffice baselines, and many save/round-trip tests. The most important missing safety net before a presentation refactor is an explicit renderer-parity harness that compares old and v2 page geometry, cursor mapping, and save/reopen behavior on the full DOCX fixture corpus.

## Architecture map

### Repository-level topology

```text
Electron shell (apps/shell)
  ├─ BrowserWindow + WebContentsView tab manager
  ├─ shell menus, tabs, updater, project/chat IPC
  └─ module runtimes / renderer URLs
       ├─ Docs      (apps/docs)      ── docx-engine + Tiptap/ProseMirror
       ├─ Sheets    (apps/sheets)    ── Univer + XLSX gateway + Rust sidecar
       ├─ Slides    (apps/slides)    ── pptx-engine + pptx-render + Konva
       ├─ PDF       (apps/pdf)       ── PDF.js + PDFium/PDF-lib + edit overlays
       └─ Markdown  (apps/markdown)  ── Tiptap Markdown + plain .md serialization

Shared packages
  ├─ docx-engine       OOXML parse/model/generate/patch/save
  ├─ pptx-engine       PPTX parse/model/generate/element patch/save
  ├─ pptx-render       EMU-to-pixel render tree and text layout
  ├─ pdf2docx          PDFium extraction and DOCX reconstruction
  ├─ agent-core        ReAct loop, skills, tools, transport interfaces
  ├─ ai-provider       provider registry, chat, streaming protocols, watchdogs
  ├─ ai-search         web/image search, Genspark-first fallback policy
  ├─ project-store     local project/chat/attachment persistence
  ├─ file-parse        shared PDF/Office/text attachment parsing
  ├─ font-metrics      system font lookup and vertical/advance metrics
  ├─ electron-utils    IPC/runtime/security/file/menu helpers
  ├─ i18n              shared localization
  └─ ui                shared renderer components/styles
```

### Confirmed workspace relationships

| Boundary | Confirmed implementation | Architectural meaning |
|---|---|---|
| Workspace | `package.json`: workspaces `apps/*`, `packages/*`; root scripts run package/app tests and typechecks in dependency order | Applications are composed from local packages rather than one shared document runtime. |
| Shell | `apps/shell/src/main/index.ts:193-268`, `apps/shell/src/main/tab-manager.ts:49-69` | One Electron shell owns the tab strip and hosts editor modules as views. |
| Docs runtime | `apps/docs/src/main/docs-main.ts:101-102`, `createDocsWindow`, `createDocsView`, `registerAiIpc`, `registerProjectIpc`, `registerDocsIpc` | Docs can run standalone or in shell tab mode with the same renderer/IPC contracts. |
| Sheets runtime | `apps/sheets/src/main/sheets-main.ts:1450-1535`, `registerSheetsIpc`, `registerSheetsAiIpc` | Sheets has the same standalone/tab runtime shape, plus sidecar integration. |
| DOCX core | `packages/docx-engine/src/parse.ts`, `generate.ts`, `patch.ts`, `types.ts`, `zip-load.ts` | OOXML fidelity and persistence are isolated in a reusable package. |
| PPTX core | `packages/pptx-engine/src/index.ts`, `parse.ts`, `generate.ts`, `zip.ts` | Presentation parsing and surgical archive patching are isolated from the UI. |
| PPTX presentation | `packages/pptx-render/src/build-slide.ts:121`, `text-layout.ts:783`, `render-tree.ts:203`, `apps/slides/src/renderer/SlideCanvas.tsx:510` | Slide layout/rendering is a separate package/UI concern from PPTX save. |
| PDF | `apps/pdf/src/renderer/PdfPage.tsx:47`, `apps/pdf/src/main/pdf-main.ts:724`, `save-pdf.ts:839-998` | PDF viewing/edit overlays and PDF byte mutation are separate processes/layers. |
| AI | `packages/agent-core/src/types.ts:2-111`, `skill.ts:15-85`, `electron-transport.ts:67-138`; app `*/renderer/ai/*` | Tools and document skills live in apps; the model loop and transport contracts are reusable. |
| Local projects | `packages/project-store/src/store.ts:77`, `ipc.ts:74-75` | There is local project/chat storage, not a confirmed multi-tenant 9Profs workspace backend. |

### Major dependency relationships

- `apps/docs` depends on `docx-engine`, Tiptap/ProseMirror, `font-metrics`, `file-parse`, AI packages, `project-store`, and shared UI. Its save path calls back into `docx-engine` rather than serializing OOXML in the renderer.
- `apps/sheets` depends on Univer for the grid UI/model, but its preservation boundary is `renderer/save-actions.ts` → `gateway/xlsx-gateway.ts` / `xlsx-package-io.ts` → `XlsxSidecarClient` → Rust `native/xlsx-engine`.
- `apps/slides` depends on `pptx-engine` for the editable PPTX model and `pptx-render` for a high-fidelity render tree; Konva is a renderer adapter, not the persistence engine.
- `apps/pdf` uses PDF.js for page bitmaps/text layers and PDFium/PDF-lib-backed main-process operations for extraction, text replacement, page operations, and save.
- `apps/markdown` uses Tiptap Markdown for the editor model and its own envelope/body serializer for `.md`; DOCX export is an optional conversion path through `docx-engine`.
- `agent-core` depends on no application document model. Each app supplies its own `AgentSkill`, tool set, context builder, and Electron transport.
- `ai-provider` contains protocol adapters and provider selection. `ai-search` contains a Genspark-first web/image search policy with non-Genspark fallbacks.

## Editor matrix

| Product | Import / source model | Render and edit boundary | Save / round-trip boundary | AI boundary | Classification |
|---|---|---|---|---|---|
| DOCX | `docs-main.ts:loadDocx` → `docx-engine:parseDocx` → `Block[]` with OOXML anchors | Tiptap/ProseMirror (`editor/extensions.ts`, `convert.ts`) plus renderer pagination/layout | `pmDocToSavePlan` → `buildDocBytes` → `docx-engine:saveDocx` → reparse | `docs-skill.ts`, `tools.ts`, `protocol.ts`, `registerAiIpc` | KEEP + EXTEND; presentation layer can MODIFY selectively |
| Sheets | XLSX ZIP/XML plus lazy workbook snapshot; Univer model | Univer renderer/model, `univer-sync.ts`, edit journal | `save-actions.ts` → `xlsx-gateway.ts` / sidecar → atomic archive save | `workbook-skill.ts`, `tools.ts`, `registerSheetsAiIpc` | KEEP |
| Slides | `openPptx` → `Slide`/element model with anchors and inheritance | `pptx-render:buildRenderSlide` → RenderTree → Konva `SlideCanvas` | `savePptx` / `savePptxToFile` → `patchSlideXml` and archive updates | `slides-skill.ts`, `layout-script.ts`, `ai-ipc.ts` | KEEP |
| PDF | PDF.js document proxy plus PDFium-derived text/geometry and annotations | `PdfPage`, text layer, edit/image/annotation overlays, `App` edit state | `App.save` → `pdf:save` → `savePdfToPath` / `applySaveRequest` | `PdfAiDeps`, `pdf-skill.ts`, `tools.ts` | KEEP + EXTEND at backend AI seam |
| Markdown | `parseDocText` envelope + body parsed by Tiptap Markdown | `useEditor` and Markdown extensions in `renderer/App.tsx` | `serializeDocText` → Markdown file; optional `exportDocxBytes` | `markdown-skill.ts`, `tools.ts`, `AiPanel` | KEEP + EXTEND |

## Shell, runtime, and shared infrastructure

### Electron and IPC

**Confirmed.** The shell is a single Electron application with one real `BrowserWindow`; editor modules are hosted as `WebContentsView` tabs. `apps/shell/src/main/index.ts:193-268` defines the module renderer URLs, and `apps/shell/src/main/tab-manager.ts:49-69` documents the view ownership. The shell registers Docs/project/home/tab IPC in `apps/shell/src/main/index.ts:3908-3912`.

Each editor retains a standalone-compatible main/preload/renderer split. Docs exposes `configureDocsRuntime`, `registerAiIpc`, `registerProjectIpc`, `registerDocsIpc`, `createDocsWindow`, and `createDocsView` in `apps/docs/src/main/docs-main.ts:2543-4041`. Sheets has corresponding runtime and IPC functions in `apps/sheets/src/main/sheets-main.ts:1450-2594`. Slides and PDF use the same pattern in `apps/slides/src/main/slides-main.ts` and `apps/pdf/src/main/pdf-main.ts`.

**Inference.** This is a good future host for account/workspace/usage routing because the shell already owns application lifecycle and cross-module IPC. It should not become the owner of format-specific persistence logic. A future SaaS layer should add explicit service clients and authenticated IPC contracts rather than adding account concerns to `docx-engine`, `pptx-engine`, the Sheets gateway, or PDF byte mutation.

### Reusable shared infrastructure

- `packages/electron-utils` provides shared menus, safe URL/file helpers, atomic writes, navigation/security utilities, default save directory, and context-menu infrastructure.
- `packages/project-store` is a local, injected-path store. `ProjectStore` in `packages/project-store/src/store.ts:77` and `ProjectApi` in `packages/project-store/src/ipc.ts:74-75` are useful contracts for later remote synchronization, but the current implementation is not a SaaS workspace service.
- `packages/file-parse/src/parse.ts` and its Office/PDF helpers provide common attachment parsing.
- `packages/font-metrics/src/metrics.ts:114`, `font-locate.ts:61-197`, and `advance.ts:173` provide shared font discovery/measurement primitives used most heavily by Docs and Slides.

## Deep DOCX architecture

### Complete lifecycle

```text
DOCX bytes
  → Electron main loadDocx()
  → ZIP/XML/relationship/theme/style/numbering parse
  → docx-engine Block[] + raw XML + docxIndex anchors
  → blocksToPmDoc()
  → Tiptap/ProseMirror document and nested editors
  → DOM/CSS measurement and pagination
  → page gaps, header/footer overlays, cursor/selection mapping
  → user/AI transactions and dirty state
  → pmDocToSavePlan()
  → docx-engine saveDocx()
  → atomic IPC write
  → reparse saved bytes and rebase editor state
```

#### 1. File open and decrypted input

**Confirmed.** `apps/docs/src/main/docs-main.ts:2307-2358 (loadDocx)` is the Electron main-process entry point. It validates the path, reads DOCX bytes, detects encrypted CFB content, decrypts when required, archives the original, checks recovery copies, records file state, and returns the bytes/hash/encryption/recovery metadata to the renderer. `apps/docs/src/preload/index.ts:63-85` exposes the save/font/export bridge.

The renderer does not receive a filesystem path and independently parse arbitrary files; it receives the main-process result and calls the engine parser.

#### 2. OOXML parse and intermediate model

**Confirmed.** `packages/docx-engine/src/parse.ts:184-422 (parseDocx)` loads the DOCX ZIP, resolves `word/document.xml`, parses theme, styles/docDefaults, relationships, numbering, comments, protection, notes, sources, headers/footers, sections, and compatibility/layout settings. It builds `BodyElement[]` and `Block[]`, retains original XML fragments and `docxIndex` anchors, and returns `internal.originalBytes`, document XML boundaries, and `extras` such as chart parts.

Important parser responsibilities:

- Block construction: `packages/docx-engine/src/parse.ts:670 (buildBlock)`.
- Run extraction and run model: `parse.ts:3064 (extractRuns)` and `parse.ts:3380 (buildRun)`.
- Tables/cells: `parse.ts:3734 (extractTableModel)` and `parse.ts:4050 (extractCell)`.
- Headers/footers and nested content: `readHeaderFooterPart`, `hfParagraphs`, `hfTableRowParagraphs`, `hfCellContent`, and `textboxParagraphs` around `parse.ts:4499-4755`.
- Styles: `parseStyles` around `parse.ts:5748-5968`, with run resolution in `styleRunFormat`.
- Comments: `parseComments` around `parse.ts:6206-6246`.
- Numbering: `parseNumbering` around `parse.ts:6375-6440`.
- Public model contracts: `packages/docx-engine/src/types.ts:876-878 (Block)` and `packages/docx-engine/src/patch.ts:64-90 (ParsedDocFull, SaveBlock, SaveOptions)`.

**Confirmed preservation property.** The parser returns both a normalized model and raw/part-level information. That dual representation is the foundation for surgical save and is the most important DOCX contract to protect.

#### 3. Conversion into Tiptap/ProseMirror

**Confirmed.** `apps/docs/src/renderer/file-actions.ts:243-359 (loadFile)` calls `parseDocx`, loads numbering/style data into editor storage, applies `applyDocLayoutSettings`, converts blocks through `blocksToPmDoc`, and initializes sections, header/footer variants, comments, watermark, ink, notes, sources, theme, protection, and compare state.

`apps/docs/src/renderer/editor/convert.ts:62-81 (blocksToPmDoc)` maps the engine blocks to ProseMirror nodes, skips hidden blocks, carries `docxIndex` and related source attributes, caps overly large rows, and falls back to a paragraph node for unsupported/empty content.

**Confirmed.** `apps/docs/src/renderer/editor/extensions.ts` states that the custom schema mirrors the `docx-engine` Block model and carries patch anchors. The `anchorAttrs` contract includes `docxIndex`, style and numbering attributes, bookmarks, comments, revisions, paragraph properties, SDT shells, and raw/preservation-related attributes.

#### 4. Editor initialization and schema/extensions

**Confirmed.** `apps/docs/src/renderer/App.tsx:641-772` initializes the editor with `useEditor`, `editorExtensions`, initial `docParagraph` content, paste handlers, selection updates, update/dirty callbacks, protection/read-only behavior, and track-changes storage. `onUpdate` sets `dirtyRef.current = true`; `onSelectionUpdate` triggers selection-dependent rendering.

`apps/docs/src/renderer/editor/extensions.ts:3477+ (editorExtensions)` includes the document/text primitives, hard breaks, notes, fields, images, math, paragraph/heading/list/table nodes, nested tables, protected content, field/revision/comment marks, undo/redo, search/comment decorations, native table support, track changes, line factors, pagination gaps, inactive selections, page-gap navigation, columns, tabs, word line height, drop caps, paragraph borders, SDTs, move revisions, paragraph-property revisions, and direction handling.

Nested text boxes are mounted by `mountTextboxEditors` in `extensions.ts:2941-3068`. This creates a nested Tiptap `Editor` and commits nested paragraphs back into the parent node. A presentation refactor must preserve this nested-editor transaction boundary.

#### 5. Rendering, pagination, and layout

The renderer is DOM/CSS-backed but uses a dedicated layout algorithm to derive pages.

| Concern | Confirmed implementation |
|---|---|
| Page slicing | `apps/docs/src/renderer/pagination.ts:160 (computePageSlices)`, `:190 (computeSectionedSlices)`, and `:286 (computeSectionedSlicesF2)` |
| Placement | `pagination.ts:838 (_placeTable)`, `:967 (_placeParaBlock)`, `:1117 (_hardCutLines)` |
| Section geometry | `pagination.ts:1349 (sectionGeoms)` derives page size, margins, header/footer reserve, columns, and forced section breaks |
| DOM block measurement | `pagination.ts:1719 (measureBlocks)` measures block boxes, table widths, margins/gaps, break flags, and float exclusions |
| Line/table boundaries | `pagination.ts:1994 (fillLineBoxes)`, `tableRowFlags`, `tableHeaderFlags`, `domTableRows`, `cellCutYs`, `lineBreakBoundaries` |
| Page lookup/start anchors | `pagination.ts:1646 (pageAt)` and `:2510 (pageStartBlocks)` |
| Line height and font metrics | `apps/docs/src/renderer/line-metrics.ts:323 (computeLineHeight)`, `:919 (simulateLines)`, `:1226 (computeLineMetrics)`, `:1328 (estimateHfHeight)`, `:1432 (estimateFootnoteHeight)` |
| System font data | `packages/font-metrics/src/metrics.ts:114 (familyVerticalMetrics)`, `font-locate.ts:61 (findSystemFont)`, `:155 (findFontCovering)` |
| Styles/fonts/CSS | `apps/docs/src/renderer/doc-style-css.ts:30 (docThemeCss)`, `:118 (docStyleCss)`; `docBodyFont` at `:113` |
| Page gaps | `apps/docs/src/renderer/editor/pagination-gaps.ts:16 (PaginationGapsExtension)`, `:116 (setPageGaps)`, `:245 (syncCutOverlays)`, `:343 (syncFloatShifts)` |
| Cursor crossing page gaps | `apps/docs/src/renderer/editor/page-gap-nav.ts:57 (crossPageGap)`, `:149 (PageGapNavExtension)` |
| Columns | `apps/docs/src/renderer/editor/column-layout.ts:24 (ColumnLayoutExtension)`, `:60 (setColumnLayout)` |

`computeSectionedSlicesF2` is not a simple visual paginator. It handles sections, multi-column layout, keep-next, widow/orphan constraints, tables at row level, lines, floats, forced/column breaks, section changes, and column balancing. `measureBlocks` and `fillLineBoxes` depend on the rendered DOM and therefore couple browser layout, CSS, font availability, and pagination decisions.

**Inference.** This renderer/layout cluster is the correct location for lessons from Casual Docs and SuperDoc. It is not evidence that the underlying OOXML engine or ProseMirror model should be replaced.

#### 6. Styles, fonts, tables, headers, and footers

- **Fonts/styles:** `doc-style-css.ts` generates document theme/default/style CSS, dual font slots, font size/color/weight/style, alignment, line height, table-style CSS, spacing, indents, and contextual spacing. `line-metrics.ts` resolves Latin/CJK/complex-script families and system metrics. Treat these as presentation inputs and compatibility-sensitive contracts.
- **Tables:** OOXML extraction is in `docx-engine/src/parse.ts:3734/4050`; ProseMirror table nodes/extensions are in `editor/extensions.ts`; DOM row/cell measurement and row-level page breaking are in `pagination.ts:1994+`; table display fallback and protected render behavior are in `editor/protected-render.ts:873-1027 (renderTableSpec)`.
- **Headers/footers:** OOXML parsing is in `readHeaderFooterPart` and `hfParagraphs` (`packages/docx-engine/src/parse.ts:4499-4755`). Renderer-side display is in `apps/docs/src/renderer/editor/hf-dom.ts`, including `hfHasVisibleContent`, `hfFloatPagePos`, `makeHfFloatImgEl`, and `makeGapHfEl`. `App.tsx:391-561` manages section/variant state and `file-actions.ts:531-561 (buildDocBytes)` passes header/footer data into the save engine.
- **Fields/protected rendering:** `apps/docs/src/renderer/editor/protected-render.ts:60 (renderFieldSpec)`, `:91 (renderFormulaSpec)`, `:128 (renderChartSpec)`, and `:873 (renderTableSpec)` render read-only/protected or special content. Layout changes may affect their display boxes, but their model and save semantics should not move into v2.

#### 7. Comments, revisions, cursor, and selection

- Comments are modeled and mutated by `apps/docs/src/renderer/editor/comments.ts:19 (addCommentToSelection)`, `:45 (removeCommentFromDoc)`, and `:86 (addReplyToCommentRange)`. OOXML emission is `packages/docx-engine/src/patch.ts:1495 (buildCommentsXml)` and `:1538 (buildCommentsExtendedXml)`.
- Track changes are implemented by `apps/docs/src/renderer/editor/revisions.ts:269 (applyRevisions)`, `:491 (acceptAllRevisions)`, `:495 (rejectAllRevisions)`, `:500 (applyRevisionsBy)`, `:520 (acceptCurrentRevision)`, `:527 (rejectCurrentRevision)`, and `:574 (TrackChangesExtension)`. The extension's transaction mapping is around `:674-1137`.
- Cursor and selection are driven by ProseMirror state in `App.tsx`, including `selectionPos` during save, `onSelectionUpdate`, and `TextSelection` mapping in `revisions.ts:535 (gotoRevision)`. `page-gap-nav.ts:57 (crossPageGap)` maps positions across visual page gaps; `margin-annotations.ts:194 (anchorPointFor)` uses `EditorView.coordsAtPos`.

**Inference.** A v2 renderer must preserve stable document positions and expose an equivalent position-to-rectangle/rectangle-to-position service. Visual page slices may change; ProseMirror positions and revision/comment anchors must not.

#### 8. AI editing boundary

**Confirmed.** Docs AI is a document-tool loop, not a second document persistence engine:

- `apps/docs/src/renderer/ai/protocol.ts:15-116` defines the document-first tool protocol and prompt; `getSelectionScope`, `buildDocumentContext`, `serializeRangeToHtml`, `replaceBlockRange`, and `insertBlocksAfter` are context/transaction helpers (`protocol.ts:137-888`).
- `apps/docs/src/renderer/ai/tools.ts:28 (AGENT_TOOLS)`, `:264 (markDocSeen)`, `:417 (executeTool)`, and `:441 (executeSyncTool)` expose read/modify operations against the current editor.
- `apps/docs/src/renderer/ai/docs-skill.ts:11 (createDocsSkill)` adapts those tools to the generic `AgentSkill` contract.
- `apps/docs/src/renderer/ai/transport.ts:6 (createElectronTransport)` routes stream/cancel calls to the preload bridge.
- `apps/docs/src/main/docs-main.ts:2543 (registerAiIpc)` handles `ai:get-settings`, `ai:stream`, `ai:stream-cancel`, search, and chat IPC.

**Inference.** The AI layer should continue to call document tools and editor commands. It should not call a new presentation renderer directly. A presentation-v2 renderer may change what the user sees after an AI transaction, but the AI transaction should still be applied to the same ProseMirror model and saved through the same `pmDocToSavePlan` path.

#### 9. Dirty tracking, save, and round-trip

**Confirmed dirty state.** `apps/docs/src/renderer/doc-dirty.ts:6 (DocDirtyState)` and `:32 (isDocDirty)` combine ProseMirror dirty state with section properties, page color, headers/footers/variants, page numbering, numbering/style changes, title/even-odd settings, watermark, ink, notes, sources, theme, comments, and protection. `App.tsx` owns `dirtyRef` and marks it in editor updates and nested text-box updates.

**Confirmed save plan.** `apps/docs/src/renderer/editor/convert.ts:1114 (pmDocToSavePlan)` maps the current ProseMirror JSON back to original blocks, carries `docxIndex` anchors, preserves SDT shells, and creates `SaveBlock[]` plus chart patches. Unchanged source blocks can remain represented by their original XML.

**Confirmed byte construction.** `apps/docs/src/renderer/file-actions.ts:466 (buildDocBytes)` gathers the save plan, chart and ink changes, section property rewrites, header/footer variants, and document metadata, then calls `saveDocx`. `packages/docx-engine/src/patch.ts:380 (saveDocx)` preserves unmodified ZIP entries and patches only the parts required by the save options, including document XML, relationships/media, comments, headers/footers, styles, numbering, notes, theme, settings, and properties.

**Confirmed persistence and race handling.** `apps/docs/src/renderer/file-actions.ts:662 (saveOnce)` snapshots the ProseMirror document and selection, builds bytes, invokes the preload save bridge, detects edits that landed during the write, reparses the written bytes, and only resets editor content/history when the reparse is not equal to the live document. This preserves undo, caret, and scroll when possible. `apps/docs/src/renderer/save-until-persisted.ts:28 (createSaveSerializer)` and `:60 (saveUntilPersisted)` serialize saves FIFO and retry until persisted.

### DOCX contracts that should remain unchanged

The following are the high-risk persistence and semantic contracts. They should be treated as read-only inputs to a future presentation layer:

1. `packages/docx-engine/src/parse.ts`: `parseDocx`, `Block`, `docxIndex`, raw XML fragments, original bytes, section/header/footer metadata, and unsupported-content preservation.
2. `packages/docx-engine/src/patch.ts`: `ParsedDocFull`, `SaveBlock`, `SaveOptions`, `saveDocx`, comments/revisions/headers/footer/relationship/part patching, and preservation of untouched archive entries.
3. `packages/docx-engine/src/generate.ts`: OOXML fragment generation and schema-ordered property assembly.
4. `packages/docx-engine/src/types.ts`: `Block`, run/table/section/style/revision/comment contracts.
5. `apps/docs/src/renderer/editor/convert.ts`: `blocksToPmDoc` and `pmDocToSavePlan`; the model-to-editor and editor-to-save identity map.
6. `apps/docs/src/renderer/editor/extensions.ts`, `marks.ts`, `revisions.ts`, and `comments.ts`: schema, command, transaction, revision, and comment semantics.
7. `apps/docs/src/renderer/doc-dirty.ts`, `file-actions.ts`, `save-until-persisted.ts`, preload, and Docs main IPC: dirty, save ordering, race, and reparse behavior.

### DOCX presentation-v2 boundary

The following are the natural presentation-v2 candidates:

- `apps/docs/src/renderer/pagination.ts`: block measurement, section geometry, line boxes, table row cuts, page/column placement, balancing, keep-next, widow/orphan, float and forced break behavior.
- `apps/docs/src/renderer/line-metrics.ts`: font availability, font fallback, line wrapping, CJK/complex-script measurement, line height, doc-grid snapping, and header/footer/footnote height estimates.
- `apps/docs/src/renderer/doc-style-css.ts` and relevant generated-style sections in `styles.css`: the CSS realization of style/theme/spacing/layout inputs.
- `apps/docs/src/renderer/editor/pagination-gaps.ts`, `page-gap-nav.ts`, `column-layout.ts`, and `hf-dom.ts`: presentation-only page gaps, column decorations, header/footer overlays, float shifts, and cursor navigation across visual gaps.
- `apps/docs/src/renderer/App.tsx` measurement/placement wiring and `PaginationPreview`: the orchestration seam where a renderer can be selected.
- `apps/docs/src/renderer/editor/protected-render.ts`: only for layout-specific display changes; field/chart/table/protected semantics remain shared.

**Recommended v2 contract (inference):**

```text
PresentationInput
  = live PM/DOM view + BlockBox/LineBox measurement inputs
  + section/header/footer/theme/font inputs

PresentationOutput
  = PageSlice[] + SectionGeom[] + line/table cut geometry
  + page-gap/column/header-footer/float decorations
  + position ↔ rectangle mapping for cursor/selection
```

The current `PageSlice`, `BlockBox`, `SectionGeom`, and line/table geometry types in `pagination.ts` are the strongest existing seam. A v2 implementation should either produce these same outputs or expose an adapter that does. The save path must continue to consume the document model, not page slices.

**Parallel renderer practicality (inference).** A feature flag is practical with moderate wiring because `saveOnce` and `buildDocBytes` depend on editor state and dirty metadata, not on page slices. The flag must isolate all presentation side effects together: measurement, `setPageGaps`, float shifts, phantom row spans, header/footer gap overlays, pagination preview, and position mapping. There is no existing renderer flag confirmed in the repository; the flag and adapter would be new wiring.

## Sheets architecture

### Confirmed lifecycle and boundaries

1. **Load/model:** `apps/sheets/src/renderer/create-univer.ts:26-57 (createUniver)` creates the Univer instance and deduplicates plugin registration. `apps/sheets/src/renderer/univer-sync.ts:133 (syncUniver)`, `:149 (loadSnapshotIntoUniver)`, `:332 (loadWorkbookSkeleton)`, `:814 (loadVisibleRange)`, and `:1790 (ensureLazyRangeLoaded)` synchronize the workbook snapshot and visible ranges into Univer.
2. **Domain/edit journal:** `apps/sheets/src/domain/workbook.types.ts`, `in-memory-workbook.ts`, `workbook-dsl.ts`, and `apps/sheets/src/renderer/edit-journal.ts` define the app-side model and pending edits. `univer-sync.ts:2320 (applyJournalOverlay)` and `:2741 (toRichTextDocument)` reconcile overlays and rich text.
3. **Import:** `apps/sheets/src/gateway/xlsx-gateway.ts:404 (readBasicWorkbook)` reads the XLSX ZIP, workbook XML, shared strings, and worksheets into a snapshot. The Rust sidecar indexes and reads ranges through `apps/sheets/native/xlsx-engine/src/lib.rs:560 (open)`, `:743 (read_range)`, `:763 (read_formula_cells)`, and `:1600 (index_worksheet)`.
4. **Save planning:** `apps/sheets/src/renderer/save-actions.ts:64 (handleSave)` captures the view and drains cell, bulk, structural, chart, visual, table, pivot, sheet, and hyperlink edits. It sends a save edit request and reopens/reconciles the workbook while preserving view and undo behavior.
5. **OOXML preservation:** `apps/sheets/src/gateway/xlsx-gateway.ts:600 (planCellEditsToXlsx)` applies fail-closed guards and targeted XML patches. `:1491 (assertOnlyTouchedEntriesChanged)`, `:1551 (syncFileBestEffort)`, `:1572 (writeXlsxAtomically)`, and `:1584 (mutateXlsxFile)` enforce preservation/atomicity.
6. **Sidecar/archive:** `apps/sheets/src/gateway/xlsx-package-io.ts:149 (saveWorkbookViaSidecar)` uses a temporary work directory, sidecar manifest, targeted replacements/additions, `XlsxSidecarClient`, manifest preservation checks, and atomic promotion. `apps/sheets/src/main/xlsx-sidecar-client.ts:27 (XlsxSidecarClient)` and `:150 (request)` own the newline-delimited JSON child-process protocol. Rust archive operations are `apps/sheets/native/xlsx-engine/src/archive.rs:59 (archive_manifest)`, `:173 (save_archive)`, and `:240 (validate_edit_sets)`.
7. **AI:** `apps/sheets/src/renderer/ai/tools.ts:207 (WORKBOOK_TOOLS)`, `:539 (buildWorkbookContext)`, `:670 (executeWorkbookTool)`, and `workbook-skill.ts:21 (createWorkbookSkill)` keep AI edits in a read-before-write/change-plan boundary. The tools do not directly bypass the workbook save pipeline.

**Classification: KEEP.** The XLSX gateway, sidecar, Univer integration, and AI change-plan discipline are already strong preservation boundaries. Future 9Profs account/usage/storage work should wrap the session and save lifecycle; it should not modify XLSX patch semantics without a separate compatibility reason.

## Slides architecture

### Confirmed lifecycle and boundaries

- **Open/parse:** `packages/pptx-engine/src/zip.ts:90 (PackageArchive.readPresentation)`, `parse.ts:153 (parseSlide)`, and `index.ts:297 (parseSlideFromArchive)` parse slide parts, shapes, pictures, tables, runs, paragraphs, fills, strokes, groups, and inheritance. `apps/slides/src/main/slides-main.ts:644` calls `openPptx` and builds all render slides.
- **Engine model and preservation:** `packages/pptx-engine/src/index.ts:582-605` materializes slide models from the archive while retaining source relationships/anchors. `patchSlideXml` is `index.ts:727`; untouched elements are preserved and changed elements are patched.
- **Render:** `packages/pptx-render/src/build-slide.ts:121 (buildRenderSlide)` builds a `RenderSlide` tree. Text layout is `packages/pptx-render/src/text-layout.ts:500 (layoutParagraph)`, `:783 (layoutText)`, `:936 (layoutTextVertical)`, and `:1170 (layoutAll)`. The render data contract is `packages/pptx-render/src/render-tree.ts:203 (RenderTextLayout)` and `:471 (RenderSlide)`. `apps/slides/src/renderer/SlideCanvas.tsx:510` renders the canvas/editor view.
- **Edit:** renderer actions (`slide-actions.ts`, `style-actions.ts`, `table-actions.ts`, `insert-actions.ts`, `keyboard-actions.ts`) update the slide model; main-process operations in `apps/slides/src/main/ops/*` provide transactional element/slide/table operations for AI and UI paths.
- **Save:** `packages/pptx-engine/src/index.ts:622 (savePptx)`, `:640 (savePptxToFile)`, and `:727 (patchSlideXml)` produce the archive. `apps/slides/src/main/slides-main.ts:901 (registerSlidesIpc)`, `:3707 (slides:is-dirty)`, and `:3718 (slides:save)` connect renderer state to disk.
- **AI:** `apps/slides/src/renderer/ai/slides-skill.ts:1529 (createSlidesSkill)`, `:1689 (executeTool)`, and `:1840 (execute_slide_script)` provide deck generation, element edits, transactional scripts, and layout audits. `apps/slides/src/renderer/ai/layout-script.ts:97 (runLayoutScript)` is the script interpreter boundary. Cloud/Genspark generation hooks are explicit in the skill access contract and should be treated as provider-specific.

**Classification: KEEP.** The engine/render split is analogous to the DOCX distinction between persistence and presentation. No Slides rewrite is warranted by the observed structure.

## PDF architecture

### Confirmed lifecycle and boundaries

1. **Read/view:** `apps/pdf/src/renderer/App.tsx:256 (App)` calls `window.pdfApi.readFile` and loads a PDF.js `PDFDocumentProxy`. `apps/pdf/src/renderer/PdfPage.tsx:47 (PdfPage)` renders the bitmap and creates a PDF.js `TextLayer` at `:114`. `PdfPage`, `search.ts`, `text-block.ts`, `text-line.ts`, `text-wrap.ts`, and `selectionQuadsByPage` carry page/text geometry.
2. **Renderer edit state:** `apps/pdf/src/renderer/edit-state.ts` stores unsaved annotations, note edits, page operations, text edits/inserts, image edits, form edits, and save snapshots. `App.tsx:345-352` owns local text edit state; `:1378-1390` computes and publishes dirty state; `:1681` computes selection quads; `:2682+` manages text-edit drafts; `:2968 (save)` builds the save payload.
3. **Text extraction/edit:** `apps/pdf/src/main/text-edit.ts` loads PDFium/WASM and fonts, preserves extracted codepoints, validates edits at `:2041 (validateTextEdits)`, and validates the final output around `:2202`. Its coordinate logic is part of the PDF-specific text rewrite pipeline.
4. **Save:** `apps/pdf/src/main/pdf-main.ts:724 (registerPdfIpc)` registers `pdf:read-file`, `pdf:save`, validation, extraction, page operations, image/form/signature operations, and related handlers. `pdf-main.ts:746` handles save and calls `savePdfToPath` at `:757`. `apps/pdf/src/main/save-pdf.ts:839 (savePdfToPath)` and `:863 (applySaveRequest)` apply markups, forms, page operations, text edits/inserts, images, and metadata. Atomic writing is in `apps/pdf/src/main/atomic-write.ts`.
5. **AI:** `apps/pdf/src/renderer/ai/tools.ts:26 (PdfAiDeps)` defines the capability surface supplied by `App`; `AGENT_TOOLS` begins at `:85`; `readPages`, `searchText`, `markupText`, `editText`, `editBlock`, `insertTextTool`, and image tools translate model requests into the same pending edit state used by the UI. `pdf-skill.ts` and `transport.ts:6 (createElectronTransport)` adapt this to `agent-core`.

### Future Dify integration point

**Confirmed current state:** no Dify dependency, import, client, or integration module was found in the repository's application/package structure. The existing PDF AI tool surface is local and document-aware.

**Recommended future boundary (inference):**

```text
PDF renderer
  → PdfAiDeps / PDF context request (document id, page range, selection/text, geometry, attachments)
  → preload/main IPC or shared 9Profs backend client
  → 9Profs backend (auth, workspace, usage, retrieval policy, audit)
  → Dify workflow/RAG application
  → grounded answer / structured workflow result
  → PDF AI panel and, only with explicit tool approval, local pending edits
```

The clean seam is beside `apps/pdf/src/renderer/ai/pdf-skill.ts`, `tools.ts`, and the existing `registerPdfIpc`/preload bridge—not inside `PdfPage`, PDF.js text layers, `text-edit.ts`, or `save-pdf.ts`. The backend should receive bounded context rather than raw uncontrolled filesystem access. Local edit tools should remain the authority for modifying the open PDF and for dirty/save behavior.

## Markdown architecture

**Confirmed.** `apps/markdown/src/renderer/App.tsx:2` uses Tiptap `EditorContent`/`useEditor`. At `:172-207`, it initializes the editor, reads text through the preload bridge, calls `parseDocText`, strips legacy fenced blocks, and calls `setContent(..., { contentType: 'markdown' })`. `apps/markdown/src/renderer/markdown/docText.ts:21 (parseDocText)` parses the document envelope/frontmatter and `:109 (serializeDocText)` rebuilds the full Markdown file.

The save flow in `App.tsx:247-252` serializes the current editor body and sends it through the Markdown main-process save path. `apps/markdown/src/main/markdown-main.ts:526 (registerMarkdownIpc)` handles read/save, Save As, atomic writing, and relative image/asset lifecycle. `apps/markdown/src/main/atomic-write.ts`, `asset-lifecycle.ts`, and `conversion-lifecycle.ts` protect file/asset consistency.

The editor feature surface is in `apps/markdown/src/renderer/editor/extensions.ts`, with code blocks, math, slash commands, local images, block drag/keymap, and AI highlights. Optional DOCX export is explicit: `apps/markdown/src/renderer/export/docxExport.ts:179 (walkBlock)`, `:260 (mapDocToSaveBlocks)`, and `:294 (exportDocxBytes)` map the Tiptap JSON to `docx-engine` save blocks. AI is `markdown-skill.ts:37 (createMarkdownSkill)`, `tools.ts:22 (markDocSeen)`, `:executeTool`, and `AiPanel.tsx`.

**Classification: KEEP + EXTEND.** Markdown is a useful low-risk place to add future research/review skills because its serialization contract is simpler than OOXML. Its existing `docxExport` should remain an explicit export path, not become a hidden dependency of the Markdown editor.

## AI and agent infrastructure

### Generic reusable core

- `packages/agent-core/src/types.ts:2-111` defines `AgentToolDef`, `AgentToolCall`, `AgentToolResult`, messages, stream callbacks, handles, and `AgentTransport`.
- `packages/agent-core/src/skill.ts:15 (AgentSkill)` defines the application skill contract; `:46 (composeSkills)`, `:67 (buildContext)`, `:72 (executeTool)`, and `:79 (verifyResponse)` provide composition and execution helpers.
- `packages/agent-core/src/loop.ts:161 (AgentLoop)` owns the multi-turn ReAct/tool loop, compaction, streaming, tool execution, and result lifecycle.
- `packages/agent-core/src/electron-transport.ts:67 (createIpcTransport)` bridges renderer agents to main-process model providers and exposes cancellation/timeouts.
- `packages/ai-provider/src/types.ts:25 (AiProviderConfig)`, `:41 (AiSettings)`, and stream request/chunk types define provider-neutral settings and streaming payloads.
- `packages/ai-provider/src/registry.ts:7 (AiProtocol)`, `:23 (ProviderAdapter)`, and `:155 (getProviderAdapter)` isolate Anthropic, Gemini, OpenAI-compatible, custom, and Genspark endpoint selection.
- `packages/ai-provider/src/stream.ts:16 (streamForProvider)` and `chat.ts` route streaming and one-shot calls; watchdog/error/fetch helpers provide common resilience.
- `packages/ai-search/src/index.ts:25 (webSearch)` and `:87 (imageSearch)` are shared main-process search APIs with fallback behavior.

### Genspark-specific portions

The following are not generic platform contracts and are candidates for future adapter isolation:

- `packages/ai-provider/src/types.ts:19 (GenSparkAccountStatus)` and `:45-51` Genspark cloud-tools settings.
- `packages/ai-provider/src/registry.ts` Genspark endpoint/auth capabilities and attribution headers.
- `packages/ai-search/src/index.ts:2-5,15-18,35-39` and `gsk.ts`: Genspark CLI/search-first behavior and `hasGskAuth`.
- `apps/*/src/main/*` AI IPC handlers that read `ai-settings.json`, expose Genspark login/toggles, or call cloud-only generation/image functions.
- Product prompts that explicitly name GenOffice, including `apps/docs/src/renderer/ai/protocol.ts:98`.
- Slides skill access fields and comments that call Genspark cloud slide generation (`apps/slides/src/renderer/ai/slides-skill.ts:115`, `:2333+`).

**Inference.** Keep the `AgentSkill` and `AgentTransport` shapes stable. Replace or wrap only provider/auth/search/cloud-generation adapters. Do not put 9Profs billing, workspace membership, usage accounting, or RAG policy in document engines or renderer tool implementations.

### Likely 9Profs AI Gateway boundary

The most compatible boundary is between app-specific `createElectronTransport` implementations and the provider-specific main-process adapters:

```text
app skill + tools + document context
  → AgentTransport-compatible 9Profs gateway client
  → 9Profs backend gateway
  → provider routing / Dify / retrieval / policy / usage / billing
```

The gateway client should preserve existing stream callbacks, tool-call payloads, cancellation, and error semantics. It can be introduced behind the current preload/main IPC without changing the DOCX, XLSX, PPTX, PDF, or Markdown model contracts.

## Testing and safety nets

### Confirmed coverage inventory

At audit time, a repository file inventory found 586 test/spec files. The largest relevant groups were:

| Area | Observed test/spec files | Representative coverage |
|---|---:|---|
| `apps/docs` | 95 | pagination, line metrics, comments, revisions, dirty state, protection, fonts, tables, AI tools, render isolation, save/recovery |
| `packages/docx-engine` | 77 | OOXML parse/patch, sections, styles, comments, revisions, tables, images, raw properties, resource cleanup, round-trip regressions |
| `apps/sheets` | 129 | XLSX save/patch/fidelity, formulas, edits, Univer flows, AI tools |
| `packages/pptx-engine` | 71 | parse/patch/transform, comments/notes, charts/tables, resource preservation |
| `apps/slides` | 52 | canvas/text hit, editing, save/fidelity, AI generation/ops, layout, theme, animation |
| `apps/pdf` | 40 | PDF text edit/wrap/font cmap, annotations, forms, images, search, save, autosave, page operations |
| `apps/markdown` | 9 | Markdown nodes/round-trip, doc text, DOCX export, assets, AI tools |
| `e2e` | 15 named specs | shell/home/onboarding, Markdown tab, Sheets edit/save, visual themes, new-file flows |

Root test orchestration is explicit in `package.json:27-35`: package/app unit tests, typecheck, build, and Playwright E2E. Root scripts include `test:e2e`, `build:all`, `lint`, and `format:check`.

### DOCX-specific strengths

- Pagination fixture corpus: `apps/docs/tests/pagination-corpus/docx/*` includes English, Chinese, mixed-language, doc-grid, headings/keep-next, tables, footnotes, page breaks, sections, columns, repeated table headers, and oversized tables.
- Baselines: `apps/docs/tests/pagination-corpus/baseline-word.json` and `baseline-lo.json`, exercised by `apps/docs/tests/pagination-parity.test.ts` and `pagination.test.ts`.
- External comparison tooling: `scripts/pagination-baseline.mjs`, `scripts/pagination-baseline-word.mjs`, `scripts/docs-word-fidelity.mjs`, and `tools/fidelity-compare.mjs`.
- Renderer/layout unit coverage: `pagination-gaps.test.ts`, `pagination-preview-prune.test.ts`, `line-metrics.test.ts`, `paragraph-strut-size.test.ts`, `render-isolation.test.ts`, `native-table.test.ts`, `nested-table-edit.test.ts`, `hf-page-mark.test.ts`, and `float-spill.test.ts`.
- Persistence/semantic coverage: `raw-ppr.test.ts`, `raw-rpr.test.ts`, `font-paste-roundtrip.test.ts`, `para-paste-roundtrip.test.ts`, `comments.test.ts`, `revisions.test.ts`, `doc-dirty.test.ts`, `save-until-persisted.test.ts`, encryption tests, and the DOCX engine's preservation/regression corpus.

### Important gaps before DOCX presentation refactoring

These are inferred gaps from the test inventory, not claims that no related assertion exists anywhere:

1. **Explicit old-v2 renderer parity.** Add a harness that runs the same parsed document through both renderers and compares page count, section/page geometry, line breaks, table row cuts, header/footer positions, and float placement. Existing Word/LibreOffice baselines are valuable but do not replace old-v2 parity.
2. **Position mapping parity.** Add tests for `coordsAtPos`/selection rectangles, cursor movement across page gaps, comments/revision margin anchors, nested text boxes, and selections spanning page/column boundaries.
3. **Save-independent rendering.** Prove that switching renderer flags cannot change `pmDocToSavePlan`, dirty state, or saved OOXML for the same editor model. This should include reparse-and-compare after save.
4. **Preservation assertions by part.** For unchanged input, assert that document parts not intentionally touched remain byte- or semantically-identical: headers/footers, styles, numbering, comments, notes, relationships, media, theme, settings, custom XML, SDT shells, and unmodeled runs/properties.
5. **Font matrix expansion.** Existing font tests are good, but v2 should run deterministic fixtures across missing fonts, fallback fonts, CJK, RTL, complex scripts, doc-grid snapping, mixed run fonts, and font metrics differences between Chromium and Word/LibreOffice.
6. **Concurrency/dirty safety.** Exercise AI edits, nested text-box transactions, fast typing, autosave, Save As, close guards, and edits arriving during `saveOnce`. Preserve the existing snapshot/reparse/selection guarantees.
7. **Flag rollout telemetry/rollback.** Add a deterministic renderer selection flag and a diagnostic page/fixture mode before broad rollout. The first version should be easy to disable without touching document data.

## KEEP / MODIFY classification

The classification is intentionally conservative. `REPLACE` is avoided unless the repository provides direct evidence that a subsystem is unusable; it does not.

| Major subsystem | Classification | Audit decision |
|---|---|---|
| Unified Electron shell/tab/runtime | KEEP + EXTEND | Preserve module runtime and IPC shape; later add SaaS/session capabilities at explicit service boundaries. |
| `packages/docx-engine` OOXML parser/model | KEEP | Do not rewrite or move into presentation-v2. Protect raw XML, anchors, parts, and unsupported content. |
| `packages/docx-engine` patch/generate/save | KEEP | This is the DOCX round-trip safety boundary. Add tests/adapters only if required. |
| Docs Tiptap/ProseMirror schema and commands | KEEP + EXTEND | Preserve node/mark/revision/comment/save contracts; add presentation-facing metadata only additively. |
| Docs dirty/save/reparse/IPC | KEEP | Presentation selection must not alter save races, dirty semantics, or round-trip. |
| Docs current pagination/layout | MODIFY selectively | Candidate for a parallel presentation-v2 algorithm behind a flag; retain old path until parity is proven. |
| Docs styles/font/layout CSS | KEEP + EXTEND | Reuse existing inputs and isolate v2 CSS/measurement adapters; avoid changing OOXML style semantics. |
| Sheets Univer/XLSX gateway/native sidecar | KEEP | Existing preservation and fail-closed boundaries are strong. |
| Slides PPTX engine/render/Konva editor | KEEP | Existing parse/render/patch separation matches 9Profs preserve-by-default plan. |
| PDF viewer/edit/save | KEEP + EXTEND | Preserve local PDF editing and byte save; add backend AI/RAG behind `PdfAiDeps`/IPC. |
| Markdown Tiptap/Markdown serialization | KEEP + EXTEND | Add research/review or gateway skills without changing plain Markdown persistence. |
| `agent-core` loop/skills/transport | KEEP | Reuse as the generic AI execution foundation. |
| `ai-provider` protocol adapters | KEEP + EXTEND | Add a 9Profs gateway adapter; isolate Genspark auth/settings. |
| `ai-search` Genspark-first implementation | MODIFY | Keep search contracts; make Genspark, 9Profs gateway, and fallback providers explicit adapters. |
| Genspark cloud generation/auth/prompt branding | MODIFY / REPLACE SELECTIVELY | Replace only the product-specific adapter/prompt portions when 9Profs services exist. |
| `project-store` local project/chat persistence | KEEP + EXTEND | Reuse local contract; add remote workspace sync behind a service rather than rewriting the store. |
| Account/workspace/billing/usage/storage/AI gateway | REBUILD SELECTIVELY | No complete 9Profs SaaS layer is confirmed in this repository; implement only these new layers, not document engines. |

## DOCX presentation refactor blast radius

### Files likely to change

Likely presentation-only changes:

- `apps/docs/src/renderer/pagination.ts`
- `apps/docs/src/renderer/line-metrics.ts`
- `apps/docs/src/renderer/doc-style-css.ts`
- relevant presentation CSS in `apps/docs/src/renderer/styles.css`
- `apps/docs/src/renderer/editor/pagination-gaps.ts`
- `apps/docs/src/renderer/editor/page-gap-nav.ts`
- `apps/docs/src/renderer/editor/column-layout.ts`
- `apps/docs/src/renderer/editor/hf-dom.ts`
- `apps/docs/src/renderer/App.tsx` only where it selects/feeds a renderer, measures pages, mounts overlays, or maps cursor geometry
- possibly `apps/docs/src/renderer/editor/protected-render.ts` for display geometry only
- new tests and fixtures under `apps/docs/tests` for parity/feature-flag behavior

### Files that should remain read-only during the first refactor

- `packages/docx-engine/src/parse.ts`
- `packages/docx-engine/src/patch.ts`
- `packages/docx-engine/src/generate.ts`
- `packages/docx-engine/src/types.ts`
- `packages/docx-engine/src/zip-load.ts`
- `packages/docx-engine/src/section.ts`
- `packages/docx-engine/src/theme.ts`
- `apps/docs/src/renderer/editor/convert.ts`, especially `blocksToPmDoc` and `pmDocToSavePlan`
- `apps/docs/src/renderer/editor/revisions.ts`
- `apps/docs/src/renderer/editor/comments.ts`
- `apps/docs/src/renderer/editor/marks.ts`
- `apps/docs/src/renderer/doc-dirty.ts`
- `apps/docs/src/renderer/file-actions.ts` save/reparse behavior
- `apps/docs/src/preload/index.ts` and `apps/docs/src/main/docs-main.ts` save/open IPC

“Read-only” here means no semantic changes in the initial presentation experiment. A later additive adapter may be necessary, but it should be reviewed as a contract change rather than bundled into layout work.

### Regression risks

- Font fallback, CJK/RTL/complex-script metrics, hyphenation, doc-grid snapping, and mixed font runs can change line breaks and all downstream pages.
- Keep-next, widow/orphan, explicit page/column breaks, section size/margin changes, continuous sections, columns, and column balancing can alter page count.
- Tables are high risk: repeated headers, row spans, nested tables, cant-split behavior, oversized rows, cell cuts, and table width measurement.
- Header/footer height and variants (`first`, `even`, default), floating images/text boxes, footnotes, and section transitions affect available page height.
- Cursor/selection coordinates, comment/revision margin anchors, and page-gap navigation can regress even when visual output looks correct.
- New renderer transactions or nested-editor updates can accidentally change dirty state or cause a save race.
- Altered model attributes can cause `pmDocToSavePlan` to regenerate blocks that were previously preserved as raw XML, increasing OOXML loss risk.
- Print/export and `PaginationPreview` can use different measurement paths from the main editor and need parity coverage.

### Safe extension points

1. Keep the existing `PresentationInput`/`PresentationOutput` concept at the `pagination.ts` types boundary.
2. Let v2 consume the current editor DOM/model and return the existing `PageSlice`, `BlockBox`, `SectionGeom`, and line/table boundary concepts.
3. Keep page-gap DOM decorations and header/footer overlays behind a renderer-owned adapter so old and v2 paths cannot both mutate the same decorations.
4. Keep model changes and AI edits routed through existing Tiptap transactions.
5. Keep all bytes and save decisions routed through `pmDocToSavePlan` and `saveDocx`.
6. Add a flag at the App/pagination orchestration boundary, not inside `docx-engine`.

### Parallel renderer recommendation

**Inference: practical, with conditions.** A parallel renderer/feature flag is safer than replacing the current paginator in place. It is practical because save uses the live editor model and dirty metadata, while pagination is an output of rendering. It is not a zero-cost switch: the renderer selection must cover measurement, page-gap decoration, header/footer DOM, floats, columns, preview, and cursor mapping as one unit. The old renderer should remain the fallback until parity and round-trip gates pass.

## Recommended next engineering step

Do not start with a broad layout rewrite. The next engineering step should be a small, reversible **DOCX presentation seam proof**:

1. Define the renderer input/output contract around the existing `BlockBox`, `SectionGeom`, `LineBox`, table-row cuts, and `PageSlice` types.
2. Add one feature flag that selects the current renderer or a no-op/parallel adapter without changing the editor model or save plan.
3. Run the two paths over a representative fixture set: simple English, CJK/doc-grid, headers/footers, multi-section, two-column, long table/repeated header, nested table, comments/revisions, float/text box, and an unsupported-content preservation fixture.
4. Compare page geometry, line/table cuts, cursor/selection mapping, and save/reopen equality. Use the existing Word/LibreOffice baselines as external reference, not as the only gate.
5. Only after that proof should the v2 line-breaking/placement algorithms be changed incrementally.

This step validates the intended boundary while preserving the requested GenOffice DOCX engine, Tiptap/ProseMirror model, dirty tracking, OOXML preservation, and round-trip save behavior.
