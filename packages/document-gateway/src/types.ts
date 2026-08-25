export type DocumentId = string
export type DocumentChangeSetId = string
export type DocumentVersion = number

export type DocumentAuthority =
  | {
      readonly kind: 'genoffice-active'
      readonly documentId: DocumentId
      readonly writeAuthority: 'genoffice'
    }
  | {
      readonly kind: 'detached'
      readonly documentId: DocumentId
      readonly writeAuthority: 'unowned'
    }
  | {
      readonly kind: 'inspection'
      readonly documentId: DocumentId
      readonly writeAuthority: 'none'
    }

export type GenOfficeActiveDocumentAuthority = Extract<
  DocumentAuthority,
  { kind: 'genoffice-active' }
>

export interface DocumentInspectionRequest {
  readonly documentId: DocumentId
  readonly query?: string
}

export interface DocumentInspection {
  readonly documentId: DocumentId
  readonly authority: DocumentAuthority
  readonly version: DocumentVersion
  readonly value: unknown
}

/** Generic read/inspection boundary. It exposes no engine or command model. */
export interface DocumentInspector {
  inspect(request: DocumentInspectionRequest): Promise<DocumentInspection>
}

/** Small, extensible change envelope. Operation details belong to a concrete adapter. */
export interface DocumentChange {
  readonly type: string
  readonly payload?: Readonly<Record<string, unknown>>
}

export interface DocumentChangeSetBase {
  readonly id: DocumentChangeSetId
  readonly target: DocumentAuthority
  readonly changes: readonly DocumentChange[]
}

export interface ProposedDocumentChangeSet extends DocumentChangeSetBase {
  readonly status: 'proposed'
}

export interface ApprovedDocumentChangeSet extends DocumentChangeSetBase {
  readonly status: 'approved'
  readonly target: GenOfficeActiveDocumentAuthority
  readonly baseVersion: DocumentVersion
  readonly approval: {
    readonly approvedBy: string
    readonly approvedAt: string
  }
}

export interface RejectedDocumentChangeSet extends DocumentChangeSetBase {
  readonly status: 'rejected'
  readonly rejection: {
    readonly rejectedBy: string
    readonly rejectedAt: string
    readonly reason?: string
  }
}

export type DocumentChangeSet =
  ProposedDocumentChangeSet | ApprovedDocumentChangeSet | RejectedDocumentChangeSet

export interface DocumentMutationAppliedResult {
  readonly changeSetId: DocumentChangeSetId
  readonly documentId: DocumentId
  readonly status: 'applied'
  readonly previousVersion: DocumentVersion
  readonly newVersion: DocumentVersion
  readonly commandCount: number
  readonly changedCount: number
}

export interface DocumentVersionConflictResult {
  readonly changeSetId: DocumentChangeSetId
  readonly documentId: DocumentId
  readonly status: 'conflict'
  readonly reason: 'stale-version'
  readonly baseVersion: DocumentVersion
  readonly currentVersion: DocumentVersion
}

export type DocumentMutationResult = DocumentMutationAppliedResult | DocumentVersionConflictResult

/** Only approved changes for an active GenOffice document may cross this boundary. */
export interface DocumentMutationGateway {
  commit(changeSet: ApprovedDocumentChangeSet): Promise<DocumentMutationResult>
}
