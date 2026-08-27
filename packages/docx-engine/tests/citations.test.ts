import JSZip from 'jszip'
import { describe, expect, it } from 'vitest'
import {
  extractDocxCitations,
  parseCitationInstruction,
  parseDocx,
  saveDocx,
  type SaveBlock,
} from '../src/index'
import { buildDocx } from './helpers/build-docx'

const XML_DECL = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\r\n'
const SOURCES_XML =
  XML_DECL +
  '<b:Sources xmlns:b="http://schemas.openxmlformats.org/officeDocument/2006/bibliography">' +
  '<b:Source><b:Tag>Smith2020</b:Tag><b:SourceType>JournalArticle</b:SourceType>' +
  '<b:Author><b:Author><b:NameList><b:Person><b:Last>Smith</b:Last>' +
  '<b:First>Jane</b:First></b:Person></b:NameList></b:Author></b:Author>' +
  '<b:Title>Safe treatment</b:Title><b:Year>2020</b:Year></b:Source>' +
  '</b:Sources>'

function field(instruction: string, rendered: string, instructionRuns?: string[]): string {
  const runs = (instructionRuns ?? [instruction])
    .map((part) => `<w:r><w:instrText xml:space="preserve">${part}</w:instrText></w:r>`)
    .join('')
  return (
    '<w:r><w:fldChar w:fldCharType="begin"/></w:r>' +
    runs +
    '<w:r><w:fldChar w:fldCharType="separate"/></w:r>' +
    `<w:r><w:t>${rendered}</w:t></w:r>` +
    '<w:r><w:fldChar w:fldCharType="end"/></w:r>'
  )
}

function paragraph(...parts: string[]): string {
  return `<w:p>${parts.join('')}</w:p>`
}

function textRun(text: string): string {
  const preserve = /^\s|\s$/.test(text) ? ' xml:space="preserve"' : ''
  return `<w:r><w:t${preserve}>${text}</w:t></w:r>`
}

function zoteroField(rendered: string, items: unknown[]): string {
  const instruction =
    ' ADDIN ZOTERO_ITEM CSL_CITATION ' +
    JSON.stringify({ citationID: 'citation-1', citationItems: items })
  return field(instruction, rendered)
}

