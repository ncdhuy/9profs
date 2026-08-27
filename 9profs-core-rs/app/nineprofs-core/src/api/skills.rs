use crate::api::ApiError;
use crate::api::AppState;
use axum::Router;
use axum::extract::Path;
use axum::extract::State;
use axum::routing::get;
use axum::routing::post;
use nineprofs_api_types::ApiResponse;
use nineprofs_api_types::EventEnvelope;
use nineprofs_api_types::SkillCatalogDto;
use nineprofs_api_types::SkillDto;
use nineprofs_api_types::SkillIssueDto;
use nineprofs_skills::Skill;
use nineprofs_skills::SkillSource;

async fn list_skills(State(state): State<AppState>) -> axum::Json<ApiResponse<SkillCatalogDto>> {
    axum::Json(ApiResponse::ok(skill_catalog_dto(
        state.runtime.skill_catalog().scan(),
        false,
    )))
}

async fn get_skill(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<SkillDto>>, ApiError> {
    let skill = state
        .runtime
        .skill_catalog()
        .resolve(&id)
        .ok_or_else(|| ApiError::NotFound(id.clone()))?;
    Ok(axum::Json(ApiResponse::ok(skill_dto(&skill, true))))
}

async fn scan_skills(State(state): State<AppState>) -> axum::Json<ApiResponse<SkillCatalogDto>> {
    let catalog = skill_catalog_dto(state.runtime.skill_catalog().scan(), false);
    let _ = state.runtime.event_bus().publish(EventEnvelope::new(
        "skill.catalogChanged",
        serde_json::json!({ "skill_count": catalog.skills.len(), "issue_count": catalog.issues.len() }),
    ));
    axum::Json(ApiResponse::ok(catalog))
}

pub(super) fn skill_catalog_dto(
    scan: nineprofs_skills::SkillScan,
    include_content: bool,
) -> SkillCatalogDto {
    SkillCatalogDto {
        skills: scan
            .skills
            .iter()
            .map(|skill| skill_dto(skill, include_content))
            .collect(),
        issues: scan
            .issues
            .iter()
            .map(|issue| SkillIssueDto {
                root: issue.root.display().to_string(),
                path: issue.path.as_ref().map(|path| path.display().to_string()),
                message: issue.message.clone(),
            })
            .collect(),
    }
}

pub(super) fn skill_dto(skill: &Skill, include_content: bool) -> SkillDto {
    SkillDto {
        id: skill.id.clone(),
        name: skill.name.clone(),
        description: skill.description.clone(),
        source: match skill.source {
            SkillSource::Builtin => "builtin".to_owned(),
            SkillSource::Custom => "custom".to_owned(),
            SkillSource::Extension => "extension".to_owned(),
        },
        location: skill.location.display_path(),
        content: include_content.then(|| skill.content.clone()),
    }
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/skills", get(list_skills))
        .route("/api/skills/{id}", get(get_skill))
        .route("/api/skills/scan", post(scan_skills))
}
