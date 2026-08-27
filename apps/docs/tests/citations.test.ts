import { afterEach, describe, expect, it } from 'vitest'
import { Editor } from '@tiptap/core'
import { NodeSelection } from '@tiptap/pm/state'
import { parseDocx, saveDocx, type DocxCitation, type Run } from '@genoffice/docx-engine'
import { createCoreTransport } from '@genoffice/9profs-core'
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
import {
  buildManuscriptCitationSyncInput,
  buildManuscriptReferenceCatalogInput,
  buildManuscriptClaimExtractionInput,
} from '../src/renderer/editor/manuscript-citations'

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
    { ordinal: 1, referenceKey: '12', itemId: '12', uris: ['zotero://select/items/12'] },
    { ordinal: 2, referenceKey: '13', itemId: '13' },
  ],
  originalXml:
    '<w:r><w:fldChar w:fldCharType="begin"/></w:r>' +
    '<w:r><w:instrText xml:space="preserve"> ADDIN ZOTERO_ITEM CSL_CITATION {"citationItems":[{"id":12},{"id":13}]}</w:instrText></w:r>' +
    '<w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>[12,13]</w:t></w:r>' +
    '<w:r><w:fldChar w:fldCharType="end"/></w:r>',
}

const wordCitation: DocxCitation = {
  format: 'WordNative',
  renderedText: '[Smith2020]',
  instruction: ' CITATION Smith2020 \\l 1033 ',
  targets: [
    {
      ordinal: 1,
      referenceKey: 'Smith2020',
      source: { tag: 'Smith2020', title: 'Safe title', author: 'Smith', year: '2020' },
    },
  ],
  originalXml: '<w:fldSimple w:instr=" CITATION Smith2020 \\l 1033 "/>',
}

function sourceBlock(selectedCitation: DocxCitation = citation): never {
  const run: Run = { text: 'Drug A works ' }
  const citationRun: Run = { text: selectedCitation.renderedText, citation: selectedCitation }
  const tail: Run = { text: ' in adults.' }
  return {
    id: 'b7',
    type: 'paragraph',
    docxIndex: 7,
    originalXml: null,
    runs: [run, citationRun, tail],
  } as never
}

