# 9Profs architecture re-baseline audit

Audit date: 2026-08-27

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
- Phase 5A–5C2A Rust Core research domain, SQLite migrations, API DTO/routes, and
  TypeScript Core transport boundary.

This audit records repository facts after Phase 5C2A implementation. It does
not claim manuscript/bibliography extraction, Sheets verification, research UI,
or Agent research tools.

## Confirmed repository architecture

The root is a private `genoffice` workspace with `apps/*` and `packages/*`
workspaces. Product scripts run tests and typechecks for the shared packages,
Docs, Sheets, Shell, Slides, PDF, and Markdown.

| Current area                           | Confirmed implementation                                                                                                                                              | Current status                                                         |
| -------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| Electron shell                         | `apps/shell/src/main/index.ts`, `tab-manager.ts`, preload and renderer UI; hosts app views/tabs                                                                       | Implemented                                                            |
| Docs/DOCX                              | `apps/docs` plus `packages/docx-engine`; Tiptap/ProseMirror model, renderer pagination, AI tools, dirty/save/reparse paths                                            | Implemented; canonical Office editor                                   |
| Sheets/XLSX                            | Univer renderer, `apps/sheets/src/gateway/xlsx-gateway.ts`, `xlsx-package-io.ts`, Rust `native/xlsx-engine`                                                           | Implemented; independent Office editor                                 |
| Slides/PPTX                            | `packages/pptx-engine`, `packages/pptx-render`, `apps/slides/src/renderer/SlideCanvas.tsx`                                                                            | Implemented; independent Office editor                                 |
| PDF                                    | PDF.js viewer/editor UI, PDFium/PDF-lib main-process operations, `apps/pdf/src/main/save-pdf.ts`                                                                      | Implemented; independent Office editor                                 |
| Markdown                               | Tiptap editor, `apps/markdown/src/renderer/markdown/docText.ts`, optional `export/docxExport.ts`                                                                      | Implemented; plain-file-first                                          |
| Shared AI                              | `agent-core`, `ai-provider`, `ai-search`, and app-level AI skills/tools/transports                                                                                    | Implemented local foundation; not 9Profs Core                          |
| Local workspace data                   | `packages/project-store` local projects/chats/attachments                                                                                                             | Implemented local persistence; not SaaS                                |
| Phase 0 contracts                      | `packages/9profs-core`, `packages/document-gateway`, and compile-checked adapter seams                                                                                | Implemented; contracts only                                            |
| Rust Core foundation                   | `9profs-core-rs/` common/API/SQLite/realtime/runtime/app crates; loopback HTTP/WebSocket bootstrap                                                                    | Implemented; foundation only                                           |
| Phase 1B Assistant domain              | `nineprofs-assistant`; builtin/custom assistants, Rules, CRUD persistence, skill bindings, backend metadata reference                                                 | Implemented                                                            |
| Phase 1B Skills catalog                | `nineprofs-skills`; builtin/custom `SKILL.md` loading, deterministic precedence, extension-ready provider boundary                                                    | Implemented                                                            |
| Phase 2C0 Tool Runtime                 | `nineprofs-tools`; definitions, provider/registry, policy metadata, deny-by-default ToolSet, AionRS adapter                                                           | Implemented                                                            |
| Phase 2C1 MCP provider                 | `nineprofs-mcp`; config/persistence, redacted lifecycle APIs, pinned AionRS MCP transport/client, discovery, shared registry provider                                 | Implemented                                                            |
| Phase 3A OfficeCLI provider            | `nineprofs-officecli`; pinned v1.0.144 sidecar, typed read-only operations, HTML-to-PNG raster boundary, artifact containment, shared registry provider, status API   | Implemented; real DOCX/XLSX/PPTX production qualification passes       |
| Phase 3B OfficeCLI detached mutation   | `office.create`, `office.mutate_detached`, writable eligibility, copy-on-write revision transaction, structural validation, HTML-to-PNG render gate, atomic promotion | Implemented; real DOCX/XLSX/PPTX create/mutation qualification passes  |
| Phase 4A active DOCX GenOffice adapter | `packages/genoffice-adapter/`; active inspection, DocumentVersion, stale-change protection, approved command-envelope gateway, existing Docs command engine           | Implemented; editor-side adapter and focused transaction/version tests |
| Phase 4B Rust Core ↔ renderer bridge   | Active-document transport, renderer bridge, versioned inspection/mutation proxy                                                                                       | Implemented                                                            |
| Phase 4C1 document proposals           | `nineprofs-document-tools`; explicit active-document tools, bounded proposal store, freshness, safe proposal APIs/events                                              | Implemented                                                            |
| Phase 4C2 proposal review/live commit  | Core-owned proposal workflow, trusted decisions, Docs review card, renderer idempotency, and approved live commit through the existing bridge                         | Implemented                                                            |
| Phase 5A–5C1 research domain           | `nineprofs-research`; SQLite cases/sources/immutable snapshots/evidence/locators/claims/claim-evidence assessments/citation occurrences/targets/bindings/claim links; Core API/transport | Implemented; verification, adapters, review, and product services remain future |
| Research/product backend               | Account/billing backend or OfficeCLI resident mode                                                                                                                    | Future                                                                 |

