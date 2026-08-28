import type { Editor } from '@tiptap/core'
import type { CitationReviewItem } from '@genoffice/9profs-core'
import { extractDocxCitationsFromPmDoc, type PmNode } from './convert'

type NavigationItem = Pick<
  CitationReviewItem,
  'documentBlockId' | 'start' | 'end' | 'renderedText' | 'referenceKey' | 'citedLocator'
>

function pmDocFromEditor(editor: Pick<Editor, 'state'>): PmNode {
  const doc = editor.state.doc as unknown as PmNode & { toJSON?: () => PmNode }
  return Array.isArray(doc.content) ? doc : (doc.toJSON?.() ?? doc)
}

function sameNullable(left: string | null | undefined, right: string | null | undefined): boolean {
  return (left ?? null) === (right ?? null)
}

/**
 * Resolve a review item to the exact inline citation atom that produced it.
 *
 * Citation offsets are Unicode code-point offsets in a document-format block;
 * they are deliberately never used as ProseMirror positions here. The live
 * citation extractor and the PM traversal must agree on one unique occurrence,
 * otherwise navigation fails closed instead of guessing.
 */
export function findCitationNodePosition(
  editor: Pick<Editor, 'state'>,
  item: NavigationItem,
): number | null {
  const descriptors = extractDocxCitationsFromPmDoc(pmDocFromEditor(editor))
  const nodes: Array<{ pos: number }> = []
  editor.state.doc.descendants((node, pos) => {
    if (node.type.name === 'docxCitation') nodes.push({ pos })
  })

  if (nodes.length !== descriptors.length) return null

  const matches: number[] = []
  descriptors.forEach((descriptor, index) => {
    if (
      descriptor.blockId !== item.documentBlockId ||
      descriptor.start !== item.start ||
      descriptor.end !== item.end ||
      descriptor.renderedText !== item.renderedText
    ) {
      return
    }
    const targetMatches = descriptor.targets.filter(
      (target) =>
        target.referenceKey === item.referenceKey &&
        sameNullable(target.citedLocator, item.citedLocator),
    )
    if (targetMatches.length === 1) matches.push(index)
  })

  return matches.length === 1 ? nodes[matches[0]].pos : null
}
