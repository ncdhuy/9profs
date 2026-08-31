use serde::{Deserialize, Serialize};

pub const DOCUMENT_MAP_CONTRACT_VERSION: &str = "document-map-v1";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentMap {
    pub contract_version: String,
    pub document_id: String,
    pub version: i64,
    pub sections: Vec<DocumentMapSection>,
    pub blocks: Vec<DocumentMapBlock>,
    pub tables: Vec<DocumentMapTable>,
    pub figures: Vec<DocumentMapFigure>,
    pub citations: Vec<DocumentMapCitation>,
    pub references: Vec<DocumentMapReference>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentMapLocator {
    pub document_id: String,
    pub version: i64,
    pub block_id: String,
    pub block_ordinal: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docx_index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentMapSection {
    pub id: String,
    pub heading_text: String,
    pub level: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub locator: DocumentMapLocator,
    pub block_ids: Vec<String>,
    pub is_deleted: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentMapBlock {
    pub id: String,
    pub ordinal: u32,
    pub kind: DocumentMapBlockKind,
    pub text: String,
    pub locator: DocumentMapLocator,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading_level: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    pub is_deleted: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DocumentMapBlockKind {
    Paragraph,
    Heading,
    ListItem,
    Table,
    Figure,
    Other,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentMapTable {
    pub id: String,
    pub locator: DocumentMapLocator,
    pub row_count: u32,
    pub column_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentMapFigure {
    pub id: String,
    pub locator: DocumentMapLocator,
    pub figure_type: DocumentMapFigureType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DocumentMapFigureType {
    Image,
    Chart,
    Other,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentMapCitation {
    pub id: String,
    pub locator: DocumentMapLocator,
    pub text: String,
    /// Unicode scalar/code-point offsets within the containing block text.
    pub start: u32,
    pub end: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentMapReference {
    pub id: String,
    pub locator: DocumentMapLocator,
    pub text: String,
}

pub fn is_document_map_current(map: &DocumentMap, document_id: &str, version: i64) -> bool {
    map.document_id == document_id && map.version == version
}

pub fn is_document_map_stale(map: &DocumentMap, document_id: &str, version: i64) -> bool {
    !is_document_map_current(map, document_id, version)
}
