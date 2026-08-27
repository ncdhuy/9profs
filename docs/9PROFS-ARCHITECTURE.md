# 9Profs architecture baseline

Status: canonical architecture and migration baseline for the current
`develop` branch. Audited 2026-08-27 from repository source, manifests, tests,
and a read-only comparison with `baseline/genoffice`.

This document describes what exists, what remains GenOffice-derived, and the
target boundaries for 9Profs. The pinned OfficeCLI sidecar is implemented for
read-only inspection plus transactional detached creation and mutation;
the Phase 5B1 reference-PDF ingestion seam, Phase 5B2 scoped retrieval, Phase
5C1 citation binding domain, Phase 5C2A citation verification orchestration,
Phase 5C2B model-backed citation assessment, Phase 5C3A inline citation
inventory, Phase 5C3B1 live manuscript citation synchronization, and Phase
5C3B2 atomic manuscript claim extraction are implemented. Citation UI and SaaS
services remain future work.

## Non-negotiable rules

1. `baseline/genoffice` is reference-only and must never be modified.
2. GenOffice-derived Office behavior remains authoritative for interactive
   document editing.
3. One active Office document has one canonical write authority. For a file
   actively opened in GenOffice, GenOffice owns editor state and canonical
   save. OfficeCLI may inspect, query, render, validate, or write a detached
   copy, but must not compete for the active file's canonical write.
4. AI changes documents through typed tools and editor/document transactions.
   It must not mutate presentation DOM or bypass save/reparse contracts.
5. `packages/docx-engine` is a protected persistence boundary. Its parse, patch,
   generate, raw XML, anchors, and round-trip contracts are not replaced by a
   new core or external tool.

## Current repository topology

The repository is a TypeScript/Electron workspace whose root and package
manifests still use `genoffice` and `@genoffice/*`. `package.json` defines
`apps/*` and `packages/*` workspaces and runs product tests/typechecks in
dependency order.

```text
Product Layer
└─ apps/shell + packages/project-store + local settings/recent files

Research Domain Layer (Phases 5A, 5B1, 5B2, 5C1, 5C3B1, and 5C3B2 implemented)
└─ nineprofs-research provenance, evidence, claims, citation occurrences/targets/bindings/sync

AI / Agent Core
├─ packages/agent-core       current loop, skills, tools, IPC transport
├─ packages/ai-provider      current provider protocols and streaming
├─ packages/ai-search        current Genspark/Serper/DuckDuckGo search
└─ 9Profs runtime, assistants, skills, MCP; extensions remain future

Document Integration Layer
├─ current apps/*/src/*/ai tools and document commands
└─ future DocumentChangeSet, DocumentMutationGateway, adapters, ownership

GenOffice-derived Office Core
├─ apps/docs + packages/docx-engine
├─ apps/sheets + XLSX gateway + Rust sidecar
├─ apps/slides + packages/pptx-engine + packages/pptx-render
├─ apps/pdf + PDF.js/PDFium/PDF-lib paths
└─ apps/markdown + Tiptap Markdown serialization

External Tool Adapters
├─ current provider/search/file-format/native sidecar integrations
└─ future OfficeCLI and optional Dify-backed workflows
```

The Rust Core runtime lives in `9profs-core-rs/` behind an HTTP/WebSocket
transport boundary. It is not a TypeScript package dependency and does not own
Office document state or persistence.

The Phase 2B1 portion of that runtime owns Assistant/Rules/Skills composition,
agent backend resolution, task execution, AionRS provider adaptation, and
transport-safe streaming. MCP, extensions, and external agent backends remain
outside the implemented boundary.

The Electron shell hosts the Office applications; it is not a second Office
document writer. Shared packages such as `file-parse`, `pdf2docx`,
`font-metrics`, `electron-utils`, `i18n`, and `ui` remain supporting
infrastructure.

## Implemented, inherited, diverged, and future

### Implemented today

| Area                               | Evidence                                                                                                                      | Status                                                 |
| ---------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------ |
| Electron document workspace        | `apps/shell/src/main/index.ts`, `tab-manager.ts`; standalone app entrypoints under `apps/*/src/main` and `src/renderer`       | Implemented                                            |
| DOCX persistence                   | `packages/docx-engine/src/parse.ts`, `patch.ts`, `generate.ts`, `types.ts`, `zip-load.ts`                                     | Implemented; protected                                 |
| DOCX editing                       | `apps/docs/src/renderer/editor/convert.ts`, Tiptap/ProseMirror extensions, comments, revisions, selections, undo/redo         | Implemented; GenOffice authority                       |
| DOCX presentation V1               | `apps/docs/src/renderer/pagination.ts`, `line-metrics.ts`, `doc-style-css.ts`, pagination-gap/header-footer/column extensions | Implemented; default compatibility baseline            |
| DOCX presentation V2               | `apps/docs/src/renderer/presentation-v2/*`, App wiring, unit tests, and `e2e/docs-presentation-parity.spec.ts`                | Implemented experimental; V1 remains default           |
| XLSX editing and preservation save | Univer renderer, `apps/sheets/src/gateway/xlsx-gateway.ts`, `xlsx-package-io.ts`, and `apps/sheets/native/xlsx-engine`        | Implemented; independent Office Core                   |
| PPTX model, render, and save       | `packages/pptx-engine`, `packages/pptx-render`, `apps/slides/src/renderer/SlideCanvas.tsx`                                    | Implemented; independent Office Core                   |
| PDF viewer/editor                  | PDF.js renderer, PDFium/PDF-lib main-process operations, `apps/pdf/src/main/save-pdf.ts`                                      | Implemented; independent Office Core                   |
| Markdown editor/serialization      | `apps/markdown/src/renderer/markdown/docText.ts`, Tiptap editor, `export/docxExport.ts`                                       | Implemented; plain-file-first                          |
| Local AI foundation                | `packages/agent-core`, `packages/ai-provider`, `packages/ai-search`, app-specific AI tools/skills                             | Implemented; GenOffice-shaped, not yet 9Profs Core     |
| Local project/chat persistence     | `packages/project-store`                                                                                                      | Implemented; not a remote multi-tenant product backend |

### Inherited or retained from GenOffice

The root identity, workspace layout, Office applications, format engines,
Electron shell, Tiptap/ProseMirror Docs state, and current AI packages remain
GenOffice-derived or GenOffice-shaped. The current branch's manifests still
confirm that identity. A read-only `baseline/genoffice..develop` comparison
shows no current-branch architecture replacement of the Office Core.

### Diverged from the baseline

The read-only comparison reports 42 changed files and 10,091 added lines,
concentrated in the Docs renderer and its validation surface. Material current
branch divergence includes:

- expanded Docs `App.tsx`, pagination, line metrics, pagination preview, and
  presentation-related editor behavior;
- new `apps/docs/src/renderer/presentation-v2/` modules for page slicing,
  sections, measurement context/refinement, invalidation, performance,
  geometry, post-render capture, probes, and diagnostics;
- new Docs V2 unit tests, geometry tests, caret hit-testing/performance tests,
  and `e2e/docs-presentation-parity.spec.ts`;
- current architecture/governance documentation.

`packages/agent-core`, `packages/ai-provider`, and `packages/ai-search` are
present and useful, but are not evidence that 9Profs Core has been implemented.

### Phase 0 status

