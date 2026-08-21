import { expect, test, type Page, type TestInfo } from '@playwright/test'
import { mkdir } from 'node:fs/promises'
import { resolve } from 'node:path'
import {
  compareDiagnosticParity,
  formatDiagnosticDiffs,
  PRESENTATION_GEOMETRY_TOLERANCE_PX,
} from '../apps/docs/src/renderer/presentation-v2/diagnostics'
import {
  closeAndSaveVideo,
  launchShell,
  screenshotPath,
  waitForPageWithUrl,
  type LaunchedApp,
} from './helpers'

type Renderer = 'v1' | 'v2'
type DebugValue = Record<string, unknown>

interface PageDebugSnapshot extends DebugValue {
  renderer?: Renderer
  postRender?: DebugValue
}

interface PresentationFixture {
  id: string
  file: string
  features: string[]
}

interface RendererRun {
  renderer: Renderer
  launched: LaunchedApp
  editorPage: Page
  observations: Record<string, PageDebugSnapshot>
}

const CORPUS = resolve(__dirname, '../apps/docs/tests/pagination-corpus/docx')

const FIXTURES: PresentationFixture[] = [
  {
    id: 'simple-multi-page',
    file: resolve(CORPUS, '01-simple-english.docx'),
    features: ['multi-page prose', 'caret', 'selection'],
  },
  {
    id: 'multi-section-paper',
    file: resolve(CORPUS, '08-multi-section-paper.docx'),
    features: ['section/page-size change', 'page gaps'],
  },
  {
    id: 'kitchen-sink',
    file: resolve(CORPUS, 'kitchen-sink.docx'),
    features: ['headers/footers', 'variants when exposed', 'floats/textboxes', 'comments/revisions'],
  },
  {
    id: 'two-columns',
    file: resolve(CORPUS, '22-two-columns.docx'),
    features: ['columns', 'column-boundary selection'],
  },
  {
    id: 'table-header-repeat',
    file: resolve(CORPUS, '23-tblheader-repeat.docx'),
    features: ['table spanning pages', 'repeated table header'],
  },
  {
    id: 'cjk-doc-grid',
    file: resolve(CORPUS, '02-chinese-long-docgrid.docx'),
    features: ['CJK', 'document grid'],
  },
]

function editorModifier(): 'Control' | 'Meta' {
  return process.platform === 'darwin' ? 'Meta' : 'Control'
}

async function settlePresentation(page: Page): Promise<void> {
  await page.evaluate(
    () =>
      new Promise<void>((resolve) => {
        requestAnimationFrame(() => requestAnimationFrame(() => resolve()))
      }),
  )
}

async function readPageDebug(page: Page): Promise<PageDebugSnapshot> {
  const debug = await page.evaluate(() => {
    const value = (window as unknown as { __pageDebug?: DebugValue }).__pageDebug
    if (!value) return null
    const refresh = value.refreshPostRender
    if (typeof refresh === 'function') refresh()
    return JSON.parse(JSON.stringify(value)) as PageDebugSnapshot
  })
  if (!debug) throw new Error('Docs page did not expose __pageDebug')
  return debug
}

async function waitForPageDebug(page: Page, renderer: Renderer, fixture: string): Promise<void> {
  await expect
    .poll(
      () =>
        page.evaluate(
          ({ expectedRenderer }) => {
            const debug = (window as unknown as { __pageDebug?: DebugValue }).__pageDebug
            const postRender = debug?.postRender as { pages?: unknown[] } | undefined
            return (
              debug?.renderer === expectedRenderer &&
              Array.isArray(postRender?.pages) &&
              postRender.pages.length > 0
            )
          },
          { expectedRenderer: renderer },
        ),
      { message: `fixture=${fixture} renderer=${renderer} post-render diagnostics`, timeout: 45_000 },
    )
    .toBe(true)
}

async function focusEditorAtStart(page: Page): Promise<void> {
  const editor = page.locator('.ProseMirror').first()
  await expect(editor).toBeVisible()
  await editor.click({ position: { x: 24, y: 24 } })
  await page.keyboard.press(`${editorModifier()}+Home`)
  await settlePresentation(page)
}

