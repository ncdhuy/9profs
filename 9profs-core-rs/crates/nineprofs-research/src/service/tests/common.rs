use super::{
    Arc, BTreeMap, BroadcastEventBus, CaptureMethod, CaptureSourceSnapshot, Database,
    ResearchService, ResearchSourceId, SourceOrigin,
};

pub(super) async fn service() -> (Database, ResearchService) {
    let database = Database::in_memory().await.unwrap();
    let service = ResearchService::new(
        crate::SqliteResearchRepository::new(database.pool().clone()),
        Arc::new(BroadcastEventBus::new(64)),
    );
    (database, service)
}

pub(super) fn origin() -> SourceOrigin {
    SourceOrigin::UploadedArtifact {
        artifact_id: "artifact-1".to_owned(),
        revision_id: Some("revision-1".to_owned()),
    }
}

pub(super) fn snapshot_input(source_id: ResearchSourceId, content: &[u8]) -> CaptureSourceSnapshot {
    CaptureSourceSnapshot {
        source_id,
        content: content.to_vec(),
        capture_method: CaptureMethod::UploadedArtifact,
        origin: origin(),
        metadata: BTreeMap::new(),
    }
}
