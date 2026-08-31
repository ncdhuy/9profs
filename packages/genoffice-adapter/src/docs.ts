import type {
  ApprovedDocumentChangeSet,
  DocumentAuthority,
  DocumentId,
  DocumentInspection,
  DocumentInspectionRequest,
  DocumentInspector,
  DocumentMap,
  DocumentMutationGateway,
  DocumentMutationResult,
  DocumentVersion,
  GenOfficeActiveDocumentAuthority,
} from '@genoffice/document-gateway'
import type { GenOfficeAdapter } from './index'
import { GenOfficeDocumentVersionTracker, type SubscribeToGenOfficeTransactions } from './version'

export const DOCS_COMMAND_ENVELOPE = 'docs.commandEnvelope'
export const DEFAULT_GENOFFICE_AI_AUTHOR = '9Profs AI'

export interface DocsCommandContext {
  readonly track: { readonly author: string }
}

export interface DocsCommandResult {
  readonly changed?: number
}

export interface DocsCommandExecution {
  readonly ok: boolean
  readonly results: readonly DocsCommandResult[]
  readonly summary: string
  readonly error?: string
}

/** Runtime callbacks supplied by Docs; keeps this package free of PM/Tiptap types. */
export interface GenOfficeDocsRuntime {
  readonly subscribeToTransactions: SubscribeToGenOfficeTransactions
  readonly buildDocumentContext: () => unknown
  readonly getSelectionContext: () => unknown
  readonly buildDocumentMap?: (version: DocumentVersion) => DocumentMap
  readonly executeCommands: (
    commands: readonly unknown[],
    context: DocsCommandContext,
  ) => DocsCommandExecution
}

export interface GenOfficeDocsInspectionValue {
  readonly context: unknown
  readonly selection: unknown
}

export type GenOfficeDocsMutationErrorCode =
  | 'invalid-status'
  | 'authority-mismatch'
  | 'document-mismatch'
  | 'invalid-base-version'
  | 'unsupported-change'
  | 'invalid-change-payload'
  | 'invalid-command-envelope'
  | 'change-set-reuse'

export class GenOfficeDocsMutationError extends Error {
  readonly code: GenOfficeDocsMutationErrorCode

  constructor(code: GenOfficeDocsMutationErrorCode, message: string) {
    super(message)
    this.name = 'GenOfficeDocsMutationError'
    this.code = code
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === 'object' && !Array.isArray(value)
}

function activeAuthority(documentId: DocumentId): GenOfficeActiveDocumentAuthority {
  return { kind: 'genoffice-active', documentId, writeAuthority: 'genoffice' }
}

export class GenOfficeDocsInspector implements DocumentInspector {
  readonly authority: GenOfficeActiveDocumentAuthority

  constructor(
    private readonly documentId: DocumentId,
    private readonly runtime: GenOfficeDocsRuntime,
    private readonly versions: GenOfficeDocumentVersionTracker,
  ) {
    this.authority = activeAuthority(documentId)
  }

  async inspect(request: DocumentInspectionRequest): Promise<DocumentInspection> {
    if (request.documentId !== this.documentId) {
      throw new GenOfficeDocsMutationError(
        'document-mismatch',
        `inspection targets ${request.documentId}, active document is ${this.documentId}`,
      )
    }
    const value: GenOfficeDocsInspectionValue = {
      context: this.runtime.buildDocumentContext(),
      selection: this.runtime.getSelectionContext(),
    }
    const version = this.versions.version
    const documentMap = this.runtime.buildDocumentMap?.(version)
    return {
      documentId: this.documentId,
      authority: this.authority,
      version,
      value,
      ...(documentMap ? { documentMap } : {}),
    }
  }
}

export class GenOfficeDocsMutationGateway implements DocumentMutationGateway {
  private readonly appliedChangeSets = new Map<
    string,
    {
      readonly documentId: DocumentId
      readonly baseVersion: DocumentVersion
      readonly result: Extract<DocumentMutationResult, { status: 'applied' }>
    }
  >()

  private static readonly MAX_APPLIED_CHANGE_SETS = 64

  constructor(
    private readonly documentId: DocumentId,
    private readonly runtime: GenOfficeDocsRuntime,
    private readonly versions: GenOfficeDocumentVersionTracker,
    private readonly author: string,
  ) {}