function editorForCitation(selectedCitation: DocxCitation = citation): Editor {
  const editor = new Editor({
    element: document.createElement('div'),
    extensions: editorExtensions,
    content: blocksToPmDoc([sourceBlock(selectedCitation)] as never) as never,
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

  it('builds the sync payload from the PM inventory with explicit document identity', () => {
    const editor = editorForCitation()
    expect(
      buildManuscriptCitationSyncInput({
        editor,
        documentId: 'doc-1',
        documentVersion: 7,
      }),
    ).toEqual({
      documentId: 'doc-1',
      documentVersion: 7,
      citations: [
        {
          format: 'zotero',
          renderedText: '[12,13]',
          blockId: 'b7',
          start: 13,
          end: 20,
          targets: [
            { ordinal: 1, referenceKey: '12', citedLocator: null },
            { ordinal: 2, referenceKey: '13', citedLocator: null },
          ],
        },
      ],
    })
  })

  it('builds a reference catalog payload from live DOCX metadata and exact sync target IDs', async () => {
    const editor = editorForCitation()
    const input = buildManuscriptReferenceCatalogInput({
      editor,
      activeDocument: { documentId: 'doc-1', version: 7 },
      syncRun: {
        syncRunId: 'sync-1',
        researchCaseId: 'case-1' as never,
        manuscriptSourceId: 'source-1' as never,
        documentId: 'doc-1',
        documentVersion: 7,
        inventoryHash: { algorithm: 'sha256', value: 'hash-1' },
        status: 'completed',
        occurrenceCount: 1,
        createdAtMs: 1,
        completedAtMs: 2,
        failureCode: null,
      },
      syncOccurrences: [
        {
          syncOccurrenceId: 'sync-occurrence-1',
          syncRunId: 'sync-1',
          ordinal: 0,
          citationOccurrenceId: 'citation-occurrence-1',
          documentBlockId: 'b7',
          start: 13,
          end: 20,
          format: 'zotero',
        },
      ],
      syncTargets: [
        {
          syncTargetId: 'sync-target-1',
          syncOccurrenceId: 'sync-occurrence-1',
          documentTargetOrdinal: 1,
          citationTargetId: 'citation-target-12',
        },
        {
          syncTargetId: 'sync-target-2',
          syncOccurrenceId: 'sync-occurrence-1',
          documentTargetOrdinal: 2,
          citationTargetId: 'citation-target-13',
        },
      ],
    })
    expect(input).toEqual({
      documentId: 'doc-1',
      documentVersion: 7,
      citations: [
        {
          citationOccurrenceId: 'citation-occurrence-1',
          blockId: 'b7',
          start: 13,
          end: 20,
          format: 'zotero',
          targets: [
            {
              citationTargetId: 'citation-target-12',
              ordinal: 1,
              referenceKey: '12',
              zotero: { itemId: '12', uris: ['zotero://select/items/12'] },
            },
            {
              citationTargetId: 'citation-target-13',
              ordinal: 2,
              referenceKey: '13',
              zotero: { itemId: '13', uris: [] },
            },
          ],
        },
      ],
    })

    let postedBody: BodyInit | null | undefined
    const transport = createCoreTransport('http://127.0.0.1:39761/', async (_request, init) => {
      postedBody = init?.body
      return {
        ok: true,
        json: async () => ({ success: true, data: { catalogRunId: 'catalog-1' } }),
      }
    })
    await expect(transport.syncManuscriptReferenceCatalog('sync-1', input)).resolves.toEqual({
      catalogRunId: 'catalog-1',
    })
    expect(postedBody).toBe(JSON.stringify(input))
  })

  it('preserves bounded Word source hints without mutating the live editor', () => {
    const editor = editorForCitation(wordCitation)
    const occurrence = extractDocxCitationsFromPmDoc(editor.state.doc.toJSON() as PmNode)[0]
    const before = editor.getJSON()
    const input = buildManuscriptReferenceCatalogInput({
      editor,
      activeDocument: { documentId: 'doc-1', version: 7 },
      syncRun: {
        syncRunId: 'sync-word-1',
        researchCaseId: 'case-1' as never,
        manuscriptSourceId: 'source-1' as never,
        documentId: 'doc-1',
        documentVersion: 7,
        inventoryHash: { algorithm: 'sha256', value: 'hash-word-1' },
        status: 'completed',
        occurrenceCount: 1,
        createdAtMs: 1,
        completedAtMs: 2,
        failureCode: null,
      },
      syncOccurrences: [
        {
          syncOccurrenceId: 'sync-word-occurrence-1',
          syncRunId: 'sync-word-1',
          ordinal: 0,
          citationOccurrenceId: 'citation-word-occurrence-1',
          documentBlockId: occurrence.blockId,
          start: occurrence.start,
          end: occurrence.end,
          format: 'word_native',
        },
      ],
      syncTargets: [
        {
          syncTargetId: 'sync-word-target-1',
          syncOccurrenceId: 'sync-word-occurrence-1',
          documentTargetOrdinal: 1,
          citationTargetId: 'citation-word-target-1',
        },
      ],
    })

    expect(input.citations).toEqual([
      {
        citationOccurrenceId: 'citation-word-occurrence-1',
        blockId: occurrence.blockId,
        start: occurrence.start,
        end: occurrence.end,
        format: 'word_native',
        targets: [
          {
            citationTargetId: 'citation-word-target-1',
            ordinal: 1,
            referenceKey: 'Smith2020',
            wordSource: {
              tag: 'Smith2020',
              title: 'Safe title',
              author: 'Smith',
              year: '2020',
            },
          },
        ],
      },
    ])
    expect(editor.getJSON()).toEqual(before)
  })

  it('builds claim extraction blocks from live PM text and completed sync occurrence IDs', () => {
    const editor = editorForCitation()
    const input = buildManuscriptClaimExtractionInput({
      editor,
      activeDocument: { documentId: 'doc-1', version: 7 },
      syncRun: {
        syncRunId: 'sync-1',
        researchCaseId: 'case-1' as never,
        manuscriptSourceId: 'source-1' as never,
        documentId: 'doc-1',
        documentVersion: 7,
        inventoryHash: { algorithm: 'sha256', value: 'hash-1' },
        status: 'completed',
        occurrenceCount: 1,
        createdAtMs: 1,
        completedAtMs: 2,
        failureCode: null,
      },
      syncOccurrences: [
        {
          syncOccurrenceId: 'sync-occurrence-1',
          syncRunId: 'sync-1',
          ordinal: 0,
          citationOccurrenceId: 'citation-occurrence-1',
          documentBlockId: 'b7',
          start: 13,
          end: 20,
          format: 'zotero',
        },
      ],
    })

    expect(input).toEqual({
      documentId: 'doc-1',
      documentVersion: 7,
      blocks: [
        {
          blockId: 'b7',
          text: 'Drug A works [12,13] in adults.',
          citations: [
            {
              citationOccurrenceId: 'citation-occurrence-1',
              start: 13,
              end: 20,
              renderedText: '[12,13]',
            },
          ],
        },
      ],
    })
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
