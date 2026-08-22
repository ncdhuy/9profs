import { expect, test, type Page, type TestInfo } from '@playwright/test'
import { mkdir } from 'node:fs/promises'
import { resolve } from 'node:path'
import {
  compareDiagnosticParity,
  formatDiagnosticDiffs,
  PRESENTATION_GEOMETRY_TOLERANCE_PX,
} from '../apps/docs/src/renderer/presentation-v2/diagnostics'
import {
  geometryProbeDiagnostics,
  type GeometryProbe as GeometryProbeSpec,
  type GeometryProbeResult,
} from '../apps/docs/src/renderer/presentation-v2/geometry-probes'
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
  probes?: DebugValue[]
}

interface PresentationFixture {
  id: string
  file: string
  features: string[]
  expectedPageCount?: number
}

interface RendererRun {
  renderer: Renderer
  launched: LaunchedApp
  editorPage: Page
  observations: Record<string, PageDebugSnapshot>
}

const CORPUS = resolve(__dirname, '../apps/docs/tests/pagination-corpus/docx')

const MIXED_CONTENT_FIXTURE: PresentationFixture = {
  id: 'mixed-content-ordering',
  file: resolve(CORPUS, '15-mixed-content.docx'),
  features: ['CJK', 'document grid', 'headings', 'tables'],
  expectedPageCount: 4,
}

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
    features: [
      'headers/footers',
      'variants when exposed',
      'floats/textboxes',
      'comments/revisions',
    ],
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
  {
    id: 'headings-keepnext-doc-grid',
    file: resolve(CORPUS, '04-headings-keepnext.docx'),
    features: ['CJK', 'document grid', 'keepNext'],
    expectedPageCount: 4,
  },
]

const PROBES_BY_FIXTURE: Record<string, GeometryProbeSpec[]> = {
  'simple-multi-page': [
    {
      id: 'paragraph-middle',
      fixtureId: 'simple-multi-page',
      semanticCase: 'paragraph-middle',
      anchor: { kind: 'node', nodeType: 'docParagraph', occurrence: 0, offset: 'middle' },
      expected: { nodeType: 'docParagraph' },
    },
    {
      id: 'line-start',
      fixtureId: 'simple-multi-page',
      semanticCase: 'line-start',
      anchor: { kind: 'node', nodeType: 'docParagraph', occurrence: 0, offset: 'start' },
      expected: { nodeType: 'docParagraph' },
    },
    {
      id: 'line-end',
      fixtureId: 'simple-multi-page',
      semanticCase: 'line-end',
      anchor: { kind: 'node', nodeType: 'docParagraph', occurrence: 0, offset: 'end' },
      expected: { nodeType: 'docParagraph' },
    },
    {
      id: 'page-before-gap',
      fixtureId: 'simple-multi-page',
      semanticCase: 'page-before-gap',
      anchor: { kind: 'page-boundary', pageIndex: 1, side: 'before-gap' },
      expected: { pageIndex: 0 },
    },
    {
      id: 'page-after-gap',
      fixtureId: 'simple-multi-page',
      semanticCase: 'page-after-gap',
      anchor: { kind: 'page-boundary', pageIndex: 1, side: 'after-gap' },
      expected: { pageIndex: 1 },
    },
  ],
  'two-columns': [
    {
      id: 'column-1',
      fixtureId: 'two-columns',
      semanticCase: 'column-1',
      anchor: { kind: 'column', columnIndex: 0, side: 'first' },
      expected: { columnIndex: 0 },
    },
    {
      id: 'column-2',
      fixtureId: 'two-columns',
      semanticCase: 'column-2',
      anchor: { kind: 'column', columnIndex: 1, side: 'first' },
      expected: { columnIndex: 1 },
    },
    {
      id: 'column-transition',
      fixtureId: 'two-columns',
      semanticCase: 'column-transition',
      anchor: { kind: 'column-transition', fromColumn: 0, toColumn: 1 },
    },
  ],
  'table-header-repeat': [
    {
      id: 'table-cell',
      fixtureId: 'table-header-repeat',
      semanticCase: 'table-cell',
      anchor: { kind: 'table-cell', tableOccurrence: 0, row: 0, cell: 0, offset: 'middle' },
      expected: { nodeType: 'docTable', table: { row: 0, cell: 0 } },
    },
    {
      id: 'table-row-boundary',
      fixtureId: 'table-header-repeat',
      semanticCase: 'table-row-boundary',
      anchor: { kind: 'table-row-boundary', tableOccurrence: 0, row: 'last', offset: 'middle' },
      optional: true,
    },
    {
      id: 'repeated-header',
      fixtureId: 'table-header-repeat',
      semanticCase: 'repeated-header',
      anchor: { kind: 'node', nodeType: 'docTableHeader', occurrence: 0, offset: 'middle' },
      optional: true,
    },
  ],
  'cjk-doc-grid': [
    {
      id: 'cjk-run',
      fixtureId: 'cjk-doc-grid',
      semanticCase: 'cjk-run',
      anchor: { kind: 'node', nodeType: 'docParagraph', occurrence: 0, offset: 'middle' },
      expected: { textScript: 'cjk' },
    },
  ],
  'kitchen-sink': [
    {
      id: 'floating-object-anchor',
      fixtureId: 'kitchen-sink',
      semanticCase: 'floating-object-anchor',
      anchor: { kind: 'node', nodeType: 'docProtected', occurrence: 0, offset: 'start' },
      optional: true,
    },
    {
      id: 'textbox',
      fixtureId: 'kitchen-sink',
      semanticCase: 'textbox',
      anchor: { kind: 'node', nodeType: 'docProtected', occurrence: 1, offset: 'start' },
      optional: true,
    },
    {
      id: 'header',
      fixtureId: 'kitchen-sink',
      semanticCase: 'header',
      anchor: { kind: 'header-footer', part: 'header' },
      optional: true,
    },
    {
      id: 'footer',
      fixtureId: 'kitchen-sink',
      semanticCase: 'footer',
      anchor: { kind: 'header-footer', part: 'footer' },
      optional: true,
    },
    {
      id: 'comment-range',
      fixtureId: 'kitchen-sink',
      semanticCase: 'comment-range',
      anchor: { kind: 'mark-range', markType: 'comment', occurrence: 0 },
      expected: { markType: 'comment' },
      optional: true,
    },
    {
      id: 'revision-range',
      fixtureId: 'kitchen-sink',
      semanticCase: 'revision-range',
      anchor: { kind: 'mark-range', markType: 'ins', occurrence: 0 },
      expected: { markType: 'ins' },
      optional: true,
    },
  ],
}

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

