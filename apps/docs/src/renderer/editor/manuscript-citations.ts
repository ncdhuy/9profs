import type { Editor } from '@tiptap/core'
import type {
  ActiveDocument,
  ManuscriptCitationFormat,
  ManuscriptCitationSyncOccurrence,
  ManuscriptCitationSyncRun,
  ManuscriptCitationSyncTarget,
  StartManuscriptClaimInventoryInput,
  StartManuscriptCitationReviewInput,
  StartManuscriptResearchReviewInput,
  SyncManuscriptReferenceCatalogInput,
  SyncManuscriptCitationsInput,
} from '@genoffice/9profs-core'
import {
  extractDocxCitationsFromPmDoc,
  extractDocxClaimBlocksFromPmDoc,
  extractWholeManuscriptClaimBlocksFromPmDoc,
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

export interface BuildManuscriptCitationReviewInputOptions {
  readonly editor: Pick<Editor, 'state'>
  readonly activeDocument: Pick<ActiveDocument, 'documentId' | 'version'>
  readonly manuscriptSourceId: string
  readonly pmDoc?: PmNode
}

export function buildManuscriptCitationReviewInput({
  editor,
  activeDocument,
  manuscriptSourceId,
  pmDoc,
}: BuildManuscriptCitationReviewInputOptions): StartManuscriptCitationReviewInput {
  const doc = pmDoc ?? pmDocFromEditor(editor)
  const citations = extractDocxCitationsFromPmDoc(doc).flatMap((citation) => {
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
          ...(format === 'word_native' && target.source
            ? {
                wordSource: {
                  tag: target.source.tag,
                  title: target.source.title,
                  author: target.source.author,
                  year: target.source.year,
                },
              }
            : {}),
          ...(format === 'zotero'
            ? {
                zotero: {
                  itemId: target.itemId ?? null,
                  uris: target.uris ?? [],
                },
              }
            : {}),
        })),
      },
    ]
  })
  const supportedRanges = new Set(
    citations.map((citation) => `${citation.blockId}:${citation.start}:${citation.end}`),
  )
  return {
    manuscriptSourceId,
    documentId: activeDocument.documentId,
    documentVersion: activeDocument.version,
    citations,
    blocks: extractDocxClaimBlocksFromPmDoc(doc).map((block) => ({
      blockId: block.blockId,
      text: block.text,
      citations: block.citations
        .filter((citation) =>
          supportedRanges.has(`${citation.blockId}:${citation.start}:${citation.end}`),
        )
        .map((citation) => ({
          start: citation.start,
          end: citation.end,
          renderedText: citation.renderedText,
        })),
    })),
  }
}

export interface BuildManuscriptClaimInventoryInputOptions {
  readonly editor: Pick<Editor, 'state'>
  readonly activeDocument: Pick<ActiveDocument, 'documentId' | 'version'>
  readonly manuscriptSourceId: string
  readonly pmDoc?: PmNode
}

export function buildManuscriptClaimInventoryInput({
  editor,
  activeDocument,
  manuscriptSourceId,
  pmDoc,
}: BuildManuscriptClaimInventoryInputOptions): StartManuscriptClaimInventoryInput {
  const doc = pmDoc ?? pmDocFromEditor(editor)
  return {
    manuscriptSourceId,
    documentId: activeDocument.documentId,
    documentVersion: activeDocument.version,
    blocks: extractWholeManuscriptClaimBlocksFromPmDoc(doc).map((block) => ({
      blockId: block.blockId,
      blockOrdinal: block.blockOrdinal,
      blockKind: block.blockKind,
      text: block.text,
      citations: block.citations.map((citation) => ({
        start: citation.start,
        end: citation.end,
        renderedText: citation.renderedText,
      })),
    })),
  }
}

export interface BuildManuscriptResearchReviewInputOptions {
  readonly editor: Pick<Editor, 'state'>
  readonly activeDocument: Pick<ActiveDocument, 'documentId' | 'version'>
  readonly manuscriptSourceId: string
}

