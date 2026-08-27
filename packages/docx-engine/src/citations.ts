import type { Block, ParsedDoc, Run, SourceInfo, TableModel } from './types'

export type DocxCitationFormat = 'WordNative' | 'Zotero' | 'UnknownStructured'

export interface DocxCitationSourceMetadata {
  tag: string
  title: string
  author: string
  year: string
}

export interface DocxCitationTarget {
  ordinal: number
  referenceKey: string
  itemId?: string
  citedLocator?: string
  citedLabel?: string
  prefix?: string
  suffix?: string
  suppressAuthor?: boolean
  uris?: string[]
  source?: DocxCitationSourceMetadata
}

/** Document-format citation payload. Not a Research-domain identity. */
export interface DocxCitation {
  format: DocxCitationFormat
  renderedText: string
  instruction: string
  targets: DocxCitationTarget[]
  /** Exact field span, including an inline SDT shell when one wraps only this field. */
  originalXml: string
}

export interface DocxCitationOccurrenceDescriptor {
  format: DocxCitationFormat
  renderedText: string
  /** Current parsed block identity, normally `b${docxIndex}`. */
  blockId: string
  docxIndex: number | null
  /** Unicode code-point offsets in visible paragraph text. */
  start: number
  end: number
  targets: DocxCitationTarget[]
}

export interface DocxCitationFieldSpan {
  originalXml: string
  instruction: string
  renderedText: string
  hasSeparate: boolean
  nested: boolean
  /** False when the field shares a run with visible content and is unsafe to atomize. */
  safe: boolean
}

export interface DocxCitationFieldScan {
  fields: DocxCitationFieldSpan[]
  balanced: boolean
  oversized: boolean
}

export const DOCX_CITATION_LIMITS = {
  fieldXml: 256 * 1024,
  instruction: 32 * 1024,
  zoteroJson: 64 * 1024,
  targets: 64,
  string: 4096,
  uris: 16,
} as const

function decodeXml(text: string): string {
  return text
    .replace(/&#(?:x([0-9a-f]+)|([0-9]+));/gi, (entity, hex: string, decimal: string) => {
      const value = parseInt(hex ?? decimal ?? '', hex ? 16 : 10)
      return Number.isSafeInteger(value) && value >= 0 && value <= 0x10ffff
        ? String.fromCodePoint(value)
        : entity
    })
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&apos;/g, "'")
    .replace(/&amp;/g, '&')
}

function attr(tag: string, name: string): string | undefined {
  return new RegExp(`\\s${name}="([^"]*)"`).exec(tag)?.[1]
}

function boundedString(value: unknown): string | undefined {
  if (typeof value !== 'string' && typeof value !== 'number') return undefined
  const text = String(value)
  return text.length > 0 && text.length <= DOCX_CITATION_LIMITS.string ? text : undefined
}

function validItemId(value: unknown): string | undefined {
  if (typeof value === 'number') {
    return Number.isSafeInteger(value) && value > 0 ? String(value) : undefined
  }
  return typeof value === 'string' && /^[^\\s]{1,4096}$/.test(value) ? value : undefined
}

function urisOf(value: unknown): string[] | undefined {
  if (!Array.isArray(value) || value.length > DOCX_CITATION_LIMITS.uris) return undefined
  const uris = value.map(boundedString)
  return uris.every(Boolean) ? (uris as string[]) : undefined
}

function fieldInfo(
  xml: string,
  hasSeparate: boolean,
  nested: boolean,
  safe: boolean,
): DocxCitationFieldSpan | null {
  if (xml.length > DOCX_CITATION_LIMITS.fieldXml) return null
  let instruction = ''
  for (const match of xml.matchAll(/<w:instrText(?:\s[^>]*)?>([\s\S]*?)<\/w:instrText>/g)) {
    instruction += decodeXml(match[1])
    if (instruction.length > DOCX_CITATION_LIMITS.instruction) return null
  }
  const separate = xml.match(
    /<w:fldChar(?:\s[^>]*)?\bw:fldCharType="separate"[^>]*(?:\/>|>[\s\S]*?<\/w:fldChar>)/,
  )
  let renderedText = ''
  if (separate) {
    const tail = xml.slice((separate.index ?? 0) + separate[0].length)
    for (const match of tail.matchAll(
      /<w:t(?:\s[^>]*)?>([\s\S]*?)<\/w:t>|<w:delText(?:\s[^>]*)?>([\s\S]*?)<\/w:delText>|<w:tab\s*\/>|<w:br(?:\s[^>]*)?\/>/g,
    )) {
      if (match[0].startsWith('<w:tab')) renderedText += '\t'
      else if (match[0].startsWith('<w:br')) renderedText += '\n'
      else renderedText += decodeXml(match[1] ?? match[2] ?? '')
      if (renderedText.length > DOCX_CITATION_LIMITS.string) return null
    }
  }
  return { originalXml: xml, instruction, renderedText, hasSeparate, nested, safe }
}