async function readPageDebug(
  page: Page,
  probes: readonly GeometryProbeSpec[] = [],
): Promise<PageDebugSnapshot> {
  const debug = await page.evaluate((probeSpecs) => {
    const value = (window as unknown as { __pageDebug?: DebugValue }).__pageDebug
    if (!value) return null
    const refresh = value.refreshPostRender
    if (typeof refresh === 'function') refresh()
    const snapshot = JSON.parse(JSON.stringify(value)) as PageDebugSnapshot
    const probeGeometry = value.probeGeometry
    if (typeof probeGeometry === 'function')
      snapshot.probes = probeGeometry(probeSpecs) as DebugValue[]
    return snapshot
  }, probes)
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
      {
        message: `fixture=${fixture} renderer=${renderer} post-render diagnostics`,
        timeout: 45_000,
      },
    )
    .toBe(true)
}

async function focusEditorAtStart(page: Page, fixture: string): Promise<void> {
  const editor = page.locator('.ProseMirror').first()
  await expect(editor).toBeVisible()
  if (fixture === 'table-header-repeat') await editor.click({ position: { x: 24, y: 24 } })
  else await editor.focus()
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
  const probes = PROBES_BY_FIXTURE[fixture] ?? []
  const observations: Record<string, PageDebugSnapshot> = {
    loaded: await readPageDebug(page, probes),
  }
  await focusEditorAtStart(page, fixture)
  observations.caretBeforeBoundary = await readPageDebug(page, probes)

  await page.keyboard.press(`${editorModifier()}+End`)
  await settlePresentation(page)
  observations.caretAfterBoundary = await readPageDebug(page, probes)

  await focusEditorAtStart(page, fixture)
  await selectWithoutChangingDocument(page, async () => {
    for (let index = 0; index < 12; index += 1) await page.keyboard.press('ArrowRight')
  })
  observations.selectionSingleLine = await readPageDebug(page, probes)

  await focusEditorAtStart(page, fixture)
  await selectWithoutChangingDocument(page, async () => {
    await page.keyboard.press('ArrowDown')
  })
  observations.selectionMultiLine = await readPageDebug(page, probes)

  await focusEditorAtStart(page, fixture)
  await selectWithoutChangingDocument(page, async () => {
    await page.keyboard.press(`${editorModifier()}+End`)
  })
  observations.selectionPageBoundary = await readPageDebug(page, probes)
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
  return { postRender: debug.postRender, probes: debug.probes }
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
  expect(
    Array.isArray(pages) && pages.length > 0,
    `fixture=${fixture.id} renderer=${run.renderer} pages`,
  ).toBe(true)
  expect(
    pages?.some((page) => page.pageRect && typeof (page.pageRect as DebugValue).top === 'number'),
    `fixture=${fixture.id} renderer=${run.renderer} physical page rectangle`,
  ).toBe(true)

  const probes = (run.observations.loaded.probes ?? []) as unknown as GeometryProbeResult[]
  const expectedProbes = PROBES_BY_FIXTURE[fixture.id] ?? []
  expect(
    probes.length,
    `fixture=${fixture.id} renderer=${run.renderer} deterministic probe count`,
  ).toBe(expectedProbes.length)
  const failures = geometryProbeDiagnostics(probes)
  expect(
    failures,
    `fixture=${fixture.id} renderer=${run.renderer} deterministic geometry probes\n${formatDiagnosticDiffs(failures)}`,
  ).toEqual([])
  for (const probe of expectedProbes.filter((item) => !item.optional)) {
    const result = probes.find((item) => item.probe.id === probe.id)
    expect(
      result?.status,
      `fixture=${fixture.id} renderer=${run.renderer} probe=${probe.id} status`,
    ).toBe('resolved')
    expect(
      result?.roundTrip?.status,
      `fixture=${fixture.id} renderer=${run.renderer} probe=${probe.id} point-to-position round trip`,
    ).toMatch(/^(exact|boundary-ambiguous)$/)
  }
  expect(
    Array.isArray(loaded?.pageGaps),
    `fixture=${fixture.id} renderer=${run.renderer} page-gap diagnostics`,
  ).toBe(true)
  const geometry = loaded?.geometry as DebugValue | undefined
  const geometryPages = geometry?.pages as unknown[] | undefined
  expect(
    geometry?.coordinateSpaces,
    `fixture=${fixture.id} renderer=${run.renderer} geometry coordinate spaces`,
  ).toBeTruthy()
  expect(
    Array.isArray(geometryPages) && geometryPages.length === pages?.length,
    `fixture=${fixture.id} renderer=${run.renderer} normalized page geometry`,
  ).toBe(true)

  for (const state of ['caretBeforeBoundary', 'caretAfterBoundary']) {
    const caret = run.observations[state].postRender
      ? ((run.observations[state].postRender as DebugValue).caret as DebugValue | undefined)
      : undefined
    expect(
      typeof caret?.position,
      `fixture=${fixture.id} renderer=${run.renderer} ${state} PM position`,
    ).toBe('number')
    expect(typeof caret?.page, `fixture=${fixture.id} renderer=${run.renderer} ${state} page`).toBe(
      'number',
    )
    expect(
      typeof (caret?.pageRect as DebugValue | undefined)?.top,
      `fixture=${fixture.id} renderer=${run.renderer} ${state} rendered rectangle`,
    ).toBe('number')
    const positions = (run.observations[state].postRender as DebugValue | undefined)?.geometry as
      DebugValue | undefined
    const positionGeometry = (positions?.positions as DebugValue[] | undefined)?.[0]
    expect(
      positionGeometry?.status,
      `fixture=${fixture.id} renderer=${run.renderer} ${state} position mapping`,
    ).toBe('resolved')
  }

  for (const state of ['selectionSingleLine', 'selectionMultiLine', 'selectionPageBoundary']) {
    const selection = (run.observations[state].postRender as DebugValue | undefined)?.selection as
      DebugValue | undefined
    const pmRange = selection?.pmRange as DebugValue | undefined
    expect(
      typeof pmRange?.from,
      `fixture=${fixture.id} renderer=${run.renderer} ${state} PM from`,
    ).toBe('number')
    expect(
      typeof pmRange?.to,
      `fixture=${fixture.id} renderer=${run.renderer} ${state} PM to`,
    ).toBe('number')
    const geometry = (run.observations[state].postRender as DebugValue | undefined)?.geometry as
      DebugValue | undefined
    const selectionGeometry = (geometry?.selections as DebugValue[] | undefined)?.[0]
    expect(selectionGeometry?.from).toBe(pmRange?.from)
    expect(selectionGeometry?.to).toBe(pmRange?.to)
    if (selectionGeometry?.status === 'resolved') {
      expect(Array.isArray(selectionGeometry.rects)).toBe(true)
      expect(Array.isArray(selectionGeometry.pages)).toBe(true)
    }
  }
}

