# HIU RegulationRequirementCandidate live dogfood

status: `EXTRACTION_NEEDS_TARGETED_FIX`
run_date: `2026-08-31`
branch: `develop`
head: `3da4e3b2`
extraction_contract_version: `regulation-requirement-extraction-v0.1`
implementation_version: `model-regulation-requirement-candidate-extractor-v1`

## Smoke test

The continuation smoke test used the same production shared path:
`StructuredModelConfig::from_env()` -> `StructuredModelClient::execute_json()`.
It sent only the synthetic prompt `Return exactly: OK`; no HIU OCR content was
sent during this step.

- provider: `openai`
- model: `gpt-5.6-luna`
- base URL: `https://api.openai.com/v1`
- endpoint: `https://api.openai.com/v1/chat/completions`
- credential environment name: `OPENAI_API_KEY`
- stale `OPENAI_KEY`: unset in child process
- OpenAI reached: yes
- HTTP result: success (`200`)
- shared-client error class: none
- model response received: yes

The first compatible-request diagnosis found two concrete endpoint defects in
the regulation OpenAI payload: `max_tokens` was rejected in favor of
`max_completion_tokens`, and `temperature: 0` was rejected because this model
only supports its default temperature. The production request was changed only
to use the supported completion-token field and omit temperature. The
regulation extraction budget was raised from `1024` to `8192` because the model
was using the smaller budget for reasoning and returned no content.

These changes preserve the extraction contract and validation rules. No
alternate client or configuration path was introduced.

## Authorized live extraction

The real dogfood used the existing production path:

`OpenDataLoader JSON` -> `normalize_json` -> `ResearchPdfExtraction` ->
`RegulationRequirementCandidate` extractor -> shared structured-model client ->
production validation -> candidate persistence.

All four semantic requests reached OpenAI and received model responses. The
runtime transport/authentication blocker is therefore resolved. The run did not
fail at transport, response parsing, or structured-output decoding; it failed
at strict production validation for model-produced candidates.

| Page chunk | Provider outputs | Result |
|---|---:|---|
| 1-4 | 22 | stopped on non-exact OCR excerpt validation |
| 5-8 | 8 | stopped on unsupported applicability value `reporting_guidelines` |
| 9-12 | 19 | stopped on empty authority-locator article |
| 13-16 | 4 | stopped on non-exact OCR excerpt validation |
| **Total** | **53** | **18 candidates persisted before chunk aborts** |

The persisted total is lower than the provider total because the existing
service persists valid candidates sequentially and stops a chunk at its first
invalid candidate. No validation was relaxed and no candidate was
hand-constructed.

Observed aggregate validation evidence over all 53 provider outputs:

- independently valid under the unchanged service rules: `26`
- exact OCR excerpt: `50/53`
- non-empty normalized requirement: `53/53`
- valid applicability facets: `47/53`
- valid source locators: `53/53`
- valid authority locators: `34/53`
- non-exact OCR excerpt violations: `3`
- unsupported applicability values: `6`
- invalid authority locators with an empty article: `19`
- model responses received: `4/4`
- transport failures after the compatibility fix: `0`

The model also emitted OCR-risk annotations. Across provider outputs these
included OCR damage `9`, missing text `6`, word-order damage `10`, spelling
damage `5`, and OCR noise `18` flag occurrences. These are evidence for human
review, not reasons to weaken exact grounding.

## Input and normalization

- OpenDataLoader JSON: `data/regulation-dogfood/_opendataloader-spike-20260830/outputs/vi-en-full-java21-cache-net/hiu-master-thesis-format.json`
- production `normalize_json`: used successfully
- normalized pages: `16`
- non-empty pages: `16`
- normalized UTF-8 bytes: `29921`
- extraction status: `Ready`
- extraction contract: `regulation-requirement-extraction-v0.1`
- provider/model/base URL: `openai` / `gpt-5.6-luna` / `https://api.openai.com/v1`

The report intentionally does not contain OCR excerpts, candidate text,
credentials, or the source PDF.

## Benchmark against 24 manual requirements

The workspace contains the stated manual benchmark total but no row-level
benchmark fixture or authoritative regulation benchmark table. The previous
report recorded only the total and marked all rows blocked. Consequently, this
run records each row as not evaluated rather than inferring matches from the
53 provider outputs or 18 persisted candidates.

