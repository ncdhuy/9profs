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

| Current area              | Confirmed implementation                                                                                                   | Current status                                |
| ------------------------- | -------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------- |
| Electron shell            | `apps/shell/src/main/index.ts`, `tab-manager.ts`, preload and renderer UI; hosts app views/tabs                            | Implemented                                   |
| Docs/DOCX                 | `apps/docs` plus `packages/docx-engine`; Tiptap/ProseMirror model, renderer pagination, AI tools, dirty/save/reparse paths | Implemented; canonical Office editor          |
| Sheets/XLSX               | Univer renderer, `apps/sheets/src/gateway/xlsx-gateway.ts`, `xlsx-package-io.ts`, Rust `native/xlsx-engine`                | Implemented; independent Office editor        |
| Slides/PPTX               | `packages/pptx-engine`, `packages/pptx-render`, `apps/slides/src/renderer/SlideCanvas.tsx`                                 | Implemented; independent Office editor        |
| PDF                       | PDF.js viewer/editor UI, PDFium/PDF-lib main-process operations, `apps/pdf/src/main/save-pdf.ts`                           | Implemented; independent Office editor        |
| Markdown                  | Tiptap editor, `apps/markdown/src/renderer/markdown/docText.ts`, optional `export/docxExport.ts`                           | Implemented; plain-file-first                 |
| Shared AI                 | `agent-core`, `ai-provider`, `ai-search`, and app-level AI skills/tools/transports                                         | Implemented local foundation; not 9Profs Core |
| Local workspace data      | `packages/project-store` local projects/chats/attachments                                                                  | Implemented local persistence; not SaaS       |
| Phase 0 contracts         | `packages/9profs-core`, `packages/document-gateway`, and compile-checked adapter seams                                     | Implemented; contracts only                   |
| Rust Core foundation      | `9profs-core-rs/` common/API/SQLite/realtime/runtime/app crates; loopback HTTP/WebSocket bootstrap                         | Implemented; foundation only                  |
| Phase 1B Assistant domain | `nineprofs-assistant`; builtin/custom assistants, Rules, CRUD persistence, skill bindings, backend metadata reference      | Implemented                                   |
| Phase 1B Skills catalog   | `nineprofs-skills`; builtin/custom `SKILL.md` loading, deterministic precedence, extension-ready provider boundary         | Implemented                                   |
| Research/product backend  | No research domain, OfficeCLI process integration, or account/billing backend                                              | Future                                        |

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
3. Current GenOffice AI state remains per-run/local in app panels, agent loop,
   and provider streams. Phase 2B1 adds an independent Rust Core execution
   path; it does not replace the current app AI path.
4. Phase 1B owns assistant and shared skill catalog metadata in
   `9profs-core-rs`; Phase 2B1 composes those records through
   `AgentExecutionService` and an executor boundary.
5. Current search/provider/native integrations are adapters, not an Office
   ownership layer.
6. No research domain owns evidence or review state yet.
7. OfficeCLI must be introduced as an inspection/rendering/detached-generation
   sidecar, never as a competing writer for an active GenOffice file.

## Required future boundaries

The target architecture proposes these compatible future boundaries:

- `packages/9profs-core/` for runtime, workspace, policy, events, and service
  composition;
- `9profs-core-rs/crates/nineprofs-runtime/` for execution composition and
  `nineprofs-agent/` for the 9Profs-owned executor boundary and task runtime;
- `9profs-core-rs/crates/nineprofs-assistant/` for assistant descriptors,
  Rules, persistence, and stable skill bindings;
- `9profs-core-rs/crates/nineprofs-skills/` for shared skill metadata and
  resource loading; TypeScript `packages/9profs-core` remains transport-only;
- future `mcp/` and `extensions/` runtime boundaries for capability
  registration and lifecycle;
- `packages/research-domain/` for research/review/citation/regulation;
- `packages/document-gateway/` for `DocumentChangeSet`, snapshots, and active
  document ownership/mutation policy;
- `packages/genoffice-adapter/` for approved editor transactions and existing
  save/reparse integration;
- `packages/officecli-adapter/` for pinned OfficeCLI calls and detached-file
  policy.

