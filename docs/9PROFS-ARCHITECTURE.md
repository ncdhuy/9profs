# 9Profs current architecture

Status: canonical current-state architecture for the `develop` branch, audited
from repository source, manifests, tests, and current documentation on
2026-08-28.

This document is the current cross-product architecture authority. It describes
what is implemented at the audited repository state, then separates historical
milestones and future work. The [architecture audit](9PROFS-ARCHITECTURE-AUDIT.md)
is retained as time-bound historical evidence. The [DOCX presentation boundary](DOCX-PRESENTATION-V2.md)
and [DOCX reference map](DOCX-V2-REFERENCE-MAP.md) are focused design/reference
documents, not competing cross-product architecture authorities.

9Profs is a strategic fork of GenOffice, not a rewrite. GenOffice-derived Office
behavior remains the default product foundation. Rust Core, research services,
and agent/tool seams extend that foundation without replacing the protected
document model or save pipeline.

## Current topology

```text
Electron shell and product apps
├─ apps/shell
├─ apps/docs       DOCX editor
├─ apps/sheets     XLSX editor
├─ apps/slides     PPTX editor
├─ apps/pdf        PDF editor/viewer
└─ apps/markdown   Markdown editor

GenOffice Office Core
├─ packages/docx-engine       DOCX OOXML persistence and model semantics
├─ packages/pptx-engine        PPTX model and persistence
├─ packages/pptx-render        PPTX rendering
├─ apps/sheets/native          XLSX Rust sidecar
├─ apps/docs                   Tiptap/ProseMirror editing and presentation
└─ app-specific save, renderer, and command paths

9Profs Core runtime
└─ 9profs-core-rs/app/nineprofs-core
   └─ CoreRuntime
      ├─ HTTP API and realtime/event transport
      ├─ AssistantService and SkillCatalog
      ├─ AgentRegistry, task lifecycle, and AgentExecutionService
      ├─ ToolRegistry and ToolProvider implementations
      ├─ active-document bridge and proposal workflow
      ├─ ResearchService and research adapters
      └─ SQLite-backed local persistence

Transport and editor adapters
├─ packages/9profs-core          TypeScript Core transport/types
├─ packages/document-gateway     active-document contracts
├─ packages/genoffice-adapter    GenOffice Docs adapter
└─ packages/officecli-adapter    OfficeCLI transport-neutral contracts
```

The Rust workspace is physically split into bounded crates. The current
runtime composition includes `nineprofs-common`, `nineprofs-api-types`,
`nineprofs-db`, `nineprofs-realtime`, `nineprofs-documents`,
`nineprofs-document-tools`, `nineprofs-assistant`, `nineprofs-skills`,
`nineprofs-tools`, `nineprofs-officecli`, `nineprofs-mcp`, `nineprofs-agent`,
`nineprofs-research`, `nineprofs-research-dify`,
`nineprofs-structured-model`, `nineprofs-research-assessor`,
`nineprofs-research-claim-extractor`, `nineprofs-research-verification`,
`nineprofs-runtime`, and the `nineprofs-core` application.

The current product is local-first. `CoreRuntime` uses a local data directory
and SQLite database, exposes loopback HTTP/WebSocket services, and coordinates
the desktop apps. Accounts, billing, multi-tenancy, remote cloud storage, and a
remote SaaS backend are not current product architecture.

## Non-negotiable authority rules

1. GenOffice-derived interactive Office editors own active editor state.
2. One active document has one canonical write authority. For a document open in
   GenOffice, GenOffice owns inspection, mutation, dirty state, and save.
3. AI uses existing document/editor contracts and typed tools. It does not edit
   presentation DOM or bypass document-model and save/reparse semantics.
4. `packages/docx-engine` remains the protected DOCX persistence boundary. Its
   parse, patch, generate, raw-XML, anchor, and round-trip contracts are not
   replaced by Core, OfficeCLI, or a new document engine.
5. Research records observations and provenance relationships. It does not turn
   retrieval ranking or model output into truth automatically.

## Core, agent, skills, and tools

### Rust Core runtime

