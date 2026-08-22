import { expect, test, type Page } from '@playwright/test'
import JSZip from 'jszip'
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { tmpdir } from 'node:os'
import { closeAndSaveVideo, launchShell, waitForPageWithUrl } from './helpers'

type PerformanceSnapshot = {
  totalMs: number
  sectionNormalizationMs: number
  initialPageSolveMs: number
  measurementRefinementMs: number
  parityFinalizationMs: number
  refinementPasses: number
  reSolves: number
  measurementCandidates: number
  measurementAttempts: number
  actualDomSamples: number
  cacheHits: number
  cacheMisses: number
  lineDomSamples: number
  tableDomSamples: number
}

type LayoutMetrics = {
  pages: number
  blocks: number
  paginationRunId: number
  layoutRuns?: number
  remeasureMs?: number
  measureMs?: number
  sliceMs?: number
  gapsBuildMs?: number
  setGapsMs?: number
  columnsMs?: number
  floatShiftsMs?: number
  geometryMs?: number
  annotationsMs?: number
  postRenderMs?: number
  v2Performance?: PerformanceSnapshot
  paginationPreview?: {
    pages: number
    blocks: number
    clonePairs: number
    mode: 'full' | 'pruned'
    paginationMs: number
    cloneBuildMs: number
    clonePayloadChars: number
    totalMs: number
  }
}

type Workload = {
  name: string
  paragraphs: number
  targetPages: number
}

const WORKLOADS: Workload[] = [
  { name: 'control-10p', paragraphs: 180, targetPages: 10 },
  { name: 'medium-50p', paragraphs: 900, targetPages: 50 },
  { name: 'large-100p', paragraphs: 1800, targetPages: 100 },
]

const XML_DECL = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\r\n'
const DOC_NS =
  'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" ' +
  'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"'
const A4_SECT_PR =
  '<w:sectPr><w:pgSz w:w="11906" w:h="16838"/>' +
  '<w:pgMar w:top="1440" w:right="1800" w:bottom="1440" w:left="1800" w:header="708" w:footer="708" w:gutter="0"/>' +
  '</w:sectPr>'
const STYLES_XML =
  XML_DECL +
  '<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">' +
  '<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/>' +
  '<w:sz w:val="24"/><w:szCs w:val="24"/></w:rPr></w:rPrDefault>' +
  '<w:pPrDefault><w:pPr><w:spacing w:after="160" w:line="276" w:lineRule="auto"/></w:pPr></w:pPrDefault>' +
  '</w:docDefaults>' +
  '<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>' +
  '<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/>' +
  '<w:pPr><w:keepNext/><w:spacing w:before="240" w:after="120"/></w:pPr>' +
  '<w:rPr><w:b/><w:sz w:val="32"/><w:szCs w:val="32"/></w:rPr></w:style>' +
  '</w:styles>'

const PROSE =
  'The report records ordinary document prose so the presentation pipeline can be observed at realistic paragraph boundaries. Each section explains a stable operational result, preserves readable wrapping, and provides enough repeated content to exercise the document flow.'

