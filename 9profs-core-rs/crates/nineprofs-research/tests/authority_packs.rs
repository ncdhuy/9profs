use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Deserialize;

const EXPECTED_PACK_IDS: [&str; 4] = [
    "research.core",
    "artifact.master-thesis",
    "domain.med",
    "editorial.vi",
];

#[derive(Debug, Deserialize)]
struct PackManifest {
    id: String,
    version: String,
    kind: String,
    title: String,
    description: String,
    applicability: BTreeMap<String, Vec<String>>,
    content: ContentManifest,
    provenance: BTreeMap<String, serde_yaml::Value>,
}

#[derive(Debug, Deserialize)]
struct ContentManifest {
    knowledge: Vec<String>,
    review_guidance: Vec<String>,
    machine_facts: Vec<serde_yaml::Value>,
}

#[derive(Debug)]
struct ValidatedPack {
    manifest: PackManifest,
    knowledge_files: Vec<PathBuf>,
    review_guidance_files: Vec<PathBuf>,
}

fn authority_pack_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/authority-packs")
}

fn discover_pack_dirs(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut directories = fs::read_dir(root)
        .map_err(|error| {
            format!(
                "cannot read authority-pack root {}: {error}",
                root.display()
            )
        })?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("cannot read authority-pack entry: {error}"))?;
    directories.retain(|path| path.is_dir());
    directories.sort();
    Ok(directories)
}

fn validate_all(root: &Path) -> Result<Vec<ValidatedPack>, String> {
    let directories = discover_pack_dirs(root)?;
    if directories.is_empty() {
        return Err(format!("no authority packs found in {}", root.display()));
    }

    let mut ids = BTreeSet::new();
    let mut packs = Vec::with_capacity(directories.len());
    for directory in directories {
        let pack = validate_pack(&directory)?;
        if !ids.insert(pack.manifest.id.clone()) {
            return Err(format!(
                "duplicate authority-pack id `{}`",
                pack.manifest.id
            ));
        }
        packs.push(pack);
    }
    Ok(packs)
}

fn validate_pack(directory: &Path) -> Result<ValidatedPack, String> {
    let manifest_path = directory.join("pack.yaml");
    let manifest_bytes = fs::read(&manifest_path)
        .map_err(|error| format!("cannot read {}: {error}", manifest_path.display()))?;
    let manifest: PackManifest = serde_yaml::from_slice(&manifest_bytes)
        .map_err(|error| format!("cannot parse {}: {error}", manifest_path.display()))?;

    for (field, value) in [
        ("id", &manifest.id),
        ("version", &manifest.version),
        ("kind", &manifest.kind),
        ("title", &manifest.title),
        ("description", &manifest.description),
    ] {
        if value.trim().is_empty() {
            return Err(format!("{} has empty {field}", manifest_path.display()));
        }
    }
    if manifest
        .applicability
        .keys()
        .any(|dimension| dimension.trim().is_empty())
        || manifest
            .provenance
            .keys()
            .any(|field| field.trim().is_empty())
    {
        return Err(format!(
            "{} has an empty applicability or provenance key",
            manifest_path.display()
        ));
    }

    let knowledge_files =
        validate_markdown_refs(directory, "knowledge", &manifest.content.knowledge)?;
    let review_guidance_files = validate_markdown_refs(
        directory,
        "review_guidance",
        &manifest.content.review_guidance,
    )?;

    Ok(ValidatedPack {
        manifest,
        knowledge_files,
        review_guidance_files,
    })
}

fn validate_markdown_refs(
    directory: &Path,
    field: &str,
    references: &[String],
) -> Result<Vec<PathBuf>, String> {
    references
        .iter()
        .map(|reference| {
            let relative_path = Path::new(reference);
            if relative_path.is_absolute()
                || relative_path.components().any(|component| {
                    matches!(
                        component,
                        Component::ParentDir | Component::RootDir | Component::Prefix(_)
                    )
                })
            {
                return Err(format!(
                    "{}/pack.yaml {field} reference is not relative: {reference}",
                    directory.display()
                ));
            }

            let path = directory.join(relative_path);
            if !path.is_file() {
                return Err(format!(
                    "{}/pack.yaml {field} reference is missing: {reference}",
                    directory.display()
                ));
            }

            let bytes = fs::read(&path)
                .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
            let text = String::from_utf8(bytes)
                .map_err(|error| format!("{} is not UTF-8: {error}", path.display()))?;
            if text.trim().is_empty() {
                return Err(format!("{} is empty", path.display()));
            }
            Ok(path)
        })
        .collect()
}

#[test]
fn all_expected_packs_are_discoverable_and_ids_are_unique() {
    let packs = validate_all(&authority_pack_root()).unwrap();
    let ids = packs
        .iter()
        .map(|pack| pack.manifest.id.as_str())
        .collect::<BTreeSet<_>>();

    assert_eq!(ids, EXPECTED_PACK_IDS.into_iter().collect());
    assert_eq!(packs.len(), EXPECTED_PACK_IDS.len());
}

#[test]
fn manifests_and_referenced_markdown_assets_are_structurally_valid() {
    let packs = validate_all(&authority_pack_root()).unwrap();

    assert_eq!(
        packs
            .iter()
            .map(|pack| pack.knowledge_files.len())
            .sum::<usize>(),
        25
    );
    assert_eq!(
        packs
            .iter()
            .map(|pack| pack.review_guidance_files.len())
            .sum::<usize>(),
        23
    );
    assert!(
        packs
            .iter()
            .all(|pack| pack.manifest.content.machine_facts.is_empty())
    );
}

#[test]
fn future_kind_values_are_not_blocked_by_a_closed_enum() {
    let fixture = Fixture::new(
        r#"id: future.example
version: 0.1.0
kind: future_kind
title: Future kind
description: Future kind fixture
applicability:
  study_designs:
    - future
content:
  knowledge:
    - knowledge/knowledge.md
  review_guidance:
    - review/guidance.md
  machine_facts: []
provenance:
  type: test
"#,
        &["knowledge/knowledge.md", "review/guidance.md"],
    );

    assert!(validate_pack(&fixture.pack_dir).is_ok());
}

#[test]
fn dangling_content_reference_is_rejected() {
    let fixture = Fixture::new(
        r#"id: broken.example
version: 0.1.0
kind: custom
title: Broken pack
description: Broken pack fixture
applicability: {}
content:
  knowledge:
    - knowledge/missing.md
  review_guidance:
    - review/guidance.md
  machine_facts: []
provenance:
  type: test
"#,
        &["review/guidance.md"],
    );

    let error = validate_pack(&fixture.pack_dir).unwrap_err();
    assert!(error.contains("knowledge reference is missing"), "{error}");
}

struct Fixture {
    root: PathBuf,
    pack_dir: PathBuf,
}

impl Fixture {
    fn new(manifest: &str, files: &[&str]) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("nineprofs-authority-packs-{unique}"));
        let pack_dir = root.join("pack");
        fs::create_dir_all(&pack_dir).unwrap();
        fs::write(pack_dir.join("pack.yaml"), manifest).unwrap();
        for file in files {
            let path = pack_dir.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "fixture content\n").unwrap();
        }
        Self { root, pack_dir }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