`9profs-core-rs/app/nineprofs-core` starts `nineprofs_runtime::CoreRuntime`.
`CoreRuntime` composes the current services: assistants, skills, agent
registry/task management, agent execution, tools, MCP, OfficeCLI, active
documents, research, Dify retrieval, and citation verification. API modules
under `app/nineprofs-core/src/api` expose these services through typed HTTP and
WebSocket boundaries; realtime events use the shared event bus.

The TypeScript `packages/agent-core` package remains implemented GenOffice/app
AI infrastructure for local loops, skills, tools, stream callbacks, and IPC.
It is not the authority for the Rust 9Profs Core runtime. The Rust runtime is
the current composition point for Core-owned assistants, skills, tools,
research orchestration, and active-document workflows.

### Assistant and skill metadata

- `nineprofs-assistant` owns built-in and custom assistant descriptors, rules,
  persistence, and stable backend metadata.
- `nineprofs-skills` owns built-in/custom `SKILL.md` loading, deterministic
  precedence, and skill catalog resources.
- Assistant configuration and skill instructions provide persona and
  instructions. They do not silently register or authorize tools.

### Agent execution authority

The current execution chain is:

```text
Assistant / Skill catalog
        |
        v
AgentExecutionService
        |
        v
AgentExecutor registry
        |
        v
AionRsExecutor
        |
        v
AionRS AgentEngine
```

`nineprofs-agent` owns the 9Profs executor boundary, backend/task identities,
execution events, provider configuration, and the `AionRsExecutor` adapter.
AionRS is the embedded/default agent execution backend. AionRS types remain
behind the 9Profs-owned executor and tool adapters.

### Tool authority and MCP

`nineprofs-tools` owns the transport-neutral tool definitions, effects, policy
metadata, enabled state, provider registry, and handler execution. Its
`ToolRegistry` is the source of truth for available 9Profs tools. Availability
and per-run authorization are separate:

```text
available ToolRegistry != tools granted to current ToolSet
```

The default tool set is bounded/empty unless a caller supplies an explicit
per-run `ToolSet`. Disabled tools remain discoverable but cannot execute.
`ToolProvider` implementations contribute typed registrations; effects and
tool arguments remain explicit.

`AionRsToolAdapter` converts only the authorized 9Profs registrations into the
minimum AionRS tool surface. AionRS's own registry is an execution adapter,
not the source of truth for tool policy or availability. `nineprofs-mcp` owns
MCP configuration, persistence, connection/discovery, redacted lifecycle
summaries, and its shared provider boundary. MCP registration does not become
available to an agent until the Core tool policy explicitly authorizes it.

Provider credentials and session secrets are resolved at controlled boundaries.
Secret values are not exposed in configuration debug output, DTOs, lifecycle
events, or normalized errors.

## Active Office document authority

The active-document workflow is proposal-based and user-bounded:

```text
Agent
  |
  v
DocumentChangeSet proposal
  |
  v
user review / trusted approval
  |
  v
Core proposal workflow
  |
  v
DocumentMutationGateway / document bridge
  |
  v
GenOffice adapter
  |
  v
executeCommands()
  |
  v
editor transaction
  |
  v
dirty tracking and save/reparse
```

The exact implementation is split across these boundaries:

- `nineprofs-document-tools` owns explicit active-document tools, proposal
  state, freshness checks, and the trusted proposal workflow.
- `nineprofs-documents` owns the ephemeral active-session registry and
  bidirectional renderer bridge. Rust Core routes and correlates requests; it
  does not own editor state or DOCX objects.
- `packages/document-gateway` defines typed active-document inspection,
  `DocumentChangeSet`, approval, mutation, and result contracts.
- `packages/genoffice-adapter` binds the active Docs session to
  `buildDocumentContext` and `executeCommands`, performs document-version
  checks, and reuses the existing Docs command engine.
- `apps/docs/src/renderer/ai/commands.ts` remains the command execution path.

The active target is identified by the stable `DocumentId` for one continuous
editor session and a monotonic `DocumentVersion` for optimistic concurrency.
Save, autosave, Save As, and same-document reparse preserve the session;
replacement of the active document rotates the identity. A stale or
unapproved change set fails before commit. Agent code cannot silently approve
or commit its own active-document mutation.