describe('DOCX citation field model', () => {
  it('parses a Word-native citation and enriches it from b:Sources without changing source identity', async () => {
    const doc = await parseDocx(
      await buildDocx({
        bodyXml: paragraph(
          textRun('Treatment reduced mortality '),
          field(' CITATION Smith2020', '[12]'),
          textRun('.'),
        ),
        extraParts: [
          {
            path: 'customXml/item1.xml',
            xml: SOURCES_XML,
            contentType: 'application/xml',
          },
        ],
      }),
    )
    const run = doc.blocks[0].runs?.[1]
    expect(doc.blocks[0].type).toBe('paragraph')
    expect(doc.sources).toMatchObject([
      { tag: 'Smith2020', title: 'Safe treatment', author: 'Smith, Jane', year: '2020' },
    ])
    expect(run).toMatchObject({
      text: '[12]',
      citation: {
        format: 'WordNative',
        renderedText: '[12]',
        instruction: ' CITATION Smith2020',
        targets: [
          {
            ordinal: 1,
            referenceKey: 'Smith2020',
            source: {
              tag: 'Smith2020',
              title: 'Safe treatment',
              author: 'Smith, Jane',
              year: '2020',
            },
          },
        ],
      },
    })
    expect(extractDocxCitations(doc)).toEqual([
      expect.objectContaining({ blockId: 'b0', start: 28, end: 32, renderedText: '[12]' }),
    ])
  })

  it('reconstructs a split Word field instruction completely', async () => {
    const doc = await parseDocx(
      await buildDocx({
        bodyXml: paragraph(field('unused', '[12]', [' CIT', 'ATION Smith2020 ', '\\l 1033'])),
      }),
    )
    expect(doc.blocks[0].type).toBe('paragraph')
    expect(doc.blocks[0].runs?.[0].citation).toMatchObject({
      format: 'WordNative',
      instruction: ' CITATION Smith2020 \\l 1033',
      targets: [{ referenceKey: 'Smith2020' }],
    })
  })

  it('parses one grouped Zotero field into ordered targets with bounded item metadata', async () => {
    const doc = await parseDocx(
      await buildDocx({
        bodyXml: paragraph(
          textRun('Evidence '),
          zoteroField('[12,13,14]', [
            { id: 12, locator: '4', label: 'page', prefix: 'see', 'suppress-author': true },
            { id: '13', locator: '9' },
            { id: 14 },
          ]),
          textRun('.'),
        ),
      }),
    )
    const citation = doc.blocks[0].runs?.[1].citation
    expect(citation).toMatchObject({
      format: 'Zotero',
      renderedText: '[12,13,14]',
      targets: [
        {
          ordinal: 1,
          referenceKey: '12',
          itemId: '12',
          citedLocator: '4',
          citedLabel: 'page',
          prefix: 'see',
          suppressAuthor: true,
        },
        { ordinal: 2, referenceKey: '13', itemId: '13', citedLocator: '9' },
        { ordinal: 3, referenceKey: '14', itemId: '14' },
      ],
    })
    expect(extractDocxCitations(doc)).toEqual([
      expect.objectContaining({
        renderedText: '[12,13,14]',
        start: 9,
        end: 19,
        targets: citation?.targets,
      }),
    ])
    const saved = await saveDocx(doc, [
      { kind: 'generated', block: { type: 'paragraph', runs: doc.blocks[0].runs ?? [] } },
    ])
    const savedXml = await (await JSZip.loadAsync(saved)).file('word/document.xml')!.async('string')
    expect(savedXml).toContain(citation!.originalXml)
  })

  it('keeps adjacent native fields as separate occurrences in source order', async () => {
    const doc = await parseDocx(
      await buildDocx({
        bodyXml: paragraph(
          field(' CITATION Smith2020', '[12]'),
          field(' CITATION Jones2021', '[13]'),
        ),
      }),
    )
    expect(
      extractDocxCitations(doc).map((occurrence) => occurrence.targets[0].referenceKey),
    ).toEqual(['Smith2020', 'Jones2021'])
  })

  it('keeps ordinary paragraphs, malformed fields, and unsupported managers safe', async () => {
    const malformed =
      '<w:r><w:fldChar w:fldCharType="begin"/></w:r>' +
      '<w:r><w:instrText> CITATION Smith2020</w:instrText></w:r>' +
      '<w:r><w:fldChar w:fldCharType="separate"/></w:r>' +
      '<w:r><w:t>[12]</w:t></w:r>'
    const unsupported = field(' ADDIN EN.CITE {123}', '[13]')
    const doc = await parseDocx(
      await buildDocx({
        bodyXml: paragraph(textRun('Plain text.')) + paragraph(malformed) + paragraph(unsupported),
      }),
    )
    const paragraphBlocks = doc.blocks.filter((block) => block.originalXml?.startsWith('<w:p'))
    expect(paragraphBlocks.map((block) => block.type)).toEqual([
      'paragraph',
      'passthrough',
      'passthrough',
    ])
    expect(paragraphBlocks[1].originalXml).toContain('CITATION Smith2020')
    expect(paragraphBlocks[2].originalXml).toContain('ADDIN EN.CITE')
    expect(extractDocxCitations(doc)).toEqual([])
  })

  it('keeps a citation that shares a run with visible text on the protected path', async () => {
    const unsafe = await parseDocx(
      await buildDocx({
        bodyXml: paragraph(
          '<w:r><w:t>before</w:t><w:fldChar w:fldCharType="begin"/></w:r>',
          '<w:r><w:instrText xml:space="preserve"> CITATION Smith2020</w:instrText></w:r>',
          '<w:r><w:fldChar w:fldCharType="separate"/></w:r>',
          '<w:r><w:t>[12]</w:t></w:r>',
          '<w:r><w:fldChar w:fldCharType="end"/><w:t>after</w:t></w:r>',
        ),
      }),
    )
    expect(unsafe.blocks[0].type).toBe('passthrough')
    expect(unsafe.blocks[0].originalXml).toContain('CITATION Smith2020')
  })

  it('keeps citations inside revision wrappers protected so wrapper semantics survive', async () => {
    const wrapped = await parseDocx(
      await buildDocx({
        bodyXml: paragraph(
          '<w:ins w:author="Author" w:id="1">' + field(' CITATION Smith2020', '[12]') + '</w:ins>',
        ),
      }),
    )
    expect(wrapped.blocks[0].type).toBe('passthrough')
    expect(wrapped.blocks[0].originalXml).toContain('<w:ins')
    expect(wrapped.blocks[0].originalXml).toContain('CITATION Smith2020')
  })

  it('preserves the field payload when surrounding prose is edited and reparsed', async () => {
    const bytes = await buildDocx({
      bodyXml: paragraph(
        textRun('Drug A works '),
        field(' CITATION Smith2020', '[12]'),
        textRun(' in adults.'),
      ),
    })
    const doc = await parseDocx(bytes)
    const originalCitation = doc.blocks[0].runs?.find((run) => run.citation)?.citation
    expect(originalCitation).toBeDefined()
    const saved = await saveDocx(doc, [
      {
        kind: 'generated',
        block: {
          type: 'paragraph',
          runs: [
            { text: 'Drug A works well ' },
            { text: originalCitation!.renderedText, citation: originalCitation },
            { text: ' in selected adults.' },
          ],
        },
      },
    ])
    const zip = await JSZip.loadAsync(saved)
    const savedXml = await zip.file('word/document.xml')!.async('string')
    expect(savedXml).toContain(originalCitation!.originalXml)
    expect(savedXml).toContain('Drug A works well')
    expect(savedXml).toContain('in selected adults.')
    const reparsed = await parseDocx(saved)
    expect(reparsed.blocks[0].type).toBe('paragraph')
    expect(reparsed.blocks[0].runs?.find((run) => run.citation)?.citation).toMatchObject({
      instruction: originalCitation!.instruction,
      targets: [{ referenceKey: 'Smith2020' }],
      originalXml: originalCitation!.originalXml,
    })
  })

  it('keeps an inline SDT shell around a supported citation intact', async () => {
    const sdt =
      '<w:sdt><w:sdtPr><w:tag w:val="citation"/><w:citation/></w:sdtPr><w:sdtContent>' +
      field(' CITATION Smith2020', '[12]') +
      '</w:sdtContent></w:sdt>'
    const doc = await parseDocx(
      await buildDocx({ bodyXml: paragraph(textRun('Before '), sdt, textRun(' after')) }),
    )
    const citationRun = doc.blocks[0].runs?.find((run) => run.citation)
    expect(doc.blocks[0].type).toBe('paragraph')
    expect(citationRun?.citation?.originalXml).toContain('<w:sdt>')
    expect(citationRun?.citation?.originalXml).toContain('<w:citation/>')
    const saved = await saveDocx(doc, [
      {
        kind: 'generated',
        block: { type: 'paragraph', runs: [{ text: 'Before ' }, citationRun!, { text: ' after' }] },
      },
    ])
    const xml = await (await JSZip.loadAsync(saved)).file('word/document.xml')!.async('string')
    expect(xml).toContain(citationRun!.citation!.originalXml)
  })

  it('deleting an atom removes the complete field rather than leaving orphan fldChar elements', async () => {
    const doc = await parseDocx(
      await buildDocx({
        bodyXml: paragraph(
          textRun('Before '),
          field(' CITATION Smith2020', '[12]'),
          textRun(' after'),
        ),
      }),
    )
    const saved = await saveDocx(doc, [
      { kind: 'generated', block: { type: 'paragraph', runs: [{ text: 'Before  after' }] } },
    ] satisfies SaveBlock[])
    const xml = await (await JSZip.loadAsync(saved)).file('word/document.xml')!.async('string')
    expect(xml).not.toContain('fldChar')
    expect(xml).toContain('Before  after')
  })

  it('uses visible code-point offsets for Unicode text and never exposes field internals', async () => {
    const doc = await parseDocx(
      await buildDocx({
        bodyXml: paragraph(
          textRun('Trị liệu '),
          field(' CITATION Smith2020', '[12]'),
          textRun(' 😀 hiệu quả.'),
        ),
      }),
    )
    const [occurrence] = extractDocxCitations(doc)
    const visible = 'Trị liệu [12] 😀 hiệu quả.'
    expect(occurrence).toMatchObject({
      start: [...'Trị liệu '].length,
      end: [...'Trị liệu [12]'].length,
    })
    expect([...visible].slice(occurrence.start, occurrence.end).join('')).toBe('[12]')
    expect(JSON.stringify(occurrence)).not.toContain('CITATION')
    expect(JSON.stringify(occurrence)).not.toContain('ZOTERO_ITEM')
    expect(visible).toContain('😀')
  })
})

describe('citation instruction limits', () => {
  it('rejects malformed Zotero JSON and overlarge grouped citations', () => {
    expect(
      parseCitationInstruction(' ADDIN ZOTERO_ITEM CSL_CITATION {bad', '[12]', '<field/>'),
    ).toBeNull()
    expect(
      parseCitationInstruction(
        ' ADDIN ZOTERO_ITEM CSL_CITATION ' +
          JSON.stringify({ citationItems: Array.from({ length: 65 }, (_, id) => ({ id })) }),
        '[12]',
        '<field/>',
      ),
    ).toBeNull()
  })
})