Phase 4A status: active DOCX inspection adapter — IMPLEMENTED; active DOCX
`DocumentVersion` — IMPLEMENTED; stable active-session `DocumentId` —
IMPLEMENTED; stale ChangeSet protection — IMPLEMENTED; approved active DOCX
mutation gateway — IMPLEMENTED; existing GenOffice Docs command engine reuse —
IMPLEMENTED. OfficeCLI writes detached artifacts; GenOffice writes active
documents.

For an active DOCX session, `DocumentId` identifies one continuous editor
session and rotates only when the active document is replaced. `DocumentVersion`
is the monotonic content revision within that session. The file path is an
independent persistence location and may change through Save As:

```text
Active Document Session
├── DocumentId       stable
├── DocumentVersion  monotonic
└── filePath         may change via Save As
```

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
6. `nineprofs-research` owns evidence/provenance state. Evidence is an
   observation anchored to an immutable source snapshot, not truth; a
   `ClaimEvidenceLink` is a separately attributed categorical assessment.
7. OfficeCLI is an inspection/rendering/detached-generation and detached-
   mutation sidecar, never a competing writer for an active GenOffice file.
   Tool authorization is distinct from document write authority: only
   detached, unowned, or newly-created controlled artifacts are writable.

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
- `packages/officecli-adapter/` for transport-neutral OfficeCLI status,
  inspection, typed mutation, revision, validation, render, and
  artifact-reference contracts.

Phase 0 contract portions are implemented in `packages/9profs-core/` and
`packages/document-gateway/`. Phase 1A Rust Core foundation and Phase 1B
Assistant/Skills foundation are implemented in `9profs-core-rs/`;
`packages/genoffice-adapter/` remains a compile-checked skeleton;
`packages/officecli-adapter/` is a transport-neutral TypeScript boundary.
Rust process execution lives in `nineprofs-officecli`.

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

Phase 2C0 Tool Runtime Foundation is IMPLEMENTED in
`9profs-core-rs/crates/nineprofs-tools/`. It is the 9Profs source of truth for
transport-neutral tool definitions, provider contributions, deterministic
concurrency-safe registry lookup, enabled/disabled state, coarse effect
metadata, handler execution, and future transport-safe tool events. Registry
availability is not per-run authorization: the default `ToolSet` is empty and
only an explicit run tool set can grant a registered enabled tool. Assistant
and Skill composition does not implicitly register or authorize tools.

`nineprofs-agent` contains the narrow `AionRsToolAdapter`. It supplies only
explicitly authorized 9Profs handlers to the pinned AionRS `ToolRegistry`.
AionRS remains an execution engine, not the source of truth for tools; its
bootstrap/default tool registry is still not enabled.

Still NOT IMPLEMENTED: ACP/external CLI backends, full Extensions runtime,
OfficeCLI resident mode, Sheets/Slides adapters,
manuscript/bibliography extraction, research UI, conversation/session
persistence, and account/product services. Phase 4C2
extends the Core-generated proposed ChangeSet with trusted user decision
workflow and the existing approved bridge path; no Agent tool can create an
approved status or call the document mutation gateway.