  async commit(changeSet: ApprovedDocumentChangeSet): Promise<DocumentMutationResult> {
    const candidate = changeSet as ApprovedDocumentChangeSet & {
      readonly status?: unknown
    }
    if (!candidate || typeof candidate !== 'object' || candidate.status !== 'approved') {
      throw new GenOfficeDocsMutationError(
        'invalid-status',
        'only approved document change sets may be committed',
      )
    }

    const target = candidate.target as DocumentAuthority | undefined
    if (!target || target.kind !== 'genoffice-active' || target.writeAuthority !== 'genoffice') {
      throw new GenOfficeDocsMutationError(
        'authority-mismatch',
        'active GenOffice mutation requires genoffice-active authority',
      )
    }
    if (target.documentId !== this.documentId) {
      throw new GenOfficeDocsMutationError(
        'document-mismatch',
        `change set targets ${target.documentId}, active document is ${this.documentId}`,
      )
    }

    if (!Number.isInteger(candidate.baseVersion) || candidate.baseVersion < 0) {
      throw new GenOfficeDocsMutationError(
        'invalid-base-version',
        'approved active change set requires a non-negative integer baseVersion',
      )
    }

    const cached = this.appliedChangeSets.get(candidate.id)
    if (cached) {
      if (
        cached.documentId !== target.documentId ||
        cached.baseVersion !== candidate.baseVersion
      ) {
        throw new GenOfficeDocsMutationError(
          'change-set-reuse',
          'approved change set id was already applied to a different document version',
        )
      }
      this.appliedChangeSets.delete(candidate.id)
      this.appliedChangeSets.set(candidate.id, cached)
      return cached.result
    }

    const currentVersion = this.versions.version
    if (candidate.baseVersion !== currentVersion) {
      return {
        changeSetId: candidate.id,
        documentId: this.documentId,
        status: 'conflict',
        reason: 'stale-version',
        baseVersion: candidate.baseVersion,
        currentVersion,
      }
    }

    if (!Array.isArray(candidate.changes)) {
      throw new GenOfficeDocsMutationError(
        'invalid-change-payload',
        'approved active change set requires changes[]',
      )
    }
    for (const change of candidate.changes) {
      if (!isRecord(change) || change.type !== DOCS_COMMAND_ENVELOPE) {
        throw new GenOfficeDocsMutationError(
          'unsupported-change',
          `unsupported active Docs change type: ${isRecord(change) ? String(change.type) : 'unknown'}`,
        )
      }
    }

    const commands: unknown[] = []
    for (const [index, change] of candidate.changes.entries()) {
      const payload = change.payload
      if (!isRecord(payload) || !Array.isArray(payload.commands)) {
        throw new GenOfficeDocsMutationError(
          'invalid-change-payload',
          `change #${index} docs.commandEnvelope payload must contain commands[]`,
        )
      }
      commands.push(...payload.commands)
    }

    const outcome = this.runtime.executeCommands(commands, { track: { author: this.author } })
    if (!outcome.ok) {
      throw new GenOfficeDocsMutationError(
        'invalid-command-envelope',
        outcome.error ?? 'Docs command envelope was rejected',
      )
    }

    const changedCount = outcome.results.reduce(
      (count, result) => count + (Number.isFinite(result.changed) ? Number(result.changed) : 0),
      0,
    )
    const applied: Extract<DocumentMutationResult, { status: 'applied' }> = {
      changeSetId: candidate.id,
      documentId: this.documentId,
      status: 'applied',
      previousVersion: currentVersion,
      newVersion: this.versions.version,
      commandCount: commands.length,
      changedCount,
    }
    this.appliedChangeSets.set(candidate.id, {
      documentId: target.documentId,
      baseVersion: candidate.baseVersion,
      result: applied,
    })
    while (this.appliedChangeSets.size > GenOfficeDocsMutationGateway.MAX_APPLIED_CHANGE_SETS) {
      const oldest = this.appliedChangeSets.keys().next().value
      if (oldest === undefined) break
      this.appliedChangeSets.delete(oldest)
    }
    return applied
  }
}

export interface GenOfficeDocsAdapterOptions {
  readonly documentId: DocumentId
  readonly runtime: GenOfficeDocsRuntime
  readonly aiAuthor?: string
  readonly initialVersion?: DocumentVersion
}

export class GenOfficeDocsAdapter implements GenOfficeAdapter {
  readonly documentId: DocumentId
  readonly versionTracker: GenOfficeDocumentVersionTracker
  readonly inspector: GenOfficeDocsInspector
  readonly mutationGateway: GenOfficeDocsMutationGateway

  constructor(options: GenOfficeDocsAdapterOptions) {
    this.documentId = options.documentId
    this.versionTracker = new GenOfficeDocumentVersionTracker(
      options.runtime.subscribeToTransactions,
      options.initialVersion,
    )
    this.inspector = new GenOfficeDocsInspector(
      options.documentId,
      options.runtime,
      this.versionTracker,
    )
    this.mutationGateway = new GenOfficeDocsMutationGateway(
      options.documentId,
      options.runtime,
      this.versionTracker,
      options.aiAuthor ?? DEFAULT_GENOFFICE_AI_AUTHOR,
    )
  }

  resetVersion(version: DocumentVersion = 0): void {
    this.versionTracker.reset(version)
  }

  dispose(): void {
    this.versionTracker.dispose()
  }
}

export function createGenOfficeDocsAdapter(
  options: GenOfficeDocsAdapterOptions,
): GenOfficeDocsAdapter {
  return new GenOfficeDocsAdapter(options)
}
