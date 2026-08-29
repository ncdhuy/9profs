import type { Editor } from '@tiptap/core'
import type { ManuscriptResearchReviewClaimItem } from '@genoffice/9profs-core'
import { inlineToRuns, type PmNode } from './convert'

type ClaimLocator = Pick<
  ManuscriptResearchReviewClaimItem,
  'documentBlockId' | 'sourceStart' | 'sourceEnd' | 'sourceExcerpt'
>

export interface ManuscriptClaimRange {
  readonly from: number
  readonly to: number
}

function codePointLength(value: string): number {
  return Array.from(value).length
}

function utf16Length(value: string): number {
  return value.length
}

function blockId(docxIndex: number | null, path: string): string {
  if (docxIndex === null) return `pm-block-${path}`
  return path.includes('.') ? `b${docxIndex}:pm-block-${path}` : `b${docxIndex}`
}

function jsonNode(node: PmNode): PmNode {
  const candidate = node as PmNode & { toJSON?: () => PmNode }
  return candidate.toJSON?.() ?? node
}

function nodeTypeName(node: PmNode): string {
  if (typeof node.type === 'string') return node.type
  return (node.type as unknown as { name: string }).name
}

function nodeText(node: PmNode): string {
  const json = jsonNode(node)
  return inlineToRuns(json.content ?? [])
    .map((run) => run.text)
    .join('')
}

function inlineText(node: PmNode): string {
  const json = jsonNode(node)
  if (json.type === 'text') return json.text ?? ''
  return inlineToRuns([json])
    .map((run) => run.text)
    .join('')
}

function pmNodeSize(node: PmNode): number {
  const size = (node as unknown as { nodeSize?: number }).nodeSize
  return size ?? inlineText(node).length + 2
}

function children(node: PmNode): PmNode[] {
  if (Array.isArray(node.content)) return node.content
  const pmNode = node as unknown as { childCount?: number; child?: (index: number) => PmNode }
  if (!pmNode.child || pmNode.childCount === undefined) return []
  return Array.from({ length: pmNode.childCount }, (_, index) => pmNode.child?.call(node, index)).filter(
    (child): child is PmNode => child !== undefined,
  )
}

/**
 * Resolve Core's Unicode code-point block locator to real ProseMirror positions.
 * Any identity or excerpt mismatch fails closed; source offsets are never used
 * as PM positions directly.
 */
export function findManuscriptClaimRange(
  editor: Pick<Editor, 'state'>,
  claim: ClaimLocator,
): ManuscriptClaimRange | null {
  const root = editor.state.doc as unknown as PmNode
  const wantedStart = claim.sourceStart
  const wantedEnd = claim.sourceEnd
  if (
    !Number.isInteger(wantedStart) ||
    !Number.isInteger(wantedEnd) ||
    wantedStart < 0 ||
    wantedEnd <= wantedStart
  ) {
    return null
  }

  let match: ManuscriptClaimRange | null = null
  let matchCount = 0
  const visit = (node: PmNode, path: string, basePos: number, inheritedDocxIndex: number | null) => {
    for (const [index, child] of children(node).entries()) {
      const childPath = path ? `${path}.${index}` : String(index)
      const childPos = basePos
      const rawDocxIndex = child.attrs?.docxIndex
      const childDocxIndex =
        typeof rawDocxIndex === 'number' && Number.isInteger(rawDocxIndex)
          ? rawDocxIndex
          : inheritedDocxIndex
      const isBlock = ['docParagraph', 'docHeading', 'docListItem'].includes(nodeTypeName(child))

      if (isBlock) {
        const text = nodeText(child)
        const id = blockId(childDocxIndex, childPath)
        const excerpt = Array.from(text).slice(wantedStart, wantedEnd).join('')
        if (
          id === claim.documentBlockId &&
          excerpt === claim.sourceExcerpt &&
          wantedEnd <= codePointLength(text)
        ) {
          const contentStart = childPos + 1
          let codePoints = 0
          let from: number | null = null
          let to: number | null = null
          let offset = 0
          for (const inline of children(child)) {
            const text = inlineText(inline)
            const nextCodePoints = codePoints + codePointLength(text)
            const nextOffset = offset + pmNodeSize(inline)
            if (from === null && wantedStart >= codePoints && wantedStart <= nextCodePoints) {
              const within = Array.from(text).slice(0, wantedStart - codePoints).join('')
              from = contentStart + offset + utf16Length(within)
            }
            if (to === null && wantedEnd >= codePoints && wantedEnd <= nextCodePoints) {
              const within = Array.from(text).slice(0, wantedEnd - codePoints).join('')
              to = contentStart + offset + utf16Length(within)
            }
            codePoints = nextCodePoints
            offset = nextOffset
          }
          if (from !== null && to !== null && to > from) {
            matchCount += 1
            if (matchCount === 1) match = { from, to }
          }
        }
      }

      visit(child, childPath, childPos + 1, childDocxIndex)
      basePos += pmNodeSize(child)
    }
  }

  visit(root, '', 0, null)
  return matchCount === 1 ? match : null
}