Phase 1B upstream adaptation record: assistant resource/catalog patterns came
from `crates/aionui-assistant/src/builtin.rs` and `service.rs`; SKILL.md
discovery, source handling, and configured path safety came from
`crates/aionui-extension/src/skill_service.rs`, `loader.rs`, and
`asset_paths.rs`; representative resources came from
`crates/aionui-app/assets/builtin-assistants` and `builtin-skills`.
`nineprofs-assistant` and `nineprofs-skills` contain 9Profs-owned adaptations;
no AionRS, agent runtime, MCP, OfficeCLI, or frontend architecture was copied.

## Migration conclusion

MCP is now implemented behind the 9Profs Tool Runtime: MCP server ->
`nineprofs-mcp` -> `nineprofs-tools` -> explicit `ToolSet` ->
`AionRsToolAdapter`. Direct MCP -> AionRS registry registration is prohibited.
Phase 3A adds OfficeCLI only as a pinned read-only sidecar:
`OfficeCliToolProvider` -> `OfficeCliRunner` -> OfficeCLI. It cannot mutate
active GenOffice documents, install skills, self-update, or use OfficeCLI MCP.
The document gateway routes approved `DocumentChangeSet` values into GenOffice transactions. Research and
SaaS/product services come after those boundaries are proven.

The full migration matrix and Phase 0–6 sequence are maintained in
[9PROFS-ARCHITECTURE.md](9PROFS-ARCHITECTURE.md).

## Phase 3A OfficeCLI qualification status

The Phase 3A sidecar remains read-only and deny-by-default. The exact
qualification binary used for this audit was the Windows x64 v1.0.144 release
asset from pinned upstream commit
`1ced45e900782c5083ed550ddf328ee974e425e7`, downloaded from the upstream
release URL into the external temporary path
`C:\Users\ncdhuy\AppData\Local\Temp\9profs-officecli-qualification\officecli-win-x64.exe`.
It was never copied into this repository or used as the global installation.

Run the explicit fail-closed gate with:

```powershell
$env:NINEPROFS_OFFICECLI_PATH = '<external path to the exact v1.0.144 binary>'
npm run test:officecli:qualification
```

The command verifies the exact version through `OfficeCliRunner::initialize`,
executes the real typed read-only operations, checks source-byte equality, and
exercises production HTML-to-PNG artifact containment. Ordinary tests remain
binary independent and skip the real qualification test when the opt-in variable
is absent.

Real v1.0.144 accepted the typed mappings for `view text`, `view annotated`,
`view outline`, `view stats`, `view issues`, `get`, `query`, `validate`, and
`view <file> html`. HTML is returned on stdout, not as a path. Observed output
is one HTML document containing logical `.page`, `.sheet-content[data-sheet]`,
or `.slide-container[data-slide]` nodes; the rasterizer returns one PNG
artifact reference per selected logical node. DOCX HTML retained external
KaTeX stylesheet/script URLs; XLSX and PPTX qualification HTML had no external
references. Rasterization blocks all HTTP(S)/WebSocket requests, so those
remote KaTeX URLs are intentionally unavailable rather than unrestricted.

Production visual qualification now passes with the exact pinned binary:
DOCX HTML plus PNG, XLSX HTML plus PNG, and PPTX HTML plus PNG all succeed;
all three source files remain byte-identical. The existing Electron 43.3.0
runtime is reused through a hidden/offscreen `BrowserWindow` and
`capturePage`; no Playwright browser installation was added. OfficeCLI-native
`view screenshot` remains a diagnostic only: the prior v1.0.144 Windows run
timed out, and native Word-backed rendering is unavailable in this environment.

The isolated environment sets `HOME`, `USERPROFILE`, `APPDATA`,
`LOCALAPPDATA`, and XDG paths beneath the 9Profs-owned profile, disables
auto-install, auto-update, and OfficeCLI auto-resident mode, and keeps HTML,
PNG, manifest, Electron user data, and cache output under controlled roots.
Raster limits are 16 MiB HTML, 4096 physical pixels per dimension, 64 logical
pages/slides/sheets, 64 MiB total PNG output, and a 30-second end-to-end
render timeout. Page load, capture, and settle waits are bounded; cancellation
or timeout drops the process future, and Windows subprocesses use a kill-on-job-close
boundary so OfficeCLI/Electron descendants do not survive.