GenOffice remains the only active-document writer. OfficeCLI is not part of
this write path.

## OfficeCLI sidecar

`nineprofs-officecli` is a detached/unowned Office semantic sidecar. Its
`OfficeCliToolProvider` exposes bounded typed inspection, query, outline/stats,
issue validation, rendering, and typed mutation operations for controlled
artifacts.

Detached mutation is copy-on-write: resolve a writable detached or newly
created artifact, create a same-root working revision, apply typed operations,
save, structurally validate, rasterize through the qualified HTML-to-PNG path,
and atomically promote a new revision. Source bytes remain unchanged on
failure. Process, argument, output, page, image, timeout, cancellation, and
artifact-root limits are enforced.

OfficeCLI cannot write an active GenOffice document. Active references and
inspection snapshots are rejected by the writable eligibility boundary.
OfficeCLI resident mode, active-document mutation, raw/arbitrary CLI
passthrough, and OfficeCLI-native skills remain deferred. Confirmation metadata
exists on detached write tools, but it is not a substitute for the Core
proposal/review workflow.

## Research and provenance architecture

`nineprofs-research` is the current Research Domain authority. It owns cases,
sources, immutable source snapshots, PDF extraction revisions, evidence,
claims, citation relationships, and manuscript synchronization records.
Its public model and service are exposed through Core research routes and the
TypeScript Core transport.

### Domain entities and ownership

```text
ResearchCase
├─ ResearchSource
│  └─ immutable ResearchSourceSnapshot
│     └─ ResearchPdfExtraction
│        └─ canonical ResearchPdfPage text/ranges
├─ ResearchEvidence
├─ ResearchClaim
│  └─ ClaimEvidenceLink
├─ CitationOccurrence
│  └─ CitationTarget
│     └─ CitationTargetBinding
│        └─ ResearchSource / optional exact snapshot + extraction
├─ ClaimCitationLink
├─ ManuscriptCitationSyncRun
├─ ManuscriptReferenceCatalogRun
│  └─ ManuscriptReferenceEntry + target mappings
└─ manuscript claim-extraction runs/items/coverage
```

The following distinctions are architectural invariants:

- `Evidence != truth`. `ResearchEvidence` is a bounded observation anchored to
  a source snapshot and locator; it is not a truth assertion.
- `RetrievalCandidate != ResearchEvidence`. A remote or local retrieval result
  becomes evidence only through the canonical Research workflow.
- `Dify != provenance authority`. Dify is an adapter and derived index.
- Dify text is not canonical evidence or canonical text storage.
- `ResearchSourceSnapshot` is immutable captured source state. A changed source
  creates a new snapshot rather than rewriting the old one.
- PDF evidence pins the exact `ExtractionId` plus page/range. Snapshot ID alone
  does not identify a derived PDF text revision.
- `ClaimCitationLink != ClaimEvidenceLink`. The former associates a manuscript
  claim with a citation occurrence; the latter records an attributed evidence
  relation such as supports, contradicts, contextualizes, or insufficient.
- `CitationTargetBinding` is an identity/source binding, not a citation
  verification result.
- A DOCX citation atom is an inline document-format structure. It is not a
  Research `CitationOccurrence` until the manuscript synchronization boundary
  inventories and persists that observation.
- Active document `DocumentId != ResearchSourceId`. Editor-session routing and
  research-source identity belong to different domains.
- `ManuscriptReferenceEntry != ResearchSource`. A reference-catalog entry is a
  manuscript-side metadata observation and resolution hint, not a source record.
- Models/LLMs never create provenance directly. Core services validate identity,
  ranges, hashes, source ownership, and persistence relationships.

## PDF ingestion and Dify retrieval boundary

Reference PDFs follow this authority-preserving pipeline:

