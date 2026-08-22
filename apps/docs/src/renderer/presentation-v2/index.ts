import type { SectionSettings } from '@genoffice/docx-engine'
import {
  effectiveBottomPx,
  effectiveTopPx,
  sectionPageBox,
  sliceWithLineSplit,
  type BlockBox,
  type BlockMetaOf,
  type FloatBox,
  type PageSlice,
  type SectionGeom,
  type SectionHfHeights,
} from '../pagination'
import type { HfFloatBox } from '../editor/hf-dom'
import { paginatePresentationV2 } from './page-slicer'
import type { PresentationInvalidationHint } from './measurement-invalidation'
import type { PresentationV2PerformanceSink } from './performance'

export * from './diagnostics'
export * from './geometry'
export * from './post-render'
export * from './geometry-probes'
export * from './performance'
export * from './measurement-invalidation'

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
  /** Existing measured default-variant header/footer heights used for body push-down. */
  sectionHfHeights?: SectionHfHeights[]
  /** Opt-in read-only V2 phase/caching counters; ignored by the V1 path. */
  performance?: PresentationV2PerformanceSink
  /** Transient V2-only edit hint; V1 intentionally ignores presentation pruning state. */
  invalidationHint?: PresentationInvalidationHint
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
  /** Existing derived header/footer measurements, indexed by page section. */
  readonly sectionHfHeights: SectionHfHeights[]
}

export interface HeaderFooterPagePlacement {
  readonly pageIndex: number
  readonly sectionIndex: number
  readonly pageBox: ReturnType<typeof sectionPageBox>
  readonly marginTop: number
  readonly marginBottom: number
  readonly floatBox: HfFloatBox
}

const EMPTY_HF_HEIGHTS: SectionHfHeights = { headerPx: 0, footerPx: 0 }

/**
 * Derive physical header/footer placement from the shared page result plus
 * section configuration. Content, variants, and section references remain
 * outside the snapshot; this helper only exposes render-time placement facts.
 */
export function headerFooterPagePlacement(
  snapshot: Pick<PresentationLayoutSnapshot, 'pages' | 'sectionHfHeights'>,
  pageIndex: number,
  settings: SectionSettings,
  variantHeights?: SectionHfHeights,
  fallbackHeights: SectionHfHeights = EMPTY_HF_HEIGHTS,
): HeaderFooterPagePlacement | null {
  const page = snapshot.pages[pageIndex]
  if (!page) return null
  const hf = variantHeights ?? snapshot.sectionHfHeights[page.section] ?? fallbackHeights
  const pageBox = sectionPageBox(settings)
  const marginTop = effectiveTopPx(settings, hf.headerPx)
  const marginBottom = effectiveBottomPx(settings, hf.footerPx)
  return {
    pageIndex,
    sectionIndex: page.section,
    pageBox,
    marginTop,
    marginBottom,
    floatBox: {
      pageW: pageBox.width,
      pageH: pageBox.height,
      marginLeft: pageBox.marginLeft,
      marginRight: pageBox.marginRight,
      marginTop,
      marginBottom,
      headerDist: pageBox.headerDist,
      sectMarginTop: pageBox.marginTop,
    },
  }
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
    sectionHfHeights: input.sectionHfHeights ?? [],
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

export function renderPresentationV2(input: PresentationInput): PageSlice[] {
  return paginatePresentationV2(input)
}
