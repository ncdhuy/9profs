# 9Profs Research MVP dogfooding

This is the short local workflow for using the existing Research Review MVP
with real manuscripts. Research Review is read-only and does not change the
document.

## Prerequisites

- Node.js `>=22.12.0` (`.nvmrc` currently specifies Node 22).
- npm `>=10`.
- Rust and `cargo` on `PATH` for 9Profs Core.
- A real `.docx` manuscript. Reference PDFs are needed for citation evidence
  verification.

## First setup

From the repository root:

```bash
npm install
npm run setup:9profs
```

Edit `.env.9profs`. The setup command creates it only when missing and never
overwrites an existing file.

Check readiness, then launch the complete local topology:

```bash
npm run doctor:9profs
npm run dev:9profs
```

`dev:9profs` loads `.env.9profs`, starts or reuses a compatible 9Profs Core,
waits for `/api/health` and `/api/runtime`, then starts the existing renderer
development servers and Electron shell. Ctrl+C shuts down Core only when this
command started it. `npm run dev` remains unchanged.

## Research configuration

`.env.9profs.example` contains the exact current variable names. Configure
each semantic task separately, even when all tasks use the same provider,
model, and key environment variable:

- Claim extraction: `NINEPROFS_CLAIM_EXTRACTOR_*`.
- Citation assessment: `NINEPROFS_CITATION_ASSESSOR_*`.
- Citation expectation: `NINEPROFS_CITATION_EXPECTATION_ASSESSOR_*`.
- Cross-claim candidate discovery: `NINEPROFS_CROSS_CLAIM_CANDIDATE_*`.
- Cross-claim consistency assessment:
  `NINEPROFS_CROSS_CLAIM_CONSISTENCY_ASSESSOR_*`.

Each task uses `PROVIDER`, `MODEL`, `BASE_URL`, and `API_KEY_ENV`; the four
assessor/discovery tasks also accept their existing `TIMEOUT_MS` variable.
Current structured-model providers are `openai` and `anthropic`. An
OpenAI-compatible or local server uses `openai` with its own `BASE_URL` and a
non-empty key value in the environment named by `API_KEY_ENV`; the server may
ignore that value if it does not require authentication.

Dify is separate:

- `NINEPROFS_DIFY_BASE_URL`
- `NINEPROFS_DIFY_API_KEY`
- `NINEPROFS_DIFY_TIMEOUT_MS`
- `NINEPROFS_DIFY_INDEXING_TECHNIQUE`

Core uses `NINEPROFS_CORE_ADDR` (default `127.0.0.1:39761`) and
`NINEPROFS_CORE_DATA_DIR` (default `data/9profs-core`). Docs uses
`NINEPROFS_CORE_URL` when set; otherwise it uses `NINEPROFS_CORE_ADDR`.

## Readiness levels

Level 1 — Editor only

No Research AI configuration is required. Core still starts with the local
development topology, and the document editor can be used without Research
Review.

Level 2 — Research semantic review

Configure claim extraction, citation expectation, cross-claim candidate
discovery, and cross-claim consistency assessment. These support claim,
Evidence Coverage, and Internal Consistency analysis.

Level 3 — Full citation evidence verification

Also configure citation assessment and both Dify variables. Dify retrieves
evidence from reference-PDF indexes; the relevant Research case must contain
the reference PDFs and their retrieval indexes must be ready.

Without Dify, Whole Research Review still completes its unrelated semantic
stages. Citation items that require retrieval remain unavailable with the
bounded `retrieval_not_configured` verification failure state; this is not a
systemic failure of claim, coverage, expectation, or consistency review.

`doctor:9profs` checks Core using its health/runtime APIs, checks Dify through
the existing per-case retrieval-readiness API, and reports semantic provider
configuration without printing key values. It returns a failing exit status
only when Core configuration or reachability is not ready; missing optional
Research levels remain visible in the report.

## First real test

1. Open a real DOCX manuscript.
2. Open `Review` → `Research Review`.
3. Create or select a `ResearchCase`.
4. Create or select the manuscript `ResearchSource`.
5. Add or select reference PDFs where applicable.
6. Select `Run Research Review`.
7. Inspect `Evidence Coverage`, citation verification, and `Internal Consistency`.
8. Edit the manuscript manually.
9. Select `Re-run Research Review`.

## Dogfooding feedback checklist

For each real manuscript, record only:

```text
SETUP
- Anything preventing startup?

CORRECTNESS
- Any obviously wrong Research judgment?

UX
- Anything confusing or cumbersome?

PERFORMANCE
- Anything noticeably slow?

MISSING
- Something genuinely necessary to complete the workflow?
```

Real usage → evidence → next engineering task. This document covers the
current Research MVP only; it does not add later Research phases.
