import { expectTypeOf, test } from 'vitest'
import type { ToolProvider } from '@genoffice/9profs-core'
import type { DocumentInspector } from '@genoffice/document-gateway'
import type {
  OfficeCLIAdapter,
  OfficeCliArtifactReference,
  OfficeCliArtifactRevision,
  OfficeCliCreateRequest,
  OfficeCliDetachedMutationRequest,
  OfficeCliDocumentReference,
  OfficeCliInspectionResult,
  OfficeCliMutationResult,
  OfficeCliStatus,
} from '../src'

test('OfficeCLI adapter surface uses generic contracts', () => {
  expectTypeOf<OfficeCLIAdapter['inspector']>().toEqualTypeOf<DocumentInspector>()
  expectTypeOf<OfficeCLIAdapter['tools']>().toEqualTypeOf<ToolProvider>()
})

test('OfficeCLI transport boundary has typed mutation without filesystem paths', () => {
  expectTypeOf<OfficeCliDocumentReference>().toHaveProperty('artifact_id')
  expectTypeOf<OfficeCliStatus>().toHaveProperty('availability')
  expectTypeOf<OfficeCliInspectionResult>().toHaveProperty('operation')
  expectTypeOf<OfficeCliArtifactReference>().toHaveProperty('id')
  expectTypeOf<OfficeCliCreateRequest>().toHaveProperty('document_type')
  expectTypeOf<OfficeCliDetachedMutationRequest>().toHaveProperty('operations')
  expectTypeOf<OfficeCliArtifactRevision>().toHaveProperty('revision_id')
  expectTypeOf<OfficeCliMutationResult>().toHaveProperty('validation')
})
