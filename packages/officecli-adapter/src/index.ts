import type { ToolProvider } from '@genoffice/9profs-core'
import type { DocumentInspector } from '@genoffice/document-gateway'

export type OfficeCliAvailability = 'available' | 'unavailable' | 'version-mismatch'

export interface OfficeCliStatus {
  readonly configured: boolean
  readonly availability: OfficeCliAvailability
  readonly supported_version: string
  readonly detected_version: string | null
  readonly capabilities: readonly string[]
}

export interface OfficeCliDocumentReference {
  /** 9Profs-owned detached artifact or inspection snapshot identifier. */
  readonly artifact_id: string
}

export type OfficeCliInspectionOperation =
  | 'view_text'
  | 'view_annotated'
  | 'view_outline'
  | 'view_stats'
  | 'view_issues'
  | 'get'
  | 'query'
  | 'validate'
  | 'screenshot'

export interface OfficeCliArtifactReference {
  readonly id: string
  readonly kind: 'office-render'
}

export interface OfficeCliInspectionResult {
  readonly operation: OfficeCliInspectionOperation
  readonly document_id: string
  readonly data: unknown
  readonly artifact?: OfficeCliArtifactReference
  readonly artifacts?: readonly OfficeCliArtifactReference[]
}

/** Transport-neutral boundary. It exposes no CLI flags, process handles, or file paths. */
export interface OfficeCLIAdapter {
  readonly inspector: DocumentInspector
  readonly tools: ToolProvider
}
