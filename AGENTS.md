# 9Profs agent governance

These rules apply to coding agents working in this repository.

## Product posture

- 9Profs is a strategic fork of GenOffice, not a rewrite.
- Preserve GenOffice behavior by default.
- Docs/DOCX, Sheets/XLSX, Slides/PPTX, PDF, Markdown, and the shared Electron shell remain part of 9Profs.
- Avoid unrelated refactors, dependency changes, speculative abstractions, and cross-product cleanup.

## Protected boundaries

- `packages/docx-engine` is the protected DOCX persistence boundary. Do not replace or casually modify its parse, patch, generate, types, raw-XML, anchor, or round-trip contracts.
- Tiptap/ProseMirror remains the DOCX editing-state engine. Preserve its schema, commands, transaction behavior, revisions, comments, selections, and nested-editor behavior.
- DOCX presentation changes must not alter save/round-trip semantics: preserve dirty tracking, `pmDocToSavePlan`, `saveDocx`, save ordering, reparse, undo, caret, and OOXML behavior.
- AI must operate through existing editor/document tools and commands. It must not edit presentation DOM or bypass the document model and save pipeline.
- DOCX tasks must not modify Sheets, Slides, PDF, Markdown, or shell behavior unless the task explicitly authorizes that work.

## DOCX presentation work

- Treat `apps/docs/src/renderer/pagination.ts`, `line-metrics.ts`, `doc-style-css.ts`, and presentation-related editor extensions as the likely presentation boundary.
- The first DOCX presentation-v2 work must be a seam/proof behind one feature flag, not a new layout algorithm or an in-place replacement.
- Keep V1 available until V2 passes geometry, position-mapping, save/reopen, and preservation checks.
- Keep OOXML identity (`docxIndex` and raw fragments), editor positions, comments, revisions, and header/footer semantics stable.

## External references

- SuperDoc is an architecture/design reference only by default. Do not copy AGPL source into 9Profs. Any direct code reuse requires separately approved commercial/license review.
- Casual Docs is an architecture and implementation reference. Study, adapt, or port implementation only when the relevant source/license permits it; preserve required copyright notices, attribution, and license obligations.
- Do not blindly copy whole subsystems from Casual Docs. Prefer adapting narrow presentation components to GenOffice contracts.
- GenOffice remains the primary implementation base. Neither Casual Docs nor SuperDoc may replace `packages/docx-engine`, Tiptap/ProseMirror document state, save/round-trip, comments/revisions, or dirty tracking.

## Change discipline

- Read the architecture audit and the relevant local contracts before editing.
- Prefer the narrowest responsible change and reuse existing boundaries.
- State confirmed repository facts separately from inference.
- Do not install or upgrade dependencies unless explicitly authorized.
- Do not change application source code when the task requests documentation, governance, or audit work only.
