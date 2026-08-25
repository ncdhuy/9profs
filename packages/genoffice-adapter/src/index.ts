import type { DocumentInspector, DocumentMutationGateway } from '@genoffice/document-gateway'

export * from './docs'
export { GenOfficeDocumentVersionTracker } from './version'
export type { GenOfficeDocumentTransaction, SubscribeToGenOfficeTransactions } from './version'

/** Future GenOffice integrations must expose 9Profs-owned document contracts. */
export interface GenOfficeAdapter {
  readonly inspector: DocumentInspector
  readonly mutationGateway: DocumentMutationGateway
}
