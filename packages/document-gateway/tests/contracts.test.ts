import { describe, expect, it } from 'vitest'
import type {
  ApprovedDocumentChangeSet,
  DocumentAuthority,
  DocumentChange,
  DocumentId,
  ProposedDocumentChangeSet,
  RejectedDocumentChangeSet,
} from '../src'

const documentId: DocumentId = 'document-1'
const activeAuthority: Extract<DocumentAuthority, { kind: 'genoffice-active' }> = {
  kind: 'genoffice-active',
  documentId,
  writeAuthority: 'genoffice',
}
const detachedAuthority: DocumentAuthority = {
  kind: 'detached',
  documentId,
  writeAuthority: 'unowned',
}
const inspectionAuthority: DocumentAuthority = {
  kind: 'inspection',
  documentId,
  writeAuthority: 'none',
}
const changes: readonly DocumentChange[] = [
  { type: 'generic-intent', payload: { value: 'kept generic' } },
]

const proposed: ProposedDocumentChangeSet = {
  id: 'changes-1',
  target: activeAuthority,
  changes,
  status: 'proposed',
}
const approved: ApprovedDocumentChangeSet = {
  id: proposed.id,
  target: activeAuthority,
  changes: proposed.changes,
  status: 'approved',
  approval: { approvedBy: 'reviewer-1', approvedAt: '2026-08-23T00:00:00Z' },
}
const rejected: RejectedDocumentChangeSet = {
  ...proposed,
  status: 'rejected',
  rejection: {
    rejectedBy: 'reviewer-1',
    rejectedAt: '2026-08-23T00:01:00Z',
    reason: 'Needs review',
  },
}

describe('document gateway contracts', () => {
  it('distinguishes active, detached, and inspection authority', () => {
    expect([activeAuthority.kind, detachedAuthority.kind, inspectionAuthority.kind]).toEqual([
      'genoffice-active',
      'detached',
      'inspection',
    ])
    expect(activeAuthority.writeAuthority).toBe('genoffice')
    expect(detachedAuthority.writeAuthority).toBe('unowned')
    expect(inspectionAuthority.writeAuthority).toBe('none')
  })

  it('keeps proposal approval state explicit', () => {
    expect(proposed.status).toBe('proposed')
    expect(approved.status).toBe('approved')
    expect(approved.approval.approvedBy).toBe('reviewer-1')
    expect(rejected.status).toBe('rejected')
    expect(rejected.rejection.reason).toBe('Needs review')
    expect(proposed.changes[0]).toEqual(changes[0])
  })
})
