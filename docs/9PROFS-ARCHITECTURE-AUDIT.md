# 9Profs architecture re-baseline audit

Audit date: 2026-08-23

This audit records confirmed current-branch facts. Source code, package
manifests, tests, and read-only Git comparisons are authoritative; older
architecture prose is not. The accepted target and migration plan are in
[9PROFS-ARCHITECTURE.md](9PROFS-ARCHITECTURE.md).

## Audit scope and method

Inspected:

- root `package.json`, workspace scripts, app/package manifests, and `AGENTS.md`;
- `apps/docs`, `apps/sheets`, `apps/slides`, `apps/pdf`, `apps/markdown`, and
  `apps/shell` source and test paths;
- `packages/docx-engine`, `pptx-engine`, `pptx-render`, `agent-core`,
  `ai-provider`, `ai-search`, `project-store`, `file-parse`, `pdf2docx`,
  `font-metrics`, `electron-utils`, `i18n`, and `ui`;
- DOCX presentation V2 source, unit tests, fixture tests, and
  `e2e/docs-presentation-parity.spec.ts`;
- existing architecture documents and a read-only
  `baseline/genoffice..develop` comparison.

No source code, dependency, baseline, or generated file was changed for this
audit.

## Confirmed repository architecture

The root is a private `genoffice` workspace with `apps/*` and `packages/*`
workspaces. Product scripts run tests and typechecks for the shared packages,
Docs, Sheets, Shell, Slides, PDF, and Markdown.

| Current area             | Confirmed implementation                                                                                                   | Current status                                |
| ------------------------ | -------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------- |
| Electron shell           | `apps/shell/src/main/index.ts`, `tab-manager.ts`, preload and renderer UI; hosts app views/tabs                            | Implemented                                   |
| Docs/DOCX                | `apps/docs` plus `packages/docx-engine`; Tiptap/ProseMirror model, renderer pagination, AI tools, dirty/save/reparse paths | Implemented; canonical Office editor          |
| Sheets/XLSX              | Univer renderer, `apps/sheets/src/gateway/xlsx-gateway.ts`, `xlsx-package-io.ts`, Rust `native/xlsx-engine`                | Implemented; independent Office editor        |
| Slides/PPTX              | `packages/pptx-engine`, `packages/pptx-render`, `apps/slides/src/renderer/SlideCanvas.tsx`                                 | Implemented; independent Office editor        |
| PDF                      | PDF.js viewer/editor UI, PDFium/PDF-lib main-process operations, `apps/pdf/src/main/save-pdf.ts`                           | Implemented; independent Office editor        |
| Markdown                 | Tiptap editor, `apps/markdown/src/renderer/markdown/docText.ts`, optional `export/docxExport.ts`                           | Implemented; plain-file-first                 |
| Shared AI                | `agent-core`, `ai-provider`, `ai-search`, and app-level AI skills/tools/transports                                         | Implemented local foundation; not 9Profs Core |
| Local workspace data     | `packages/project-store` local projects/chats/attachments                                                                  | Implemented local persistence; not SaaS       |
| Phase 0 contracts        | `packages/9profs-core`, `packages/document-gateway`, and compile-checked adapter seams                                     | Implemented; contracts only                   |
| Research/product backend | No research domain, runtime, OfficeCLI process integration, or account/billing backend                                     | Future                                        |

## GenOffice inheritance and current divergence

Current root/package names and Office applications remain GenOffice-derived.
The branch comparison shows no rewrite or replacement of the Office Core.

The read-only comparison reports 42 changed files and 10,091 added lines,
concentrated in the Docs presentation/layout and its proof surface. Confirmed
divergence includes:

- expanded Docs `App.tsx`, `pagination.ts`, `line-metrics.ts`,
  `PaginationPreview.tsx`, and presentation-related editor behavior;
- new `apps/docs/src/renderer/presentation-v2/` implementation modules;
- V2 geometry, diagnostics, dirty-range, measurement, section, and seam tests;
- Docs caret/performance/parity end-to-end coverage;
- architecture and governance documents.

The current `agent-core`, `ai-provider`, and `ai-search` packages are present as
GenOffice-shaped AI infrastructure. They are not a 9Profs backend or a
document-mutation gateway.

## DOCX presentation V2 finding

The former description of V2 as future/design-only is incorrect.

### Implemented

- `presentation-v2/index.ts` defines V1/V2 renderer types, V1 defaulting, and
  internal renderer resolution.
