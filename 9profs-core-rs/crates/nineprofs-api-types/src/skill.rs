use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillDto {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: String,
    pub location: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillIssueDto {
    pub root: String,
    pub path: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillCatalogDto {
    pub skills: Vec<SkillDto>,
    pub issues: Vec<SkillIssueDto>,
}
