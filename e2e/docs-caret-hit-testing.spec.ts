import { expect, test, type Page } from '@playwright/test'
import { resolve } from 'node:path'
import { closeAndSaveVideo, launchShell, waitForPageWithUrl, type LaunchedApp } from './helpers'
import type { GeometryProbe, GeometryProbeResult } from '../apps/docs/src/renderer/presentation-v2'

type Renderer = 'v1' | 'v2'

const CORPUS = resolve(__dirname, '../apps/docs/tests/pagination-corpus/docx')

const PROBES: Array<{ fixture: string; file: string; renderer: Renderer; probe: GeometryProbe }> = [
  {
    fixture: 'simple-multi-page',
    file: resolve(CORPUS, '01-simple-english.docx'),
    renderer: 'v2',
    probe: {
      id: 'wrapped-line-middle',
      fixtureId: 'simple-multi-page',
      semanticCase: 'paragraph-middle',
      anchor: { kind: 'node', nodeType: 'docParagraph', occurrence: 0, offset: 'middle' },
    },
  },
  {
    fixture: 'simple-multi-page',
    file: resolve(CORPUS, '01-simple-english.docx'),
    renderer: 'v2',
    probe: {
      id: 'wrapped-line-start',
      fixtureId: 'simple-multi-page',
      semanticCase: 'line-start',
      anchor: { kind: 'node', nodeType: 'docParagraph', occurrence: 0, offset: 'start' },
    },
  },
  {
    fixture: 'simple-multi-page',
    file: resolve(CORPUS, '01-simple-english.docx'),
    renderer: 'v2',
    probe: {
      id: 'line-end',
      fixtureId: 'simple-multi-page',
      semanticCase: 'line-end',
      anchor: { kind: 'node', nodeType: 'docParagraph', occurrence: 0, offset: 'end' },
    },
  },
  {
    fixture: 'two-columns',
    file: resolve(CORPUS, '22-two-columns.docx'),
    renderer: 'v2',
    probe: {
      id: 'column-1',
      fixtureId: 'two-columns',
      semanticCase: 'column-1',
      anchor: { kind: 'column', columnIndex: 0, side: 'first' },
    },
  },
  {
    fixture: 'two-columns',
    file: resolve(CORPUS, '22-two-columns.docx'),
    renderer: 'v2',
    probe: {
      id: 'column-2',
      fixtureId: 'two-columns',
      semanticCase: 'column-2',
      anchor: { kind: 'column', columnIndex: 1, side: 'first' },
    },
  },
  {
    fixture: 'two-columns',
    file: resolve(CORPUS, '22-two-columns.docx'),
    renderer: 'v2',
    probe: {
      id: 'column-transition',
      fixtureId: 'two-columns',
      semanticCase: 'column-transition',
      anchor: { kind: 'column-transition', fromColumn: 0, toColumn: 1 },
    },
  },
  {
    fixture: 'table-header-repeat',
    file: resolve(CORPUS, '23-tblheader-repeat.docx'),
    renderer: 'v2',
    probe: {
      id: 'table-cell',
      fixtureId: 'table-header-repeat',
      semanticCase: 'table-cell',
      anchor: { kind: 'table-cell', tableOccurrence: 0, row: 0, cell: 0, offset: 'middle' },
    },
  },
  {
    fixture: 'cjk-doc-grid',
    file: resolve(CORPUS, '02-chinese-long-docgrid.docx'),
    renderer: 'v2',
    probe: {
      id: 'cjk-run',
      fixtureId: 'cjk-doc-grid',
      semanticCase: 'cjk-run',
      anchor: { kind: 'node', nodeType: 'docParagraph', occurrence: 0, offset: 'middle' },
      expected: { textScript: 'cjk' },
    },
  },
]

async function settle(page: Page): Promise<void> {
  await page.evaluate(
    () =>
      new Promise<void>((resolve) => {
        requestAnimationFrame(() => requestAnimationFrame(() => resolve()))
      }),
  )
}

async function waitForGeometry(page: Page, renderer: Renderer): Promise<void> {
  await expect
    .poll(
      () =>
        page.evaluate(
          ({ expectedRenderer }) => {
            const debug = (window as unknown as { __pageDebug?: Record<string, unknown> })
              .__pageDebug
            const postRender = debug?.postRender as { pages?: unknown[] } | undefined
            return debug?.renderer === expectedRenderer && (postRender?.pages?.length ?? 0) > 0
          },
          { expectedRenderer: renderer },
        ),
      { timeout: 45_000 },
    )
    .toBe(true)
}

async function probe(page: Page, spec: GeometryProbe): Promise<GeometryProbeResult> {
  const result = await page.evaluate((probeSpec) => {
    const debug = (window as unknown as { __pageDebug?: Record<string, unknown> }).__pageDebug
    const capture = debug?.probeGeometry
    if (typeof capture !== 'function') throw new Error('Docs page did not expose probeGeometry')
    return (capture as (probes: GeometryProbe[]) => GeometryProbeResult[])([probeSpec])[0]
  }, spec)
  if (!result) throw new Error(`Missing geometry probe result: ${spec.id}`)
  return result
}

async function selection(page: Page): Promise<{ from: number; to: number }> {
  return page.evaluate(() => {
    const aidocs = (
      window as unknown as {
        __aidocs?: { editor?: { state?: { selection?: { from: number; to: number } } } }
      }
    ).__aidocs
    const current = aidocs?.editor?.state?.selection
    if (!current) throw new Error('Docs page did not expose editor selection')
    return { from: current.from, to: current.to }
  })
}

