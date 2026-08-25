import { afterEach, describe, expect, it } from 'vitest'
import { Editor } from '@tiptap/core'
import { createGenOfficeDocsAdapter, DOCS_COMMAND_ENVELOPE } from '@genoffice/genoffice-adapter'
import { editorExtensions } from '../src/renderer/editor/extensions'
import { executeCommands, type Command } from '../src/renderer/ai/commands'
import { buildDocumentContext } from '../src/renderer/ai/protocol'
import { collectRevisions } from '../src/renderer/editor/revisions'

const editors = new Set<Editor>()

afterEach(() => {
  for (const editor of editors) editor.destroy()
  editors.clear()
})

function createEditor(): Editor {
  const editor = new Editor({
    element: document.createElement('div'),
    extensions: editorExtensions,
    content: {
      type: 'doc',
      content: [
        {
          type: 'docParagraph',
          attrs: { docxIndex: null },
          content: [{ type: 'text', text: 'active document' }],
        },
      ],
    },
  })
  editors.add(editor)
  return editor
}

function createAdapter(editor: Editor) {
  return createGenOfficeDocsAdapter({
    documentId: 'active-doc-1',
    runtime: {
      subscribeToTransactions(listener) {
        const onTransaction = ({ transaction }: { transaction: Parameters<typeof listener>[0] }) =>
          listener(transaction)
        editor.on('transaction', onTransaction)
        return () => editor.off('transaction', onTransaction)
      },
      buildDocumentContext: () => buildDocumentContext(editor),
      getSelectionContext: () => {
        const { from, to, empty } = editor.state.selection
        return { from, to, empty }
      },
      executeCommands: (commands, context) =>
        executeCommands(editor, { commands: commands as Command[] }, context),
    },
  })
}

function changeSet(baseVersion: number, commands: readonly unknown[]) {
  return {
    id: 'active-change-1',
    status: 'approved' as const,
    target: {
      kind: 'genoffice-active' as const,
      documentId: 'active-doc-1',
      writeAuthority: 'genoffice' as const,
    },
    baseVersion,
    changes: [{ type: DOCS_COMMAND_ENVELOPE, payload: { commands } }],
    approval: { approvedBy: 'test', approvedAt: '2026-08-26T00:00:00Z' },
  }
}

describe('active DOCX GenOffice adapter integration', () => {
  it('reuses Docs commands, version listener, Track Changes, dirty update, and undo', async () => {
    const editor = createEditor()
    const adapter = createAdapter(editor)
    let updateCount = 0
    editor.on('update', () => updateCount++)

    const initial = await adapter.inspector.inspect({ documentId: 'active-doc-1' })
    expect(initial.authority.kind).toBe('genoffice-active')
    expect(initial.version).toBe(0)
    expect((initial.value as { context: string }).context).toContain('active document')

    editor.commands.insertContentAt(1, ' manual')
    const manual = await adapter.inspector.inspect({ documentId: 'active-doc-1' })
    expect(manual.version).toBe(1)
    const stale = await adapter.mutationGateway.commit(
      changeSet(0, [
        {
          updateTextStyle: {
            target: { blockIndexes: [0] },
            style: { bold: true },
            fields: ['bold'],
          },
        },
      ]),
    )
    expect(stale).toMatchObject({ status: 'conflict', reason: 'stale-version', currentVersion: 1 })
    expect(editor.state.doc.textContent).toContain('manual')

    const result = await adapter.mutationGateway.commit(
      changeSet(1, [
        {
          updateTextStyle: {
            target: { blockIndexes: [0] },
            style: { bold: true },
            fields: ['bold'],
          },
        },
      ]),
    )
    expect(result).toMatchObject({ status: 'applied', previousVersion: 1, newVersion: 2 })
    expect(collectRevisions(editor.state.doc)).toMatchObject([
      { kind: 'rPrChange', author: '9Profs AI' },
    ])
    expect(updateCount).toBeGreaterThan(0)

    editor.commands.undo()
    expect(collectRevisions(editor.state.doc)).toHaveLength(0)
    expect(
      editor.state.doc.firstChild?.firstChild?.marks.some((mark) => mark.type.name === 'bold'),
    ).toBe(false)
    adapter.dispose()
  })

  it('keeps invalid command envelopes atomic and ignores selection-only/no-op transactions', async () => {
    const editor = createEditor()
    const adapter = createAdapter(editor)
    editor.commands.setTextSelection({ from: 1, to: 1 })
    expect((await adapter.inspector.inspect({ documentId: 'active-doc-1' })).version).toBe(0)

    const before = editor.getJSON()
    await expect(
      adapter.mutationGateway.commit(
        changeSet(0, [
          {
            updateTextStyle: {
              target: { blockIndexes: [0] },
              style: { italic: true },
              fields: ['italic'],
            },
          },
          { notACommand: {} },
        ]),
      ),
    ).rejects.toMatchObject({ code: 'invalid-command-envelope' })
    expect(editor.getJSON()).toEqual(before)
    expect((await adapter.inspector.inspect({ documentId: 'active-doc-1' })).version).toBe(0)

    const noOp = await adapter.mutationGateway.commit(
      changeSet(0, [
        {
          updateTextStyle: {
            target: { blockIndexes: [99] },
            style: { italic: true },
            fields: ['italic'],
          },
        },
      ]),
    )
    expect(noOp).toMatchObject({
      status: 'applied',
      previousVersion: 0,
      newVersion: 0,
      changedCount: 0,
    })
    adapter.dispose()
  })
})