```text
original PDF bytes
    |
    v
immutable ResearchSourceSnapshot
    |
    v
immutable ResearchPdfExtraction revision
    |
    v
canonical per-page text and ranges
    |
    v
deterministic local chunks and range mappings
    |
    v
Dify derived vector index
    |
    v
remote candidate identity and score
    |
    v
local canonical mapping
    |
    v
hash/range verification
    |
    v
ResearchRetrievalCandidate
```

`ResearchArtifactStore` captures bounded PDF bytes and content identity.
`ResearchSourceSnapshot` records the verified artifact hash and origin.
Page-preserving extraction persists immutable extraction revisions, one-based
page text, page hashes, extraction hash, extractor identity, and explicit
failed/empty/password-required statuses.

`nineprofs-research-dify` owns the Dify adapter and local index metadata. It
creates or refreshes Dify indexes from canonical extraction pages, provisions
and verifies canonical extraction/source/snapshot metadata, maps remote
document/segment identity to local chunk/range rows, and fails closed on
unknown segments, scope violations, index drift, or hash mismatch.

`ResearchRetrievalScope` supports case-wide retrieval and bounded exact source
or extraction identity scopes. Exact extraction retrieval requires case
ownership, a ready extraction, and a metadata-qualified local Dify index.
`ResearchRetrievalCandidate` carries the remote result only after local scope,
identity, hash, and range checks. Dify can be rebuilt or degraded without
changing canonical source, extraction, evidence, or provenance records.

Dify is therefore:

- a derived retrieval/index adapter;
- not source authority;
- not evidence authority;
- not provenance authority; and
- not canonical text storage.

Live Dify qualification requires configured external credentials. Unit and
contract paths use isolated fixtures/mocks; no unimplemented Dify product/UI
capabilities are implied here.

## Citation, verification, and manuscript workflows

### Citation domain

`CitationOccurrence` records an observed manuscript marker/group with its
document origin. `CitationTarget` records the ordered reference keys within an
occurrence. Targets may remain unresolved. `CitationTargetBinding` records an
append-oriented source identity binding and, when available, exact
`SourceSnapshotId` and PDF `ExtractionId`.

`ClaimCitationLink` is a many-to-many association between a `ResearchClaim` and
an observed citation occurrence. It does not assert support, contradiction, or
evidence. `ClaimEvidenceLink` remains the separate attributed assessment
boundary.

### Citation verification

`nineprofs-research-verification` is the citation verification orchestrator.
It selects the exact cited source and exact bound extraction, requests scoped
retrieval, validates candidate identity and ranges, promotes only verified
canonical excerpts into `ResearchEvidence`, and persists the verification
record and evidence relation.

`CitationAssessmentProvider` is a semantic assessment seam. The model-backed
`ModelCitationAssessor` in `nineprofs-research-assessor` evaluates only the
canonical candidates supplied by the orchestrator. It does not retrieve,
create provenance, select arbitrary sources, or invent evidence IDs, source
IDs, page ranges, or truth values.

### DOCX citation inventory and manuscript synchronization

`apps/docs/src/renderer/editor/manuscript-citations.ts` inventories the
structured DOCX citation atoms. A completed `ManuscriptCitationSyncRun` stores
the document identity/version, occurrence descriptors, and ordered targets in
the Research Domain. The adapter is read-only with respect to PM state and
DOCX XML; it does not turn an atom into a source binding by itself.

The current manuscript claim extractor is also bounded and read-only:

- it consumes a complete, exact citation-sync inventory and bounded manuscript
  block text;
- it returns atomic propositions, code-point ranges, and existing occurrence
  IDs;
- it does not retrieve evidence, invent provenance, return source IDs or PDF
  IDs, or write the document.

`ManuscriptReferenceCatalogRun` and `ManuscriptReferenceEntry` persist a
version-scoped reference catalog from structured DOCX citation metadata,
including bounded Word-native and Zotero hints, with exact target mappings.
The catalog does not resolve a reference to `ResearchSource`, create a
`CitationTargetBinding`, create evidence, or perform verification.

## Shared structured-model transport

`nineprofs-structured-model` is shared provider infrastructure for semantic
adapters. It owns:

- provider configuration and endpoint resolution;
- credential environment-variable resolution;
- authenticated HTTP transport;
- bounded timeouts, response bytes, and output limits; and
- transport error normalization and secret-safe diagnostics.

