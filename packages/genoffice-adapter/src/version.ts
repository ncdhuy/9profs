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

  constructor(subscribe: SubscribeToGenOfficeTransactions, initialVersion: DocumentVersion = 0) {
    this.currentVersion = initialVersion
    this.unsubscribe = subscribe((transaction) => {
      if (transaction.docChanged) this.currentVersion++
    })
  }

  get version(): DocumentVersion {
    return this.currentVersion
  }

  reset(version: DocumentVersion = 0): void {
    this.currentVersion = version
  }

  dispose(): void {
    this.unsubscribe?.()
    this.unsubscribe = null
  }
}
