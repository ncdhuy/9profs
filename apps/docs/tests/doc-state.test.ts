import { describe, expect, it } from 'vitest'
import type { ParsedDocFull } from '@genoffice/docx-engine'
import { createDocumentId, withSavedDocumentState, type DocState } from '../src/renderer/doc-state'

const parsed = {} as ParsedDocFull

function activeState(): DocState {
  return {
    parsed,
    documentId: 'active-session-a',
    filePath: null,
    fileName: 'Untitled.docx',
    hash: '',
  }
}

describe('active document session identity', () => {
  it('preserves identity across autosave, Save As, and same-document reparse', () => {
    const initial = activeState()
    const saved = withSavedDocumentState(initial, parsed, 'C:\\docs\\active.docx')
    const autosaved = withSavedDocumentState(saved, parsed, saved.filePath)
    const savedAs = withSavedDocumentState(autosaved, parsed, 'D:\\archive\\active.docx')
    const reparsed = withSavedDocumentState(savedAs, parsed, savedAs.filePath)

    expect(saved.documentId).toBe(initial.documentId)
    expect(autosaved.documentId).toBe(initial.documentId)
    expect(savedAs.documentId).toBe(initial.documentId)
    expect(reparsed.documentId).toBe(initial.documentId)
    expect(savedAs.filePath).toBe('D:\\archive\\active.docx')
  })

  it('provides distinct identities for callers replacing the active document', () => {
    expect(createDocumentId()).not.toBe(createDocumentId())
  })
})