Semantic adapters retain their own prompts, request/response schemas, parsers,
semantic validation, and domain contracts. In particular, the citation
assessor and manuscript claim extractor are adapters, not provenance
authorities. They receive bounded inputs and return constrained semantic
outputs to Core-owned workflows.

## Research service and repository architecture

The research implementation currently has one public service and one
object-safe repository abstraction:

```text
ResearchService
    |
    | Arc<dyn ResearchRepository>
    v
ResearchRepository
    ^
    |
SqliteResearchRepository
```

`ResearchService` is split by bounded service modules for case/source
snapshots, PDF extraction, evidence/claims, citations, manuscript citation
sync, manuscript claim extraction, and reference catalogs. The SQLite
implementation is split by repository domains for those same concerns.

`ResearchRepository` remains one object-safe abstraction over the complete
Research Domain, while `SqliteResearchRepository` is its current implementation.
This is the deliberate stabilized shape; the current architecture does not
require another capability-trait split or another round of service/repository
decomposition. `model.rs` size alone is not architectural debt.

## DOCX architecture

DOCX persistence, editing state, presentation, and Core integration remain
separate concerns:

| Boundary                    | Current authority                                                                                                                                |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| OOXML persistence/model     | `packages/docx-engine`: parse, block model, raw XML, anchors, patch/generate, ZIP preservation, and round-trip semantics                         |
| Editable document state     | Tiptap/ProseMirror in `apps/docs`; schema, commands, transactions, selections, nested editors, undo/redo, comments, and revisions                |
| Editor/model conversion     | `blocksToPmDoc`, `pmDocToSavePlan`, stable `docxIndex` identity, and save-plan semantics                                                         |
| Presentation/layout         | `apps/docs/src/renderer/pagination.ts`, `line-metrics.ts`, `doc-style-css.ts`, and `presentation-v2/`; derived geometry only                     |
| Dirty/save/reparse          | Docs dirty tracking and `saveDocx`/save-until-persisted paths; explicit save ordering, atomicity, reparse, recovery, caret, and history behavior |
| Active-document integration | `packages/document-gateway`, `packages/genoffice-adapter`, Rust document bridge, and existing `executeCommands()`                                |
| Inline citations            | DOCX citation atoms and inventory; separate from Research provenance records                                                                     |

Presentation V1 remains the production/default compatibility path. Presentation
V2 is implemented as an experimental, feature/test-selected derived-layout
path. V2 owns renderer selection, pagination orchestration, measurement
refinement, invalidation, geometry, post-render readback, and parity diagnostics;
it reuses established GenOffice primitives and does not own OOXML, PM state,
dirty tracking, save, reparse, comments, revisions, anchors, or document
identity. V1 remains available until broader geometry, position-mapping,
save/reopen, and preservation proof supports a deliberate default change.

The detailed V2 boundary and external reference comparison remain in the
focused documents linked above. They are not a second current architecture.

## Other Office components

These components retain their GenOffice-derived boundaries and persistence:

- Sheets uses its Univer-based renderer, XLSX gateway/package I/O, and Rust XLSX
  sidecar. Research does not own XLSX persistence.
- Slides uses `pptx-engine`, `pptx-render`, and the Slides renderer. Research
  does not own PPTX persistence.
- PDF uses the PDF.js/PDFium/pdf-lib viewer and main-process save/edit paths.
  Research may consume captured sources through explicit Research workflows but
  does not own PDF editor persistence.
- Markdown uses its Tiptap editor and Markdown serialization/export paths.
  Research does not own Markdown persistence.
- The Electron shell hosts app views/tabs and shared UI, settings, and local
  workspace facilities.

## CI and governance baseline

R2A quality governance is part of the current architecture baseline. The
canonical operational details are in [CI-QUALITY-GATES.md](CI-QUALITY-GATES.md).

