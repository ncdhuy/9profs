import type { DocumentInspector, DocumentMutationGateway } from '@genoffice/document-gateway'

/** Future GenOffice integrations must expose 9Profs-owned document contracts. */
export interface GenOfficeAdapter {
  readonly inspector: DocumentInspector
  readonly mutationGateway: DocumentMutationGateway
}