export function buildManuscriptResearchReviewInput({
  editor,
  activeDocument,
  manuscriptSourceId,
}: BuildManuscriptResearchReviewInputOptions): StartManuscriptResearchReviewInput {
  const pmDoc = pmDocFromEditor(editor)
  const citationReview = buildManuscriptCitationReviewInput({
    editor,
    activeDocument,
    manuscriptSourceId,
    pmDoc,
  })
  const claimInventory = buildManuscriptClaimInventoryInput({
    editor,
    activeDocument,
    manuscriptSourceId,
    pmDoc,
  })
  return {
    manuscriptSourceId,
    documentId: activeDocument.documentId,
    documentVersion: activeDocument.version,
    citationReviewObservations: {
      citations: citationReview.citations,
      citationBlocks: citationReview.blocks,
    },
    claimInventoryObservations: {
      wholeManuscriptBlocks: claimInventory.blocks,
    },
  }
}

export interface BuildManuscriptReferenceCatalogInputOptions {
  readonly editor: Pick<Editor, 'state'>
  readonly activeDocument: Pick<ActiveDocument, 'documentId' | 'version'>
  readonly syncRun: ManuscriptCitationSyncRun
  readonly syncOccurrences: readonly ManuscriptCitationSyncOccurrence[]
  readonly syncTargets: readonly ManuscriptCitationSyncTarget[]
}

export function buildManuscriptReferenceCatalogInput({
  editor,
  activeDocument,
  syncRun,
  syncOccurrences,
  syncTargets,
}: BuildManuscriptReferenceCatalogInputOptions): SyncManuscriptReferenceCatalogInput {
  if (
    syncRun.status !== 'completed' ||
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
  const targetsByOccurrence = new Map<string, ManuscriptCitationSyncTarget[]>()
  for (const target of syncTargets) {
    const targets = targetsByOccurrence.get(target.syncOccurrenceId) ?? []
    targets.push(target)
    targetsByOccurrence.set(target.syncOccurrenceId, targets)
  }
  return {
    documentId: syncRun.documentId,
    documentVersion: syncRun.documentVersion,
    citations: extractDocxCitationsFromPmDoc(pmDocFromEditor(editor)).flatMap((citation) => {
      const format = manuscriptCitationFormat(citation.format)
      if (format === undefined) return []
      const occurrence = occurrenceByPosition.get(
        `${citation.blockId}:${citation.start}:${citation.end}`,
      )
      if (occurrence === undefined || occurrence.syncRunId !== syncRun.syncRunId) {
        throw new Error('citation sync occurrence does not match live PM citation')
      }
      const targets = [...(targetsByOccurrence.get(occurrence.syncOccurrenceId) ?? [])].sort(
        (left, right) => left.documentTargetOrdinal - right.documentTargetOrdinal,
      )
      if (targets.length !== citation.targets.length) {
        throw new Error('citation sync targets do not match live PM citation')
      }
      return [
        {
          citationOccurrenceId: occurrence.citationOccurrenceId,
          blockId: citation.blockId,
          start: citation.start,
          end: citation.end,
          format,
          targets: citation.targets.map((target) => {
            const syncTarget = targets.find(
              (candidate) => candidate.documentTargetOrdinal === target.ordinal,
            )
            if (syncTarget === undefined || syncTarget.documentTargetOrdinal !== target.ordinal) {
              throw new Error('citation sync target does not match live PM citation')
            }
            return {
              citationTargetId: syncTarget.citationTargetId,
              ordinal: target.ordinal,
              referenceKey: target.referenceKey,
              ...(format === 'word_native' && target.source
                ? {
                    wordSource: {
                      tag: target.source.tag,
                      title: target.source.title,
                      author: target.source.author,
                      year: target.source.year,
                    },
                  }
                : {}),
              ...(format === 'zotero'
                ? {
                    zotero: {
                      itemId: target.itemId ?? null,
                      uris: target.uris ?? [],
                    },
                  }
                : {}),
            }
          }),
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