Phase 0 contract portions are implemented in `packages/9profs-core/` and
`packages/document-gateway/`. Phase 1A Rust Core foundation and Phase 1B
Assistant/Skills foundation are implemented in `9profs-core-rs/`;
`packages/genoffice-adapter/` and
`packages/officecli-adapter/` are compile-checked skeletons only.

Phase 2A Agent Registry + Task Lifecycle Foundation is implemented in
`9profs-core-rs/crates/nineprofs-agent/`. It owns metadata-only backend
descriptors, builtin/custom catalog hydration, deterministic registry lookup,
explicit availability, independent `RunId`/`AgentTaskId` identities,
concurrency-safe task lifecycle, cancellation, and transport-safe realtime
events. Assistant remains a configured persona and stores only a stable
backend ID; it does not depend on agent executor types.

Phase 2A source audit used the pinned local AionCore checkout
`D:\startup\upstream\AionCore` at
`7ac84f93c5f81e1b1cc41f8119c089df72d63afc`, including
`crates/aionui-ai-agent/src/registry.rs`, `registry_tests.rs`,
`task_manager.rs`, `agent_task.rs`, `lib.rs`, metadata repository/API types,
and application composition wiring. Reused concepts are catalog hydration,
stable IDs, repository boundaries, deterministic ordering, explicit
availability, and concurrent task ownership. Phase 2B1 additionally inspected
`agent_runtime.rs`, `agent_task.rs`, `task_manager.rs`, `factory/`, `manager/`,
`protocol/`, `session_context.rs`, and service composition at the same pinned
SHA. AionRS tag `v0.2.11` resolves to
`8e61a90329fa9f67c4fdf7e97fe02c24dba33f75`; its `aion-agent`, `aion-config`,
`aion-providers`, `aion-types`, `aion-protocol`, `aion-mcp`, and `aion-tools`
sources were inspected. AionCore's real dependency edge is pinned to that
tag, not a floating branch.

Phase 2B1 real execution is IMPLEMENTED in `nineprofs-agent`,
`nineprofs-runtime`, `nineprofs-api-types`, and the core app transport. The
9Profs-owned `AgentExecutor` boundary keeps AionRS types inside
`AionRsExecutor`; the direct AionRS engine is constructed with an empty tool
registry. Therefore shell, file mutation, subprocess, MCP, sub-agent, and
upstream global-skill discovery capabilities are disabled. Provider/model
configuration is launch-scoped through `NINEPROFS_AGENT_*` variables, with the
API-key environment variable name retained but never the secret. Phase 2B1.1
requires explicit provider and model values, reports invalid configuration as
unavailable, clears AionRS hooks/configured skill permissions, and defers
dependency-size optimization; environment configuration remains temporary.

Still NOT IMPLEMENTED: ACP/external CLI backends, MCP, full Extensions
runtime, OfficeCLI integration, GenOffice mutation adapter, research domain,
conversation/session persistence, and the GenOffice AI bridge.

Phase 1B upstream adaptation record: assistant resource/catalog patterns came
from `crates/aionui-assistant/src/builtin.rs` and `service.rs`; SKILL.md
discovery, source handling, and configured path safety came from
`crates/aionui-extension/src/skill_service.rs`, `loader.rs`, and
`asset_paths.rs`; representative resources came from
`crates/aionui-app/assets/builtin-assistants` and `builtin-skills`.
`nineprofs-assistant` and `nineprofs-skills` contain 9Profs-owned adaptations;
no AionRS, agent runtime, MCP, OfficeCLI, or frontend architecture was copied.

## Migration conclusion

Next work starts with contracts and V2 proof, then builds 9Profs Core around
existing GenOffice behavior. Agent runtime/MCP/extensions runtime follow.
OfficeCLI is
introduced as a sidecar with ownership checks. The document gateway then routes
approved `DocumentChangeSet` values into GenOffice transactions. Research and
SaaS/product services come after those boundaries are proven.

The full migration matrix and Phase 0–6 sequence are maintained in
[9PROFS-ARCHITECTURE.md](9PROFS-ARCHITECTURE.md).
