import { expectTypeOf, test } from 'vitest'
import type { ToolProvider } from '@genoffice/9profs-core'
import type { DocumentInspector } from '@genoffice/document-gateway'
import type {
  OfficeCLIAdapter,
  OfficeCliArtifactReference,
  OfficeCliDocumentReference,
  OfficeCliInspectionResult,
  OfficeCliStatus,
} from '../src'

test('OfficeCLI adapter surface uses generic contracts', () => {
  expectTypeOf<OfficeCLIAdapter['inspector']>().toEqualTypeOf<DocumentInspector>()
  expectTypeOf<OfficeCLIAdapter['tools']>().toEqualTypeOf<ToolProvider>()
})

test('OfficeCLI transport boundary has no filesystem path or mutation surface', () => {
  expectTypeOf<OfficeCliDocumentReference>().toHaveProperty('artifact_id')
  expectTypeOf<OfficeCliStatus>().toHaveProperty('availability')
  expectTypeOf<OfficeCliInspectionResult>().toHaveProperty('operation')
  expectTypeOf<OfficeCliArtifactReference>().toHaveProperty('id')
})
