//! Read-only, pinned OfficeCLI sidecar boundary.
//!
//! This crate owns binary verification, process isolation, typed read-only
//! operations, artifact resolution, and the OfficeCLI ToolProvider. It never
//! exposes arbitrary command strings and never writes active GenOffice files.

mod artifact;
mod config;
mod operations;
mod process;
mod provider;
mod runner;

pub use artifact::{
    ArtifactError, ArtifactKind, ArtifactResolver, DocumentResolver, ResolvedDocument,
};
pub use config::{OfficeCliAvailability, OfficeCliConfig, OfficeCliStatus, SUPPORTED_VERSION};
pub use operations::{
    DocumentReference, GetRequest, IssuesRequest, OfficeCliOperation, QueryRequest,
    ScreenshotRequest, ValidateRequest, ViewRequest,
};
pub use provider::{
    OFFICE_FIND_ISSUES, OFFICE_GET, OFFICE_INSPECT_ANNOTATED, OFFICE_INSPECT_OUTLINE,
    OFFICE_INSPECT_STATS, OFFICE_INSPECT_TEXT, OFFICE_QUERY, OFFICE_RENDER, OFFICE_VALIDATE,
    OfficeCliToolProvider,
};
pub use runner::{
    ArtifactReference, OfficeCliCancellation, OfficeCliError, OfficeCliResponse, OfficeCliRunner,
};
