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

export type OfficeDocumentType = 'docx' | 'xlsx' | 'pptx'

export type OfficeMutation =
  | {
      readonly op: 'set'
      readonly selector: string
      readonly properties: Readonly<Record<string, string>>
    }
  | {
      readonly op: 'add'
      readonly parent: string
      readonly element_type: string
      readonly properties: Readonly<Record<string, string>>
    }
  | {
      readonly op: 'remove'
      readonly selector: string
    }
  | {
      readonly op: 'move' | 'copy'
      readonly selector: string
      readonly target: string
      readonly index?: number
    }
  | {
      readonly op: 'swap'
      readonly first: string
      readonly second: string
    }

export interface OfficeCliCreateRequest {
  readonly document_type: OfficeDocumentType
  readonly logical_name?: string
  readonly operations?: readonly OfficeMutation[]
}

export interface OfficeCliDetachedMutationRequest {
  readonly document: OfficeCliDocumentReference
  readonly operations: readonly OfficeMutation[]
  readonly base_revision_id?: string
}

export interface OfficeCliValidationDiagnostic {
  readonly severity: string
  readonly message: string
}

export interface OfficeCliValidationSummary {
  readonly structural_valid: boolean
  readonly diagnostics: readonly OfficeCliValidationDiagnostic[]
}

export interface OfficeCliRenderSummary {
  readonly artifacts: readonly OfficeCliArtifactReference[]
  readonly blocked_network_requests: number
}

export interface OfficeCliArtifactRevision {
  readonly artifact_id: string
  readonly revision_id: string
  readonly parent_revision_id: string | null
  readonly document_type: OfficeDocumentType
  readonly content_hash: string
  readonly created_at_ms: number
  readonly reference: OfficeCliDocumentReference
  readonly logical_name?: string | null
}

export interface OfficeCliMutationResult {
  readonly revision: OfficeCliArtifactRevision
  readonly operations_requested: number
  readonly operations_applied: number
  readonly validation: OfficeCliValidationSummary
  readonly render: OfficeCliRenderSummary
  readonly warnings: readonly string[]
}

/** Transport-neutral boundary. It exposes no CLI flags, process handles, or file paths. */
export interface OfficeCLIAdapter {
  readonly inspector: DocumentInspector
  readonly tools: ToolProvider
}