At a high level, the required mergeability gate is the single `9Profs Required`
check. It composes mandatory Rust workspace, TypeScript correctness/tests,
Docs critical tests, and Docs production-build checks. Inherited baseline
diagnostics, environment-sensitive tests, and live OfficeCLI/Dify/model
qualifications are classified separately and do not silently become green
dependencies of `9Profs Required`.

## Historical implementation milestones

This short list explains how current seams arrived; it is not a second current
architecture and does not override source evidence.

- Phase 0 established transport-neutral TypeScript contracts for Core and
  document integration.
- Phases 1-2 established Rust Core bootstrap, assistants, skills, agent/task
  lifecycle, AionRS execution, tool policy, and MCP provider boundaries.
- Phases 3-4 established the bounded OfficeCLI sidecar, detached mutation, the
  active GenOffice adapter, the document bridge, and proposal-based active
  document mutation.
- Phase 5 established the Research Domain, immutable PDF provenance, scoped
  Dify retrieval, citation binding, citation verification, model-backed
  assessment, DOCX citation inventory, manuscript synchronization, atomic claim
  extraction, reference-catalog synchronization, and deterministic manuscript
  reference resolution with exact citation binding.
- R2A established the `9Profs Required` CI contract.

For original audit evidence and phase-specific rationale, see
[9PROFS-ARCHITECTURE-AUDIT.md](9PROFS-ARCHITECTURE-AUDIT.md),
[DOCX-PRESENTATION-V2.md](DOCX-PRESENTATION-V2.md), and
[DOCX-V2-REFERENCE-MAP.md](DOCX-V2-REFERENCE-MAP.md).

## Current research roadmap

- 5C Citation Checker (IMPLEMENTED): Core owns manuscript citation sync,
  reference catalog, resolution, citation-scoped claim extraction, review
  projection, and verification. Docs supplies live citation observations and
  exact PM-atom navigation without mutating the document or bypassing the
  save pipeline.
- 5D1 Claim Inventory (IMPLEMENTED): Whole-Manuscript Claim Inventory. Docs supplies every
  eligible visible paragraph, heading, and list-item block, including uncited
  blocks; Core persists bounded observation-first claims, coverage, Unicode
  source ranges, compact review kinds, provider identity, and explicit scope
  limitations. Inventory runs do not create ResearchClaim, ClaimCitationLink,
  ResearchEvidence, Dify, or Agent records.

### Next/future

- 5D2A Structural Coverage (IMPLEMENTED): Core composes one completed 5D1
  inventory with one compatible completed Citation Review. It proves only
  exact span-and-proposition claim bridges, neutral structural citation
  observations, target-level Citation Review states, and existing evidence
  relations. Structural citation observation, citation expectation, and
  evidence relation remain separate; citation expectation/judgment is 5D2B.
- 5D2B Citation Expectation + Coverage Judgment (IMPLEMENTED): Core preserves
  deterministic 5D2A structural coverage, sends only blind claim semantics to
  an optional model-backed citation-expectation assessor, and composes the
  closed-set attention state deterministically from expectation plus coverage
  facts. It does not retrieve evidence, create provenance, or assert truth;
  failed item assessments remain retryable and scope limitations are carried
  forward.
- 5D3A Cross-Claim Consistency Candidate Discovery (IMPLEMENTED): Core schedules
  deterministic same-batch and cross-batch comparisons over one completed 5D1
  inventory, persists resumable window/run state, and validates model output as a
  closed set of bounded potential consistency candidates. It is candidate
  discovery only: it does not assert contradiction, truth, evidence, provenance,
  or final review judgment. Because 5D1 has no canonical semantic section
  hierarchy, this is claim-global rather than a typed Abstract/Methods/Results
  section-consistency engine. 5D3B final cross-claim assessment is next.
- 5D4 Whole Review UI (FUTURE): whole-manuscript review experience.
- 5E: manuscript and Sheets/data consistency.
- 5F: methodology and domain research skills.

Other explicitly deferred capabilities include active-document writing through
OfficeCLI, OfficeCLI resident mode, a V2 default replacement without the
required proof, citation/research review UI beyond the listed phases, and SaaS
account/billing/multi-tenant/cloud product layers. These remain future work;
their architectural seams do not imply implementation.
