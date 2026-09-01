import type { Editor } from '@tiptap/core'
import { TextSelection } from '@tiptap/pm/state'
import type { ManuscriptReviewLocator } from '@genoffice/9profs-core'

function blockId(node: { attrs?: Record<string, unknown> }, ordinal: number): string {
  const docxIndex = node.attrs?.docxIndex
  return typeof docxIndex === 'number' && Number.isInteger(docxIndex)
    ? `b${docxIndex}`
    : `block-${ordinal}`
}

/** Navigate only by the validated DocumentMap identity; never search finding text. */
export function navigateToManuscriptReviewLocation(
  editor: Pick<Editor, 'state' | 'view' | 'commands'>,
  locator: ManuscriptReviewLocator,
): boolean {
  const targets: Array<{ pos: number; nodeSize: number }> = []

  editor.state.doc.forEach((node, pos, ordinal) => {
    const exactId = blockId(node, ordinal) === locator.blockId
    const exactDocxIndex =
      locator.docxIndex !== null &&
      locator.docxIndex !== undefined &&
      node.attrs?.docxIndex === locator.docxIndex
    if (targets.length === 0 && (exactId || exactDocxIndex)) {
      targets.push({ pos, nodeSize: node.nodeSize })
    }
  })

  const target = targets[0]
  if (!target) return false

  try {
    const from = target.pos + 1
    const to = Math.max(from, target.pos + Math.max(1, target.nodeSize - 1))
    editor.view.dispatch(
      editor.state.tr
        .setSelection(TextSelection.create(editor.state.doc, from, to))
        .scrollIntoView(),
    )
    editor.commands.focus()
    return true
  } catch {
    return false
  }
}
