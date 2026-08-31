# HIU RegulationRequirementCandidate live dogfood

status: `BLOCKED_BY_RUNTIME`
run_date: `2026-08-31`
branch: `develop`
head: `cea9686f`
extraction_contract_version: `regulation-requirement-extraction-v0.1`
implementation_version: `model-regulation-requirement-candidate-extractor-v1`

## Continuation smoke test

The 2026-08-31 continuation first exercised the same production shared path:
`StructuredModelConfig::from_env()` → `StructuredModelClient::execute_json()`.
The request contained only the synthetic prompt `Return exactly: OK`; no HIU
OCR content was transmitted during this step.

- provider: `openai`
- model: `gpt-5.6-luna`
- base URL: `https://api.openai.com/v1`
- endpoint: `https://api.openai.com/v1/chat/completions`
- credential environment name: `OPENAI_API_KEY`
- stale `OPENAI_KEY`: unset in child process
- local `.env.9profs`: used only to supply missing shared configuration
- OpenAI reached: yes
- HTTP result class: `Unauthorized` (shared client normalizes HTTP 401/403)
- shared-client error class: `Unauthorized`
- model response received: no

The real authorized HIU dogfood was not retried because the required smoke
gate failed at authentication. No new OCR content was transmitted, no new
candidates were produced, and no extractor code was changed.

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

Continuation rerun through the same production normalization function also
returned `16` pages, `16` non-empty pages, and `29921` normalized UTF-8 bytes
(local extraction ID `84407e29-eacb-4fc0-8ebf-b88065e25224`). It did not contact
the model provider.

Shared semantic configuration resolved in the candidate process, with no secret printed:

- provider: `openai`
- model: `gpt-5.6-luna`
- base URL: `https://api.openai.com/v1`
- API-key environment name: `OPENAI_API_KEY`
- timeout: `120s`
- stale `OPENAI_KEY`: unset for the run

Root `.env.9profs` was loaded only for missing process variables; process-provided values retained precedence.

## Previous live extraction result

The harness completed with process exit `0`, but every semantic request failed before receiving a model response:

| Page chunk | Provider outputs | Persisted candidates | Result |
|---|---:|---:|---|
| 1–4 | 0 | 0 | transport failure |
| 5–8 | 0 | 0 | transport failure |
| 9–12 | 0 | 0 | transport failure |
| 13–16 | 0 | 0 | transport failure |

Candidates produced: `0` persisted candidates. Provider error surfaced by the existing contract: `Transport`.

That earlier attempt was blocked before model output by runtime transport. No
extraction-quality evidence exists from it.

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

Evidence supports working production normalization and correct shared model configuration resolution. It does not support an extraction-quality or promotion-seam verdict because the earlier four live semantic calls failed at transport before model output, and this continuation smoke call failed authentication before the real dogfood gate.

Continuation evidence supersedes the earlier runtime diagnosis for the next
gate: the shared smoke request reached OpenAI and was rejected with the
`Unauthorized` HTTP status class. The configured `OPENAI_API_KEY` must be repaired or replaced
before the real HIU OCR dogfood can run. This report still contains no OCR
excerpts, credentials, or source document content.
