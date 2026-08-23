import { expectTypeOf, test } from 'vitest'
import type { DocumentInspector, DocumentMutationGateway } from '@genoffice/document-gateway'
import type { GenOfficeAdapter } from '../src'

test('GenOffice adapter surface uses document gateway contracts', () => {
  expectTypeOf<GenOfficeAdapter['inspector']>().toEqualTypeOf<DocumentInspector>()
  expectTypeOf<GenOfficeAdapter['mutationGateway']>().toEqualTypeOf<DocumentMutationGateway>()
})