function escXml(text: string): string {
  return text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

function paragraph(text: string, style?: string): string {
  const pPr = style ? `<w:pPr><w:pStyle w:val="${style}"/><w:keepNext/></w:pPr>` : ''
  return `<w:p>${pPr}<w:r><w:t xml:space="preserve">${escXml(text)}</w:t></w:r></w:p>`
}

async function buildLongDocx(workload: Workload): Promise<Uint8Array> {
  const body: string[] = []
  for (let index = 0; index < workload.paragraphs; index++) {
    if (index % 25 === 0) {
      body.push(
        paragraph(`Section ${Math.floor(index / 25) + 1}: Operational findings`, 'Heading1'),
      )
    }
    body.push(paragraph(`${PROSE} Paragraph ${index + 1} of ${workload.paragraphs}.`))
  }

  const zip = new JSZip()
  zip.file(
    '[Content_Types].xml',
    `${XML_DECL}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">` +
      '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>' +
      '<Default Extension="xml" ContentType="application/xml"/>' +
      '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>' +
      '<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>' +
      '</Types>',
  )
  zip.file(
    '_rels/.rels',
    `${XML_DECL}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">` +
      '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>' +
      '</Relationships>',
  )
  zip.file(
    'word/_rels/document.xml.rels',
    `${XML_DECL}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">` +
      '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>' +
      '</Relationships>',
  )
  zip.file('word/styles.xml', STYLES_XML)
  zip.file(
    'word/document.xml',
    `${XML_DECL}<w:document ${DOC_NS}><w:body>${body.join('')}${A4_SECT_PR}</w:body></w:document>`,
  )
  return zip.generateAsync({ type: 'uint8array', compression: 'DEFLATE' })
}

async function writeWorkloads(dir: string): Promise<Map<string, string>> {
  const paths = new Map<string, string>()
  for (const workload of WORKLOADS) {
    const path = join(dir, `${workload.name}.docx`)
    await writeFile(path, await buildLongDocx(workload))
    paths.set(workload.name, path)
  }
  return paths
}

async function runTwoFrames(page: Page): Promise<void> {
  await page.evaluate(
    () =>
      new Promise<void>((resolvePromise) => {
        requestAnimationFrame(() => requestAnimationFrame(() => resolvePromise()))
      }),
  )
}

async function readRun(page: Page): Promise<{ renderer: string; runId: number; pages: number }> {
  return page.evaluate(() => {
    const debug = (window as unknown as { __pageDebug?: Record<string, unknown> }).__pageDebug
    const postRender = debug?.postRender as { pages?: unknown[] } | undefined
    return {
      renderer: String(debug?.renderer ?? ''),
      runId: typeof debug?.paginationRunId === 'number' ? debug.paginationRunId : 0,
      pages: Array.isArray(debug?.slices)
        ? debug.slices.length
        : Array.isArray(postRender?.pages)
          ? postRender.pages.length
          : 0,
    }
  })
}

async function readMetrics(page: Page): Promise<LayoutMetrics> {
  return page.evaluate(() => {
    const debug = (window as unknown as { __pageDebug?: Record<string, unknown> }).__pageDebug
    const postRender = debug?.postRender as { pages?: unknown[] } | undefined
    const number = (key: string): number | undefined =>
      typeof debug?.[key] === 'number' ? (debug[key] as number) : undefined
    return {
      pages: Array.isArray(debug?.slices)
        ? debug.slices.length
        : Array.isArray(postRender?.pages)
          ? postRender.pages.length
          : 0,
      blocks: Array.isArray(debug?.blocks) ? debug.blocks.length : 0,
      paginationRunId: typeof debug?.paginationRunId === 'number' ? debug.paginationRunId : 0,
      remeasureMs: number('remeasureMs'),
      measureMs: number('measureMs'),
      sliceMs: number('sliceMs'),
      gapsBuildMs: number('gapsBuildMs'),
      setGapsMs: number('setGapsMs'),
      columnsMs: number('columnsMs'),
      floatShiftsMs: number('floatShiftsMs'),
      geometryMs: number('geometryMs'),
      annotationsMs: number('annotationsMs'),
      postRenderMs: number('postRenderMs'),
      v2Performance: debug?.v2Performance as PerformanceSnapshot | undefined,
      paginationPreview: debug?.paginationPreview as LayoutMetrics['paginationPreview'],
    }
  })
}

async function measurePaginationPreview(page: Page): Promise<LayoutMetrics['paginationPreview']> {
  await page.evaluate(() => {
    const aidocs = (window as unknown as { __aidocs?: { pagePreview?: () => void } }).__aidocs
    if (typeof aidocs?.pagePreview !== 'function')
      throw new Error('Docs page did not expose page preview automation')
    aidocs.pagePreview()
  })
  await expect
    .poll(
      () =>
        page.evaluate(() => {
          const debug = (window as unknown as { __pageDebug?: Record<string, unknown> }).__pageDebug
          return (
            typeof (debug?.paginationPreview as { totalMs?: unknown } | undefined)?.totalMs ===
            'number'
          )
        }),
      { timeout: 180_000 },
    )
    .toBe(true)
  const preview = await page.evaluate(
    () =>
      (window as unknown as { __pageDebug?: Record<string, unknown> }).__pageDebug
        ?.paginationPreview as LayoutMetrics['paginationPreview'],
  )
  await page.evaluate(() => {
    const aidocs = (window as unknown as { __aidocs?: { closePagePreview?: () => void } }).__aidocs
    aidocs?.closePagePreview?.()
  })
  return preview
}

async function waitForSettledRun(page: Page, previousRunId: number): Promise<LayoutMetrics> {
  await expect
    .poll(
      async () => {
        const run = await readRun(page)
        return run.renderer === 'v2' && run.pages > 0 && run.runId > previousRunId
      },
      { timeout: 180_000, intervals: [100, 250, 500, 1000] },
    )
    .toBe(true)

  for (;;) {
    const first = await readRun(page)
    await page.waitForTimeout(350)
    await runTwoFrames(page)
    const second = await readRun(page)
    if (second.runId === first.runId) return readMetrics(page)
  }
}

async function invokeRemeasure(page: Page): Promise<void> {
  await page.evaluate(() => {
    const debug = (window as unknown as { __pageDebug?: Record<string, unknown> }).__pageDebug
    const remeasure = debug?.remeasure
    if (typeof remeasure !== 'function') throw new Error('Docs page did not expose debug remeasure')
    remeasure()
  })
}

async function editAt(page: Page, fraction: number): Promise<void> {
  await page.evaluate((positionFraction) => {
    const aidocs = (
      window as unknown as {
        __aidocs?: {
          editor?: {
            state: { doc: { content: { size: number } }; selection: unknown }
            commands: { setTextSelection: (pos: number) => void; focus: () => void }
            view: { focus: () => void }
          }
        }
      }
    ).__aidocs
    const editor = aidocs?.editor
    if (!editor) throw new Error('Docs page did not expose the editor automation hook')
    const size = editor.state.doc.content.size
    const pos = Math.max(1, Math.min(size - 1, Math.floor(size * positionFraction)))
    editor.commands.setTextSelection(pos)
    editor.commands.focus()
    editor.view.focus()
  }, fraction)
  await page.locator('.ProseMirror').first().focus()
  await page.keyboard.insertText('x')
}

function median(values: number[]): number {
  const ordered = [...values].sort((a, b) => a - b)
  return ordered[Math.floor(ordered.length / 2)] ?? 0
}

function medianPerformance(samples: LayoutMetrics[]): PerformanceSnapshot | undefined {
  const snapshots = samples
    .map((sample) => sample.v2Performance)
    .filter((value): value is PerformanceSnapshot => value !== undefined)
  if (snapshots.length === 0) return undefined
  const keys: Array<keyof PerformanceSnapshot> = [
    'totalMs',
    'sectionNormalizationMs',
    'initialPageSolveMs',
    'measurementRefinementMs',
    'parityFinalizationMs',
    'refinementPasses',
    'reSolves',
    'measurementCandidates',
    'measurementAttempts',
    'actualDomSamples',
    'cacheHits',
    'cacheMisses',
    'lineDomSamples',
    'tableDomSamples',
  ]
  return Object.fromEntries(
    keys.map((key) => [key, median(snapshots.map((snapshot) => snapshot[key]))]),
  ) as PerformanceSnapshot
}

function medianSample(samples: LayoutMetrics[], elapsedMs: number[]): Record<string, unknown> {
  const numeric = (key: keyof LayoutMetrics): number | undefined => {
    const values = samples
      .map((sample) => sample[key])
      .filter((value): value is number => typeof value === 'number')
    return values.length > 0 ? median(values) : undefined
  }
  return {
    elapsedMs: median(elapsedMs),
    pages: samples[0]?.pages ?? 0,
    blocks: samples[0]?.blocks ?? 0,
    layoutRuns: median(samples.map((sample) => sample.layoutRuns ?? sample.paginationRunId)),
    remeasureMs: numeric('remeasureMs'),
    measureMs: numeric('measureMs'),
    sliceMs: numeric('sliceMs'),
    gapsBuildMs: numeric('gapsBuildMs'),
    setGapsMs: numeric('setGapsMs'),
    columnsMs: numeric('columnsMs'),
    floatShiftsMs: numeric('floatShiftsMs'),
    geometryMs: numeric('geometryMs'),
    annotationsMs: numeric('annotationsMs'),
    postRenderMs: numeric('postRenderMs'),
    v2Performance: medianPerformance(samples),
  }
}

test('DOCX V2 long-document cold, warm, and local-edit performance', async () => {
  test.setTimeout(1_200_000)
  const workDir = await mkdtemp(join(tmpdir(), '9profs-docx-performance-'))
  const outputPath = resolve(__dirname, '../test-results/long-document-performance.json')
  const paths = await writeWorkloads(workDir)
  const results: Record<string, unknown>[] = []

  try {
    for (const workload of WORKLOADS) {
      const coldStartedAt = performance.now()
      const launched = await launchShell({
        onboardingSeen: true,
        openFile: paths.get(workload.name),
        presentationRenderer: 'v2',
        videoDir: `docs-long-document-performance-${workload.name}`,
      })
      try {
        const page = await waitForPageWithUrl(launched.app, 'docs/out')
        await expect(page.locator('.ProseMirror').first()).toBeVisible({ timeout: 180_000 })
        const initial = await waitForSettledRun(page, 0)
        initial.layoutRuns = initial.paginationRunId
        const coldMs = performance.now() - coldStartedAt

        const warmSamples: LayoutMetrics[] = []
        const warmElapsed: number[] = []
        let warmBaseline = initial.paginationRunId
        for (let sample = 0; sample < 3; sample++) {
          const previousRunId = warmBaseline
          const startedAt = performance.now()
          await invokeRemeasure(page)
          const warm = await waitForSettledRun(page, warmBaseline)
          warmBaseline = warm.paginationRunId
          warm.layoutRuns = warm.paginationRunId - previousRunId
          warmSamples.push(warm)
          warmElapsed.push(performance.now() - startedAt)
        }

        const edits: Record<string, unknown> = {}
        for (const [label, fraction] of [
          ['beginning', 0.02],
          ['middle', 0.5],
          ['end', 0.98],
        ] as const) {
          const samples: LayoutMetrics[] = []
          const elapsed: number[] = []
          let editBaseline = warmBaseline
          for (let sample = 0; sample < 3; sample++) {
            const previousRunId = editBaseline
            const startedAt = performance.now()
            await editAt(page, fraction)
            const edited = await waitForSettledRun(page, editBaseline)
            editBaseline = edited.paginationRunId
            edited.layoutRuns = edited.paginationRunId - previousRunId
            samples.push(edited)
            elapsed.push(performance.now() - startedAt)
          }
          warmBaseline = editBaseline
          edits[label] = medianSample(samples, elapsed)
        }
        const paginationPreview = await measurePaginationPreview(page)

        results.push({
          workload: workload.name,
          targetPages: workload.targetPages,
          paragraphs: workload.paragraphs,
          cold: { elapsedMs: coldMs, ...initial },
          warm: medianSample(warmSamples, warmElapsed),
          edits,
          paginationPreview,
        })
      } finally {
        await closeAndSaveVideo(launched, `docs-long-document-performance-${workload.name}`)
      }
    }

    await mkdir(dirname(outputPath), { recursive: true })
    await writeFile(
      outputPath,
      JSON.stringify(
        {
          generatedAt: new Date().toISOString(),
          methodology: {
            cold: 'one fresh Electron launch per workload',
            warm: 'three same-DOM explicit debug remeasure runs',
            edits: 'three beginning/middle/end text edits per workload',
            median: true,
          },
          results,
        },
        null,
        2,
      ),
    )
    console.log(`LONG_DOCUMENT_PERFORMANCE_JSON ${JSON.stringify({ results })}`)
  } finally {
    await rm(workDir, { recursive: true, force: true })
  }
})
