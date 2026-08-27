use crate::api::ApiError;
use axum::http::HeaderMap;
use nineprofs_api_types::ResearchContentHashDto;
use nineprofs_api_types::ResearchHashAlgorithmDto;
use nineprofs_research::HashAlgorithm;

pub(crate) fn header_text(headers: &HeaderMap, name: &str) -> Result<Option<String>, ApiError> {
    headers
        .get(name)
        .map(|value| {
            value
                .to_str()
                .map(str::to_owned)
                .map_err(|_| ApiError::InvalidRequest(format!("invalid {name} header")))
        })
        .transpose()
}

pub(crate) fn safe_upload_label(value: Option<&str>, fallback: &str) -> Result<String, ApiError> {
    let value = value.unwrap_or(fallback);
    let label = value.rsplit(['/', '\\']).next().unwrap_or(value).trim();
    if label.is_empty() || label.len() > nineprofs_research::MAX_SOURCE_LABEL_BYTES {
        return Err(ApiError::InvalidRequest(
            "PDF filename/label is empty or too long".to_owned(),
        ));
    }
    if label.chars().any(char::is_control) {
        return Err(ApiError::InvalidRequest(
            "PDF filename/label contains control characters".to_owned(),
        ));
    }
    Ok(label.to_owned())
}

pub(crate) fn research_content_hash_dto(
    value: nineprofs_research::ContentHash,
) -> ResearchContentHashDto {
    ResearchContentHashDto {
        algorithm: match value.algorithm {
            HashAlgorithm::Sha256 => ResearchHashAlgorithmDto::Sha256,
        },
        value: value.value,
    }
}