| Area                                          | Status      | Evidence                                                                                                                                                                                |
| --------------------------------------------- | ----------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Phase 0 contracts                             | IMPLEMENTED | `packages/9profs-core/`, `packages/document-gateway/`, and compile-checked adapter seams                                                                                                |
| Phase 1A Rust Core                            | IMPLEMENTED | `9profs-core-rs/` transport/runtime foundation; no product domains                                                                                                                      |
| Phase 2A agent metadata/catalog               | IMPLEMENTED | `nineprofs-agent` descriptors, builtin catalog, minimal SQLite custom metadata persistence                                                                                              |
| Phase 2A Agent Registry                       | IMPLEMENTED | hydrated authoritative catalog, stable lookup/order, explicit availability, custom updates                                                                                              |
| Phase 2A task lifecycle                       | IMPLEMENTED | `RunId`, `AgentTaskId`, state transitions, cancellation, ownership, lifecycle events                                                                                                    |
| Phase 2B1 real agent execution                | IMPLEMENTED | 9Profs executor boundary, AionRS backend, streaming, cancellation, and run APIs                                                                                                         |
| Phase 2C1 MCP Tool Provider                   | IMPLEMENTED | `nineprofs-mcp`, SQLite config, pinned AionRS client mechanics, shared ToolRegistry provider                                                                                            |
| Phase 3A OfficeCLI read-only provider         | IMPLEMENTED | `nineprofs-officecli`, pinned v1.0.144 sidecar, typed read-only tools, HTML-to-PNG raster boundary, artifact boundary, status API                                                       |
| Phase 3B OfficeCLI detached mutation          | IMPLEMENTED | Typed create/mutation tools, writable eligibility, copy-on-write revisions, validation, HTML-to-PNG render gate, and atomic promotion                                                   |
| Phase 4A active DOCX GenOffice adapter        | IMPLEMENTED | Active inspection, DocumentVersion/preconditions, stale-change protection, approved mutation gateway, existing Docs command engine reuse                                                |
| Phase 4B Rust Core ↔ renderer bridge          | IMPLEMENTED | Active document registry, dedicated bidirectional `/ws/documents`, DOCX inspection proxy, approved mutation proxy, and version synchronization                                          |
| Phase 4C1 active-document proposals           | IMPLEMENTED | `nineprofs-document-tools`, explicit list/inspect/propose tools, ephemeral proposal store, freshness, read-only proposal APIs/events                                                    |
| Phase 4C2 proposal review/live commit         | IMPLEMENTED | Core-owned review workflow, trusted approve/reject/retry endpoints, bounded renderer idempotency, and existing Docs AI-panel command summaries                                          |
| Phase 4D1 Core-owned active Docs runs         | IMPLEMENTED | Trusted Docs run API, server-owned proposal-only ToolSet, per-run document scope enforcement, and typed `/ws` event client                                                              |
| Phase 4D1.1 Docs Core Agent readiness         | IMPLEMENTED | Executable `document-foundation` binding, Core-owned Docs Agent profile, truthful readiness diagnostics, and safe profile API                                                           |
| Phase 4D1.2 Docs Core conversation continuity | IMPLEMENTED | Core-owned conversation identity, bounded ephemeral state, transactional multi-turn AionRS context, and conversation-bound Docs scope                                                   |
| Phase 4D2 Docs AiPanel Core migration         | IMPLEMENTED | Core-default fresh text chat, streamed multi-turn UI, safe tool activity, proposal review integration, and Legacy attachment/history fallback                                           |
| Research domain                               | IMPLEMENTED | `nineprofs-research`, migrations `0005`–`0010`, content-addressed PDF artifacts, scoped retrieval, evidence/claims, 5C1 citation binding, 5C2A orchestration, and 5C2B model assessment |

### Phase 1A Rust Core foundation

The Phase 1A Rust Core foundation is intentionally limited to:

- `nineprofs-core-rs/` with common, API DTO, SQLite, realtime, runtime, and
  composition-root crates;
- `/api/health`, `/api/runtime`, safe `/api/documents` metadata endpoints,
  generic `/ws` events, and dedicated `/ws/documents` active-session transport;
- loopback-only default binding at `127.0.0.1:39761` with no wildcard CORS;
- a reserved launch-scoped session-secret field, with authentication deferred;
- `packages/9profs-core/src/transport.ts` as an optional TypeScript mapping.
  It does not import Rust code or replace Electron IPC.

AionCore audit source: commit
`7ac84f93c5f81e1b1cc41f8119c089df72d63afc` on `main`. Phase 2B1 also
inspected `crates/aionui-ai-agent/src/agent_runtime.rs`, `agent_task.rs`,
`task_manager.rs`, `factory/`, `manager/`, `protocol/`, `session_context.rs`,
and service composition. AionCore's pinned root manifest identified the
AionRS `v0.2.11` dependency boundary. Adapted source patterns came from
`ARCHITECTURE.md`, the root `Cargo.toml`, `crates/aionui-common`,
`aionui-api-types`, `aionui-db`, `aionui-realtime`, `aionui-runtime`, and
`aionui-app`. GenOffice baseline inspection used commit
`f68df70e222d47aa08211f9a2d7748c610d1d6aa` on `main`.

## Phase 0–1B implementation status

- Phase 0 contracts — IMPLEMENTED in `packages/9profs-core/`.
- Phase 1A Rust Core foundation — IMPLEMENTED in `9profs-core-rs/` common,
  API-types, SQLite, realtime, runtime, and app crates.
- Phase 1B Assistant domain — IMPLEMENTED in
  `9profs-core-rs/crates/nineprofs-assistant/`, including builtin/custom
  catalogs, Rules, SQLite CRUD, ordered skill bindings, and metadata-only
  backend-agent references.
- Phase 1B Skills catalog/loading — IMPLEMENTED in
  `9profs-core-rs/crates/nineprofs-skills/`, including embedded representative
  builtin resources, configured-root custom `SKILL.md` discovery, malformed
  skill reporting, and extension-ready provider boundary.
- Phase 1B Assistant ↔ Skills binding — IMPLEMENTED. Assistants persist stable
  skill IDs; service validation resolves IDs through `SkillCatalog`.

### Phase 2A — Agent Registry + Task Lifecycle Foundation

Phase 2A is IMPLEMENTED in `9profs-core-rs/crates/nineprofs-agent/` and the
runtime composition/API boundaries:

- `AgentBackendDescriptor` is metadata only: stable ID, name, description,
  source, kind, capabilities, explicit availability, enabled state, order,
  version, and timestamps.
- `AgentRegistry` hydrates builtin/resource descriptors and persisted custom
  descriptors, owns deterministic list/lookup/resolution behavior, rejects
  duplicate IDs, and publishes `agent.registryChanged`.
- `AgentTaskManager` owns independent `RunId` and `AgentTaskId` identities,
  concurrency-safe task state, cancellation signals, terminal cleanup, and
  `agent.task*` lifecycle events. Tasks are not keyed by conversation ID.
- Assistant records continue to store only the stable `backend_agent_id`
  string. Runtime resolution returns explicit not-configured, missing, unknown,
  unavailable, disabled, or resolved outcomes; Assistant does not depend on
  executor/process types.
- Read-only HTTP catalog endpoints are available at `/api/agents` and
  `/api/agents/:id`. Phase 2B1 adds `POST /api/agent-runs`,
  `GET /api/agent-runs/:run_id`, `GET /api/agent-runs/:run_id/tasks`, and
  `POST /api/agent-tasks/:task_id/cancel`.

The pinned AionCore audit source is
`7ac84f93c5f81e1b1cc41f8119c089df72d63afc` at the local read-only clone
`D:\startup\upstream\AionCore`. Phase 2A inspected
`crates/aionui-ai-agent/src/registry.rs`, `registry_tests.rs`,
`task_manager.rs`, `agent_task.rs`, `lib.rs`, the agent metadata repository/API
types, and `aionui-app/src/services.rs` composition wiring.

Adapted concepts: metadata as catalog source of truth, startup hydration,
stable backend IDs, deterministic ordering, repository boundaries, explicit
availability, serialized registry mutation, and concurrency-safe task
ownership/lifecycle patterns.

Intentionally deferred: CLI discovery/path resolution, `--version` probes,
ACP handshakes, model discovery, provider health checks, user CLI environment
management, installer/update behavior, conversation sessions, leases, idle
cleanup, warmup, background subagents, and external `AgentFactory`/executor
implementations. Builtin Codex/Claude descriptors remain `unknown` and no
external binary is invoked.

### Phase 2B1 — Real AionRS agent execution

Phase 2B1 is IMPLEMENTED behind a 9Profs-owned execution boundary:

```text
Assistant -> AgentRegistry -> AgentExecutionService -> AgentExecutor
          -> AionRsExecutor -> AionRS -> configured LLM provider
```

- `AgentExecutor` exposes only transport-neutral request, result, event, and
  cancellation concepts. AionRS, ACP, CLI, and provider SDK types stop at the
  concrete adapter.
- `AgentExecutorRegistry` maps backend IDs to executors. The only executable
  backend is `nineprofs-default`, implemented by `AionRsExecutor`.
- The adapter uses the pinned AionRS `v0.2.11` engine directly with an empty
  `aion_tools::ToolRegistry`. This preserves the upstream run/stream loop while
  explicitly disabling shell, file mutation, subprocess, MCP, sub-agent, and
  upstream global-skill discovery capabilities. AionRS config hooks, shell
  settings, and skill permission lists are cleared before construction as well.
- Provider configuration is provider-neutral and launch-scoped. Provider and
  model are both required; no implicit model fallback exists. Optional base URL
  and API-key environment-variable reference are read from `NINEPROFS_AGENT_*`
  variables. Empty values, unsupported providers, missing credentials, and
  invalid endpoints make `nineprofs-default` unavailable with an explicit
  reason. Secrets never enter DTOs, events, logs, or errors.
