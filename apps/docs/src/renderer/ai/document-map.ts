import type { Editor } from '@tiptap/core'
import type { Node as ProseMirrorNode } from '@tiptap/pm/model'
import {
  DOCUMENT_MAP_CONTRACT_VERSION,
  type DocumentMap,
  type DocumentMapBlock,
  type DocumentMapBlockKind,
  type DocumentMapCitation,
  type DocumentMapFigure,
  type DocumentMapFigureType,
  type DocumentMapLocator,
  type DocumentMapSection,
  type DocumentMapTable,
} from '@genoffice/document-gateway'
import { isTrackedDeleted } from './protocol'

type BlockIdentity = {
  id: string
  docxIndex?: number
}

type CitationDraft = {
  text: string
  start: number
  end: number
  format?: string
}

type ContentDraft = {
  text: string
  citations: CitationDraft[]
}

type SectionDraft = Omit<DocumentMapSection, 'blockIds'> & { blockIds: string[] }

function integerAttr(node: ProseMirrorNode, name: string): number | undefined {
  const value = node.attrs?.[name]
  return typeof value === 'number' && Number.isInteger(value) ? value : undefined
}

function blockIdentity(node: ProseMirrorNode, ordinal: number): BlockIdentity {
  const docxIndex = integerAttr(node, 'docxIndex')
  return {
    id: docxIndex === undefined ? `block-${ordinal}` : `b${docxIndex}`,
    ...(docxIndex === undefined ? {} : { docxIndex }),
  }
}

function blockKind(node: ProseMirrorNode): DocumentMapBlockKind {
  switch (node.type.name) {
    case 'docParagraph':
      return 'paragraph'
    case 'docHeading':
      return 'heading'
    case 'docListItem':
      return 'listItem'
    case 'docTable':
      return 'table'
    case 'image':
      return 'figure'
    case 'docProtected': {
      const blockType = String(node.attrs?.blockType ?? '')
      if (blockType === 'table' || node.attrs?.table) return 'table'
      if (blockType === 'image' || blockType === 'chart' || node.attrs?.imageDataUrl) {
        return 'figure'
      }
      return 'other'
    }
    default:
      return 'other'
  }
}

function figureType(node: ProseMirrorNode): DocumentMapFigureType {
  const blockType = String(node.attrs?.blockType ?? '')
  if (
    blockType === 'chart' ||
    String(node.attrs?.oleProgId ?? '')
      .toLowerCase()
      .includes('chart')
  ) {
    return 'chart'
  }
  if (blockType === 'image' || node.type.name === 'image' || node.attrs?.imageDataUrl) {
    return 'image'
  }
  return 'other'
}

function optionalCaption(node: ProseMirrorNode): string | undefined {
  const previewText = String(node.attrs?.previewText ?? '')
  return previewText.trim() ? previewText : undefined
}

function leafText(
  node: ProseMirrorNode,
  parent: ProseMirrorNode,
  pos: number,
  index: number,
): string {
  const spec = node.type.spec as typeof node.type.spec & {
    leafText?: (leaf: ProseMirrorNode) => string
    toText?: (props: {
      node: ProseMirrorNode
      pos: number
      parent: ProseMirrorNode
      index: number
    }) => string
  }
  return spec.toText?.({ node, pos, parent, index }) ?? spec.leafText?.(node) ?? ''
}

function codePointLength(text: string): number {
  return Array.from(text).length
}

function collectContent(node: ProseMirrorNode): ContentDraft {
  const parts: string[] = []
  const citations: CitationDraft[] = []
  let offset = 0

  const append = (text: string, citation?: { format?: string }): void => {
    const start = offset
    parts.push(text)
    offset += codePointLength(text)
    if (citation) citations.push({ text, start, end: offset, ...citation })
  }

  const visit = (
    current: ProseMirrorNode,
    parent: ProseMirrorNode,
    pos: number,
    index: number,
  ): void => {
    if (current.marks.some((mark) => mark.type.name === 'del')) return
    if (current.isText) {
      append(current.text ?? '')
      return
    }
    if (current.type.name === 'docxCitation') {
      const text = String(current.attrs?.renderedText ?? '')
      const format = String(current.attrs?.format ?? '')
      append(text, format ? { format } : {})
      return
    }
    if (current.type.name === 'hardBreak') {
      append('\n')
      return
    }
    if (current.isLeaf) {
      append(leafText(current, parent, pos, index))
      return
    }
    current.forEach((child, childPos, childIndex) => visit(child, current, childPos, childIndex))
  }

  node.forEach((child, pos, index) => visit(child, node, pos, index))
  return { text: parts.join(''), citations }
}

