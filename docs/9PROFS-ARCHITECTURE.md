# 9Profs architecture baseline

Status: canonical architecture and migration baseline for the current
`develop` branch. Audited 2026-08-23 from repository source, manifests, tests,
and a read-only comparison with `baseline/genoffice`.

This document describes what exists, what remains GenOffice-derived, and the
target boundaries for 9Profs. The pinned OfficeCLI sidecar is implemented for
read-only inspection plus transactional detached creation and mutation;
research workflows and SaaS services remain future work.

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

Research Domain Layer (future)
└─ research/review, citation, regulation, methodology workflows

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

| Area                                   | Status          | Evidence                                                                                                                                 |
| -------------------------------------- | --------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| Phase 0 contracts                      | IMPLEMENTED     | `packages/9profs-core/`, `packages/document-gateway/`, and compile-checked adapter seams                                                 |
| Phase 1A Rust Core                     | IMPLEMENTED     | `9profs-core-rs/` transport/runtime foundation; no product domains                                                                       |
| Phase 2A agent metadata/catalog        | IMPLEMENTED     | `nineprofs-agent` descriptors, builtin catalog, minimal SQLite custom metadata persistence                                               |
| Phase 2A Agent Registry                | IMPLEMENTED     | hydrated authoritative catalog, stable lookup/order, explicit availability, custom updates                                               |
| Phase 2A task lifecycle                | IMPLEMENTED     | `RunId`, `AgentTaskId`, state transitions, cancellation, ownership, lifecycle events                                                     |
| Phase 2B1 real agent execution         | IMPLEMENTED     | 9Profs executor boundary, AionRS backend, streaming, cancellation, and run APIs                                                          |
| Phase 2C1 MCP Tool Provider            | IMPLEMENTED     | `nineprofs-mcp`, SQLite config, pinned AionRS client mechanics, shared ToolRegistry provider                                             |
| Phase 3A OfficeCLI read-only provider  | IMPLEMENTED     | `nineprofs-officecli`, pinned v1.0.144 sidecar, typed read-only tools, HTML-to-PNG raster boundary, artifact boundary, status API        |
| Phase 3B OfficeCLI detached mutation   | IMPLEMENTED     | Typed create/mutation tools, writable eligibility, copy-on-write revisions, validation, HTML-to-PNG render gate, and atomic promotion    |
| Phase 4A active DOCX GenOffice adapter | IMPLEMENTED     | Active inspection, DocumentVersion/preconditions, stale-change protection, approved mutation gateway, existing Docs command engine reuse |
| Phase 4B Rust Core ↔ renderer bridge   | IMPLEMENTED     | Active document registry, dedicated bidirectional `/ws/documents`, DOCX inspection proxy, approved mutation proxy, and version synchronization |
| Phase 4C1 active-document proposals   | IMPLEMENTED     | `nineprofs-document-tools`, explicit list/inspect/propose tools, ephemeral proposal store, freshness, read-only proposal APIs/events    |
| Phase 4C2 proposal review/live commit | NOT IMPLEMENTED | No review/diff UI, approval/rejection workflow, confirmation enforcement, or active-document commit tool                            |
| Research domain                        | NOT IMPLEMENTED | No research/review/citation/regulation package exists                                                                                    |

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
bridge, OfficeCLI, Document Gateway implementation, Research Domain, and
Product Layer.

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

- real agent execution, backend executors, or provider runtime probing;
- an extension host or skill filesystem loader;
- a research/review/citation/regulation domain;
- OfficeCLI resident mode;
- the Rust Core ↔ renderer active-document bridge;
- an active-document ToolProvider, proposal/approval workflow, or review UI;
- Sheets or Slides adapters;
- account, subscription, credits, remote workspace, or SaaS billing services.

These are future architecture, not current capabilities.

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

| Concern                | Current owner/source of truth                                                                                                                 | Target rule                                                                                                                  |
| ---------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| Document editing state | GenOffice app editor: ProseMirror for Docs/Markdown, Univer/journal for Sheets, PPTX model/canvas for Slides, PDF renderer edit state for PDF | GenOffice-derived Office Core remains canonical for interactive editing.                                                     |
| Persisted Office bytes | Format-specific engines and app save paths; DOCX through `docx-engine` and Docs save/reparse                                                  | One canonical writer per active document. Presentation, AI, and external tools cannot become parallel writers.               |
| AI/runtime state       | `agent-core` loop/transport, app panels, provider streams; mostly per-run/local                                                               | Future 9Profs Core owns runtime/session/policy state. It does not own Office bytes or editor state.                          |
| Skills                 | Current app skills under `apps/*/src/*/ai`; shared `AgentSkill` contract in `packages/agent-core/src/skill.ts`                                | Future skill registry owns discovery/versioning/execution policy; skills call document tools, never persistence internals.   |
| Assistants             | Phase 1B metadata/CRUD plus Phase 2A backend-ID resolution                                                                                    | Future phases may add policy and model routing; Assistant remains separate from executable backends.                         |
| External tools         | Current provider/search/file/native adapters                                                                                                  | Adapters own transport and normalized observations. OfficeCLI can write only new, detached, or unowned documents.            |
| Research workflows     | No research domain package exists; current search is generic/app support                                                                      | Future Research Domain owns evidence, provenance, review state, citations, and domain workflows, not Office canonical bytes. |
| Presentation           | Docs renderer V1/V2 and visual extensions                                                                                                     | Presentation is derived state. It never becomes the document persistence authority.                                          |

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

