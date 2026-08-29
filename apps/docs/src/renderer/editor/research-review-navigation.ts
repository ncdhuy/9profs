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
  if (typeof size === 'number') return size
  if (nodeTypeName(node) === 'text') return String(jsonNode(node).text ?? '').length
  return children(node).reduce((total, child) => total + pmNodeSize(child), 2)
}

function children(node: PmNode): PmNode[] {
  if (Array.isArray(node.content)) return node.content
  const pmNode = node as unknown as { childCount?: number; child?: (index: number) => PmNode }
  if (!pmNode.child || pmNode.childCount === undefined) return []
  return Array.from({ length: pmNode.childCount }, (_, index) => pmNode.child?.call(node, index)).filter(
    (child): child is PmNode => child !== undefined,
  )
}

type BoundaryKind = 'start' | 'end'
type BoundaryMapping = number | 'unrepresentable' | null

/**
 * Map a canonical boundary inside one inline representation to a PM position.
 * Rendered text owned by a non-text node is selectable only as the complete
 * atom, never as fabricated character positions inside that atom.
 */
function mapCanonicalBoundaryIntoInlineNode(
  inlineNode: PmNode,
  canonicalText: string,
  codePointOffsetWithinInline: number,
  pmOffset: number,
): number | null {
  const canonicalLength = codePointLength(canonicalText)
  if (
    !Number.isInteger(codePointOffsetWithinInline) ||
    codePointOffsetWithinInline < 0 ||
    codePointOffsetWithinInline > canonicalLength
  ) {
    return null
  }

  if (nodeTypeName(inlineNode) === 'text') {
    const prefix = Array.from(canonicalText).slice(0, codePointOffsetWithinInline).join('')
    return pmOffset + utf16Length(prefix)
  }

  if (codePointOffsetWithinInline === 0) return pmOffset
  if (codePointOffsetWithinInline === canonicalLength) {
    return pmOffset + pmNodeSize(inlineNode)
  }
  return null
}

/** Resolve one canonical block boundary without entering non-text inline nodes. */
function mapCanonicalBoundary(
  inlines: PmNode[],
  wantedOffset: number,
  contentStart: number,
  kind: BoundaryKind,
): BoundaryMapping {
  if (!Number.isInteger(wantedOffset) || wantedOffset < 0) return null

  let canonicalOffset = 0
  let pmOffset = contentStart
  let previousNonEmptyEnd: number | null = null

  for (const inline of inlines) {
    const canonicalText = inlineText(inline)
    const canonicalLength = codePointLength(canonicalText)
    const nextPmOffset = pmOffset + pmNodeSize(inline)

    // Empty atoms do not consume canonical text. Keep advancing through their
    // real PM sizes so a start boundary after them lands after the atoms.
    if (canonicalLength === 0) {
      pmOffset = nextPmOffset
      continue
    }

    const nextCanonicalOffset = canonicalOffset + canonicalLength
    if (wantedOffset < canonicalOffset) return null

    if (wantedOffset === canonicalOffset) {
      if (kind === 'start') {
        const position = mapCanonicalBoundaryIntoInlineNode(inline, canonicalText, 0, pmOffset)
        return position
      }
      return previousNonEmptyEnd ?? contentStart
    }

    if (wantedOffset < nextCanonicalOffset) {
      const position = mapCanonicalBoundaryIntoInlineNode(
        inline,
        canonicalText,
        wantedOffset - canonicalOffset,
        pmOffset,
      )
      return position === null ? 'unrepresentable' : position
    }

    previousNonEmptyEnd = nextPmOffset
    canonicalOffset = nextCanonicalOffset
    pmOffset = nextPmOffset
  }

  if (wantedOffset !== canonicalOffset) return null
  return kind === 'start' ? pmOffset : (previousNonEmptyEnd ?? pmOffset)
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
  let invalidMatch = false
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
          const from = mapCanonicalBoundary(children(child), wantedStart, contentStart, 'start')
          const to = mapCanonicalBoundary(children(child), wantedEnd, contentStart, 'end')
          matchCount += 1
          if (
            from === 'unrepresentable' ||
            to === 'unrepresentable' ||
            from === null ||
            to === null ||
            to <= from
          ) {
            invalidMatch = true
          } else if (matchCount === 1) {
            match = { from, to }
          }
        }
      }

      visit(child, childPath, childPos + 1, childDocxIndex)
      basePos += pmNodeSize(child)
    }
  }

  visit(root, '', 0, null)
  return !invalidMatch && matchCount === 1 ? match : null
}