function tableDimensions(node: ProseMirrorNode): { rowCount: number; columnCount: number } {
  if (node.type.name === 'docProtected') {
    const table = node.attrs?.table as { rows?: unknown[] } | null | undefined
    if (table && Array.isArray(table.rows)) {
      const columnCount = table.rows.reduce<number>((max, row) => {
        if (!Array.isArray(row)) return max
        return Math.max(max, row.length)
      }, 0)
      return { rowCount: table.rows.length, columnCount }
    }
  }

  let rowCount = 0
  let columnCount = 0
  node.forEach((row) => {
    rowCount++
    let rowColumns = 0
    row.forEach((cell) => {
      const colspan = cell.attrs?.colspan
      rowColumns += typeof colspan === 'number' && colspan > 0 ? colspan : 1
    })
    columnCount = Math.max(columnCount, rowColumns)
  })
  return { rowCount, columnCount }
}

function blockText(node: ProseMirrorNode, deleted: boolean): ContentDraft {
  if (deleted) return { text: '', citations: [] }
  if (node.type.name === 'docProtected') {
    return { text: String(node.attrs?.previewText ?? ''), citations: [] }
  }
  return collectContent(node)
}

/**
 * Build the provider-neutral map from the active editor document. The only
 * editor-native identity retained is docxIndex, which is the existing DOCX
 * top-level anchor; editor-created blocks use an explicitly ordinal fallback.
 */
export function buildDocumentMap(editor: Editor, documentId: string, version: number): DocumentMap {
  const sections: SectionDraft[] = []
  const blocks: DocumentMapBlock[] = []
  const tables: DocumentMapTable[] = []
  const figures: DocumentMapFigure[] = []
  const citations: DocumentMapCitation[] = []
  const sectionStack: SectionDraft[] = []

  editor.state.doc.forEach((node, _pos, ordinal) => {
    const identity = blockIdentity(node, ordinal)
    const kind = blockKind(node)
    const deleted = isTrackedDeleted(node)
    const content = blockText(node, deleted)
    const headingLevel =
      kind === 'heading' ? Math.min(Math.max(Number(node.attrs?.level) || 1, 1), 6) : undefined

    let currentSection = sectionStack.at(-1)
    if (headingLevel !== undefined) {
      while (sectionStack.at(-1) && sectionStack.at(-1)!.level >= headingLevel) {
        sectionStack.pop()
      }
      currentSection = undefined
      const sectionId = `section:${identity.id}`
      const parent = sectionStack.at(-1)
      const locator: DocumentMapLocator = {
        documentId,
        version,
        blockId: identity.id,
        blockOrdinal: ordinal,
        ...(identity.docxIndex === undefined ? {} : { docxIndex: identity.docxIndex }),
        sectionId,
      }
      currentSection = {
        id: sectionId,
        headingText: content.text,
        level: headingLevel,
        ...(parent ? { parentId: parent.id } : {}),
        locator,
        blockIds: [],
        isDeleted: deleted,
      }
      sections.push(currentSection)
      sectionStack.push(currentSection)
    }

    const sectionId = currentSection?.id
    const locator: DocumentMapLocator = {
      documentId,
      version,
      blockId: identity.id,
      blockOrdinal: ordinal,
      ...(identity.docxIndex === undefined ? {} : { docxIndex: identity.docxIndex }),
      ...(sectionId ? { sectionId } : {}),
    }
    sectionStack.forEach((section) => section.blockIds.push(identity.id))

    blocks.push({
      id: identity.id,
      ordinal,
      kind,
      text: content.text,
      locator,
      ...(sectionId ? { sectionId } : {}),
      ...(headingLevel === undefined ? {} : { headingLevel }),
      ...(kind === 'figure' || kind === 'table'
        ? (() => {
            const caption = optionalCaption(node)
            return caption ? { caption } : {}
          })()
        : {}),
      isDeleted: deleted,
    })

    if (kind === 'table') {
      const dimensions = tableDimensions(node)
      tables.push({
        id: identity.id,
        locator,
        ...dimensions,
        ...(optionalCaption(node) ? { caption: optionalCaption(node) } : {}),
      })
    }

    if (kind === 'figure') {
      figures.push({
        id: identity.id,
        locator,
        figureType: figureType(node),
        ...(optionalCaption(node) ? { caption: optionalCaption(node) } : {}),
      })
    }

    content.citations.forEach((citation, citationOrdinal) => {
      citations.push({
        id: `${identity.id}:citation:${citationOrdinal}`,
        locator,
        ...citation,
      })
    })
  })

  return {
    contractVersion: DOCUMENT_MAP_CONTRACT_VERSION,
    documentId,
    version,
    sections,
    blocks,
    tables,
    figures,
    citations,
    references: [],
  }
}