| Proposed boundary                              | Owns                                                                                                           | Must not own                                                            |
| ---------------------------------------------- | -------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------- |
| `packages/9profs-core/`                        | Runtime configuration, workspace/project service contracts, events, policy, usage hooks, service composition   | Office bytes, editor transactions, provider-specific wire formats       |
| `packages/9profs-core/src/agent-runtime/`      | Agent run lifecycle, context assembly, cancellation, tool execution policy; wraps/adapts `packages/agent-core` | DOCX XML, DOM mutation, canonical save                                  |
| `packages/9profs-core/src/assistant-registry/` | Assistant descriptors, allowed skills, model/policy selection, versioning                                      | Direct provider secrets or Office persistence                           |
| `packages/9profs-core/src/skills/`             | Shared skill metadata, lifecycle, permissions, input/output contracts                                          | Product-specific editor internals                                       |
| `packages/9profs-core/src/mcp/`                | MCP server/client registration, tool schemas, transport and capability policy                                  | Unmediated active-document writes                                       |
| `packages/9profs-core/src/extensions/`         | Extension discovery, lifecycle, compatibility and permissions                                                  | Loading arbitrary code into the Docs persistence boundary               |
| `packages/research-domain/`                    | Review, thesis, citation, regulation, evidence/provenance, methodology workflows                               | Canonical Office editing or provider SDK details                        |
| `packages/document-gateway/`                   | `DocumentChangeSet`, mutation validation, ownership/session checks, snapshot contracts                         | Format-specific OOXML/XLSX/PPTX/PDF implementation                      |
| `packages/genoffice-adapter/`                  | Adapter from approved changes to GenOffice editor commands/transactions and save/reparse hooks                 | Competing writer or presentation DOM mutation                           |
| `packages/officecli-adapter/`                  | Transport-neutral status, inspection results, and artifact references                                          | CLI process control or canonical writes to active GenOffice-owned files |

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

| Current module                           | Current responsibility                              | Target responsibility                                                 | Action                                 | Migration phase |
| ---------------------------------------- | --------------------------------------------------- | --------------------------------------------------------------------- | -------------------------------------- | --------------- |
| `apps/shell`                             | Electron host, tabs, settings, recent files, IPC    | Product host and 9Profs service client boundary                       | Keep; add typed clients later          | 1, 6            |
| `packages/project-store`                 | Local projects/chats/attachments                    | Local history/workspace persistence behind product services           | Keep; adapt storage boundary           | 1, 6            |
| `packages/agent-core`                    | Generic loop/skills/tools/IPC                       | Agent runtime execution kernel                                        | Adapt behind 9Profs runtime            | 1, 2            |
| `packages/ai-provider`                   | Provider protocols and GenSpark-aware routing       | Provider adapter behind policy/usage gateway                          | Adapt; isolate vendor policy           | 2               |
| `packages/ai-search`                     | Search backends and fallbacks                       | External search adapter and research evidence input                   | Adapt; preserve fallback behavior      | 2, 5            |
| `apps/*/src/*/ai`                        | Product-specific tools, prompts, transports         | Registered skills using document/research contracts                   | Adapt incrementally                    | 2, 4, 5         |
| `apps/docs` + `packages/docx-engine`     | Canonical DOCX editor, persistence, renderer        | GenOffice-derived Office Core                                         | Keep; harden V2 and add adapter seam   | 0, 4            |
| `apps/docs/src/renderer/presentation-v2` | Experimental derived layout/refinement/diagnostics  | Validated optional presentation implementation                        | Keep V1 fallback; prove before default | 0               |
| `apps/sheets`                            | Univer XLSX editing, gateway, Rust sidecar          | GenOffice-derived Office Core                                         | Keep; no cross-product rewrite         | 0, 4            |
| `apps/slides` + `packages/pptx-*`        | PPTX model, render, canvas, save                    | GenOffice-derived Office Core                                         | Keep; expose gateway later             | 0, 4            |
| `apps/pdf`                               | PDF viewer/editor/save and AI tools                 | GenOffice-derived Office Core plus bounded AI context                 | Keep; adapt AI later                   | 0, 4, 5         |
| `apps/markdown`                          | Tiptap Markdown editor and plain-file serialization | GenOffice-derived Office Core plus research-friendly document context | Keep; adapt later                      | 0, 4, 5         |
| `nineprofs-officecli`                    | Pinned read-only OfficeCLI sidecar                  | `document-gateway`, `genoffice-adapter`, `officecli-adapter`          | Detached mutation and active writer    | 3A, 3B, 4       |
| No current module                        | No research/review domain                           | `packages/research-domain`                                            | Add after evidence and skill contracts | 5               |
| No current module                        | No account/billing/usage product backend            | Product/SaaS services                                                 | Add after runtime and ownership proof  | 6               |

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

### Phase 4C2 — proposal review and live commit (NOT IMPLEMENTED)

- Add proposal panel, diff viewer, Accept/Reject actions, and confirmation UI.
- Add trusted approval transitions and the existing approved mutation gateway
  call; never expose active-document commit/apply/approve/reject to the Agent.
- Keep Sheets and Slides active-document tools deferred.

Sheets and Slides adapters remain future work.
Final Core desktop process lifecycle and mandatory bridge authentication remain
future work; local development without a configured session secret is
loopback-only by deployment convention.

### Phase 5 — research/review domain

- Add research/review/citation/regulation/methodology services and provenance.
- Register domain skills through the same agent/MCP contracts.
- Consume Office snapshots and submit approved mutations through the gateway;
  research code never writes active Office files directly.

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