- The real-provider smoke test is opt-in with
  `NINEPROFS_RUN_REAL_AGENT_SMOKE=1`; it runs a tiny exact-output prompt and
  verifies streaming plus a succeeded task. It skips when configuration is
  absent and is never required by normal CI.
- Assistant Rules and ordered SkillCatalog results are materialized into the
  system instructions before execution. The 9Profs SkillCatalog is the only
  skill authority.
- AionRS output callbacks map to `agent.outputStarted`, `agent.outputDelta`,
  `agent.outputCompleted`, and `agent.error`; Phase 2A remains authoritative
  for `agent.task*` lifecycle events and terminal state.
- Cancellation flows from the task manager watch signal into
  `AgentEngine::abort_current_turn`; no conversation/session persistence is
  introduced.
- Active runs and tasks remain memory-only in the Phase 2A `AgentTaskManager`;
  no chat history, conversation table, resume state, or session lease was
  added.

#### Phase 2B1.1 — Execution hardening

Phase 2B1.1 is an execution-hardening pass, not a new architectural layer. It
keeps environment-based provider configuration temporary, preserves the empty
AionRS tool surface, and verifies that Assistant Rules plus ordered 9Profs
`SkillCatalog` content remain the only instruction sources. AionRS dependency
size is documented as future optimization; no dependency restructuring is part
of this phase.

Pinned dependency record:

- AionCore: `7ac84f93c5f81e1b1cc41f8119c089df72d63afc`.
- AionRS: tag `v0.2.11`, resolved commit
  `8e61a90329fa9f67c4fdf7e97fe02c24dba33f75`.
- Inspected AionRS `aion-agent`, `aion-config`, `aion-providers`,
  `aion-types`, `aion-protocol`, `aion-mcp`, and `aion-tools` sources for
  engine lifecycle, provider selection, streaming, configuration,
  cancellation, prompt construction, and tool registration.

Still future: ACP/external CLI backends, Extensions runtime, the GenOffice AI
bridge, OfficeCLI, Document Gateway implementation, and Product Layer.

### Phase 2C0 — 9Profs Tool Runtime Foundation

Phase 2C0 is IMPLEMENTED. `nineprofs-tools` owns the transport-neutral tool
domain: `ToolId`, `ToolDefinition`, `ToolProvider`, `ToolRegistry`, handler
execution, structured invocation/results/errors, coarse effect metadata, and
transport-safe future tool events. The registry is concurrency-safe, provides
deterministic lookup, rejects duplicate IDs/names, and keeps disabled tools
visible without making them executable.

Tool availability and per-run authorization are separate:

```text
available registry != tools granted to current run
```

The default `ToolSet` is empty. `AgentExecutionService` can receive an
explicit per-run `ToolSet`; `start_run` continues to use the empty set so the
existing Phase 2B1 path remains tool-less. Assistant persona/configuration and
Skill instruction content do not register or authorize tools automatically.

The execution composition is now:

```text
Assistant / Skills -> AgentExecutionService -> AgentExecutor
                                             -> AionRsExecutor
                                                -> 9Profs Tool Runtime
                                                   -> AionRS ToolRegistry
                                                      -> AgentEngine
```

`AionRsToolAdapter` is the only AionRS tool boundary. It converts authorized
9Profs handlers into the minimum AionRS `Tool` surface and owns all AionRS
types. The 9Profs Tool Runtime remains the source of truth for definitions,
policy metadata, enabled state, availability, permissions, and execution
boundaries. AionRS `ToolRegistry` is an execution adapter only; AionRS bootstrap
and default tools remain disabled.

Phase 2C1 is IMPLEMENTED. MCP configuration CRUD, secret-redacted transport
summaries, explicit connect/test/disconnect lifecycle, bounded startup timeout,
stdio/SSE/streamable HTTP client integration, tools/list discovery, stable
namespaced tool identity, MCP lifecycle events, and MCP-to-ToolProvider
conversion live in `nineprofs-mcp`. MCP server configuration is persisted in
SQLite; connection handles and discovered runtime state are not persisted.

The enforced execution path is:

```text
MCP server
  -> nineprofs-mcp
  -> nineprofs-tools ToolRegistry
  -> explicit per-run ToolSet authorization
  -> AionRsToolAdapter
  -> AionRS AgentEngine
```

MCP tools never register directly in the AionRS registry. AionRS sees only
authorized registrations copied by `AionRsToolAdapter`; `start_run` remains
tool-less by default. MCP tools use conservative executable policy metadata,
and remote HTTP/SSE tools additionally carry `ExternalNetwork`.

Phase 3A/3B OfficeCLI is an inspection and detached-artifact sidecar, never
the active document authority:

```text
Agent
  -> explicit ToolSet authorization
  -> nineprofs-tools ToolRegistry
  -> OfficeCliToolProvider
  -> OfficeCliToolProvider
  -> writable detached-artifact eligibility
  -> copy-on-write mutation transaction
  -> pinned OfficeCLI v1.0.144
  -> validate -> HTML -> PNG render gate -> atomic artifact revision
```

The pinned upstream source is
`1ced45e900782c5083ed550ddf328ee974e425e7`. The sidecar accepts only
9Profs artifact references, verifies containment and `.docx`/`.xlsx`/`.pptx`
type, isolates its profile, disables auto-update, and exposes read-only
`view`, `get`, `query`, `validate`, and render operations. `validate`
remains distinct from `view issues`. Phase 3B adds only typed `office.create`
and `office.mutate_detached` operations (`set`, `add`, `remove`, `move`,
`copy`, and `swap`) that are executed against a controlled working revision.
Raw XML, raw package-part mutation, arbitrary CLI passthrough, and OfficeCLI
MCP/skill installation remain unreachable from the provider. The shared
registry contains MCP and OfficeCLI registrations; `ToolSet::default()` still
authorizes zero tools.

Production visual rendering is deliberately split:

```text
OfficeCLI v1.0.144: Office document -> HTML stdout
9Profs HtmlRasterizer: controlled HTML artifact -> PNG artifact references
```

`office.render` uses the existing Electron 43.3.0 runtime through a hidden,
offscreen window and bounded `capturePage` calls. Playwright remains an audited
test dependency only; no browser install was added. All external HTTP(S) and
WebSocket requests are blocked. Limits are 16 MiB HTML, 4096 physical pixels
per dimension, 64 logical pages/slides/sheets, 64 MiB total PNGs, and a
30-second render timeout. OfficeCLI-native screenshot is retained only as an
upstream diagnostic because v1.0.144 timed out on this Windows host.

The real qualification gate passes DOCX, XLSX, and PPTX HTML generation,
production PNG rasterization, containment, and source byte preservation.

GenOffice remains the only canonical writer for active documents. Tool
authorization is not document write authority: an explicitly authorized write
tool still rejects inspection snapshots, active GenOffice references, and
other read-only artifacts. OfficeCLI may write only detached, unowned, or
newly-created controlled artifacts. Phase 4A implements
`DocumentChangeSet -> DocumentMutationGateway -> GenOffice Docs adapter ->
existing Docs command engine -> GenOffice transaction/save`. OfficeCLI writes
detached artifacts; GenOffice writes active documents. AI active-document
mutation uses existing GenOffice editor commands, not OfficeCLI and not a new
mutation engine. Phase 3B uses sequential typed OfficeCLI calls
inside the 9Profs copy-on-write transaction; it does not rely on upstream batch
rollback for atomicity. Resident mode remains deferred, and confirmation
metadata is declared but runtime confirmation enforcement/UI remains deferred.

For the active DOCX adapter, `DocumentId` is the identity of one continuous
editor document session. It rotates only when the active document is replaced
(open another file, create a new document, or an equivalent replacement
operation). Saving, autosaving, Save As, and reparsing the same active document
preserve it. `DocumentVersion` is the monotonic content revision within that
session and is independent of persistence. The file path is only the current
persistence location and may change independently through Save As:

```text
Active Document Session
├── DocumentId       stable
├── DocumentVersion  monotonic
└── filePath         may change via Save As
```

Future: research tools, extension tools, OAuth, full
permission/confirmation UX, MCP resources, external-agent sync, and external
CLI agents.

Dependency direction remains:

```text
nineprofs-core app
├── nineprofs-runtime
│   ├── nineprofs-agent
│   │   └── nineprofs-tools
│   ├── nineprofs-mcp ── nineprofs-tools
│   ├── nineprofs-assistant ── nineprofs-skills
│   ├── nineprofs-db
│   └── nineprofs-realtime / nineprofs-api-types / nineprofs-common
└── packages/9profs-core transport + Phase 0 adapters
```

Future phases may add runtime probing and real executors behind this registry
and task boundary. They must preserve Assistant/backend separation, stable IDs,
round-trip persistence, and the no-process Phase 2A behavior.

