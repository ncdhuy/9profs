use std::{collections::BTreeMap, ffi::OsString, path::Path};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DocumentReference {
    pub artifact_id: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OfficeDocumentType {
    Docx,
    Xlsx,
    Pptx,
}

impl OfficeDocumentType {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Docx => "docx",
            Self::Xlsx => "xlsx",
            Self::Pptx => "pptx",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum OfficeMutation {
    Set {
        selector: String,
        properties: BTreeMap<String, String>,
    },
    Add {
        parent: String,
        element_type: String,
        properties: BTreeMap<String, String>,
    },
    Remove {
        selector: String,
    },
    Move {
        selector: String,
        target: String,
        index: Option<u32>,
    },
    Copy {
        selector: String,
        target: String,
        index: Option<u32>,
    },
    Swap {
        first: String,
        second: String,
    },
}

impl OfficeMutation {
    pub fn validate(&self) -> Result<(), MutationValidationError> {
        match self {
            Self::Set {
                selector,
                properties,
            } => {
                validate_selector(selector)?;
                validate_properties(properties)
            }
            Self::Add {
                parent,
                element_type,
                properties,
            } => {
                validate_selector(parent)?;
                validate_token(element_type, "element type")?;
                validate_properties(properties)
            }
            Self::Remove { selector } => validate_selector(selector),
            Self::Move {
                selector, target, ..
            }
            | Self::Copy {
                selector, target, ..
            } => {
                validate_selector(selector)?;
                validate_selector(target)
            }
            Self::Swap { first, second } => {
                validate_selector(first)?;
                validate_selector(second)
            }
        }
    }

    pub(crate) fn args(&self, path: &Path) -> Vec<OsString> {
        let path = path.to_string_lossy();
        let path = path.as_ref();
        let mut args = vec![OsString::from("--json")];
        match self {
            Self::Set {
                selector,
                properties,
            } => {
                args.extend(["set", &path, selector].map(OsString::from));
                add_properties(&mut args, properties);
            }
            Self::Add {
                parent,
                element_type,
                properties,
            } => {
                args.extend(["add", &path, parent].map(OsString::from));
                args.extend(["--type", element_type].map(OsString::from));
                add_properties(&mut args, properties);
            }
            Self::Remove { selector } => {
                args.extend(["remove", &path, selector].map(OsString::from));
            }
            Self::Move {
                selector,
                target,
                index,
            } => {
                args.extend(["move", &path, selector].map(OsString::from));
                args.extend(["--to", target].map(OsString::from));
                if let Some(index) = index {
                    args.extend(["--index", &index.to_string()].map(OsString::from));
                }
            }
            Self::Copy {
                selector,
                target,
                index,
            } => {
                args.extend(["copy", &path, selector].map(OsString::from));
                args.extend(["--to", target].map(OsString::from));
                if let Some(index) = index {
                    args.extend(["--index", &index.to_string()].map(OsString::from));
                }
            }
            Self::Swap { first, second } => {
                args.extend(["swap", &path, first, second].map(OsString::from));
            }
        }
        args
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Set { .. } => "set",
            Self::Add { .. } => "add",
            Self::Remove { .. } => "remove",
            Self::Move { .. } => "move",
            Self::Copy { .. } => "copy",
            Self::Swap { .. } => "swap",
        }
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum MutationValidationError {
    #[error("document selector must be an absolute semantic path")]
    InvalidSelector,
    #[error("document selector exceeds 4096 bytes")]
    SelectorTooLong,
    #[error("{0} is invalid")]
    InvalidToken(String),
    #[error("mutation property count exceeds 64")]
    TooManyProperties,
    #[error("mutation property is too large")]
    PropertyTooLarge,
}

fn validate_selector(selector: &str) -> Result<(), MutationValidationError> {
    if selector.len() > 4096 {
        return Err(MutationValidationError::SelectorTooLong);
    }
    if selector.is_empty()
        || !selector.starts_with('/')
        || selector.contains('\0')
        || selector.contains("..")
        || selector.contains('\\')
    {
        return Err(MutationValidationError::InvalidSelector);
    }
    Ok(())
}

fn validate_token(value: &str, label: &str) -> Result<(), MutationValidationError> {
    if value.is_empty()
        || value.len() > 128
        || value.starts_with('-')
        || value.contains('\0')
        || value.chars().any(char::is_whitespace)
    {
        return Err(MutationValidationError::InvalidToken(label.to_owned()));
    }
    Ok(())
}

fn validate_properties(
    properties: &BTreeMap<String, String>,
) -> Result<(), MutationValidationError> {
    if properties.len() > 64 {
        return Err(MutationValidationError::TooManyProperties);
    }
    for (key, value) in properties {
        validate_token(key, "property name")?;
        if value.len() > 16 * 1024 || value.contains('\0') {
            return Err(MutationValidationError::PropertyTooLarge);
        }
    }
    Ok(())
}

fn add_properties(args: &mut Vec<OsString>, properties: &BTreeMap<String, String>) {
    for (key, value) in properties {
        args.extend(["--prop", &format!("{key}={value}")].map(OsString::from));
    }
}

#[derive(Clone, Debug)]
pub enum OfficeCliOperation {
    ViewText(ViewRequest),
    ViewAnnotated(ViewRequest),
    ViewOutline(ViewRequest),
    ViewStats(ViewRequest),
    ViewIssues(IssuesRequest),
    Get(GetRequest),
    Query(QueryRequest),
    Validate(ValidateRequest),
    Screenshot(ScreenshotRequest),
}

#[derive(Clone, Debug)]
pub struct ViewRequest {
    pub document: DocumentReference,
    pub start: Option<u32>,
    pub end: Option<u32>,
    pub limit: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct IssuesRequest {
    pub document: DocumentReference,
    pub issue_type: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct GetRequest {
    pub document: DocumentReference,
    pub selector: String,
}

#[derive(Clone, Debug)]
pub struct QueryRequest {
    pub document: DocumentReference,
    pub selector: String,
}

#[derive(Clone, Debug)]
pub struct ValidateRequest {
    pub document: DocumentReference,
}

#[derive(Clone, Debug)]
pub struct ScreenshotRequest {
    pub document: DocumentReference,
    pub page: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

impl OfficeCliOperation {
    pub fn document(&self) -> &DocumentReference {
        match self {
            Self::ViewText(request)
            | Self::ViewAnnotated(request)
            | Self::ViewOutline(request)
            | Self::ViewStats(request) => &request.document,
            Self::ViewIssues(request) => &request.document,
            Self::Get(request) => &request.document,
            Self::Query(request) => &request.document,
            Self::Validate(request) => &request.document,
            Self::Screenshot(request) => &request.document,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::ViewText(_) => "view_text",
            Self::ViewAnnotated(_) => "view_annotated",
            Self::ViewOutline(_) => "view_outline",
            Self::ViewStats(_) => "view_stats",
            Self::ViewIssues(_) => "view_issues",
            Self::Get(_) => "get",
            Self::Query(_) => "query",
            Self::Validate(_) => "validate",
            Self::Screenshot(_) => "screenshot",
        }
    }

    pub(crate) fn args(&self, path: &Path, html_output: Option<&Path>) -> Vec<OsString> {
        let mut args = vec![OsString::from("--json")];
        match self {
            Self::ViewText(request) => view_args(&mut args, path, "text", request),
            Self::ViewAnnotated(request) => view_args(&mut args, path, "annotated", request),
            Self::ViewOutline(request) => view_args(&mut args, path, "outline", request),
            Self::ViewStats(request) => view_args(&mut args, path, "stats", request),
            Self::ViewIssues(request) => {
                args.extend(
                    ["view", path.to_str().unwrap_or_default(), "issues"].map(OsString::from),
                );
                if let Some(issue_type) = &request.issue_type {
                    args.extend(["--type", issue_type].map(OsString::from));
                }
            }
            Self::Get(request) => {
                args.extend(
                    ["get", path.to_str().unwrap_or_default(), &request.selector]
                        .map(OsString::from),
                );
            }
            Self::Query(request) => {
                args.extend(
                    [
                        "query",
                        path.to_str().unwrap_or_default(),
                        &request.selector,
                    ]
                    .map(OsString::from),
                );
            }
            Self::Validate(_) => {
                args.extend(["validate", path.to_str().unwrap_or_default()].map(OsString::from));
            }
            Self::Screenshot(request) => {
                args.extend(
                    ["view", path.to_str().unwrap_or_default(), "html"].map(OsString::from),
                );
                // OfficeCLI-native screenshot stays diagnostic only. Production
                // rendering consumes this HTML through the 9Profs rasterizer.
                let _ = (request, html_output);
            }
        }
        args
    }
}

fn view_args(args: &mut Vec<OsString>, path: &Path, mode: &str, request: &ViewRequest) {
    args.extend(["view", path.to_str().unwrap_or_default(), mode].map(OsString::from));
    if let Some(start) = request.start {
        args.extend(["--start", &start.to_string()].map(OsString::from));
    }
    if let Some(end) = request.end {
        args.extend(["--end", &end.to_string()].map(OsString::from));
    }
    if let Some(limit) = request.limit {
        args.extend(["--limit", &limit.to_string()].map(OsString::from));
    }
}
