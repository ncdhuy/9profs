use std::{collections::BTreeMap, sync::Arc};

use nineprofs_common::now_ms;
use nineprofs_db::Database;
use nineprofs_realtime::BroadcastEventBus;

use super::{ResearchService, sha256_hash};
use crate::{
    AssessmentMethod, CaptureMethod, CapturePdfEvidence, CapturePdfExtraction,
    CaptureSourceSnapshot, CitationOccurrenceOrigin, ClaimEvidenceRelation, ClaimOrigin,
    CreateCitationOccurrence, CreateCitationTarget, CreateCitationTargetBinding,
    CreateClaimCitationLink, CreateClaimEvidenceLink, CreateResearchCase, CreateResearchClaim,
    CreateResearchEvidence, CreateResearchSource, EvidenceLocator, MAX_CLAIM_TEXT_BYTES,
    MAX_EVIDENCE_EXCERPT_BYTES, MAX_METADATA_BYTES, PdfExtractionStatus, ResearchCaseId,
    ResearchError, ResearchSourceId, ResearchSourceSnapshotId, SourceKind, SourceOrigin,
};

mod citations;
mod common;
mod evidence_claims;
mod manuscript_sync;
mod pdf;
mod provenance;
mod repository_injection;