Pinned AionCore source remains commit
`7ac84f93c5f81e1b1cc41f8119c089df72d63afc`. Adapted upstream locations were
`crates/aionui-assistant/src/{builtin.rs,service.rs,routes.rs,state.rs}`,
`crates/aionui-extension/src/{skill_service.rs,loader.rs,asset_paths.rs}`,
and representative resources under
`crates/aionui-app/assets/{builtin-assistants,builtin-skills}`. AionRS,
`aionui-ai-agent`, conversation state, MCP runtime, Cron behavior, and full
extension loading were excluded.

Dependency direction:

```text
nineprofs-core app
├── nineprofs-runtime
│   ├── nineprofs-assistant ── nineprofs-skills
│   ├── nineprofs-db
│   └── nineprofs-realtime / nineprofs-api-types / nineprofs-common
└── packages/9profs-core transport + Phase 0 adapters
```

Skill precedence is deterministic: custom, then extension, then builtin.
Configured custom roots are ordered highest to lowest precedence. Builtin
assistants and skills remain resource-backed; only custom assistant metadata,
rules, and ordered skill assignments are persisted.

### Not implemented yet

No repository package currently establishes:

- an extension host or skill filesystem loader;
- citation verification, manuscript/bibliography extraction, Sheets verification,
  or research UI;
- OfficeCLI resident mode;
- Sheets or Slides adapters;
- account, subscription, credits, remote workspace, or SaaS billing services.

These are future architecture, not current capabilities.

Docs fresh text-only chats use the Core-owned Docs Agent when Core reports a
ready `document-foundation` profile. Legacy `AgentLoop` remains only as a
controlled compatibility path for attachments and restored historic chats.

## DOCX presentation V2 status

V2 is not design-only. Source and tests establish a real but experimental
renderer path.

| Capability               | Status                   | Evidence and boundary                                                                                                                                                                                                                                                                      |
| ------------------------ | ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Renderer selection       | Implemented experimental | `presentation-v2/index.ts` defines `PresentationRenderer`, defaults to `v1`, and resolves an internal `globalThis.__9profsDocsPresentationRenderer`/query override. No user-facing setting exists.                                                                                         |
| V1 path                  | Fallback/legacy baseline | `renderPresentationV1` uses existing `sliceWithLineSplit`; V1 is the production default while V2 is validated.                                                                                                                                                                             |
| V2 page flow             | Implemented experimental | `page-slicer.ts` owns V2 orchestration, bounded refinement, section normalization, and performance hooks while reusing GenOffice `BlockBox`, `PageSlice`, and `computeSectionedSlicesF2` primitives. It is not a second OOXML model.                                                       |
| V2 measurement           | Implemented experimental | `measurement.ts`, `measurement-context.ts`, and `measurement-invalidation.ts` support refinement windows, font/zoom invalidation, and conservative fallback for ambiguous transactions.                                                                                                    |
| V2 geometry/post-render  | Implemented experimental | `geometry.ts`, `geometry-probes.ts`, `post-render.ts`, and App `__pageDebug` expose normalized page, gap, header/footer, float, caret, selection, and position geometry for diagnostics.                                                                                                   |
| V1/V2 parity diagnostics | Implemented experimental | `diagnostics.ts` compares normalized model/presentation values with explicit geometry tolerance and categories.                                                                                                                                                                            |
| Automated proof          | Partial                  | `presentation-v2-*.test.ts`, geometry tests, Docs fixture tests, and `e2e/docs-presentation-parity.spec.ts` cover renderer selection, sections, measurement, dirty/save/reopen preservation, and browser geometry. Full default-readiness across all Office fidelity cases is not claimed. |
| V2 default replacement   | Future                   | Requires repeatable parity, position mapping, save/reopen and preservation proof across the representative corpus. Keep V1 available until then.                                                                                                                                           |

V2 remains renderer-side. It must not change Tiptap/ProseMirror schema or
transactions, `blocksToPmDoc`, `pmDocToSavePlan`, dirty tracking, `saveDocx`,
save ordering, reparse, comments, revisions, anchors, or OOXML identity.

## Layer ownership

| Concern                | Current owner/source of truth                                                                                                                 | Target rule                                                                                                                |
| ---------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| Document editing state | GenOffice app editor: ProseMirror for Docs/Markdown, Univer/journal for Sheets, PPTX model/canvas for Slides, PDF renderer edit state for PDF | GenOffice-derived Office Core remains canonical for interactive editing.                                                   |
| Persisted Office bytes | Format-specific engines and app save paths; DOCX through `docx-engine` and Docs save/reparse                                                  | One canonical writer per active document. Presentation, AI, and external tools cannot become parallel writers.             |
| AI/runtime state       | `agent-core` loop/transport, app panels, provider streams; mostly per-run/local                                                               | Future 9Profs Core owns runtime/session/policy state. It does not own Office bytes or editor state.                        |
| Skills                 | Current app skills under `apps/*/src/*/ai`; shared `AgentSkill` contract in `packages/agent-core/src/skill.ts`                                | Future skill registry owns discovery/versioning/execution policy; skills call document tools, never persistence internals. |
| Assistants             | Phase 1B metadata/CRUD plus Phase 2A backend-ID resolution                                                                                    | Future phases may add policy and model routing; Assistant remains separate from executable backends.                       |
| External tools         | Current provider/search/file/native adapters                                                                                                  | Adapters own transport and normalized observations. OfficeCLI can write only new, detached, or unowned documents.          |
| Research workflows     | `nineprofs-research` owns persistent cases, logical sources, immutable snapshots, evidence, claims, and claim/evidence assessments            | Research Domain owns evidence/provenance and future review workflows, not Office canonical bytes; evidence is not truth.   |
| Presentation           | Docs renderer V1/V2 and visual extensions                                                                                                     | Presentation is derived state. It never becomes the document persistence authority.                                        |

### Active Office document authority

```text
Agent
  -> DocumentChangeSet (future typed intent)
  -> DocumentMutationGateway (future ownership/policy boundary)
  -> GenOffice adapter/editor transaction
  -> GenOffice canonical save/reparse
```

For an active GenOffice-owned file, OfficeCLI receives a snapshot or bounded
inspection request. It does not write the active canonical file. A detached
copy or newly generated file may use OfficeCLI as its writer when no GenOffice
session owns that file.

## Proposed future module boundaries

Start with explicit modules under a small package surface; split packages only
after contracts stabilize. Proposed names are compatible with `packages/*`.

| Proposed boundary                              | Owns                                                                                                                                                                                  | Must not own                                                            |
| ---------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------- |
| `packages/9profs-core/`                        | Runtime configuration, workspace/project service contracts, events, policy, usage hooks, service composition                                                                          | Office bytes, editor transactions, provider-specific wire formats       |
| `packages/9profs-core/src/agent-runtime/`      | Agent run lifecycle, context assembly, cancellation, tool execution policy; wraps/adapts `packages/agent-core`                                                                        | DOCX XML, DOM mutation, canonical save                                  |
| `packages/9profs-core/src/assistant-registry/` | Assistant descriptors, allowed skills, model/policy selection, versioning                                                                                                             | Direct provider secrets or Office persistence                           |
| `packages/9profs-core/src/skills/`             | Shared skill metadata, lifecycle, permissions, input/output contracts                                                                                                                 | Product-specific editor internals                                       |
| `packages/9profs-core/src/mcp/`                | MCP server/client registration, tool schemas, transport and capability policy                                                                                                         | Unmediated active-document writes                                       |
| `packages/9profs-core/src/extensions/`         | Extension discovery, lifecycle, compatibility and permissions                                                                                                                         | Loading arbitrary code into the Docs persistence boundary               |
| `9profs-core-rs/crates/nineprofs-research/`    | Phases 5A–5C1 cases, sources, immutable snapshots, evidence/locators, claims, categorical assessments, citation occurrences/targets/bindings, claim links, and provenance persistence | Canonical Office editing, adapters, provider SDKs, or research UI       |
| `packages/research-domain/`                    | Future UI/workflow adapters over Core research transport                                                                                                                              | Canonical Office editing or provider SDK details                        |
| `packages/document-gateway/`                   | `DocumentChangeSet`, mutation validation, ownership/session checks, snapshot contracts                                                                                                | Format-specific OOXML/XLSX/PPTX/PDF implementation                      |
| `packages/genoffice-adapter/`                  | Adapter from approved changes to GenOffice editor commands/transactions and save/reparse hooks                                                                                        | Competing writer or presentation DOM mutation                           |
| `packages/officecli-adapter/`                  | Transport-neutral status, inspection results, and artifact references                                                                                                                 | CLI process control or canonical writes to active GenOffice-owned files |