function unavailableGeometry(debug: PageDebugSnapshot): string[] {
  const postRender = debug.postRender ?? {}
  const unavailable: string[] = []
  const selection = postRender.selection as DebugValue | undefined
  if (!Array.isArray(selection?.rects))
    unavailable.push('selection.rects: browser/editor did not expose deterministic client rects')
  const geometry = postRender.geometry as DebugValue | undefined
  if (!Array.isArray(geometry?.hitTests)) {
    unavailable.push(
      'reverse-position-mapping: browser/editor did not expose deterministic hit-test results',
    )
  }
  const headers = postRender.headerFooters as DebugValue[] | undefined
  if (!headers?.some((item) => item.variant)) {
    unavailable.push('header-footer.variant: current rendered DOM exposed no variant marker')
  }
  if (!headers?.some((item) => item.reservedRect)) {
    unavailable.push(
      'header-footer.reservedRect: current rendered DOM exposed no reserved rectangle',
    )
  }
  for (const probe of debug.probes ?? []) {
    if (probe.status === 'unavailable' || probe.mappingStatus !== 'resolved')
      unavailable.push(
        `probe=${String((probe.probe as DebugValue | undefined)?.id)} case=${String((probe.probe as DebugValue | undefined)?.semanticCase)}: ${String(probe.reason ?? probe.mappingStatus ?? 'unavailable')}`,
      )
  }
  return unavailable
}