function enclosingRunStart(xml: string, index: number): number {
  const runs = [...xml.slice(0, index).matchAll(/<w:r(?:\s[^>]*)?>/g)]
  return runs.at(-1)?.index ?? index
}

function runContainsOnlyFieldChar(xml: string, index: number): boolean {
  const start = enclosingRunStart(xml, index)
  const openEnd = xml.indexOf('>', start)
  const close = xml.indexOf('</w:r>', openEnd + 1)
  if (openEnd < 0 || close < 0 || index < openEnd || index > close) return false
  const body = xml.slice(openEnd + 1, close).replace(/<w:rPr(?:\s[^>]*)?>[\s\S]*?<\/w:rPr>/g, '')
  const fieldChars = [
    ...body.matchAll(/<w:fldChar(?:\s[^>]*)?\/>|<w:fldChar(?:\s[^>]*)?>[\s\S]*?<\/w:fldChar>/g),
  ]
  if (fieldChars.length !== 1) return false
  return body.replace(fieldChars[0][0], '').trim() === ''
}

function insideUnsupportedWrapper(xml: string, index: number): boolean {
  for (const name of ['w:hyperlink', 'w:ins', 'w:del', 'w:moveFrom', 'w:moveTo', 'w:customXml']) {
    const open =
      [...xml.slice(0, index).matchAll(new RegExp(`<${name}(?=\\s|>)`, 'g'))].at(-1)?.index ?? -1
    const close = xml.lastIndexOf(`</${name}>`, index)
    if (open > close) return true
  }
  return false
}

/**
 * Scan complete field spans in one paragraph. Only outer fields are returned;
 * nested or unbalanced structures remain unsafe for inline editing.
 */
