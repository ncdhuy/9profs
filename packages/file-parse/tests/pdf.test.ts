import { describe, expect, it } from 'vitest'
import { parseFileToText, pdfToPages } from '../src/index'
import { buildPdfFixture, buildPdfPagesFixture, writeFixture } from './helpers/fixtures'

describe('parseFileToText: pdf', () => {
  it('extracts page text via pdfjs', async () => {
    const bytes = buildPdfFixture('Hello PDF parsing')
    const pages = await pdfToPages(bytes)
    expect(pages.pageCount).toBe(1)
    expect(pages.pages).toEqual([{ page: 1, text: 'Hello PDF parsing' }])

    const path = writeFixture('doc.pdf', bytes)
    const result = await parseFileToText(path)
    expect(result.ok).toBe(true)
    expect(result.kind).toBe('text')
    expect(result.text).toContain('Hello PDF parsing')
  })

  it('preserves page boundaries for multi-page PDFs', async () => {
    const pages = await pdfToPages(buildPdfPagesFixture(['First page', 'Second page']))
    expect(pages).toEqual({
      pageCount: 2,
      pages: [
        { page: 1, text: 'First page' },
        { page: 2, text: 'Second page' },
      ],
    })
  })

  it('fails gracefully on a corrupt pdf', async () => {
    const path = writeFixture('broken.pdf', Buffer.from('%PDF-1.4 garbage'))
    const result = await parseFileToText(path)
    expect(result.ok).toBe(false)
    expect(result.error).toBeTruthy()
  })
})