async function selectWithoutChangingDocument(
  page: Page,
  movement: () => Promise<void>,
): Promise<void> {
  await page.keyboard.down('Shift')
  try {
    await movement()
  } finally {
    await page.keyboard.up('Shift')
  }
  await settlePresentation(page)
}

async function captureObservations(
  page: Page,
  fixture: string,
  renderer: Renderer,
): Promise<Record<string, PageDebugSnapshot>> {
  await waitForPageDebug(page, renderer, fixture)
  const observations: Record<string, PageDebugSnapshot> = {
    loaded: await readPageDebug(page),
  }
  await focusEditorAtStart(page)
  observations.caretBeforeBoundary = await readPageDebug(page)

  await page.keyboard.press(`${editorModifier()}+End`)
  await settlePresentation(page)
  observations.caretAfterBoundary = await readPageDebug(page)

  await focusEditorAtStart(page)
  await selectWithoutChangingDocument(page, async () => {
    for (let index = 0; index < 12; index += 1) await page.keyboard.press('ArrowRight')
  })
  observations.selectionSingleLine = await readPageDebug(page)

  await focusEditorAtStart(page)
  await selectWithoutChangingDocument(page, async () => {
    for (let index = 0; index < 3; index += 1) await page.keyboard.press('ArrowDown')
  })
  observations.selectionMultiLine = await readPageDebug(page)

  await focusEditorAtStart(page)
  await selectWithoutChangingDocument(page, async () => {
    await page.keyboard.press(`${editorModifier()}+End`)
  })
  observations.selectionPageBoundary = await readPageDebug(page)
  return observations
}

async function runRenderer(fixture: PresentationFixture, renderer: Renderer): Promise<RendererRun> {
  const launched = await launchShell({
    onboardingSeen: true,
    openFile: fixture.file,
    presentationRenderer: renderer,
    videoDir: `docs-presentation-${fixture.id}-${renderer}`,
  })
  try {
    const editorPage = await waitForPageWithUrl(launched.app, 'docs/out')
    await expect(editorPage.locator('.ProseMirror').first()).toBeVisible()
    const observations = await captureObservations(editorPage, fixture.id, renderer)
    return { renderer, launched, editorPage, observations }
  } catch (error) {
    await closeAndSaveVideo(launched, `docs-presentation-${fixture.id}-${renderer}`)
    throw error
  }
}

function comparableDebug(debug: PageDebugSnapshot): DebugValue {
  return { postRender: debug.postRender }
}

function parityDifferences(
  fixture: PresentationFixture,
  state: string,
  expected: PageDebugSnapshot,
  actual: PageDebugSnapshot,
) {
  return compareDiagnosticParity(comparableDebug(expected), comparableDebug(actual), {
    fixture: `${fixture.id}/${state}`,
    renderer: 'v2',
    geometryTolerancePx: PRESENTATION_GEOMETRY_TOLERANCE_PX,
    maxDifferences: 200,
  })
}

function assertRealBrowserCoverage(fixture: PresentationFixture, run: RendererRun): void {
  const loaded = run.observations.loaded.postRender as DebugValue | undefined
  const pages = loaded?.pages as DebugValue[] | undefined
  expect(Array.isArray(pages) && pages.length > 0, `fixture=${fixture.id} renderer=${run.renderer} pages`).toBe(true)
  expect(
    pages?.some((page) => page.pageRect && typeof (page.pageRect as DebugValue).top === 'number'),
    `fixture=${fixture.id} renderer=${run.renderer} physical page rectangle`,
  ).toBe(true)
  expect(
    Array.isArray(loaded?.pageGaps),
    `fixture=${fixture.id} renderer=${run.renderer} page-gap diagnostics`,
  ).toBe(true)

  for (const state of ['caretBeforeBoundary', 'caretAfterBoundary']) {
    const caret = run.observations[state].postRender
      ? (run.observations[state].postRender as DebugValue).caret as DebugValue | undefined
      : undefined
    expect(typeof caret?.position, `fixture=${fixture.id} renderer=${run.renderer} ${state} PM position`).toBe(
      'number',
    )
    expect(typeof caret?.page, `fixture=${fixture.id} renderer=${run.renderer} ${state} page`).toBe('number')
    expect(
      typeof (caret?.pageRect as DebugValue | undefined)?.top,
      `fixture=${fixture.id} renderer=${run.renderer} ${state} rendered rectangle`,
    ).toBe('number')
  }

  for (const state of ['selectionSingleLine', 'selectionMultiLine', 'selectionPageBoundary']) {
    const selection = (run.observations[state].postRender as DebugValue | undefined)?.selection as
      | DebugValue
      | undefined
    const pmRange = selection?.pmRange as DebugValue | undefined
    expect(typeof pmRange?.from, `fixture=${fixture.id} renderer=${run.renderer} ${state} PM from`).toBe('number')
    expect(typeof pmRange?.to, `fixture=${fixture.id} renderer=${run.renderer} ${state} PM to`).toBe('number')
  }
}

