import {
  sliceWithLineSplit,
  type BlockBox,
  type BlockMetaOf,
  type FloatBox,
  type PageSlice,
  type SectionGeom,
} from '../pagination'

export * from './diagnostics'
export * from './geometry'
export * from './post-render'
export * from './geometry-probes'

export type PresentationRenderer = 'v1' | 'v2'

export const DEFAULT_PRESENTATION_RENDERER: PresentationRenderer = 'v1'

export interface PresentationInput {
  blocks: BlockBox[]
  sectionGeoms: SectionGeom[]
  totalHeight: number
  zoomFactor: number
  metaOf?: BlockMetaOf
  /** Existing measured body floating boxes; omitted by consumers without live DOM measurement. */
  floats?: FloatBox[]
}

/**
 * Read-only derived layout result shared by V1 and V2 consumers. The arrays are
 * the existing GenOffice layout objects; this contract does not clone, persist,
 * or own document/editor state.
 */
export interface PresentationLayoutSnapshot {
  readonly renderer: PresentationRenderer
  readonly blocks: BlockBox[]
  readonly pages: PageSlice[]
  readonly sectionGeoms: SectionGeom[]
  readonly totalHeight: number
  readonly zoomFactor: number
  readonly floats: FloatBox[]
}

/**
 * Internal test/development switch. Set globalThis.__9profsDocsPresentationRenderer to
 * 'v2' before the Docs renderer mounts; unknown values and the unset state remain V1.
 */
export function resolvePresentationRenderer(requested?: unknown): PresentationRenderer {
  const globalRenderer = (
    globalThis as typeof globalThis & {
      __9profsDocsPresentationRenderer?: unknown
    }
  ).__9profsDocsPresentationRenderer
  const queryRenderer =
    typeof location === 'undefined'
      ? undefined
      : new URLSearchParams(location.search).get('presentationRenderer')
  return (requested ?? globalRenderer ?? queryRenderer) === 'v2'
    ? 'v2'
    : DEFAULT_PRESENTATION_RENDERER
}

export function renderPresentation(
  renderer: PresentationRenderer,
  input: PresentationInput,
): PageSlice[] {
  return renderPresentationSnapshot(renderer, input).pages
}

/** Build one coherent presentation result while preserving the existing PageSlice API. */
export function renderPresentationSnapshot(
  renderer: PresentationRenderer,
  input: PresentationInput,
): PresentationLayoutSnapshot {
  const pages = renderer === 'v2' ? renderPresentationV2(input) : renderPresentationV1(input)
  return {
    renderer,
    blocks: input.blocks,
    pages,
    sectionGeoms: input.sectionGeoms,
    totalHeight: input.totalHeight,
    zoomFactor: input.zoomFactor,
    floats: input.floats ?? [],
  }
}

/** V1 owns current Docs pagination behavior. */
export function renderPresentationV1(input: PresentationInput): PageSlice[] {
  return sliceWithLineSplit(
    input.blocks,
    input.sectionGeoms,
    input.totalHeight,
    input.zoomFactor,
    input.metaOf,
  )
}

/**
 * First V2 proof only. Keep this adapter narrow until a real V2 layout implementation is proven.
 */
export function renderPresentationV2(input: PresentationInput): PageSlice[] {
  return renderPresentationV1(input)
}