- `page-slicer.ts` owns V2 pagination orchestration and bounded refinement;
  it reuses established GenOffice block/page/section primitives rather than
  replacing the DOCX model.
- `measurement.ts`, `measurement-context.ts`, and
  `measurement-invalidation.ts` implement measurement refinement and explicit
  local/full invalidation policy.
- `geometry.ts`, `geometry-probes.ts`, `post-render.ts`, and `diagnostics.ts`
  provide normalized geometry, post-render capture, probe readback, parity
  comparison, and categorized differences.
- Docs `App.tsx` selects the renderer, records V2 performance/invalidation
  data, captures post-render state, and exposes read-only `__pageDebug` data for
  browser diagnostics.

### Partial or experimental

- V2 is not the default; V1 remains the fallback/compatibility baseline.
- V2 owns a substantial derived-layout path but continues to reuse GenOffice
  pagination and measurement primitives.
- Unit and browser parity coverage is substantial, but this audit does not
  claim complete default-readiness across every Word/LibreOffice, font,
  long-document, note, float, revision, or unsupported-content case.

### Protected

V2 does not own `packages/docx-engine`, Tiptap/ProseMirror state,
`blocksToPmDoc`, `pmDocToSavePlan`, dirty state, save/reparse, comments,
revisions, anchors, or OOXML identity. Existing V2 tests explicitly compare
model, dirty, save-plan, saved-byte, and reopen behavior for representative
fixtures.

## Existing AI package audit

| Package                | Classification | Finding                                                                                                                                                                                      |
| ---------------------- | -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `packages/agent-core`  | ADAPT          | Reusable `AgentLoop`, `AgentSkill`, tool execution, stream callbacks, IPC transport, and payload sanitization. Needs a 9Profs runtime/assistant/policy wrapper.                              |
| `packages/ai-provider` | ADAPT          | Reusable provider protocols, registry, streaming/chat, watchdogs; also contains GenSpark account, endpoint, attribution, and settings policy that should be isolated behind future adapters. |
| `packages/ai-search`   | ADAPT          | Working Genspark-first web/image search with Serper and DuckDuckGo fallbacks. Preserve behavior; later expose it as an external/research adapter with provenance.                            |

No package is removed, phased out, or replaced by this audit.

## Ownership findings

1. GenOffice app/editor state is the current source of truth for active Office
   editing.
2. Format engines and existing app save paths are the source of truth for
   persisted Office bytes. DOCX persistence is protected in `docx-engine`.
3. Current AI state is per-run/local in app panels, agent loop, and provider
   streams. No shared 9Profs runtime state owner exists.
4. Current skills are app-local; no assistant or shared skill registry exists.
5. Current search/provider/native integrations are adapters, not an Office
   ownership layer.
6. No research domain owns evidence or review state yet.
7. OfficeCLI must be introduced as an inspection/rendering/detached-generation
   sidecar, never as a competing writer for an active GenOffice file.

## Required future boundaries

The target architecture proposes these compatible future boundaries:

- `packages/9profs-core/` for runtime, workspace, policy, events, and service
  composition;
- `packages/9profs-core/src/agent-runtime/` for agent runs;
- `packages/9profs-core/src/assistant-registry/` for assistant descriptors and
  allowed capabilities;
- `packages/9profs-core/src/skills/`, `mcp/`, and `extensions/` for shared
  capability registration and lifecycle;
- `packages/research-domain/` for research/review/citation/regulation;
- `packages/document-gateway/` for `DocumentChangeSet`, snapshots, and active
  document ownership/mutation policy;
- `packages/genoffice-adapter/` for approved editor transactions and existing
  save/reparse integration;
- `packages/officecli-adapter/` for pinned OfficeCLI calls and detached-file
  policy.

Phase 0 contract portions are implemented in `packages/9profs-core/` and
`packages/document-gateway/`. `packages/genoffice-adapter/` and
`packages/officecli-adapter/` are compile-checked skeletons only.

The AionCore runtime, OfficeCLI integration, GenOffice mutation adapter, and
research domain remain NOT IMPLEMENTED.

## Migration conclusion

Next work starts with contracts and V2 proof, then builds 9Profs Core around
existing GenOffice behavior. Agent/skills/MCP/extensions follow. OfficeCLI is
introduced as a sidecar with ownership checks. The document gateway then routes
approved `DocumentChangeSet` values into GenOffice transactions. Research and
SaaS/product services come after those boundaries are proven.

The full migration matrix and Phase 0–6 sequence are maintained in
[9PROFS-ARCHITECTURE.md](9PROFS-ARCHITECTURE.md).