| Manual item | Comparison |
|---:|---|
| 01 | NOT_EVALUATED: row-level manual record unavailable |
| 02 | NOT_EVALUATED: row-level manual record unavailable |
| 03 | NOT_EVALUATED: row-level manual record unavailable |
| 04 | NOT_EVALUATED: row-level manual record unavailable |
| 05 | NOT_EVALUATED: row-level manual record unavailable |
| 06 | NOT_EVALUATED: row-level manual record unavailable |
| 07 | NOT_EVALUATED: row-level manual record unavailable |
| 08 | NOT_EVALUATED: row-level manual record unavailable |
| 09 | NOT_EVALUATED: row-level manual record unavailable |
| 10 | NOT_EVALUATED: row-level manual record unavailable |
| 11 | NOT_EVALUATED: row-level manual record unavailable |
| 12 | NOT_EVALUATED: row-level manual record unavailable |
| 13 | NOT_EVALUATED: row-level manual record unavailable |
| 14 | NOT_EVALUATED: row-level manual record unavailable |
| 15 | NOT_EVALUATED: row-level manual record unavailable |
| 16 | NOT_EVALUATED: row-level manual record unavailable |
| 17 | NOT_EVALUATED: row-level manual record unavailable |
| 18 | NOT_EVALUATED: row-level manual record unavailable |
| 19 | NOT_EVALUATED: row-level manual record unavailable |
| 20 | NOT_EVALUATED: row-level manual record unavailable |
| 21 | NOT_EVALUATED: row-level manual record unavailable |
| 22 | NOT_EVALUATED: row-level manual record unavailable |
| 23 | NOT_EVALUATED: row-level manual record unavailable |
| 24 | NOT_EVALUATED: row-level manual record unavailable |

| Measurement | Result |
|---|---:|
| Manual benchmark total | 24 |
| Candidates produced by provider | 53 |
| Candidates persisted | 18 |
| Correctly found | NOT_MEASURABLE |
| Partially found | NOT_MEASURABLE |
| Missed: source/OCR unavailable or damaged | NOT_MEASURABLE |
| Missed despite usable OCR | NOT_MEASURABLE |
| False positives | NOT_MEASURABLE |
| Modality errors | NOT_MEASURABLE |
| Negation errors | NOT_MEASURABLE |
| Numeric hallucinations/reconstructed missing numbers | NOT_MEASURABLE |
| Applicability errors | 6 provider outputs had unsupported applicability values; benchmark classification not measured |
| Material duplicate/over-split candidates | NOT_MEASURABLE |
| Materially under-split candidates | NOT_MEASURABLE |

## Human review operations and promotion seam

Exact operation counts are not measurable without the row-level manual
benchmark and a completed validated candidate batch. The actual run does show
the following minimum review needs:

- source-transcription edit or source-page inspection for at least `3` outputs
  whose OCR excerpt was not an exact supplied-page substring;
- applicability correction or discard for `6` outputs containing unsupported
  applicability values;
- locator correction or discard for `19` outputs with an empty authority
  article;
- semantic review of the `18` persisted candidates, including the emitted OCR
  risk flags;
- approve-as-is, normalized-requirement edits, merge, split, and final discard
  rates: `NOT_MEASURABLE` in this run.

The smallest promotion seam evidenced by this dogfood is a human
approve/edit/discard gate over candidates that have already passed the current
production validation, with edits available for source transcription,
normalized requirement, locator, and applicability. Original-page inspection
is needed for OCR-risk cases. This run cannot establish whether merge or split
is needed at promotion because the 24-item comparison was not evaluable.
The promotion seam is not implemented here.

## Failure boundary and verdict

The prior runtime failure had a concrete production request-compatibility
boundary. The shared OpenAI endpoint rejected the regulation payload's
`max_tokens` field and non-default temperature, and the initial small output
budget was exhausted by model reasoning. The narrow production fix and
regression tests now make the same shared transport usable.

The remaining boundary is model-output adherence to the unchanged strict
contract: exact OCR grounding, canonical applicability values, and valid
authority locators. This is extraction-quality evidence, not a runtime blocker
and not permission to relax validation.

Final verdict: `EXTRACTION_NEEDS_TARGETED_FIX`