function unavailableGeometry(debug: PageDebugSnapshot): string[] {
  const postRender = debug.postRender ?? {}
  const unavailable: string[] = []
  const selection = postRender.selection as DebugValue | undefined
  if (!Array.isArray(selection?.rects)) unavailable.push('selection.rects: browser/editor did not expose deterministic client rects')
  unavailable.push('reverse-position-mapping: EditorView hit-testing API is not exposed through __pageDebug')
  const headers = postRender.headerFooters as DebugValue[] | undefined
  if (!headers?.some((item) => item.variant)) {
    unavailable.push('header-footer.variant: current rendered DOM exposed no variant marker')
  }
  if (!headers?.some((item) => item.reservedRect)) {
    unavailable.push('header-footer.reservedRect: current rendered DOM exposed no reserved rectangle')
  }
  return unavailable
}

async function attachUnavailableGeometry(testInfo: TestInfo, fixture: string, values: string[]): Promise<void> {
  const unique = [...new Set(values)]
  if (unique.length === 0) return
  await testInfo.attach(`${fixture}-unavailable-geometry.json`, {
    body: JSON.stringify({ fixture, unavailable: unique }, null, 2),
    contentType: 'application/json',
  })
}

for (const fixture of FIXTURES) {
  test(`DOCX presentation parity: ${fixture.id}`, async ({}, testInfo) => {
    const runs: RendererRun[] = []
    try {
      runs.push(await runRenderer(fixture, 'v1'))
      runs.push(await runRenderer(fixture, 'v2'))

      const expected = runs.find((run) => run.renderer === 'v1')!
      const actual = runs.find((run) => run.renderer === 'v2')!
      assertRealBrowserCoverage(fixture, expected)
      assertRealBrowserCoverage(fixture, actual)
      expect(expected.observations.loaded.renderer, `fixture=${fixture.id} V1 debug renderer`).toBe('v1')
      expect(actual.observations.loaded.renderer, `fixture=${fixture.id} V2 debug renderer`).toBe('v2')

      const unavailable = [
        ...Object.values(expected.observations).flatMap(unavailableGeometry),
        ...Object.values(actual.observations).flatMap(unavailableGeometry),
      ]
      await attachUnavailableGeometry(testInfo, fixture.id, unavailable)

      for (const [state, expectedDebug] of Object.entries(expected.observations)) {
        const actualDebug = actual.observations[state]
        const differences = parityDifferences(fixture, state, expectedDebug, actualDebug)
        if (differences.length === 0) continue

        await mkdir(resolve(__dirname, 'artifacts/screenshots'), { recursive: true })
        await expected.editorPage.screenshot({ path: screenshotPath(`docs-presentation-${fixture.id}-v1`) })
        await actual.editorPage.screenshot({ path: screenshotPath(`docs-presentation-${fixture.id}-v2`) })
        throw new Error(
          [
            `DOCX presentation E2E parity failed: fixture=${fixture.id}`,
            `features=${fixture.features.join(', ')}`,
            `expectedRenderer=v1 actualRenderer=v2 coordinateTolerancePx=${PRESENTATION_GEOMETRY_TOLERANCE_PX}`,
            formatDiagnosticDiffs(differences),
          ].join('\n'),
        )
      }
    } finally {
      for (const run of runs.reverse()) {
        await closeAndSaveVideo(run.launched, `docs-presentation-${fixture.id}-${run.renderer}`)
      }
    }
  })
}
