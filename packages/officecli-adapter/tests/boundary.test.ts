import { expectTypeOf, test } from 'vitest'
import type { ToolProvider } from '@genoffice/9profs-core'
import type { DocumentInspector } from '@genoffice/document-gateway'
import type { OfficeCLIAdapter } from '../src'

test('OfficeCLI adapter surface uses generic contracts', () => {
  expectTypeOf<OfficeCLIAdapter['inspector']>().toEqualTypeOf<DocumentInspector>()
  expectTypeOf<OfficeCLIAdapter['tools']>().toEqualTypeOf<ToolProvider>()
})