Existing app AI directories remain product adapters until these boundaries
exist. `packages/project-store` can back local workspace history; it does not
become a remote SaaS contract by implication.

## Existing AI package classification

| Module                 | Classification | Current responsibility                                                                                                      | Why                                                                                                                                                                            |
| ---------------------- | -------------- | --------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `packages/agent-core`  | ADAPT          | `AgentLoop`, `AgentSkill`, tool execution, stream/IPC transport, payload sanitization                                       | Generic and reusable foundation, but lacks 9Profs assistant registry, policy, MCP, usage, and document gateway ownership. Preserve contracts; wrap it in future agent runtime. |
| `packages/ai-provider` | ADAPT          | Provider registry, Anthropic/Gemini/OpenAI-compatible protocols, streaming/chat, watchdogs; includes GenSpark auth/settings | Protocol contracts are reusable. GenSpark endpoints, account status, attribution, and credits policy must move behind a 9Profs provider/gateway adapter over time.             |
| `packages/ai-search`   | ADAPT          | Genspark-first web/image search with Serper and DuckDuckGo fallbacks                                                        | Keep current product behavior. Refactor its transport/auth into External Tool Adapters and expose research-safe results/provenance later; do not remove it now.                |

No package is classified PHASE OUT or REPLACE LATER in this task. Existing
behavior stays intact until a compatible replacement exists and is validated.

## Migration map

| Current module                           | Current responsibility                              | Target responsibility                                                 | Action                                           | Migration phase |
| ---------------------------------------- | --------------------------------------------------- | --------------------------------------------------------------------- | ------------------------------------------------ | --------------- |
| `apps/shell`                             | Electron host, tabs, settings, recent files, IPC    | Product host and 9Profs service client boundary                       | Keep; add typed clients later                    | 1, 6            |
| `packages/project-store`                 | Local projects/chats/attachments                    | Local history/workspace persistence behind product services           | Keep; adapt storage boundary                     | 1, 6            |
| `packages/agent-core`                    | Generic loop/skills/tools/IPC                       | Agent runtime execution kernel                                        | Adapt behind 9Profs runtime                      | 1, 2            |
| `packages/ai-provider`                   | Provider protocols and GenSpark-aware routing       | Provider adapter behind policy/usage gateway                          | Adapt; isolate vendor policy                     | 2               |
| `packages/ai-search`                     | Search backends and fallbacks                       | External search adapter and research evidence input                   | Adapt; preserve fallback behavior                | 2, 5            |
| `apps/*/src/*/ai`                        | Product-specific tools, prompts, transports         | Registered skills using document/research contracts                   | Adapt incrementally                              | 2, 4, 5         |
| `apps/docs` + `packages/docx-engine`     | Canonical DOCX editor, persistence, renderer        | GenOffice-derived Office Core                                         | Keep; harden V2 and add adapter seam             | 0, 4            |
| `apps/docs/src/renderer/presentation-v2` | Experimental derived layout/refinement/diagnostics  | Validated optional presentation implementation                        | Keep V1 fallback; prove before default           | 0               |
| `apps/sheets`                            | Univer XLSX editing, gateway, Rust sidecar          | GenOffice-derived Office Core                                         | Keep; no cross-product rewrite                   | 0, 4            |
| `apps/slides` + `packages/pptx-*`        | PPTX model, render, canvas, save                    | GenOffice-derived Office Core                                         | Keep; expose gateway later                       | 0, 4            |
| `apps/pdf`                               | PDF viewer/editor/save and AI tools                 | GenOffice-derived Office Core plus bounded AI context                 | Keep; adapt AI later                             | 0, 4, 5         |
| `apps/markdown`                          | Tiptap Markdown editor and plain-file serialization | GenOffice-derived Office Core plus research-friendly document context | Keep; adapt later                                | 0, 4, 5         |
| `nineprofs-officecli`                    | Pinned read-only OfficeCLI sidecar                  | `document-gateway`, `genoffice-adapter`, `officecli-adapter`          | Detached mutation and active writer              | 3A, 3B, 4       |
| `nineprofs-research`                     | Evidence/provenance and citation binding domain     | Core-owned Research Domain persistence and transport                  | Implemented; keep verification/adapters above it | 5A–5C1          |
| No current module                        | No account/billing/usage product backend            | Product/SaaS services                                                 | Add after runtime and ownership proof            | 6               |

## Ordered implementation sequence

### Phase 0 — architecture and contracts

- Keep this document and the DOCX status documents canonical.
- `packages/9profs-core/` and `packages/document-gateway/` define the Phase 0
  contracts; `packages/genoffice-adapter/` and `packages/officecli-adapter/`
  establish compile-checked seams only.
- Define active-document ownership and mutation gateway contracts without
  implementing a writer.
- Keep V1 as the Docs default. Continue V2 parity, geometry, position mapping,
  save/reopen, and preservation checks; treat V2 as experimental.
- Exit when architecture references match real paths and no document layer has
  an ambiguous write authority.

### Phase 1A — 9Profs Rust Core runtime foundation — IMPLEMENTED

- Rust workspace lives in `9profs-core-rs/`; root scripts expose
  `core:build`, `core:test`, and `core:run` without changing normal `dev`.
- Dependency direction is `nineprofs-core` app → `nineprofs-runtime` →
  `nineprofs-documents`/`nineprofs-realtime`/`nineprofs-db` →
  `nineprofs-api-types`/`nineprofs-common`.
- HTTP exposes `/api/health`, `/api/runtime`, and safe active-document metadata;
  generic events use `/ws`, active DOCX sessions use `/ws/documents`.
- Default bind is loopback (`127.0.0.1:39761`). No wildcard CORS policy is
  installed. The launch-scoped session secret is required by the document
  bridge handshake when configured; no broad auth subsystem exists yet.
- TypeScript keeps `@genoffice/9profs-core` as the application contract layer;
  `src/transport.ts` maps stable DTOs and does not depend on Rust details.
- Existing Electron IPC, GenOffice Office behavior, and Sheets sidecar remain
  unchanged.

### Phase 2 — agent runtime, assistants, skills, MCP, extensions

- Add agent-runtime wrapper, assistant registry, skill registry, MCP capability
  policy, and extension lifecycle.
- Adapt current app skills and provider/search adapters incrementally.
- Preserve existing stream/cancel/tool-call contracts and app fallbacks.

### Phase 3A — OfficeCLI read-only sidecar (IMPLEMENTED)

- Pin OfficeCLI v1.0.144 at commit
  `1ced45e900782c5083ed550ddf328ee974e425e7`.
- Run through `nineprofs-officecli` and shared `nineprofs-tools`
  `ToolRegistry`; never use OfficeCLI MCP mode.
- Verify configured binary/version, isolate profile/update/skill state, bound
  process output and lifetime, and expose detached artifact/snapshot references
  only.
- Support typed text/annotated/outline/stats/issues/get/query/validate and
  controlled render operations. All tools carry `Read` policy only.
- Keep default `ToolSet` empty. No active-document mutation; detached mutation
  is implemented in Phase 3B below.

### Phase 3B — OfficeCLI create and detached mutation (IMPLEMENTED)

- Create DOCX, XLSX, and PPTX only inside the controlled artifact root.
- Resolve detached, unowned, and newly-created artifacts through the explicit
  writable eligibility boundary; inspection snapshots, active GenOffice
  references, and read-only references fail before any OfficeCLI process runs.
- Copy an existing base to a same-root working revision, apply the typed
  semantic mutation model, save, structurally validate, render through the
  qualified OfficeCLI HTML -> Electron PNG path, and atomically promote a new
  revision. The base remains byte-identical on success and failure.
- Expose `office.create` and `office.mutate_detached` with `Write` policy and
  `requires_confirmation = true` metadata. The current metadata is not a
  confirmation workflow.
- Bound operation count, serialized mutation arguments, process/render output,
  cancellation, timeout, and working-copy cleanup. Independent mutations of
  the same base revision may proceed as independent copy-on-write revisions;
  no global mutation lock is used.
- Active GenOffice mutation, resident mode, raw/raw-set/add-part, arbitrary CLI
  passthrough, and OfficeCLI built-in skills remain deferred.

### Phase 4A — active DOCX GenOffice adapter (IMPLEMENTED)

`packages/genoffice-adapter/` now binds one active Docs document session to the
existing `buildDocumentContext` and `executeCommands` primitives. Inspection
returns the opaque document identity, `genoffice-active` authority, structured
context/selection, and current `DocumentVersion`. Approved
`docs.commandEnvelope` changes carry `baseVersion`; stale versions return an
explicit conflict result. The adapter combines multiple envelopes and calls
the existing command engine once, preserving whole-envelope validation,
Track Changes, dirty/save, presentation invalidation, and history behavior.

