use std::{ffi::OsString, path::Path};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DocumentReference {
    pub artifact_id: String,
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

    pub(crate) fn args(&self, path: &Path, screenshot_output: Option<&Path>) -> Vec<OsString> {
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
                    ["view", path.to_str().unwrap_or_default(), "screenshot"].map(OsString::from),
                );
                if let Some(page) = request.page {
                    args.extend(["--page", &page.to_string()].map(OsString::from));
                }
                if let Some(width) = request.width {
                    args.extend(["--screenshot-width", &width.to_string()].map(OsString::from));
                }
                if let Some(height) = request.height {
                    args.extend(["--screenshot-height", &height.to_string()].map(OsString::from));
                }
                if let Some(output) = screenshot_output {
                    args.extend(["--out", output.to_str().unwrap_or_default()].map(OsString::from));
                }
                // Never emit OfficeCLI's --browser option. Render remains a
                // local headless sidecar operation in 9Profs.
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