async function clickProbe(page: Page, spec: GeometryProbe): Promise<void> {
  let result = await probe(page, spec)
  expect(result.status, `probe=${spec.id} status`).toBe('resolved')
  expect(result.mappingStatus, `probe=${spec.id} mapping`).toMatch(
    /^(resolved|boundary-ambiguous)$/,
  )
  const expectedPosition = result.pmPosition ?? result.pmRange?.from
  expect(expectedPosition, `probe=${spec.id} PM anchor`).toEqual(expect.any(Number))
  expect(result.stablePoint, `probe=${spec.id} click point`).toBeTruthy()
  if (spec.id === 'wrapped-line-middle') {
    const lineRects = await page
      .locator('.ProseMirror p')
      .first()
      .evaluate((paragraph) => {
        const range = document.createRange()
        range.selectNodeContents(paragraph)
        return [...range.getClientRects()].filter((rect) => rect.width > 0 && rect.height > 0)
          .length
      })
    expect(
      lineRects,
      'wrapped-line-middle fixture has multiple rendered line boxes',
    ).toBeGreaterThan(1)
  }

  await page.evaluate((pmPosition) => {
    const aidocs = (
      window as unknown as {
        __aidocs?: {
          editor?: {
            view?: {
              domAtPos: (position: number, side?: number) => { node: Node }
            }
          }
        }
      }
    ).__aidocs
    const location = aidocs?.editor?.view?.domAtPos(pmPosition, 1)
    const node = location?.node
    const element =
      node?.nodeType === Node.ELEMENT_NODE
        ? (node as HTMLElement)
        : (node?.parentElement as HTMLElement | null)
    element?.scrollIntoView({ block: 'center', inline: 'nearest' })
  }, expectedPosition!)
  await settle(page)
  result = await probe(page, spec)

  const point = result.stablePoint!
  const clickPoint = await page.evaluate((geometryPoint) => {
    if (geometryPoint.space === 'viewport') return { x: geometryPoint.x, y: geometryPoint.y }
    const root = document.querySelector('.page-wrap')
    if (!root) throw new Error('Docs page did not expose page-wrap')
    const rect = root.getBoundingClientRect()
    return { x: rect.left + geometryPoint.x, y: rect.top + geometryPoint.y }
  }, point)
  await page.mouse.click(clickPoint.x, clickPoint.y)
  await settle(page)

  const actual = await selection(page)
  const boundaryAmbiguous =
    spec.semanticCase === 'line-start' ||
    spec.semanticCase === 'line-end' ||
    spec.semanticCase === 'column-transition'
  const delta = Math.abs(actual.from - expectedPosition!)
  expect(
    delta,
    `probe=${spec.id} production click selection expected=${expectedPosition} actual=${actual.from} point=${JSON.stringify(point)} geometry=${JSON.stringify(result.positionGeometry?.documentRect)}`,
  ).toBeLessThanOrEqual(boundaryAmbiguous ? 1 : 0)
  expect(actual.to, `probe=${spec.id} caret remains empty`).toBe(actual.from)
}

test('DOCX caret hit testing uses geometry for realistic V2 clicks', async () => {
  const launchedByFixture = new Map<string, LaunchedApp>()
  try {
    for (const fixture of [...new Set(PROBES.map((item) => item.fixture))]) {
      const scenario = PROBES.find((item) => item.fixture === fixture)!
      const launched = await launchShell({
        onboardingSeen: true,
        openFile: scenario.file,
        presentationRenderer: scenario.renderer,
        videoDir: `docs-caret-hit-testing-${fixture}`,
      })
      launchedByFixture.set(fixture, launched)
      const page = await waitForPageWithUrl(launched.app, 'docs/out')
      await expect(page.locator('.ProseMirror').first()).toBeVisible()
      await waitForGeometry(page, scenario.renderer)

      for (const item of PROBES.filter((candidate) => candidate.fixture === fixture))
        await clickProbe(page, item.probe)
    }

    const simplePage = await waitForPageWithUrl(
      launchedByFixture.get('simple-multi-page')!.app,
      'docs/out',
    )
    const gap = simplePage.locator('.page-gap').first()
    await gap.scrollIntoViewIfNeeded()
    const gapBox = await gap.boundingBox()
    expect(gapBox, 'multi-page fixture page gap').toBeTruthy()
    const box = gapBox!
    await simplePage.mouse.click(box.x + box.width / 2, box.y + box.height * 0.25)
    await settle(simplePage)
    const beforeGap = await simplePage.evaluate(() => {
      const debug = (window as unknown as { __pageDebug?: Record<string, unknown> }).__pageDebug
      const caret = (debug?.postRender as { caret?: { pageIndex?: number } } | undefined)?.caret
      return caret?.pageIndex
    })
    expect(beforeGap, 'before-gap click uses zero-based page index').toBe(0)

    await simplePage.mouse.click(box.x + box.width / 2, box.y + box.height * 0.75)
    await settle(simplePage)
    const afterGap = await simplePage.evaluate(() => {
      const debug = (window as unknown as { __pageDebug?: Record<string, unknown> }).__pageDebug
      const caret = (debug?.postRender as { caret?: { pageIndex?: number } } | undefined)?.caret
      return caret?.pageIndex
    })
    // Chromium/PM does not expose reliable affinity inside the visual gap: the
    // after-gap point may resolve to either adjacent caret. It must stay within
    // this boundary and use canonical zero-based page indexes.
    expect(afterGap, 'after-gap click uses canonical page index').toBeGreaterThanOrEqual(0)
    expect(afterGap, 'after-gap click stays on adjacent page').toBeLessThanOrEqual(1)
  } finally {
    for (const [fixture, launched] of launchedByFixture)
      await closeAndSaveVideo(launched, `docs-caret-hit-testing-${fixture}`)
  }
})
