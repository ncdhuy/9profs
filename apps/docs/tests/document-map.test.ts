import { readFile } from 'node:fs/promises'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import { Editor } from '@tiptap/core'
import { parseDocx } from '@genoffice/docx-engine'
import { isDocumentMapCurrent, isDocumentMapStale } from '@genoffice/document-gateway'
import { blocksToPmDoc } from '../src/renderer/editor/convert'
import { buildDocumentMap } from '../src/renderer/ai/document-map'
import { editorExtensions } from '../src/renderer/editor/extensions'

const editors = new Set<Editor>()

afterEach(() => {
  for (const editor of editors) editor.destroy()
  editors.clear()
})

function paragraph(text = '', docxIndex?: number) {
  return {
    type: 'docParagraph',
    attrs: docxIndex === undefined ? {} : { docxIndex },
    content: text ? [{ type: 'text', text }] : undefined,
  }
}

function openEditor(content: Record<string, unknown>[]) {
  const editor = new Editor({
    element: document.createElement('div'),
    extensions: editorExtensions,
    content: { type: 'doc', content } as never,
  })
  editors.add(editor)
  return editor
}

function representativeEditor() {
  return openEditor([
    {
      type: 'docHeading',
      attrs: { docxIndex: 0, level: 1 },
      content: [{ type: 'text', text: 'CHƯƠNG 1. TỔNG QUAN' }],
    },
    {
      type: 'docParagraph',
      attrs: { docxIndex: 1 },
      content: [
        { type: 'text', text: 'Mục tiêu ' },
        {
          type: 'docxCitation',
          attrs: { renderedText: '[1]', format: 'WordNative' },
        },
        { type: 'text', text: '.' },
      ],
    },
    {
      type: 'docHeading',
      attrs: { docxIndex: 2, level: 2 },
      content: [{ type: 'text', text: '1.1 Bối cảnh' }],
    },
    {
      type: 'docListItem',
      attrs: { docxIndex: 3, kind: 'bullet' },
      content: [{ type: 'text', text: 'Mục tiêu nghiên cứu' }],
    },
    {
      type: 'docTable',
      attrs: { docxIndex: 4 },
      content: [
        {
          type: 'docTableRow',
          content: [
            { type: 'docTableCell', content: [paragraph('A')] },
            { type: 'docTableCell', content: [paragraph('B')] },
          ],
        },
        {
          type: 'docTableRow',
          content: [
            { type: 'docTableCell', content: [paragraph('C')] },
            { type: 'docTableCell', content: [paragraph('D')] },
          ],
        },
      ],
    },
    {
      type: 'docProtected',
      attrs: { docxIndex: 5, blockType: 'image', previewText: 'Figure 1' },
    },
    {
      type: 'docHeading',
      attrs: { docxIndex: 6, level: 1 },
      content: [{ type: 'text', text: 'CHƯƠNG 2. PHƯƠNG PHÁP' }],
    },
    paragraph('Thiết kế nghiên cứu.', 7),
  ])
}

