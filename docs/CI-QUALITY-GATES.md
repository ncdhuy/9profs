# 9Profs CI / quality gates

Normal CI runs on pushes to `develop`, `main`, `dev_*`, and `release_*`, and on pull requests targeting `develop` or `main`. It also supports `workflow_dispatch`.

Branch protection should require the single check `9Profs Required`. That check passes only when every mandatory dependency succeeds:

- `9Profs Rust workspace`: `cargo fmt --manifest-path 9profs-core-rs/Cargo.toml --all -- --check`, locked workspace `cargo check --workspace --all-targets`, and locked workspace `cargo test --workspace`.
- `TypeScript static correctness`: `npm ci`, repository license/format/theme/comment checks, and `npm run typecheck`.
- `9Profs TypeScript tests`: package suites for `@genoffice/9profs-core`, `@genoffice/document-gateway`, `@genoffice/genoffice-adapter`, and `@genoffice/officecli-adapter`.
- `Docs critical tests`: `npm run test -w @genoffice/docs -- --testNamePattern "^(?!.*setting a modify password produces verifiable writeProtection credentials)"`.
- `Docs production build`: `npm run build -w @genoffice/docs`.

## Explicit baseline separation

The exact inherited test `ProtectDialog > setting a modify password produces verifiable writeProtection credentials` in `apps/docs/tests/protect-dialog.test.ts` is the only test excluded from the required Docs command. It failed twice in full-suite runs at the clean R2A starting commit, while isolated file runs passed. The file is unchanged from the GenOffice baseline; the failure is retained as a visible manual diagnostic in `upstream-baseline.yml`.

The manual diagnostic workflow also retains these current inherited/environment-sensitive package commands:

- `npm run test -w @genoffice/electron-utils`: Windows path normalization and read-only-directory assumptions currently fail on the clean Windows baseline.
- `npm run test -w @genoffice/ai-search`: the auth-file mode assertion currently observes Windows mode `438` instead of expected `384`.
- `npm run test -w @genoffice/slides`: current Slides QC/browser-storage setup failures remain diagnostic.
- `npm run test -w @genoffice/pdf`: `tests/generated-output.test.ts > uniqueGeneratedPdfPath > sanitizes characters that are invalid in file names` currently fails on the Windows baseline.
- `npm run test -w @genoffice/markdown`: `tests/teardown.test.ts > AiPanel teardown > cancels an in-flight IPC stream when the panel/tab unmounts` currently fails when `localStorage` is unavailable.
- `npm run lint`: current clean-baseline ESLint errors remain visible without changing product source in R2A.

These diagnostics are not dependencies of `9Profs Required`. They are not converted to success with retries, `continue-on-error`, or `|| true`.

## Manual qualification prerequisites

Normal CI has no live credentials or external OfficeCLI binary.

- OfficeCLI qualification requires an actual pinned OfficeCLI binary at `NINEPROFS_OFFICECLI_PATH`, plus the existing Electron runtime and rasterizer. Run `npm run test:officecli:qualification` or `npm run test:officecli:mutation-qualification` explicitly.
- Dify live qualification requires `NINEPROFS_DIFY_TEST_BASE_URL` and `NINEPROFS_DIFY_TEST_API_KEY`. Unit and contract tests use mocks/isolated fixtures and remain in required Rust tests.
- Live model qualification requires provider credentials configured by the selected structured-model configuration. Deterministic assessor, claim-extractor, and transport tests remain in the required Rust workspace suite.
