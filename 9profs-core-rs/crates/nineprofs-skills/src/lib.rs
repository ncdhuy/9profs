//! Skill discovery and resolution for 9Profs Core.
//!
//! Skills are Markdown resources. This crate only catalogs and reads them;
//! it does not materialize or execute an agent workspace.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const SKILL_FILE_NAME: &str = "SKILL.md";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SkillSource {
    Builtin,
    Custom,
    Extension,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SkillLocation {
    Embedded { path: String },
    Filesystem { root: PathBuf, directory: PathBuf },
}

impl SkillLocation {
    pub fn display_path(&self) -> String {
        match self {
            Self::Embedded { path } => path.clone(),
            Self::Filesystem { directory, .. } => directory.display().to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: SkillSource,
    pub location: SkillLocation,
    pub content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillIssue {
    pub root: PathBuf,
    pub path: Option<PathBuf>,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SkillScan {
    pub skills: Vec<Skill>,
    pub issues: Vec<SkillIssue>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SkillParseError {
    #[error("SKILL.md must start with YAML frontmatter")]
    MissingFrontmatter,
    #[error("SKILL.md frontmatter is not closed")]
    UnclosedFrontmatter,
    #[error("SKILL.md frontmatter line {line} is malformed")]
    MalformedFrontmatter { line: usize },
    #[error("SKILL.md frontmatter field `{0}` is required")]
    MissingField(&'static str),
    #[error("SKILL.md skill name `{0}` is invalid")]
    InvalidName(String),
}

#[derive(Debug, Error)]
pub enum SkillError {
    #[error("builtin skill catalog is invalid: {0}")]
    Builtin(String),
}

/// Provider boundary for builtin, custom, and future extension skill sources.
pub trait SkillProvider: Send + Sync {
    fn source(&self) -> SkillSource;
    fn scan(&self) -> SkillScan;
}

#[derive(Clone, Debug)]
pub struct BuiltinSkillProvider {
    skills: Vec<Skill>,
}

impl BuiltinSkillProvider {
    pub fn new() -> Result<Self, SkillError> {
        let entries = [
            (
                "document-foundation",
                include_str!("../assets/builtin/document-foundation/SKILL.md"),
            ),
            (
                "writing-foundation",
                include_str!("../assets/builtin/writing-foundation/SKILL.md"),
            ),
        ];

        let mut skills = Vec::with_capacity(entries.len());
        for (asset_name, content) in entries {
            let skill = parse_skill_md(
                content,
                SkillSource::Builtin,
                SkillLocation::Embedded {
                    path: format!("builtin/{asset_name}/{SKILL_FILE_NAME}"),
                },
            )
            .map_err(|error| SkillError::Builtin(format!("{asset_name}: {error}")))?;
            skills.push(skill);
        }

        skills.sort_by(|left, right| left.id.cmp(&right.id));
        let mut ids = BTreeSet::new();
        if skills.iter().any(|skill| !ids.insert(skill.id.clone())) {
            return Err(SkillError::Builtin("duplicate skill id".to_owned()));
        }

        Ok(Self { skills })
    }
}

impl SkillProvider for BuiltinSkillProvider {
    fn source(&self) -> SkillSource {
        SkillSource::Builtin
    }

    fn scan(&self) -> SkillScan {
        SkillScan {
            skills: self.skills.clone(),
            issues: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CustomSkillProvider {
    roots: Vec<PathBuf>,
}

impl CustomSkillProvider {
    /// Roots are ordered from highest to lowest custom precedence.
    pub fn new(roots: Vec<PathBuf>) -> Self {
        Self { roots }
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }
}

impl SkillProvider for CustomSkillProvider {
    fn source(&self) -> SkillSource {
        SkillSource::Custom
    }

    fn scan(&self) -> SkillScan {
        let mut selected = BTreeMap::new();
        let mut issues = Vec::new();
        for root in &self.roots {
            let mut root_scan = SkillScan::default();
            scan_root(root, &mut root_scan);
            issues.extend(root_scan.issues);
            for skill in root_scan.skills {
                selected.entry(skill.id.clone()).or_insert(skill);
            }
        }
        let mut scan = SkillScan {
            skills: selected.into_values().collect(),
            issues,
        };
        scan.issues.sort_by(|left, right| {
            left.root
                .cmp(&right.root)
                .then_with(|| left.path.cmp(&right.path))
                .then_with(|| left.message.cmp(&right.message))
        });
        scan
    }
}

/// No-op extension provider. Dynamic extension loading is deferred, while the
/// `SkillProvider` boundary already accepts a future extension implementation.
#[derive(Clone, Copy, Debug, Default)]
pub struct EmptyExtensionSkillProvider;

impl SkillProvider for EmptyExtensionSkillProvider {
    fn source(&self) -> SkillSource {
        SkillSource::Extension
    }

    fn scan(&self) -> SkillScan {
        SkillScan::default()
    }
}

#[derive(Clone)]
pub struct SkillCatalog {
    providers: Vec<Arc<dyn SkillProvider>>,
}

impl SkillCatalog {
    pub fn new(providers: Vec<Arc<dyn SkillProvider>>) -> Self {
        Self { providers }
    }

    pub fn with_configured_roots(roots: Vec<PathBuf>) -> Result<Self, SkillError> {
        Ok(Self::new(vec![
            Arc::new(CustomSkillProvider::new(roots)),
            Arc::new(EmptyExtensionSkillProvider),
            Arc::new(BuiltinSkillProvider::new()?),
        ]))
    }

    /// Return selected skills and scan issues. Precedence is custom, then
    /// extension, then builtin. Within custom roots, earlier configured roots
    /// win; path sorting makes duplicate discovery deterministic.
    pub fn scan(&self) -> SkillScan {
        let mut selected = BTreeMap::<String, (u8, usize, String, Skill)>::new();
        let mut issues = Vec::new();

        for (provider_index, provider) in self.providers.iter().enumerate() {
            let scan = provider.scan();
            issues.extend(scan.issues);
            let rank = source_rank(&provider.source());
            for skill in scan.skills {
                let path = skill.location.display_path();
                let candidate = (rank, provider_index, path, skill);
                selected
                    .entry(candidate.3.id.clone())
                    .and_modify(|current| {
                        if (candidate.0, candidate.1, &candidate.2)
                            < (current.0, current.1, &current.2)
                        {
                            *current = candidate.clone();
                        }
                    })
                    .or_insert(candidate);
            }
        }

        SkillScan {
            skills: selected
                .into_values()
                .map(|(_, _, _, skill)| skill)
                .collect(),
            issues,
        }
    }

    pub fn list(&self) -> SkillScan {
        self.scan()
    }

    pub fn resolve(&self, id: &str) -> Option<Skill> {
        self.scan().skills.into_iter().find(|skill| skill.id == id)
    }
}

fn source_rank(source: &SkillSource) -> u8 {
    match source {
        SkillSource::Custom => 0,
        SkillSource::Extension => 1,
        SkillSource::Builtin => 2,
    }
}

pub fn parse_skill_md(
    content: &str,
    source: SkillSource,
    location: SkillLocation,
) -> Result<Skill, SkillParseError> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let mut lines = content.lines();
    if lines.next().map(str::trim) != Some("---") {
        return Err(SkillParseError::MissingFrontmatter);
    }

    let mut name = None;
    let mut description = None;
    let mut closed = false;
    for (index, line) in lines.enumerate() {
        let line_number = index + 2;
        if line.trim() == "---" {
            closed = true;
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            return Err(SkillParseError::MalformedFrontmatter { line: line_number });
        };
        let value = trim_yaml_scalar(value.trim());
        match key.trim() {
            "name" => name = Some(value),
            "description" => description = Some(value),
            _ => {}
        }
    }

    if !closed {
        return Err(SkillParseError::UnclosedFrontmatter);
    }
    let name = name
        .filter(|value| !value.is_empty())
        .ok_or(SkillParseError::MissingField("name"))?;
    let description = description
        .filter(|value| !value.is_empty())
        .ok_or(SkillParseError::MissingField("description"))?;
    validate_skill_id(&name)?;

    Ok(Skill {
        id: name.clone(),
        name,
        description,
        source,
        location,
        content: content.to_owned(),
    })
}

fn trim_yaml_scalar(value: &str) -> String {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(value)
        .trim()
        .to_owned()
}

fn validate_skill_id(id: &str) -> Result<(), SkillParseError> {
    if id.is_empty()
        || id == "."
        || id == ".."
        || id.chars().any(|char| {
            !(char.is_ascii_lowercase() || char.is_ascii_digit() || matches!(char, '-' | '_' | '.'))
        })
    {
        return Err(SkillParseError::InvalidName(id.to_owned()));
    }
    Ok(())
}

fn scan_root(root: &Path, scan: &mut SkillScan) {
    let canonical_root = match fs::canonicalize(root) {
        Ok(path) => path,
        Err(error) => {
            scan.issues.push(SkillIssue {
                root: root.to_owned(),
                path: None,
                message: format!("cannot access configured root: {error}"),
            });
            return;
        }
    };
    scan_directory(root, &canonical_root, &canonical_root, scan);
}

fn scan_directory(root: &Path, canonical_root: &Path, directory: &Path, scan: &mut SkillScan) {
    let mut entries = match fs::read_dir(directory) {
        Ok(entries) => entries.filter_map(Result::ok).collect::<Vec<_>>(),
        Err(error) => {
            scan.issues.push(SkillIssue {
                root: root.to_owned(),
                path: Some(directory.to_owned()),
                message: format!("cannot scan directory: {error}"),
            });
            return;
        }
    };
    entries.sort_by_key(|entry| entry.path());

    let skill_file = directory.join(SKILL_FILE_NAME);
    if skill_file.is_file() {
        let canonical_skill_file = match fs::canonicalize(&skill_file) {
            Ok(path) => path,
            Err(error) => {
                scan.issues.push(SkillIssue {
                    root: root.to_owned(),
                    path: Some(skill_file),
                    message: format!("cannot resolve SKILL.md: {error}"),
                });
                return;
            }
        };
        let Some(skill_directory) = canonical_skill_file.parent() else {
            return;
        };
        if !skill_directory.starts_with(canonical_root) {
            scan.issues.push(SkillIssue {
                root: root.to_owned(),
                path: Some(skill_file),
                message: "SKILL.md resolves outside configured root".to_owned(),
            });
            return;
        }
        match fs::read_to_string(&canonical_skill_file).and_then(|content| {
            parse_skill_md(
                &content,
                SkillSource::Custom,
                SkillLocation::Filesystem {
                    root: canonical_root.to_owned(),
                    directory: skill_directory.to_owned(),
                },
            )
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
        }) {
            Ok(skill) => scan.skills.push(skill),
            Err(error) => scan.issues.push(SkillIssue {
                root: root.to_owned(),
                path: Some(canonical_skill_file),
                message: error.to_string(),
            }),
        }
        return;
    }

    for entry in entries {
        let path = entry.path();
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            continue;
        }
        let Ok(canonical_path) = fs::canonicalize(&path) else {
            continue;
        };
        if canonical_path.starts_with(canonical_root) {
            scan_directory(root, canonical_root, &canonical_path, scan);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tempdir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "nineprofs-skills-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn skill(name: &str, description: &str) -> String {
        format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n")
    }

    #[test]
    fn parses_valid_skill_frontmatter_and_preserves_content() {
        let content = skill("demo-skill", "A demo skill");
        let parsed = parse_skill_md(
            &content,
            SkillSource::Custom,
            SkillLocation::Embedded {
                path: "demo".into(),
            },
        )
        .unwrap();
        assert_eq!(parsed.id, "demo-skill");
        assert_eq!(parsed.description, "A demo skill");
        assert_eq!(parsed.content, content);
    }

    #[test]
    fn rejects_malformed_skill_deterministically() {
        let error = parse_skill_md(
            "# no frontmatter",
            SkillSource::Custom,
            SkillLocation::Embedded {
                path: "demo".into(),
            },
        )
        .unwrap_err();
        assert_eq!(error, SkillParseError::MissingFrontmatter);
    }

    #[test]
    fn scans_configured_roots_only_and_reports_invalid_skills() {
        let root = tempdir();
        let valid = root.join("valid");
        let invalid = root.join("invalid");
        fs::create_dir_all(&valid).unwrap();
        fs::create_dir_all(&invalid).unwrap();
        fs::write(valid.join(SKILL_FILE_NAME), skill("valid-skill", "valid")).unwrap();
        fs::write(invalid.join(SKILL_FILE_NAME), "---\nname: invalid\n---\n").unwrap();

        let scan = CustomSkillProvider::new(vec![root.clone()]).scan();
        assert_eq!(
            scan.skills
                .iter()
                .map(|skill| skill.id.as_str())
                .collect::<Vec<_>>(),
            ["valid-skill"]
        );
        assert_eq!(scan.issues.len(), 1);
        assert!(
            scan.issues[0]
                .path
                .as_ref()
                .unwrap()
                .ends_with("invalid\\SKILL.md")
                || scan.issues[0]
                    .path
                    .as_ref()
                    .unwrap()
                    .ends_with("invalid/SKILL.md")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn custom_skills_override_builtin_skills() {
        let root = tempdir();
        let custom = root.join("document");
        fs::create_dir_all(&custom).unwrap();
        fs::write(
            custom.join(SKILL_FILE_NAME),
            skill("document-foundation", "custom"),
        )
        .unwrap();

        let catalog = SkillCatalog::new(vec![
            Arc::new(CustomSkillProvider::new(vec![root.clone()])),
            Arc::new(BuiltinSkillProvider::new().unwrap()),
        ]);
        let resolved = catalog.resolve("document-foundation").unwrap();
        assert_eq!(resolved.source, SkillSource::Custom);
        assert_eq!(resolved.description, "custom");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn duplicate_custom_roots_choose_first_configured_root() {
        let root = tempdir();
        let first = root.join("first");
        let second = root.join("second");
        for directory in [&first, &second] {
            fs::create_dir_all(directory.join("demo")).unwrap();
        }
        fs::write(
            first.join("demo").join(SKILL_FILE_NAME),
            skill("demo", "first"),
        )
        .unwrap();
        fs::write(
            second.join("demo").join(SKILL_FILE_NAME),
            skill("demo", "second"),
        )
        .unwrap();

        let scan = CustomSkillProvider::new(vec![first, second]).scan();
        let catalog = SkillCatalog::new(vec![Arc::new(CustomSkillProvider::new(vec![
            root.join("first"),
            root.join("second"),
        ]))]);
        assert_eq!(scan.skills.len(), 1);
        assert_eq!(catalog.resolve("demo").unwrap().description, "first");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn extension_source_overrides_builtin_source() {
        #[derive(Clone)]
        struct ExtensionFixture;

        impl SkillProvider for ExtensionFixture {
            fn source(&self) -> SkillSource {
                SkillSource::Extension
            }

            fn scan(&self) -> SkillScan {
                SkillScan {
                    skills: vec![Skill {
                        id: "document-foundation".to_owned(),
                        name: "document-foundation".to_owned(),
                        description: "extension".to_owned(),
                        source: SkillSource::Extension,
                        location: SkillLocation::Embedded {
                            path: "extension/document-foundation/SKILL.md".to_owned(),
                        },
                        content: "extension".to_owned(),
                    }],
                    issues: Vec::new(),
                }
            }
        }

        let catalog = SkillCatalog::new(vec![
            Arc::new(ExtensionFixture),
            Arc::new(BuiltinSkillProvider::new().unwrap()),
        ]);
        assert_eq!(
            catalog.resolve("document-foundation").unwrap().source,
            SkillSource::Extension
        );
    }
}
