# 9Profs accepted architecture

Status: governance baseline for the strategic GenOffice fork.

Evidence base: `docs/9PROFS-ARCHITECTURE-AUDIT.md`. This document records accepted boundaries and classifications; it does not implement any product or SaaS layer.

## High-level architecture

```text
GenOffice strategic fork
├─ Office Core
│  ├─ Docs
│  ├─ Sheets
│  ├─ Slides
│  ├─ PDF
│  └─ Markdown
├─ Research Layer (future)
├─ AI Gateway (future)
├─ Dify adapter for PDF intelligence (future)
└─ SaaS/Product layer (future)
```

The Electron shell and shared packages support the Office Core and remain part of the platform. The root workspace is still packaged as `genoffice`/`@genoffice/*` in `package.json` and the application manifests; branding changes are separate from architecture preservation.

## Decision vocabulary

| Decision | Meaning in 9Profs |
|---|---|
| **KEEP** | Preserve the subsystem and its current contracts. Bug fixes and safety tests remain allowed. |
| **KEEP + EXTEND** | Preserve the subsystem and add narrowly-scoped adapters/capabilities at its existing boundary. |
| **MODIFY SELECTIVELY** | Change only the named layer, with compatibility tests and no implied permission to change adjacent persistence or products. |
| **NEW** | Future 9Profs capability not established as a complete subsystem in the audited repository. It must integrate through explicit contracts. |

## Accepted decisions

| Area | Decision | Accepted boundary and evidence |
|---|---|---|
| Unified shell/runtime | KEEP + EXTEND | Preserve the `BrowserWindow`/`WebContentsView` module model in `apps/shell/src/main/index.ts:193-268` and `tab-manager.ts:49-69`. Add future platform services through explicit IPC/service clients. |
| Docs product | KEEP + EXTEND | Preserve the Docs app, Tiptap/ProseMirror editor, AI tools, dirty state, and save lifecycle. `apps/docs/src/main/docs-main.ts`, `apps/docs/src/renderer/App.tsx`, and `file-actions.ts` remain the product boundary. |
| DOCX persistence | KEEP | `packages/docx-engine/src/parse.ts`, `patch.ts`, `generate.ts`, `types.ts`, and `zip-load.ts` preserve OOXML parts, raw fragments, `docxIndex` anchors, and surgical round-trip save. This is a protected boundary. |
| DOCX editing state | KEEP + EXTEND | `apps/docs/src/renderer/editor/convert.ts` and `editor/extensions.ts` remain the model bridge. Presentation work may add adapters, not replace schema or transaction semantics. |
| DOCX presentation/layout | MODIFY SELECTIVELY | The change surface is renderer-side: `apps/docs/src/renderer/pagination.ts`, `line-metrics.ts`, `doc-style-css.ts`, page-gap/column/header-footer presentation extensions, and App orchestration. Use a parallel V1/V2 seam first. |
| Sheets/XLSX | KEEP | Preserve Univer, `univer-sync.ts`, `save-actions.ts`, `xlsx-gateway.ts`, `xlsx-package-io.ts`, and the Rust sidecar. The existing fail-closed and touched-entry preservation behavior is an asset. |
| Slides/PPTX | KEEP | Preserve `packages/pptx-engine`, `packages/pptx-render`, `SlideCanvas`, and element-level patch/save. The engine/render separation already matches the preservation strategy. |
| PDF | KEEP + EXTEND | Preserve PDF.js viewing, renderer edit state, PDFium/PDF-lib operations, and `save-pdf.ts`. Add backend intelligence beside `apps/pdf/src/renderer/ai/pdf-skill.ts`, `ai/tools.ts`, and the PDF IPC boundary. |
| Markdown | KEEP + EXTEND | Preserve Tiptap Markdown, `parseDocText`, `serializeDocText`, asset lifecycle, and optional `docxExport.ts`. Future research/review skills may plug into its existing AI skill boundary. |
| Generic AI/agent core | KEEP + EXTEND | Reuse `packages/agent-core` (`AgentSkill`, `AgentTransport`, `AgentLoop`) and `packages/ai-provider` provider-neutral streaming/adapter contracts. Add 9Profs routing through a compatible transport. |
| Genspark-specific AI/auth/search | MODIFY SELECTIVELY | Isolate `GenSparkAccountStatus`, Genspark endpoint/auth settings, `gsk` search, cloud generation, and GenOffice-branded prompts. Keep generic tool and stream contracts stable. |
| Research Layer | NEW | Future research skills, review workflows, evidence handling, and manuscript/review services. No current Office Core persistence contract should depend on it. |
| AI Gateway | NEW | Future 9Profs backend boundary for auth, routing, policy, usage, retrieval, and provider/Dify orchestration. It should implement existing stream/cancel/tool-call semantics. |
| Dify PDF adapter | NEW | Future backend adapter from bounded PDF context to Dify workflows/RAG. Do not couple Dify to PDF.js, PDFium, text editing, or PDF save. |
| SaaS/Product layer | NEW | Future account, workspace, billing, usage, storage, and tenancy services. The current local `project-store` is reusable infrastructure, not a confirmed SaaS backend. |

## Office Core boundaries

### Docs/DOCX

The accepted DOCX lifecycle is:

