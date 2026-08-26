import { pdfToPages } from '@genoffice/file-parse'
import type { CoreTransport } from './transport'
import type {
  CaptureResearchPdfExtractionInput,
  ReferencePdfIngestion,
  ResearchPdfExtraction,
} from './types'

export async function extractResearchPdfPages(
  bytes: Uint8Array,
): Promise<CaptureResearchPdfExtractionInput> {
  try {
    const extraction = await pdfToPages(bytes)
    const pages = extraction.pages.map(({ page, text }) => ({ page, text }))
    return {
      extractor: 'pdfjs',
      pageCount: extraction.pageCount,
      status: pages.every(({ text }) => text.trim().length === 0) ? 'no_extractable_text' : 'ready',
      pages,
    }
  } catch (error) {
    const message =
      error instanceof Error ? error.message.toLowerCase() : String(error).toLowerCase()
    return {
      extractor: 'pdfjs',
      pageCount: 0,
      status: /password|encrypted|encryption/.test(message) ? 'password_required' : 'failed',
      pages: [],
    }
  }
}

export async function createResearchPdfIngestion(
  transport: CoreTransport,
  researchCaseId: string,
  bytes: Uint8Array,
  options: { readonly filename?: string; readonly label?: string } = {},
): Promise<ReferencePdfIngestion & { readonly extraction: ResearchPdfExtraction }> {
  const manifest = await extractResearchPdfPages(bytes)
  const uploaded = await transport.ingestReferencePdf(researchCaseId, bytes, options)
  const extraction = await transport.recordResearchPdfExtraction(
    uploaded.snapshot.snapshotId,
    manifest,
  )
  return { ...uploaded, extraction }
}