`ArtifactResolver::resolve` is the trusted execution boundary: registration
does not permanently trust a path. Resolution re-canonicalizes the current
target, rechecks existence, supported extension, approved-root containment,
and link escape, with deterministic move and symlink replacement tests.
Resident mode, MCP mode, skill installation, update, and active GenOffice
mutation remain intentionally outside Phase 3A/3B. Phase 4A now owns the
editor-side active DOCX gateway. Phase 3B mutation is
available only through the typed ToolProvider; raw/raw-set/add-part and
generic CLI passthrough are not exposed.

## Phase 5B2.1 retrieval boundary

The scoped Dify retrieval qualification closes the retrieval boundary needed by
Phase 5C2. Case-wide retrieval remains available for literature search, while
citation verification must pass exact cited `ExtractionId` values through the
provider-neutral `ResearchRetrievalScope`. Core validates case ownership and
qualified local index state before the remote request, then validates every
returned segment against local mappings and canonical page hashes. Dify document
metadata carries only the 9Profs extraction, source, and snapshot IDs; it never
carries paths, secrets, or evidence text. Existing unqualified Phase 5B2 indexes
remain case-wide compatible but require explicit resync before scoped use.

## Phase 5C1 citation binding boundary

`nineprofs-research` now owns provider-neutral citation identity and binding.
`CitationOccurrence` records an observed manuscript marker/group and preserves
document identity, version, and locator metadata. `CitationTarget` records
ordered reference keys, including unresolved and grouped targets. A separate
`CitationTargetBinding` may bind a target to a source, or to the exact source
snapshot and ready PDF extraction needed for future verification. Bindings are
append-oriented; a correction creates a new binding and retains history.

`ClaimCitationLink` is a many-to-many association only. It does not create
`ResearchEvidence`, make a support/contradiction assessment, or replace
`ClaimEvidenceLink`. Core writes use the trusted session-secret boundary and
emit identifier-only citation lifecycle events. No parser, UI, or Agent tool is
part of 5C1; those remain 5C2B/5C3 work.

## Phase 5C2A citation verification orchestration

`nineprofs-research-verification` now owns the provider-neutral verification
orchestrator. It validates the exact claim → citation occurrence → target →
binding chain and refuses bindings without a ready, exact PDF `ExtractionId`.
Retrieval is delegated through the provider-neutral interface and the Dify
adapter is called with an extraction-only scope.

Candidates are immutable audit records containing only canonical identity and
range data: retrieval chunk, source, snapshot, extraction, page, Unicode
offsets, canonical excerpt hash, rank, and score. Canonical page text is passed
to the assessor in memory; remote/provider text is never persisted or promoted.
The assessor returns a structured relation and selected candidate IDs. Only
those IDs are revalidated and promoted through the existing exact
`capture_pdf_evidence` path, then connected to the claim with the generic
`ClaimEvidenceLink` and explicit verification-to-evidence mapping.

Runs persist `running`, `completed`, or `failed` state, typed failure codes,
result metadata, candidate audits, and evidence mappings. Core exposes trusted
POST creation plus read/list endpoints and identifier-only lifecycle events.
No real model/provider, parser, UI, or Agent tool is included in this phase.

## Pinned OfficeCLI v1.0.144 mutation audit

The exact upstream source at commit
`1ced45e900782c5083ed550ddf328ee974e425e7` was audited before adding the
adapter. `create` dispatches to the pinned OpenXML blank-document creators for
`.docx`, `.xlsx`, and `.pptx`. The semantic handler surface includes `set`,
`add`, `remove`, `move`, `copy`, `swap`, `validate`, and `save`; the 9Profs
adapter owns all CLI flag and selector translation internally.

With the default non-resident invocation, mutations are persisted eagerly by
OfficeCLI and `save` is an explicit flush command. Upstream `batch` reports
atomic rollback by default, while `--best-effort` intentionally permits
partial success; 9Profs therefore does not depend on upstream batch rollback.
It applies the typed operations to a same-root working revision and makes the
filesystem promotion/cleanup boundary authoritative. `raw`, `raw-set`,
`add-part`, arbitrary imports, generic CLI arguments, resident mode, MCP mode,
auto-update, and skill installation are outside the Phase 3B provider.