async function attachUnavailableGeometry(
  testInfo: TestInfo,
  fixture: string,
  values: string[],
): Promise<void> {
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
      expect(expected.observations.loaded.renderer, `fixture=${fixture.id} V1 debug renderer`).toBe(
        'v1',
      )
      expect(actual.observations.loaded.renderer, `fixture=${fixture.id} V2 debug renderer`).toBe(
        'v2',
      )
      if (fixture.expectedPageCount !== undefined) {
        for (const run of [expected, actual]) {
          const pages = (run.observations.loaded.postRender as { pages?: unknown[] } | undefined)
            ?.pages
          expect(pages?.length, `fixture=${fixture.id} renderer=${run.renderer} page count`).toBe(
            fixture.expectedPageCount,
          )
        }
      }

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
        await expected.editorPage.screenshot({
          path: screenshotPath(`docs-presentation-${fixture.id}-v1`),
        })
        await actual.editorPage.screenshot({
          path: screenshotPath(`docs-presentation-${fixture.id}-v2`),
        })
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

test('DOCX fidelity: mixed-content page starts at the following section heading', async () => {
  const launched = await launchShell({
    onboardingSeen: true,
    openFile: MIXED_CONTENT_FIXTURE.file,
    presentationRenderer: 'v1',
    videoDir: `docs-fidelity-${MIXED_CONTENT_FIXTURE.id}`,
  })
  try {
    const page = await waitForPageWithUrl(launched.app, 'docs/out')
    await expect(page.locator('.ProseMirror').first()).toBeVisible()
    await waitForPageDebug(page, 'v1', MIXED_CONTENT_FIXTURE.id)
    const debug = await readPageDebug(page)
    const slices = (debug.slices as DebugValue[] | undefined) ?? []
    const blocks = (debug.blocks as DebugValue[] | undefined) ?? []
    const pageStarts = slices.map((slice) => {
      const start = typeof slice.start === 'number' ? slice.start : NaN
      return blocks.find((block) => {
        const top = typeof block.top === 'number' ? block.top : NaN
        return Number.isFinite(start) && Number.isFinite(top) && Math.abs(top - start) < 0.5
      })?.docxIndex
    })
    expect(pageStarts.length, 'mixed-content page count').toBe(4)
    expect(pageStarts.slice(0, 2), 'mixed-content first page-start anchors').toEqual([0, 19])
  } finally {
    await closeAndSaveVideo(launched, `docs-fidelity-${MIXED_CONTENT_FIXTURE.id}`)
  }
})
