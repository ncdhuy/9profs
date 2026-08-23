import type { ToolProvider } from '@genoffice/9profs-core'
import type { DocumentInspector } from '@genoffice/document-gateway'

/** Future OfficeCLI integrations expose generic inspection and tool contracts only. */
export interface OfficeCLIAdapter {
  readonly inspector: DocumentInspector
  readonly tools: ToolProvider
}