export function citationFieldSpans(xml: string): DocxCitationFieldScan {
  const fields: DocxCitationFieldSpan[] = []
  const stack: Array<{
    start: number
    nested: boolean
    hasSeparate: boolean
    safe: boolean
    sdtStart?: number
  }> = []
  const sdtStarts: number[] = []
  let balanced = true
  let oversized = false
  const tagRe =
    /<(\/)([A-Za-z0-9:._-]+)((?:"[^"]*"|'[^']*'|[^"'>])*)>|<([A-Za-z0-9:._-]+)((?:"[^"]*"|'[^']*'|[^"'>])*)>/g
  for (const match of xml.matchAll(tagRe)) {
    const closing = !!match[1]
    const name = match[2] ?? match[4]
    const body = match[3] ?? match[5] ?? ''
    const selfClosing = !closing && body.trimEnd().endsWith('/')
    if (closing) {
      if (name === 'w:sdt') {
        if (stack.length > 0) balanced = false
        sdtStarts.pop()
      }
      continue
    }
    if (name === 'w:sdt' && !selfClosing) sdtStarts.push(match.index)
    if (name !== 'w:fldChar') continue
    const type = attr(match[0], 'w:fldCharType')
    if (type === 'begin') {
      if (stack.length > 0) stack[0].nested = true
      stack.push({
        start:
          stack.length === 0
            ? (sdtStarts[sdtStarts.length - 1] ?? enclosingRunStart(xml, match.index))
            : match.index,
        nested: false,
        hasSeparate: false,
        safe:
          runContainsOnlyFieldChar(xml, match.index) && !insideUnsupportedWrapper(xml, match.index),
        ...(stack.length === 0 && sdtStarts.length > 0
          ? { sdtStart: sdtStarts[sdtStarts.length - 1] }
          : {}),
      })
    } else if (type === 'separate') {
      if (stack.length !== 1 || stack[0].hasSeparate) balanced = false
      else stack[0].hasSeparate = true
    } else if (type === 'end') {
      const entry = stack.pop()
      if (!entry) {
        balanced = false
        continue
      }
      if (stack.length > 0) {
        stack[0].nested = true
        continue
      }
      let end = match.index + match[0].length
      if (entry.sdtStart !== undefined) {
        const sdtEnd = xml.indexOf('</w:sdt>', end)
        if (sdtEnd === -1) {
          balanced = false
          continue
        }
        end = sdtEnd + '</w:sdt>'.length
      } else {
        const runEnd = xml.indexOf('</w:r>', end)
        if (runEnd === -1) {
          balanced = false
          continue
        }
        end = runEnd + '</w:r>'.length
      }
      const safe =
        entry.safe &&
        runContainsOnlyFieldChar(xml, match.index) &&
        !insideUnsupportedWrapper(xml, match.index)
      if (end - entry.start > DOCX_CITATION_LIMITS.fieldXml) {
        oversized = true
        continue
      }
      const info = fieldInfo(xml.slice(entry.start, end), entry.hasSeparate, entry.nested, safe)
      if (!info) oversized = true
      else fields.push(info)
    }
  }
  if (stack.length > 0 || sdtStarts.length > 0) balanced = false
  return { fields, balanced, oversized }
}

function sourceMetadata(source: SourceInfo | undefined): DocxCitationSourceMetadata | undefined {
  if (!source) return undefined
  if (
    [source.tag, source.title, source.author, source.year].some(
      (value) => value.length > DOCX_CITATION_LIMITS.string,
    )
  ) {
    return undefined
  }
  return { tag: source.tag, title: source.title, author: source.author, year: source.year }
}

function unknownStructured(instruction: string): boolean {
  return /^\s*ADDIN\s+(?:EN\.CITE|MENDELEY|CITAVI)\b/i.test(instruction)
}

function parseZoteroCitation(
  instruction: string,
  renderedText: string,
  originalXml: string,
): DocxCitation | null {
  const prefix = /^\s*ADDIN\s+ZOTERO_ITEM\s+CSL_CITATION\b/i.exec(instruction)
  if (!prefix) return null
  const json = instruction.slice(prefix[0].length).trim()
  if (!json || json.length > DOCX_CITATION_LIMITS.zoteroJson) return null
  let value: unknown
  try {
    value = JSON.parse(json)
  } catch {
    return null
  }
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null
  const root = value as Record<string, unknown>
  const items = root.citationItems
  if (!Array.isArray(items) || items.length === 0 || items.length > DOCX_CITATION_LIMITS.targets) {
    return null
  }
  const citationId = boundedString(root.citationID)
  const targets: DocxCitationTarget[] = []
  for (const [index, itemValue] of items.entries()) {
    if (!itemValue || typeof itemValue !== 'object' || Array.isArray(itemValue)) return null
    const item = itemValue as Record<string, unknown>
    const itemId = validItemId(item.id)
    const uris = urisOf(item.uris)
    const fallback = uris?.[0] ?? `zotero:${citationId ?? 'citation'}:${index + 1}`
    if (fallback.length > DOCX_CITATION_LIMITS.string) return null
    const target: DocxCitationTarget = {
      ordinal: index + 1,
      referenceKey: itemId ?? fallback,
      ...(itemId ? { itemId } : {}),
      ...(boundedString(item.locator) ? { citedLocator: boundedString(item.locator) } : {}),
      ...(boundedString(item.label) ? { citedLabel: boundedString(item.label) } : {}),
      ...(boundedString(item.prefix) ? { prefix: boundedString(item.prefix) } : {}),
      ...(boundedString(item.suffix) ? { suffix: boundedString(item.suffix) } : {}),
      ...(typeof item['suppress-author'] === 'boolean'
        ? { suppressAuthor: item['suppress-author'] }
        : typeof item.suppressAuthor === 'boolean'
          ? { suppressAuthor: item.suppressAuthor }
          : {}),
      ...(uris ? { uris } : {}),
    }
    targets.push(target)
  }
  return { format: 'Zotero', renderedText, instruction, targets, originalXml }
}

/** Parse only structured citation formats supported by this phase. */
export function parseCitationInstruction(
  instruction: string,
  renderedText: string,
  originalXml: string,
  sources: readonly SourceInfo[] = [],
): DocxCitation | null {
  if (
    instruction.length > DOCX_CITATION_LIMITS.instruction ||
    renderedText.length > DOCX_CITATION_LIMITS.string ||
    originalXml.length > DOCX_CITATION_LIMITS.fieldXml
  ) {
    return null
  }
  const word = /^\s*CITATION\s+([^\s\\"]+)(?:[\s\S]*)?$/i.exec(instruction)
  if (word) {
    const tag = boundedString(word[1])
    if (!tag) return null
    const source = sources.find((entry) => entry.tag === tag)
    return {
      format: 'WordNative',
      renderedText,
      instruction,
      targets: [
        {
          ordinal: 1,
          referenceKey: tag,
          ...(source ? { source: sourceMetadata(source) } : {}),
        },
      ],
      originalXml,
    }
  }
  const zotero = parseZoteroCitation(instruction, renderedText, originalXml)
  if (zotero) return zotero
  if (unknownStructured(instruction)) {
    return { format: 'UnknownStructured', renderedText, instruction, targets: [], originalXml }
  }
  return null
}

export function parseDocxCitationField(
  field: DocxCitationFieldSpan,
  sources: readonly SourceInfo[] = [],
): DocxCitation | null {
  if (!field.hasSeparate || field.nested) return null
  return parseCitationInstruction(field.instruction, field.renderedText, field.originalXml, sources)
}

export function extractDocxCitations(
  input: Pick<ParsedDoc, 'blocks' | 'sources'> | readonly Block[],
): DocxCitationOccurrenceDescriptor[] {
  const parsed = input as Pick<ParsedDoc, 'blocks' | 'sources'>
  const blocks: readonly Block[] = Array.isArray(input) ? input : parsed.blocks
  const occurrences: DocxCitationOccurrenceDescriptor[] = []
  for (const block of blocks) {
    const blockId = block.id || (block.docxIndex === null ? '' : `b${block.docxIndex}`)
    if (!blockId) continue
    if (block.runs && ['paragraph', 'heading', 'listItem'].includes(block.type)) {
      appendRunCitations(block.runs, blockId, block.docxIndex, occurrences)
    } else if (block.type === 'table' && block.table) {
      appendTableCitations(block.table, blockId, block.docxIndex, occurrences)
    }
  }
  return occurrences
}

function appendRunCitations(
  runs: readonly Run[],
  blockId: string,
  docxIndex: number | null,
  occurrences: DocxCitationOccurrenceDescriptor[],
): void {
  let offset = 0
  for (const run of runs) {
    const visible = run.citation?.renderedText ?? run.text
    if (run.citation) {
      occurrences.push({
        format: run.citation.format,
        renderedText: visible,
        blockId,
        docxIndex,
        start: offset,
        end: offset + [...visible].length,
        targets: run.citation.targets.map((target) => ({ ...target })),
      })
    }
    offset += [...visible].length
  }
}

function appendTableCitations(
  table: TableModel,
  blockId: string,
  docxIndex: number | null,
  occurrences: DocxCitationOccurrenceDescriptor[],
  path = '',
): void {
  for (const [rowIndex, row] of table.rows.entries()) {
    for (const [cellIndex, cell] of row.entries()) {
      const cellId = `${blockId}:cell:${path}${rowIndex}:${cellIndex}`
      for (const [paragraphIndex, paragraph] of (cell.richParas ?? []).entries()) {
        appendRunCitations(paragraph.runs, `${cellId}:p:${paragraphIndex}`, docxIndex, occurrences)
      }
      for (const [nestedIndex, nested] of (cell.nestedTables ?? []).entries()) {
        appendTableCitations(
          nested,
          blockId,
          docxIndex,
          occurrences,
          `${path}${rowIndex}:${cellIndex}:${nestedIndex}/`,
        )
      }
    }
  }
}
