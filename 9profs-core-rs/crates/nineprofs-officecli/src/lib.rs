//! Pinned OfficeCLI sidecar boundary with read-only inspection and detached,
//! transactional mutation.
//!
//! This crate owns binary verification, process isolation, typed read-only
//! operations, artifact resolution, copy-on-write revisions, and the OfficeCLI
//! ToolProvider. It never exposes arbitrary command strings and never writes
//! active GenOffice files.

mod artifact;
mod config;
mod mutation;
mod operations;
mod process;
mod provider;
mod rasterizer;
mod runner;

pub use artifact::{
    ArtifactError, ArtifactKind, ArtifactResolver, DocumentResolver, ResolvedDocument,
    WritableDetachedArtifact,
};
pub use config::{OfficeCliAvailability, OfficeCliConfig, OfficeCliStatus, SUPPORTED_VERSION};
pub use mutation::{
    ArtifactRevision, CreateDocumentRequest, DetachedMutationError, DetachedMutationRequest,
    DetachedMutationService, MutationResult, RenderSummary, ValidationDiagnostic,
    ValidationSummary,
};
pub use operations::{
    DocumentReference, GetRequest, IssuesRequest, MutationValidationError, OfficeCliOperation,
    OfficeDocumentType, OfficeMutation, QueryRequest, ScreenshotRequest, ValidateRequest,
    ViewRequest,
};
pub use provider::{
    OFFICE_CREATE, OFFICE_FIND_ISSUES, OFFICE_GET, OFFICE_INSPECT_ANNOTATED,
    OFFICE_INSPECT_OUTLINE, OFFICE_INSPECT_STATS, OFFICE_INSPECT_TEXT, OFFICE_MUTATE_DETACHED,
    OFFICE_QUERY, OFFICE_RENDER, OFFICE_VALIDATE, OfficeCliToolProvider,
};
pub use rasterizer::{
    ElectronHtmlRasterizer, HtmlArtifact, HtmlRasterizer, ImageArtifact, RasterLimits,
    RasterRequest, RasterizerError, RenderResult,
};
pub use runner::{
    ArtifactReference, OfficeCliCancellation, OfficeCliError, OfficeCliResponse, OfficeCliRunner,
};