Phase 4A status: active DOCX inspection adapter — IMPLEMENTED; active DOCX
`DocumentVersion` — IMPLEMENTED; stale ChangeSet protection — IMPLEMENTED;
approved active DOCX mutation gateway — IMPLEMENTED; existing GenOffice
command engine reuse — IMPLEMENTED.

OfficeCLI writes detached artifacts. GenOffice writes active documents. The
adapter never writes DOCX bytes, calls `saveDocx`, edits presentation DOM, or
owns a second mutation engine.

### Phase 4B — Rust Core ↔ renderer live-document bridge (IMPLEMENTED)

`nineprofs-documents` owns ephemeral active-session routing. Each DOCX renderer
registers one stable `DocumentId` and current `DocumentVersion` over the
dedicated bidirectional `/ws/documents` bridge. Core correlates concurrent
inspection and already-approved mutation requests, enforces request timeouts,
and fails pending work on disconnect. `GET /api/documents` and
`GET /api/documents/:id` expose safe descriptor metadata only.

Authority remains split deliberately:

```text
Rust Core       = registry, router, orchestration, transport
GenOffice Docs  = active document authority, inspection, version checks, mutation
DocumentId      = routing/session key
DocumentVersion = optimistic concurrency token
```

The renderer continues to interpret `docs.commandEnvelope` through its
existing `executeCommands()` path. Rust carries generic change payload JSON and
never receives file paths, editor instances, ProseMirror/Tiptap types, or DOCX
engine objects. Save, autosave, Save As, and same-document reparse keep the
bridge session; only true document replacement rotates `DocumentId`.

Phase 4B status: active document registry — IMPLEMENTED; dedicated
bidirectional document bridge — IMPLEMENTED; active DOCX inspection proxy —
IMPLEMENTED; approved mutation proxy — IMPLEMENTED; version synchronization —
IMPLEMENTED.

### Phase 4C1 — active-document ToolProvider and proposal store (IMPLEMENTED)

`nineprofs-document-tools` contributes exactly three explicit tools through the
existing generic runtime: `document.list_active`, `document.inspect_active`,
and `document.propose_active_changes`. Active DOCX inspection still proxies
through `DocumentBridgeService` to the owning GenOffice renderer. Proposal
creation requires the current registry version and `docs.commandEnvelope`
payload shape, then Core generates the proposal/change-set ID, proposed status,
and `genoffice-active` target before storing the immutable proposal in a bounded
in-memory store. No proposal tool sends a mutation request over
`/ws/documents`.

The store derives `fresh`, `stale`, and `unavailable` views from the active
registry without rewriting proposal history. `document.proposalCreated`
contains only safe metadata. `GET /api/document-proposals` and
`GET /api/document-proposals/:id` are read-only and support future review UI.

Agent authority ends at the `PROPOSED` ChangeSet. The Agent cannot produce an
`ApprovedDocumentChangeSet`. Only trusted user-driven application logic may
transition `proposed` to `approved`. Tool authorization is not document
mutation approval: the proposal tool has `Write` policy because it writes
Core-owned runtime proposal state, but it does not write the user document.

### Phase 4C2 — proposal review and live commit (IMPLEMENTED)

- `DocumentProposalWorkflowService` owns atomic decision claims, freshness
  preflight, Core-generated approval metadata, terminal rejection, trusted
  retry, and distinct applied/conflict/failed outcomes through the existing
  bridge.
- Trusted approve/reject/retry endpoints use the launch-scoped session secret
  when configured. The Agent still has exactly the Phase 4C1 list/inspect/
  propose tools and no commit or approval path.
- The Docs AI panel shows current-document proposals with safe command
  summaries, freshness/version state, and review actions. Existing
  `executeCommands`, Track Changes, dirty/save, undo, and renderer version
  authority remain unchanged; successful ChangeSet results are bounded and
  idempotent within the active adapter session.

### Phase 4D1 - Core-owned active Docs agent run profile (IMPLEMENTED)

The dedicated `POST /api/document-agent-runs` endpoint accepts only an
assistant ID, active document ID, and user input. Core validates the connected
`genoffice-active` DOCX and creates the server-owned ToolSet containing exactly
`document.list_active`, `document.inspect_active`, and
`document.propose_active_changes`. The Agent can inspect and propose only for
the document bound to that run; it cannot commit, approve, reject, or retry.

The run context is transport-neutral (`activeDocs` plus `documentId`) and is
returned with safe run metadata. Core adds the minimum proposal-only system
policy; the selected Assistant's Rules and ordered Skills remain authoritative.
The generic `/api/agent-runs` endpoint remains deny-by-default and tool-less.
Client chooses CONTEXT. Core chooses CAPABILITIES.

Generic agent streaming remains on `/ws`; active-document renderer RPC remains
on `/ws/documents`. The typed `@genoffice/9profs-core` event client correlates
the generic stream by run ID and does not restart runs after disconnect.

### Phase 4D1.1 - Docs Core Agent readiness (IMPLEMENTED)

The production-intent default Docs profile is explicitly bound through the
existing Assistant backend reference:

```text
document-foundation -> nineprofs-default -> AionRS
```

`document-foundation` keeps its existing Rules and ordered
`document-foundation` Skill. `writing-foundation` remains enabled but
intentionally unbound because it is not the current Docs default product path.
No Assistant or AgentBackend runtime was duplicated.

Core exposes `GET /api/document-agent-profile`. The response contains the
default assistant ID, safe readiness state/reason, backend and component
availability, provider readiness, the exact supported Docs capability surface,
and whether active Docs runs are supported. Readiness checks the full chain:
assistant, backend reference, backend state, executor, launch-scoped provider
configuration, and all three required Docs tools. It never launches an LLM
request and never returns credentials, paths, or internal errors.

Provider configuration remains launch-scoped through `NINEPROFS_AGENT_*`
environment variables. No provider settings synchronization, vault, secret
database, hot reconfiguration, or account-level provider profile exists yet.

### Phase 4D1.2 - Docs Core conversation continuity (IMPLEMENTED)

Docs conversations provide ephemeral Core-owned model context without changing
the legacy Docs UI execution path. Core generates a `ConversationId` when a
conversation is created and binds it permanently to one `DocumentId`, one
`AssistantId`, and a snapshot of the effective Assistant Rules, ordered Skills,
and Docs proposal-only policy. Each conversation turn receives a new `RunId`
and `TaskId`; those identities remain distinct:

```text
ConversationId = model-context/session identity
RunId          = one execution turn
TaskId         = lifecycle/cancellation unit
DocumentId     = active document identity
```

The conversation API is:

- `POST /api/document-agent-conversations` to create a Core-generated,
  document-bound conversation;
- `GET /api/document-agent-conversations/:id` for safe metadata;
- `POST /api/document-agent-conversations/:id/runs` for subsequent turns.

The API accepts only the assistant/document binding at creation and `input` for
a turn. Tool IDs, backend/provider selection, credentials, filesystem paths,
and AionRS session objects are never client-controlled or returned.

Pinned AionRS `v0.2.11` (`8e61a90329fa9f67c4fdf7e97fe02c24dba33f75`) state is continued using its supported `Session`
snapshot/restore API. Core creates a fresh AionRS engine and Docs ToolSet for
each turn, then commits the new session snapshot only after successful
execution. Failed or cancelled turns discard the new snapshot and restore the
last successful state. This preserves real user/assistant/tool message roles,
refreshes `ToolInvocationContext` with the current RunId/TaskId, and avoids
stale tool adapters from a reused engine.

Conversation state is bounded and in-memory at the Core domain level: at most
32 conversations, at most 24 idle entries, a 30-minute idle lifetime, and 100
successful turns per conversation. Active turns are never evicted and a second
turn on the same conversation returns `conversation_busy`; different
conversations remain concurrent. A replaced/disconnected active document makes
its conversation unavailable rather than redirecting it to another document.

AionRS's temporary session files, when needed for its supported state API, are
written only below a Core-owned process-temporary directory and removed when
the conversation store is dropped. No home-directory, AionRS-global, project,
durable chat-history, or cross-restart restore behavior is enabled.

The Docs ToolSet remains exactly `document.list_active`,
`document.inspect_active`, and `document.propose_active_changes`. No MCP,
OfficeCLI, shell/filesystem, commit, approval, or rejection capability is
added. The proposal-only authority boundary remains unchanged.

### Phase 4D2 - Docs AiPanel Core migration (IMPLEMENTED)

