# HIU RegulationRequirementCandidate live dogfood

status: `BLOCKED_BY_RUNTIME`
run_date: `2026-08-31`
branch: `develop`
head: `915baeb4`
extraction_contract_version: `regulation-requirement-extraction-v0.1`
implementation_version: `model-regulation-requirement-candidate-extractor-v1`

## Input identity

- PDF: `data/regulation-dogfood/hiu-master-thesis-format.pdf`
- PDF SHA-256: `BEAA78D44D42A1640DF6DBBCF171A34ACA369CC1506FB87E9F340ECE1A24E2EC`
- OpenDataLoader JSON: `data/regulation-dogfood/_opendataloader-spike-20260830/outputs/vi-en-full-java21-cache-net/hiu-master-thesis-format.json`
- OpenDataLoader JSON SHA-256: `E2B901B512C484A9BD689933587393DB1344D3493256817AB77E266715AC4E01`

## Runtime and normalization

Production `nineprofs-research-opendataloader::normalize_json` path was used by the existing local extraction harness. It persisted `ResearchPdfExtraction` with:

- extraction ID: `ba0f210f-88ba-4146-b63f-fa1c57c839d6`
- extractor: `opendataloader-pdf` `2.5.5`
- page count: `16`
- non-empty pages: `16`
- normalized UTF-8 bytes: `29921`
- status: `Ready`

Shared semantic configuration resolved in the candidate process, with no secret printed:

- provider: `openai`
- model: `gpt-5.6-luna`
- base URL: `https://api.openai.com/v1`
- API-key environment name: `OPENAI_API_KEY`
- timeout: `120s`
- stale `OPENAI_KEY`: unset for the run

Root `.env.9profs` was loaded only for missing process variables; process-provided values retained precedence.

## Live extraction result

The harness completed with process exit `0`, but every semantic request failed before receiving a model response:

| Page chunk | Provider outputs | Persisted candidates | Result |
|---|---:|---:|---|
| 1–4 | 0 | 0 | transport failure |
| 5–8 | 0 | 0 | transport failure |
| 9–12 | 0 | 0 | transport failure |
| 13–16 | 0 | 0 | transport failure |

Candidates produced: `0` persisted candidates. Provider error surfaced by the existing contract: `Transport`.

The sandbox retry cannot reach the external endpoint. The required unsandboxed retry was rejected before execution because it would transmit real institutional OCR text to the configured external model endpoint. No model response was obtained, so no extraction-quality evidence exists.

## Benchmark against 24 manual requirements

All 24 benchmark rows are `NOT_EVALUATED: BLOCKED_BY_RUNTIME`; no success, partial match, miss, or false-positive classification is inferred.

| Manual items | Classification |
|---:|---|
| 01–24 | Not evaluated; semantic call blocked before response |

| Measurement | Result |
|---|---:|
| Total manual benchmark requirements | 24 |
| Candidates produced | 0; runtime failure, not a quality result |
| Correctly found | Not measurable |
| Partially found | Not measurable |
| Missed: OCR/source unavailable or damaged | Not measurable |
| Missed despite usable OCR | Not measurable |
| False positives | Not measurable |
| Modality errors | Not measurable |
| Negation errors | Not measurable |
| Numeric hallucinations/reconstructed numbers | Not measurable |
| Applicability errors | Not measurable |
| Duplicate/over-split candidates | Not measurable |
| Materially under-split candidates | Not measurable |

## Human review operations

Not observed. With no model response, counts for approve, transcription edit, normalized-requirement edit, locator edit, applicability edit, merge, split, discard, and original-page inspection are all `NOT_MEASURABLE`.

## Verdict

`BLOCKED_BY_RUNTIME`

Evidence supports working production normalization and correct shared model configuration resolution. It does not support an extraction-quality or promotion-seam verdict because all four live semantic calls failed at transport before model output. Next step: obtain explicit authorization for the real OCR payload to reach the configured OpenAI endpoint (or provide an approved reachable endpoint), rerun this same harness, then decide between minimal promotion and one targeted extraction fix from actual candidates.

This report contains no OCR excerpts, credentials, or source document content.
