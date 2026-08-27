import { afterEach, describe, expect, it } from 'vitest'
import { Editor } from '@tiptap/core'
import { NodeSelection } from '@tiptap/pm/state'
import { parseDocx, saveDocx, type DocxCitation, type Run } from '@genoffice/docx-engine'
import { buildDocx } from '../../../packages/docx-engine/tests/helpers/build-docx'
import {
  blocksToPmDoc,
  extractDocxCitationsFromPmDoc,
  inlineToRuns,
  pmDocToSavePlan,
  type PmNode,
} from '../src/renderer/editor/convert'
import { editorExtensions } from '../src/renderer/editor/extensions'
import { buildDocumentContext } from '../src/renderer/ai/protocol'

const editors = new Set<Editor>()

afterEach(() => {
  for (const editor of editors) editor.destroy()
  editors.clear()
})

const citation: DocxCitation = {
  format: 'Zotero',
  renderedText: '[12,13]',
  instruction: ' ADDIN ZOTERO_ITEM CSL_CITATION {"citationItems":[{"id":12},{"id":13}]}',
  targets: [
    { ordinal: 1, referenceKey: '12', itemId: '12' },
    { ordinal: 2, referenceKey: '13', itemId: '13' },
  ],
  originalXml:
    '<w:r><w:fldChar w:fldCharType="begin"/></w:r>' +
    '<w:r><w:instrText xml:space="preserve"> ADDIN ZOTERO_ITEM CSL_CITATION {"citationItems":[{"id":12},{"id":13}]}</w:instrText></w:r>' +
    '<w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>[12,13]</w:t></w:r>' +
    '<w:r><w:fldChar w:fldCharType="end"/></w:r>',
}

function sourceBlock(): never {
  const run: Run = { text: 'Drug A works ' }
  const citationRun: Run = { text: citation.renderedText, citation }
  const tail: Run = { text: ' in adults.' }
  return {
    id: 'b7',
    type: 'paragraph',
    docxIndex: 7,
    originalXml: null,
    runs: [run, citationRun, tail],
  } as never
}

function editorForCitation(): Editor {
  const editor = new Editor({
    element: document.createElement('div'),
    extensions: editorExtensions,
    content: blocksToPmDoc([sourceBlock()] as never) as never,
  })
  editors.add(editor)
  return editor
}

describe('Docs inline citation atom', () => {
  it('round-trips the Run model through PM and exposes a structured inventory', () => {
    const pm = blocksToPmDoc([sourceBlock()] as never) as PmNode
    expect(pm.content?.[0].content?.[1]).toMatchObject({
      type: 'docxCitation',
      attrs: { format: 'Zotero', renderedText: '[12,13]' },
    })
    const runs = inlineToRuns(pm.content?.[0].content ?? [])
    expect(runs[1].citation).toEqual(citation)
    expect(extractDocxCitationsFromPmDoc(pm)).toEqual([
      expect.objectContaining({
        blockId: 'b7',
        docxIndex: 7,
        start: 13,
        end: 20,
        targets: citation.targets,
      }),
    ])
  })

  it('uses the rendered marker in AI context and keeps the atom selectable and undoable', () => {
    const editor = editorForCitation()
    expect(buildDocumentContext(editor)).toContain('Drug A works [12,13] in adults.')
    expect(buildDocumentContext(editor)).not.toContain('CSL_CITATION')
    expect(buildDocumentContext(editor)).not.toContain('citationItems')

    let citationPos = -1
    editor.state.doc.descendants((node, pos) => {
      if (node.type.name === 'docxCitation') citationPos = pos
    })
    expect(citationPos).toBeGreaterThan(0)
    editor.commands.setNodeSelection(citationPos)
    expect(editor.state.selection instanceof NodeSelection).toBe(true)
    expect((editor.state.selection as NodeSelection).node.type.name).toBe('docxCitation')
    editor.commands.deleteSelection()
    expect(
      editor.getJSON().content?.[0].content?.some((node) => node.type === 'docxCitation'),
    ).toBe(false)
    expect(editor.commands.undo()).toBe(true)
    expect(
      editor.getJSON().content?.[0].content?.some((node) => node.type === 'docxCitation'),
    ).toBe(true)
  })

  it('saves an edited surrounding paragraph through the normal PM save plan and reparses the atom', async () => {
    const sourceXml =
      '<w:p><w:r><w:t xml:space="preserve">Drug A works </w:t></w:r>' +
      citation.originalXml +
      '<w:r><w:t xml:space="preserve"> in adults.</w:t></w:r></w:p>'
    const source = await parseDocx(await buildDocx({ bodyXml: sourceXml }))
    const editor = new Editor({
      element: document.createElement('div'),
      extensions: editorExtensions,
      content: blocksToPmDoc(source.blocks) as never,
    })
    editors.add(editor)

    let citationPos = -1
    editor.state.doc.descendants((node, pos) => {
      if (node.type.name === 'docxCitation') citationPos = pos
    })
    expect(citationPos).toBeGreaterThan(0)
    editor.commands.insertContentAt(citationPos, 'well ')
    const saved = await saveDocx(
      source,
      pmDocToSavePlan(editor.getJSON() as PmNode, source.blocks).saveBlocks,
    )
    const reparsed = await parseDocx(saved)
    expect(reparsed.blocks[0].runs?.map((run) => run.text).join('')).toBe(
      'Drug A works well [12,13] in adults.',
    )
    expect(reparsed.blocks[0].runs?.find((run) => run.citation)?.citation).toEqual(
      source.blocks[0].runs?.find((run) => run.citation)?.citation,
    )
  })
})
