import type { DocumentId, DocumentVersion } from './types'

export const DOCUMENT_MAP_CONTRACT_VERSION = 'document-map-v1' as const

export type DocumentMapBlockKind =
  'paragraph' | 'heading' | 'listItem' | 'table' | 'figure' | 'other'

export type DocumentMapFigureType = 'image' | 'chart' | 'other'

export interface DocumentMapLocator {
  readonly documentId: DocumentId
  readonly version: DocumentVersion
  readonly blockId: string
  readonly blockOrdinal: number
  readonly docxIndex?: number
  readonly sectionId?: string
}

export interface DocumentMapSection {
  readonly id: string
  readonly headingText: string
  readonly level: number
  readonly parentId?: string
  readonly locator: DocumentMapLocator
  readonly blockIds: readonly string[]
  readonly isDeleted: boolean
}

export interface DocumentMapBlock {
  readonly id: string
  readonly ordinal: number
  readonly kind: DocumentMapBlockKind
  readonly text: string
  readonly locator: DocumentMapLocator
  readonly sectionId?: string
  readonly headingLevel?: number
  readonly caption?: string
  readonly isDeleted: boolean
}

export interface DocumentMapTable {
  readonly id: string
  readonly locator: DocumentMapLocator
  readonly rowCount: number
  readonly columnCount: number
  readonly caption?: string
}

export interface DocumentMapFigure {
  readonly id: string
  readonly locator: DocumentMapLocator
  readonly figureType: DocumentMapFigureType
  readonly caption?: string
}

export interface DocumentMapCitation {
  readonly id: string
  readonly locator: DocumentMapLocator
  readonly text: string
  /** Unicode scalar/code-point offsets within the containing block text. */
  readonly start: number
  readonly end: number
  readonly format?: string
}

export interface DocumentMapReference {
  readonly id: string
  readonly locator: DocumentMapLocator
  readonly text: string
}

/** Provider-neutral structural snapshot of one active manuscript version. */
export interface DocumentMap {
  readonly contractVersion: typeof DOCUMENT_MAP_CONTRACT_VERSION
  readonly documentId: DocumentId
  readonly version: DocumentVersion
  readonly sections: readonly DocumentMapSection[]
  readonly blocks: readonly DocumentMapBlock[]
  readonly tables: readonly DocumentMapTable[]
  readonly figures: readonly DocumentMapFigure[]
  readonly citations: readonly DocumentMapCitation[]
  readonly references: readonly DocumentMapReference[]
}

export function isDocumentMapCurrent(
  map: DocumentMap,
  documentId: DocumentId,
  version: DocumentVersion,
): boolean {
  return map.documentId === documentId && map.version === version
}

export function isDocumentMapStale(
  map: DocumentMap,
  documentId: DocumentId,
  version: DocumentVersion,
): boolean {
  return !isDocumentMapCurrent(map, documentId, version)
}