`apps/docs/src/renderer/ai/AiPanel.tsx` chooses one execution mode per live
chat through `core-chat-controller.ts`: `undecided`, `core`, or `legacy`.
Fresh text-only chats select Core only when the active document exists, Core
returns a ready profile with `supportsActiveDocsRuns`, and no attachment or
historic-chat compatibility rule applies. Core conversation creation is lazy:
the first eligible send creates one document-bound `ConversationId`; later
turns reuse it while each request receives a new `RunId`/`TaskId`.

Core output is streamed through the typed `/ws` event client and persisted once
per completed assistant turn. Stop cancels the current Core task without
discarding the conversation; retry remains on the same conversation, while New
Chat clears the client-side conversation reference and starts a new mode
selection. Core failures after a conversation exists remain visible and are
never silently replayed through Legacy.

Core lifecycle activity is limited to safe tool names/summaries. No document
contents, raw tool output, credentials, or provider configuration crosses the
renderer lifecycle UI. Core proposals continue through `ProposalReview`; chat
code has no approve, reject, commit, or direct document-mutation capability.

Compatibility behavior is intentional. A first send with attachments selects
Legacy. If a Core chat later receives an attachment, successful textual
`ChatEntry` history is restored into `AgentLoop` once, the current attachment
turn is sent exactly once, and the chat remains Legacy. Historic persisted
chats do not fake Core session restore and continue through the existing
Legacy history path. Existing attachment picker, previews, file skills, dirty
tracking, Track Changes, undo, and project JSONL persistence remain intact.

Still future: Core attachment/file tools, durable or cross-restart Core
conversation restore, provider-settings synchronization, final Legacy removal,
Sheets/Slides adapters, and the next Research Domain verification layers.
Final Core desktop process lifecycle and mandatory bridge authentication remain
future work; local development without a configured session secret is
loopback-only by deployment convention.

### Phase 5 — research/review domain

#### Phase 5A — evidence and provenance foundation (IMPLEMENTED)

- `nineprofs-research` owns `ResearchCase`, logical `ResearchSource`, immutable
  `ResearchSourceSnapshot`, `ResearchEvidence` with structured locators,
  `ResearchClaim`, and categorical `ClaimEvidenceLink` assessments.
- SQLite persists cases, sources, snapshots, evidence, claims, and assessments
  with foreign keys and lookup indexes.
- SHA-256 is a content fingerprint/integrity identity, not a digital signature.
- Re-capturing identical content for one logical source returns the existing
  snapshot; identical bytes from different logical sources remain distinct.
- Evidence is an observation anchored to an immutable source snapshot; evidence
  is not truth. `ClaimEvidenceLink` is a separately attributed assessment and
  does not reduce scientific interpretation to a boolean or canonical confidence.
- Core research write APIs use the launch-scoped trusted session-secret boundary;
  read APIs expose transport-neutral DTOs. Research lifecycle events carry IDs
  and safe metadata only, never excerpts, claims, or source contents.
- Research code has no dependency on Office mutation or document persistence.

Still future: OCR, layout/image understanding, manuscript/bibliography
extraction, Sheets/data verification, research UI, field methodology bundles,
and research ToolProvider exposure. Future adapters consume this domain; they do
not replace it.

#### Phase 5B1 — canonical reference-PDF ingestion (IMPLEMENTED)

- `ResearchArtifactStore` streams bounded `application/pdf` uploads into the
  configured Core data directory using SHA-256 content-addressed `.pdf` files;
  SQLite stores the artifact identity and safe original filename, never an
  arbitrary client path. Identical bytes are deduplicated and changed bytes
  create a new immutable artifact.
- Reference-PDF `ResearchSourceSnapshot` records the verified artifact hash and
  uploaded-artifact origin. Generic inline snapshot capture rejects
  `ReferencePdf` sources so the provenance chain cannot be bypassed.
- The existing `packages/file-parse` PDF.js path exposes page-preserving text
  extraction. Core persists immutable extraction revisions and one-based page
  text with per-page and extraction hashes. Empty-text, failed, and
  password-required outcomes remain explicit statuses.
- A `SourceSnapshot` may have multiple immutable PDF extraction revisions.
  `ExtractionId` is the identity of exactly one derived text representation;
  snapshot ID alone is insufficient to select derived text. Future retrieval
  indexes, evidence capture, and citation verification MUST pin an exact
  `ExtractionId`. The legacy snapshot-level read is an explicit latest-
  extraction compatibility selector ordered by `extracted_at_ms DESC, id DESC`.
- Exact extraction reads use `GET /api/research/pdf-extractions/:extractionId`.
  Bounded page reads use `startPage` (a one-based PDF page number) and `limit`
  (maximum 50), return pages in `page ASC` order, and include `hasMore` plus
  `nextStartPage` continuation metadata. Trusted internal consumers may repeat
  these bounded reads to enumerate a ready extraction completely; no unbounded
  page HTTP response is exposed.
- `pdf_text_range` evidence is created only from stored extraction pages. The
  server slices the stored page text and persists the exact excerpt; offsets
  are Unicode scalar/code-point offsets, not UTF-8 byte or UTF-16 indexes.
- Trusted upload, extraction, and evidence writes use the launch-scoped
  session-secret boundary. Read APIs expose artifact metadata, extraction
  metadata, and bounded page reads without exposing filesystem paths or source
  contents in lifecycle events.

Phase 5B2 is implemented as `nineprofs-research-dify`. It provisions one
rebuildable Dify dataset per `ResearchCase`, indexes deterministic
`pdf-page-v1` chunks pinned to one exact `ExtractionId`, persists remote
segment IDs locally, and resolves retrieval hits back to immutable local page
ranges. Dify text is never canonical evidence; retrieval returns candidates and
does not create `ResearchEvidence`. Dify credentials are launch-scoped and are
configured with `NINEPROFS_DIFY_BASE_URL`, `NINEPROFS_DIFY_API_KEY`, and the
optional `NINEPROFS_DIFY_TIMEOUT_MS`; the key is never persisted, serialized,
logged, or emitted in events.

#### Phase 5B2.1 — scoped retrieval and Dify qualification (IMPLEMENTED)

- Case-wide retrieval remains `retrieve(researchCaseId, query, topK)` and answers
  what evidence exists in the case. The provider-neutral
  `ResearchRetrievalScope` additionally supports bounded exact source or
  extraction identity lists; omitted API scope preserves case-wide behavior.
- Exact extraction scope requires the extraction to belong to the requested
  case, be `Ready`, and have a metadata-qualified local Dify index. The adapter
  constructs Dify v1.16.1 `retrieval_model.metadata_filtering_conditions` from
  canonical `ExtractionId` values; arbitrary provider filters are not exposed.
- Each indexed Dify document is bound after creation to the canonical
  `nineprofs_extraction_id`, `nineprofs_source_id`, and
  `nineprofs_snapshot_id` string metadata fields. Field provisioning is
  idempotent and field IDs are persisted locally. Remote document metadata is
  verified before an extraction index becomes `Ready`.
- Returned segment IDs are resolved through the local mapping and
  `RetrievalChunk`; unknown segments, incompatible mappings, scope violations,
  and canonical hash mismatches fail closed and mark the affected index/case
  degraded where applicable. Manual Dify dataset edits are not canonical.
- Existing Phase 5B2 mappings remain valid for case-wide retrieval. They are
  not treated as scoped-ready until an explicit extraction resync completes
  metadata qualification; no existing Dify dataset is deleted or recreated on
  Core startup.
- Dify adapter readiness is separate from case-index and extraction-index
  readiness. It reports `not_configured`, `configured`, `unreachable`,
  `unauthorized`, `reachable`, or `ready` based on configuration and a bounded
  authenticated Service API dataset-list probe; it never performs an embedding
  query merely to report readiness.

Citation verification MUST use cited-source or exact-extraction-scoped
retrieval. Case-wide support from another source does not validate a citation.

#### Phase 5C1 — citation occurrence and claim binding domain (IMPLEMENTED)

- `CitationOccurrence` records one observed manuscript marker/group with a
  transport-neutral origin. Active-document origins preserve `DocumentId`,
  `DocumentVersion`, and manuscript locator; a future immutable manuscript
  snapshot can be represented without changing occurrence identity.
- `CitationTarget` stores one reference key and ordinal inside an occurrence.
  Grouped citations keep deterministic target ordering; unresolved targets are
  valid and need not have a binding.
- `CitationTargetBinding` identifies the cited `ResearchSource` and optionally
  pins an exact `SourceSnapshotId` and exact PDF `ExtractionId`. Binding is
  append-oriented, so corrections retain historical bindings. Exact PDF
  verification-ready state requires the complete source/snapshot/ready
  extraction chain; no latest-extraction lookup is used.