describe('Document Map MVP', () => {
  it('maps identity, hierarchy, coarse blocks, tables, figures, and native citations', () => {
    const editor = representativeEditor()
    const map = buildDocumentMap(editor, 'doc-1', 7)

    expect(map.documentId).toBe('doc-1')
    expect(map.version).toBe(7)
    expect(
      map.sections.map(({ headingText, level, parentId }) => ({ headingText, level, parentId })),
    ).toEqual([
      { headingText: 'CHƯƠNG 1. TỔNG QUAN', level: 1, parentId: undefined },
      { headingText: '1.1 Bối cảnh', level: 2, parentId: 'section:b0' },
      { headingText: 'CHƯƠNG 2. PHƯƠNG PHÁP', level: 1, parentId: undefined },
    ])
    expect(map.sections[0]?.blockIds).toEqual(['b0', 'b1', 'b2', 'b3', 'b4', 'b5'])
    expect(map.sections[1]?.blockIds).toEqual(['b2', 'b3', 'b4', 'b5'])
    expect(map.sections[2]?.blockIds).toEqual(['b6', 'b7'])

    expect(map.blocks.map(({ id, kind, ordinal }) => ({ id, kind, ordinal }))).toEqual([
      { id: 'b0', kind: 'heading', ordinal: 0 },
      { id: 'b1', kind: 'paragraph', ordinal: 1 },
      { id: 'b2', kind: 'heading', ordinal: 2 },
      { id: 'b3', kind: 'listItem', ordinal: 3 },
      { id: 'b4', kind: 'table', ordinal: 4 },
      { id: 'b5', kind: 'figure', ordinal: 5 },
      { id: 'b6', kind: 'heading', ordinal: 6 },
      { id: 'b7', kind: 'paragraph', ordinal: 7 },
    ])
    expect(map.blocks[1]?.text).toBe('Mục tiêu [1].')
    expect(map.blocks[1]?.locator).toMatchObject({
      documentId: 'doc-1',
      version: 7,
      blockId: 'b1',
      blockOrdinal: 1,
      docxIndex: 1,
      sectionId: 'section:b0',
    })
    expect(map.tables).toEqual([expect.objectContaining({ id: 'b4', rowCount: 2, columnCount: 2 })])
    expect(map.figures).toEqual([
      expect.objectContaining({ id: 'b5', figureType: 'image', caption: 'Figure 1' }),
    ])
    expect(map.citations).toEqual([
      expect.objectContaining({
        id: 'b1:citation:0',
        text: '[1]',
        start: 9,
        end: 12,
        format: 'WordNative',
      }),
    ])
    expect(map.references).toEqual([])
    expect(map.blocks.filter(({ kind }) => kind === 'table')).toHaveLength(1)
    expect(JSON.stringify(map)).not.toContain('docTable')
    expect(JSON.stringify(map)).not.toContain('attrs')
  })

  it('handles empty optional structures and uses an ordinal fallback for new blocks', () => {
    const editor = openEditor([paragraph()])
    const map = buildDocumentMap(editor, 'empty-doc', 0)

    expect(map.sections).toEqual([])
    expect(map.tables).toEqual([])
    expect(map.figures).toEqual([])
    expect(map.citations).toEqual([])
    expect(map.references).toEqual([])
    expect(map.blocks[0]).toMatchObject({ id: 'block-0', kind: 'paragraph', text: '' })
  })

  it('rebuilds deterministically and detects a changed active document version', () => {
    const editor = representativeEditor()
    const first = buildDocumentMap(editor, 'doc-1', 7)
    const second = buildDocumentMap(editor, 'doc-1', 7)
    const changed = buildDocumentMap(editor, 'doc-1', 8)

    expect(second).toEqual(first)
    expect(isDocumentMapCurrent(first, 'doc-1', 7)).toBe(true)
    expect(isDocumentMapStale(first, 'doc-1', 8)).toBe(true)
    expect(changed.version).toBe(8)
    expect(changed.blocks[0]?.locator.version).toBe(8)
  })

  it('builds a map from the existing realistic multi-section DOCX fixture', async () => {
    const bytes = await readFile(
      join(process.cwd(), 'tests', 'pagination-corpus', 'docx', '08-multi-section-paper.docx'),
    )
    const parsed = await parseDocx(bytes)
    const editor = openEditor(
      (blocksToPmDoc(parsed.blocks).content ?? []) as unknown as Record<string, unknown>[],
    )
    const map = buildDocumentMap(editor, 'fixture-08-multi-section-paper', 0)
    const visibleBlockCount = parsed.blocks.filter((block) => !block.hidden).length

    expect(map.blocks.length).toBe(visibleBlockCount)
    expect(map.blocks.length).toBeGreaterThan(0)
    expect(map.sections.length).toBeGreaterThan(0)
    expect(
      map.blocks.every((block) => block.locator.documentId === 'fixture-08-multi-section-paper'),
    ).toBe(true)
    console.info(
      '[document-map fixture dogfood]',
      JSON.stringify({
        fixture: '08-multi-section-paper.docx',
        sections: map.sections.length,
        blocks: map.blocks.length,
        tables: map.tables.length,
        figures: map.figures.length,
        citations: map.citations.length,
        references: map.references.length,
        sampleSections: map.sections.slice(0, 3).map(({ headingText, level, parentId }) => ({
          headingText,
          level,
          parentId,
        })),
        sampleLocators: map.blocks.slice(0, 3).map(({ id, locator }) => ({ id, locator })),
        unsupportedBlocks: map.blocks.filter(({ kind }) => kind === 'other').length,
      }),
    )
  })
})
