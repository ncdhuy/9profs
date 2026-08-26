import type { DocumentVersion } from '@genoffice/document-gateway'

export interface GenOfficeDocumentTransaction {
  readonly docChanged: boolean
}

export type SubscribeToGenOfficeTransactions = (
  listener: (transaction: GenOfficeDocumentTransaction) => void,
) => () => void

/** Counts document-content transactions only; selection and presentation state stay out. */
export class GenOfficeDocumentVersionTracker {
  private currentVersion: DocumentVersion
  private unsubscribe: (() => void) | null
  private readonly listeners = new Set<(version: DocumentVersion) => void>()

  constructor(subscribe: SubscribeToGenOfficeTransactions, initialVersion: DocumentVersion = 0) {
    this.currentVersion = initialVersion
    this.unsubscribe = subscribe((transaction) => {
      if (transaction.docChanged) {
        this.currentVersion++
        for (const listener of this.listeners) listener(this.currentVersion)
      }
    })
  }

  get version(): DocumentVersion {
    return this.currentVersion
  }

  reset(version: DocumentVersion = 0): void {
    this.currentVersion = version
  }

  subscribe(listener: (version: DocumentVersion) => void): () => void {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  dispose(): void {
    this.unsubscribe?.()
    this.unsubscribe = null
    this.listeners.clear()
  }
}