```text
DOCX bytes
  → Docs main `loadDocx`
  → `docx-engine.parseDocx`
  → Block model + raw XML/`docxIndex`
  → `blocksToPmDoc`
  → Tiptap/ProseMirror editor state
  → renderer presentation
  → editor/AI transactions
  → `pmDocToSavePlan`
  → `buildDocBytes` / `docx-engine.saveDocx`
  → atomic write
  → reparse and state reconciliation
```

`packages/docx-engine` owns OOXML understanding and persistence. The Docs renderer owns the visual presentation. `apps/docs/src/renderer/file-actions.ts:662 (saveOnce)` remains the save/reparse authority.

### Sheets/XLSX

Sheets remains an independent Office Core subsystem: Univer and the renderer journal provide editing state; `apps/sheets/src/gateway/xlsx-gateway.ts:600 (planCellEditsToXlsx)` and `xlsx-package-io.ts:149 (saveWorkbookViaSidecar)` provide preservation-oriented save; the Rust sidecar handles archive/range/recalculation work. DOCX tasks must not cross this boundary.

### Slides/PPTX

Slides remains an independent Office Core subsystem: `packages/pptx-engine/src/parse.ts:153 (parseSlide)` and `index.ts:622/640 (savePptx/savePptxToFile)` own PPTX model and archive save; `packages/pptx-render/src/build-slide.ts:121 (buildRenderSlide)` and `text-layout.ts:783 (layoutText)` own presentation rendering; `apps/slides/src/renderer/SlideCanvas.tsx:510` owns canvas editing. DOCX tasks must not modify it.

### PDF

PDF remains a local viewer/editor. `apps/pdf/src/renderer/PdfPage.tsx:47` owns page/text-layer presentation; `apps/pdf/src/main/text-edit.ts` owns PDFium-backed text extraction/edit validation; `apps/pdf/src/main/save-pdf.ts:839/863` owns byte save. The future Dify integration is a backend AI service, not a new PDF persistence engine.

### Markdown

Markdown remains plain-file-first. `apps/markdown/src/renderer/markdown/docText.ts:21 (parseDocText)` and `:109 (serializeDocText)` own envelope/body round-trip; `apps/markdown/src/renderer/App.tsx:172-207` owns editor loading; `apps/markdown/src/renderer/export/docxExport.ts:260/294` is an explicit optional export path.

## Future layers

### Research Layer — NEW

The Research Layer may provide research skills, evidence extraction, review, citation/provenance, and future manuscript workflows. It must consume document context through bounded skill/tool contracts. It must not become a hidden dependency of `docx-engine`, Sheets, Slides, PDF save, or Markdown serialization.

### AI Gateway — NEW

The gateway should sit between app-specific skills/transports and provider-specific main-process adapters:

```text
app skill + document tools/context
  → AgentTransport-compatible 9Profs gateway client
  → 9Profs backend
  → provider routing, policy, retrieval, usage, billing, Dify adapters
  → streamed tool calls/results and cancellation
```

The existing contracts to preserve are `packages/agent-core/src/types.ts:86-111`, `agent-core/src/electron-transport.ts:67`, and `ai-provider/src/stream.ts:16 (streamForProvider)`. The gateway must not force document products to know about provider routing or tenancy.

### Dify adapter for PDF intelligence — NEW

The future flow is:

```text
PDF UI / PDF AI skill
  → bounded PDF context request
  → 9Profs backend
  → Dify workflow/RAG application
  → grounded response or approved tool intent
  → PDF AI panel / existing local pending-edit tools
```

The clean existing seam is `apps/pdf/src/renderer/ai/tools.ts:26 (PdfAiDeps)`, `pdf-skill.ts`, `ai/transport.ts`, and `apps/pdf/src/main/pdf-main.ts:724 (registerPdfIpc)`. Dify is not currently integrated and must not be added as part of a DOCX or PDF UI refactor.

### SaaS/Product layer — NEW

Account, workspace, billing, usage, storage, and gateway policy are future product services. `packages/project-store/src/store.ts:77` and `project-store/src/ipc.ts:74-75` provide local persistence contracts that may be extended or wrapped, but they do not establish a remote multi-tenant architecture.

## Governance invariants

1. Preserve GenOffice Office Core behavior unless a task names a specific compatibility change.
2. DOCX persistence and editing state are independent of presentation-v2.
3. A DOCX presentation task cannot alter Sheets, Slides, PDF, Markdown, or shell behavior without explicit authorization.
4. AI changes documents through existing document/editor commands, never by mutating presentation DOM.
5. Future layers integrate through typed boundaries and do not leak SaaS/provider concerns into file-format engines.
6. SuperDoc is an architecture/design reference only by default. Do not copy AGPL source into 9Profs; any direct code reuse requires separately approved commercial/license review.
7. Casual Docs is an architecture and implementation reference. Study, adapt, or port implementation only when the relevant source/license permits it; preserve required copyright notices, attribution, and license obligations. Do not blindly copy whole subsystems; prefer narrow presentation components adapted to GenOffice contracts.
8. GenOffice remains the primary implementation base. Casual Docs and SuperDoc must not replace `packages/docx-engine`, Tiptap/ProseMirror document state, save/round-trip, comments/revisions, or dirty tracking.
9. Every selective modification requires focused tests, preservation evidence, and an explicit stop condition.
