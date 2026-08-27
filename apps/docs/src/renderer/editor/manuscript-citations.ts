import type { Editor } from '@tiptap/core'
import type {
  ActiveDocument,
  ManuscriptCitationFormat,
  ManuscriptCitationSyncOccurrence,
  ManuscriptCitationSyncRun,
  SyncManuscriptCitationsInput,
} from '@genoffice/9profs-core'
import {
  extractDocxCitationsFromPmDoc,
  extractDocxClaimBlocksFromPmDoc,
  type PmNode,
} from './convert'

export interface BuildManuscriptCitationSyncInputOptions {
  readonly editor: Pick<Editor, 'state'>
  readonly documentId: string
  readonly documentVersion: number
}

function manuscriptCitationFormat(format: string): ManuscriptCitationFormat | undefined {
  switch (format) {
    case 'WordNative':
      return 'word_native'
    case 'Zotero':
      return 'zotero'
    default:
      return undefined
  }
}

function pmDocFromEditor(editor: Pick<Editor, 'state'>): PmNode {
  const doc = editor.state.doc as unknown as PmNode & { toJSON?: () => PmNode }
  return Array.isArray(doc.content) ? doc : (doc.toJSON?.() ?? doc)
}

export function buildManuscriptCitationSyncInput({
  editor,
  documentId,
  documentVersion,
}: BuildManuscriptCitationSyncInputOptions): SyncManuscriptCitationsInput {
  return {
    documentId,
    documentVersion,
    citations: extractDocxCitationsFromPmDoc(pmDocFromEditor(editor)).flatMap((citation) => {
      const format = manuscriptCitationFormat(citation.format)
      if (format === undefined) return []
      return [
        {
          format,
          renderedText: citation.renderedText,
          blockId: citation.blockId,
          start: citation.start,
          end: citation.end,
          targets: citation.targets.map((target) => ({
            ordinal: target.ordinal,
            referenceKey: target.referenceKey,
            citedLocator: target.citedLocator ?? null,
          })),
        },
      ]
    }),
  }
}

export interface BuildManuscriptClaimExtractionInputOptions {
  readonly editor: Pick<Editor, 'state'>
  readonly activeDocument: Pick<ActiveDocument, 'documentId' | 'version'>
  readonly syncRun: ManuscriptCitationSyncRun
  readonly syncOccurrences: readonly ManuscriptCitationSyncOccurrence[]
}

export interface ManuscriptClaimExtractionInput {
  readonly documentId: string
  readonly documentVersion: number
  readonly blocks: ReadonlyArray<{
    readonly blockId: string
    readonly text: string
    readonly citations: ReadonlyArray<{
      readonly citationOccurrenceId: string
      readonly start: number
      readonly end: number
      readonly renderedText: string
    }>
  }>
}

export function buildManuscriptClaimExtractionInput({
  editor,
  activeDocument,
  syncRun,
  syncOccurrences,
}: BuildManuscriptClaimExtractionInputOptions): ManuscriptClaimExtractionInput {
  if (
    activeDocument.documentId !== syncRun.documentId ||
    activeDocument.version !== syncRun.documentVersion
  ) {
    throw new Error('active document does not match completed citation sync')
  }
  const occurrenceByPosition = new Map(
    syncOccurrences.map((occurrence) => [
      `${occurrence.documentBlockId}:${occurrence.start}:${occurrence.end}`,
      occurrence,
    ]),
  )
  return {
    documentId: syncRun.documentId,
    documentVersion: syncRun.documentVersion,
    blocks: extractDocxClaimBlocksFromPmDoc(pmDocFromEditor(editor)).map((block) => ({
      blockId: block.blockId,
      text: block.text,
      citations: block.citations.map((citation) => {
        const occurrence = occurrenceByPosition.get(
          `${citation.blockId}:${citation.start}:${citation.end}`,
        )
        if (occurrence === undefined || occurrence.citationOccurrenceId === undefined) {
          throw new Error('citation sync occurrence does not match live PM citation')
        }
        return {
          citationOccurrenceId: occurrence.citationOccurrenceId,
          start: citation.start,
          end: citation.end,
          renderedText: citation.renderedText,
        }
      }),
    })),
  }
}