- `ClaimCitationLink` is many-to-many association only. It is not a
  `ClaimEvidenceLink` and does not mean supports, contradicts, or creates
  `ResearchEvidence`. `ClaimEvidenceLink` remains the later evidence
  assessment boundary.
- SQLite migration `0009_research_citations.sql`, Core routes, and
  `@genoffice/9profs-core` transport expose narrow create/get/list APIs. Writes
  use the trusted session-secret boundary and lifecycle events carry IDs only.

#### Phase 5C2A — citation verification orchestration (IMPLEMENTED)

- `nineprofs-research-verification` is a provider-neutral orchestration seam. A
  run validates the exact claim → citation occurrence → target → binding chain,
  requires a ready PDF extraction, and uses only exact-extraction-scoped
  retrieval.
- Retrieval candidates are canonical local page ranges, not evidence. The
  immutable candidate audit stores chunk/source/snapshot/extraction IDs, exact
  Unicode range, canonical excerpt hash, rank, and score; it never stores
  provider-returned text.
- `CitationAssessmentProvider` returns a structured relation
  (`supports`, `contradicts`, `contextualizes`, or `insufficient`) plus rationale
  and selected candidate IDs. Only selected IDs are revalidated against the
  canonical extraction, promoted through `capture_pdf_evidence`, and linked to
  the claim through the generic `ClaimEvidenceLink` with assessment metadata.
- Verification results, candidate audits, evidence mappings, failure codes, and
  identifier-only lifecycle events are append-oriented and queryable by run or
  claim.

#### Phase 5C2B — model-backed citation assessment (IMPLEMENTED)

- `nineprofs-research-assessor` implements the production
  `CitationAssessmentProvider` as one bounded, stateless model invocation. It
  supports `openai` and `anthropic`, with optional launch-scoped custom base
  URLs for OpenAI-compatible endpoints.
- Assessor configuration is separate from conversational Agent configuration:
  `NINEPROFS_CITATION_ASSESSOR_PROVIDER`, `_MODEL`, `_BASE_URL`, and
  `_API_KEY_ENV`. Only the credential environment-variable name is stored;
  credential values are resolved at readiness or invocation time and never
  serialized or logged.
- OpenAI uses bounded JSON mode; Anthropic uses a forced native tool schema.
  Both paths apply strict local `deny_unknown_fields` deserialization, accept
  only the four canonical relations, reject unknown candidate IDs and oversized
  input/output, and allow at most one narrow outer Markdown fence.
- The model receives canonical local candidate passages only. It never performs
  retrieval, uses tools, creates `ResearchEvidence`, supplies provenance, or
  writes persistence. Only the existing 5C2A orchestrator can revalidate ranges
  and promote evidence.
- Core startup remains independent of assessor credentials. Invalid or missing
  assessor configuration leaves verification available with the existing
  `assessor_not_configured` result; readiness is exposed internally without
  provider secrets.

#### Phase 5C3A — inline DOCX citation model and extraction (IMPLEMENTED)

- `packages/docx-engine` owns the document-format citation seam. Supported
  structured fields are Word-native `CITATION <Tag>` and Zotero
  `ADDIN ZOTERO_ITEM CSL_CITATION`; grouped Zotero `citationItems` become
  ordered targets with bounded metadata. EndNote, Mendeley, and Citavi remain
  protected for future adapters.
- A supported field is represented as one atomic `Run.citation` and one
  `docxCitation` Tiptap/ProseMirror inline atom. The surrounding paragraph is
  editable; the atom is selectable and all-or-nothing on deletion. Its
  preserved field span carries the exact imported field/SDT XML needed for
  faithful save.
- Word bibliography `customXml/b:Sources` support remains owned by
  `parseSourcesXml`, `ParsedDoc.sources`, and `buildSourcesXml`. A matching
  native tag may enrich the read-only document descriptor with title, author,
  and year, but it never creates or binds a Research identity.
- `extractDocxCitations` and the Docs PM adapter expose a read-only inventory
  with stable current block identity, visible rendered text, ordered targets,
  and Unicode code-point offsets. AI plain-text context includes only the
  rendered marker, never field instructions or Zotero JSON.
- A DOCX citation atom is document-format identity/preservation. A Research
  `CitationOccurrence` is persistent research-domain identity. They are
  intentionally different objects; Phase 5C3B owns synchronization between
  their descriptors and Research entities.

The synchronization boundary is documented in Phase 5C3B1 below. Claim
extraction, source/PDF binding, verification UI, Agent citation tools,
plain-text citation recognition, and additional citation-manager adapters
remain future.

Dify retrieval remains an adapter and is not a citation identity owner.

#### Phase 5C3B1 — live manuscript citation synchronization (IMPLEMENTED)

- `apps/docs` builds a bounded sync input from the live PM document through
  `extractDocxCitationsFromPmDoc`. The adapter preserves recognized Word-native
  and Zotero markers, current block IDs, Unicode code-point ranges, target
  order, reference keys, and cited locators. Unsupported structured managers
  remain absent; no text/XML recovery is attempted.
- The trusted POST sync route accepts only `documentId`, `documentVersion`, and
  the recognized citation inventory. The server computes the inventory SHA-256
  hash and requires an explicit `ResearchCaseId` plus a `SourceKind::Manuscript`
  source in the route.
- SQLite persists immutable sync runs, occurrence/target audit mappings, and
  the corresponding Research `CitationOccurrence`/`CitationTarget` rows. The
  identity `(case, manuscript source, document, version)` is idempotent for the
  same hash and conflicts for a different hash; later versions retain history.
- One sync is transactionally all-or-none. Read routes expose a run, the latest
  completed run for a manuscript source, bounded occurrence mappings, and
  target mappings. Sync creates no target bindings, claims, claim links, or
  verification state, and lifecycle payloads contain IDs/status metadata only.

#### Phase 5C3B2 — atomic manuscript claim extraction (IMPLEMENTED)

- `apps/docs` exposes a read-only adapter over the live PM document. It derives
  block text and reuses the exact Phase 5C3B1 block IDs and Unicode code-point
  citation ranges, then joins those descriptors to the completed sync run's
  persistent `CitationOccurrence` IDs. It does not edit PM state or DOCX XML.
- `nineprofs-research` accepts only an exact completed sync run, document ID,
  document version, and complete occurrence inventory. It validates every
  block/range/rendered marker against the canonical occurrence origin, sends
  only bounded block text plus occurrence IDs/ranges to a provider, and
  reconstructs all source excerpts server-side from Unicode code-point ranges.
- `nineprofs-research-claim-extractor` is a separate structured-output
  provider boundary for OpenAI and Anthropic. Its contract allows only atomic
  manuscript propositions, exact source ranges, and closed-set occurrence IDs;
  no retrieval, provenance invention, correction, or verification is allowed.
  Configuration is launch-scoped and credential-free in readiness/status
  output; an unconfigured core still starts normally.
- Extraction runs persist provider/version/model/contract identity and a
  context hash in `research_manuscript_claim_extraction_runs`. The companion
  `research_manuscript_claim_extraction_items` and
  `research_manuscript_claim_extraction_citations` tables carry exact source
  mappings, `ClaimCitationLink` foreign keys, excerpt hashes, deterministic
  ordinals, and per-citation coverage. All are committed in one transaction.
  Repeated identical requests reuse the completed run;
  different sync contexts or extractor identities retain separate history.
  Provider failures and invalid structured output cannot leave partial claims.
- Core exposes trusted creation plus read-only run/item/coverage routes and
  safe started/completed/failed lifecycle events. This phase creates no source
  bindings, evidence, verification results, retrieval state, Dify state, Agent
  tools, or citation UI.

Still future: 5C3B3 source/PDF binding, 5C3C citation UI, Agent citation tools,
plain-text recognition, and additional citation-manager adapters.

### Phase 6 — product/SaaS layer

- Add account, organization, workspace sync, subscription/billing, usage/credits,
  history, and configuration services.
- Move provider policy and runtime usage accounting behind product contracts.
- Retain local/offline Office editing and the single-writer rule.

## Reference documents

- [Architecture re-baseline audit](9PROFS-ARCHITECTURE-AUDIT.md) — confirmed
  repository findings and evidence.
- [DOCX presentation V2 boundary](DOCX-PRESENTATION-V2.md) — current V1/V2
  implementation status and protected contracts.
- [DOCX V2 reference map](DOCX-V2-REFERENCE-MAP.md) — external architecture
  comparison and implementation lessons; not a source-of-truth status document.
