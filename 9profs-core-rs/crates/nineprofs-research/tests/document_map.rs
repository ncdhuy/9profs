use nineprofs_research::{
    DOCUMENT_MAP_CONTRACT_VERSION, DocumentMap, DocumentMapBlock, DocumentMapBlockKind,
    DocumentMapCitation, DocumentMapFigure, DocumentMapFigureType, DocumentMapLocator,
    DocumentMapReference, DocumentMapSection, DocumentMapTable, is_document_map_current,
    is_document_map_stale,
};

fn locator(block_id: &str, ordinal: u32, section_id: Option<&str>) -> DocumentMapLocator {
    DocumentMapLocator {
        document_id: "doc-1".to_owned(),
        version: 4,
        block_id: block_id.to_owned(),
        block_ordinal: ordinal,
        docx_index: Some(ordinal),
        section_id: section_id.map(str::to_owned),
    }
}

fn sample_map() -> DocumentMap {
    let section_locator = locator("b0", 0, Some("section:b0"));
    let paragraph_locator = locator("b1", 1, Some("section:b0"));
    DocumentMap {
        contract_version: DOCUMENT_MAP_CONTRACT_VERSION.to_owned(),
        document_id: "doc-1".to_owned(),
        version: 4,
        sections: vec![DocumentMapSection {
            id: "section:b0".to_owned(),
            heading_text: "CHƯƠNG 1".to_owned(),
            level: 1,
            parent_id: None,
            locator: section_locator,
            block_ids: vec!["b0".to_owned(), "b1".to_owned()],
            is_deleted: false,
        }],
        blocks: vec![
            DocumentMapBlock {
                id: "b0".to_owned(),
                ordinal: 0,
                kind: DocumentMapBlockKind::Heading,
                text: "CHƯƠNG 1".to_owned(),
                locator: locator("b0", 0, Some("section:b0")),
                section_id: Some("section:b0".to_owned()),
                heading_level: Some(1),
                caption: None,
                is_deleted: false,
            },
            DocumentMapBlock {
                id: "b1".to_owned(),
                ordinal: 1,
                kind: DocumentMapBlockKind::Paragraph,
                text: "Nội dung [1].".to_owned(),
                locator: paragraph_locator.clone(),
                section_id: Some("section:b0".to_owned()),
                heading_level: None,
                caption: None,
                is_deleted: false,
            },
        ],
        tables: vec![DocumentMapTable {
            id: "b2".to_owned(),
            locator: locator("b2", 2, Some("section:b0")),
            row_count: 2,
            column_count: 2,
            caption: None,
        }],
        figures: vec![DocumentMapFigure {
            id: "b3".to_owned(),
            locator: locator("b3", 3, Some("section:b0")),
            figure_type: DocumentMapFigureType::Image,
            caption: Some("Figure 1".to_owned()),
        }],
        citations: vec![DocumentMapCitation {
            id: "b1:citation:0".to_owned(),
            locator: paragraph_locator,
            text: "[1]".to_owned(),
            start: 8,
            end: 11,
            format: Some("WordNative".to_owned()),
        }],
        references: vec![DocumentMapReference {
            id: "reference:1".to_owned(),
            locator: locator("b4", 4, Some("section:b0")),
            text: "Reference 1".to_owned(),
        }],
    }
}

#[test]
fn document_map_round_trips_as_provider_neutral_camel_case_json() {
    let map = sample_map();
    let json = serde_json::to_value(&map).unwrap();

    assert_eq!(json["contractVersion"], "document-map-v1");
    assert_eq!(json["documentId"], "doc-1");
    assert_eq!(json["blocks"][0]["kind"], "heading");
    assert_eq!(json["blocks"][1]["locator"]["docxIndex"], 1);
    assert_eq!(json["citations"][0]["start"], 8);
    assert_eq!(serde_json::from_value::<DocumentMap>(json).unwrap(), map);
}

#[test]
fn document_map_version_is_the_staleness_boundary() {
    let map = sample_map();

    assert!(is_document_map_current(&map, "doc-1", 4));
    assert!(!is_document_map_current(&map, "doc-1", 5));
    assert!(is_document_map_stale(&map, "doc-1", 5));
    assert!(is_document_map_stale(&map, "other-doc", 4));
}
