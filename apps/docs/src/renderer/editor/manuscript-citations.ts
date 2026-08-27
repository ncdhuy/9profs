import type { Editor } from '@tiptap/core'
import type { ManuscriptCitationFormat, SyncManuscriptCitationsInput } from '@genoffice/9profs-core'
import { extractDocxCitationsFromPmDoc, type PmNode } from './convert'

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
