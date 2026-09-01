use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    ContentHash, DocumentMap, DocumentMapLocator, EvidenceLocator, HashAlgorithm,
    RegulationApplicability, RegulationRequirement, RegulationRequirementId, ResearchContext,
    ResearchError, ResearchSourceId, ResearchSourceSnapshotId,
    resolve_effective_regulation_requirements,
};

pub const REVIEW_TASK_CONTRACT_VERSION: &str = "review-task-v1";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorityPackDocument {
    pub path: String,
    pub content: String,
    pub content_hash: ContentHash,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorityPackSource {
    pub manifest_path: String,
    pub manifest_hash: ContentHash,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorityPack {
    pub id: String,
    pub version: String,
    pub kind: String,
    pub title: String,
    pub description: String,
    pub applicability: BTreeMap<String, Vec<String>>,
    pub knowledge: Vec<AuthorityPackDocument>,
    pub review_guidance: Vec<AuthorityPackDocument>,
    pub machine_facts: Vec<serde_json::Value>,
    pub provenance: BTreeMap<String, serde_json::Value>,
    pub source: AuthorityPackSource,
}

impl AuthorityPack {
    fn matches(&self, context: &ResearchContext) -> bool {
        RegulationApplicability {
            facets: self.applicability.clone(),
        }
        .matches(context)
    }

    fn reference(&self) -> ReviewAuthorityReference {
        ReviewAuthorityReference::AuthorityPack {
            pack_id: self.id.clone(),
            version: self.version.clone(),
            source: self.source.clone(),
            content_paths: self
                .knowledge
                .iter()
                .chain(self.review_guidance.iter())
                .map(|document| document.path.clone())
                .collect(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AuthorityPackLoader;

impl AuthorityPackLoader {
    pub fn new() -> Self {
        Self
    }

    pub fn canonical() -> Self {
        Self
    }

    pub fn load(&self) -> Result<Vec<AuthorityPack>, ResearchError> {
        let mut packs = CANONICAL_PACKS
            .iter()
            .map(load_embedded_pack)
            .collect::<Result<Vec<_>, _>>()?;
        let mut ids = BTreeSet::new();
        for pack in &packs {
            if !ids.insert(pack.id.clone()) {
                return Err(loader_error(format!("duplicate pack id `{}`", pack.id)));
            }
        }
        packs.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(packs)
    }
}

pub fn load_canonical_authority_packs() -> Result<Vec<AuthorityPack>, ResearchError> {
    AuthorityPackLoader::canonical().load()
}

struct EmbeddedPackSpec {
    manifest_path: &'static str,
    manifest: &'static str,
    knowledge: &'static [(&'static str, &'static str)],
    review_guidance: &'static [(&'static str, &'static str)],
}

macro_rules! embedded_documents {
    ($pack:literal; $( $path:literal ),+ $(,)?) => {
        &[
            $(
                (
                    $path,
                    include_str!(concat!(
                        "../assets/authority-packs/",
                        $pack,
                        "/",
                        $path
                    )),
                ),
            )+
        ]
    };
}

const CANONICAL_PACKS: &[EmbeddedPackSpec] = &[
    EmbeddedPackSpec {
        manifest_path: "authority-packs/research.core/pack.yaml",
        manifest: include_str!("../assets/authority-packs/research.core/pack.yaml"),
        knowledge: embedded_documents!(
            "research.core";
            "knowledge/research-logic.md",
            "knowledge/claims-and-evidence.md",
            "knowledge/interpretation-and-conclusions.md",
            "knowledge/cross-section-consistency.md",
            "knowledge/uncertainty-and-limitations.md",
            "knowledge/integrity-and-transparency.md",
        ),
        review_guidance: embedded_documents!(
            "research.core";
            "review/common-review-policy.md",
            "review/research-purpose-and-logic.md",
            "review/claim-evidence-review.md",
            "review/interpretation-review.md",
            "review/cross-section-review.md",
            "review/integrity-review.md",
        ),
    },
    EmbeddedPackSpec {
        manifest_path: "authority-packs/editorial.vi/pack.yaml",
        manifest: include_str!("../assets/authority-packs/editorial.vi/pack.yaml"),
        knowledge: embedded_documents!(
            "editorial.vi";
            "knowledge/academic-vietnamese-style.md",
            "knowledge/sentence-and-paragraph-clarity.md",
            "knowledge/terminology-and-consistency.md",
            "knowledge/numbers-units-and-abbreviations.md",
            "knowledge/headings-tables-and-figures.md",
            "knowledge/citations-and-reference-language.md",
        ),
        review_guidance: embedded_documents!(
            "editorial.vi";
            "review/editorial-review-policy.md",
            "review/clarity-and-style-review.md",
            "review/terminology-consistency-review.md",
            "review/presentation-consistency-review.md",
            "review/tables-and-figures-review.md",
        ),
    },
    EmbeddedPackSpec {
        manifest_path: "authority-packs/domain.med/pack.yaml",
        manifest: include_str!("../assets/authority-packs/domain.med/pack.yaml"),
        knowledge: embedded_documents!(
            "domain.med";
            "knowledge/biomedical-research-logic.md",
            "knowledge/population-and-sampling.md",
            "knowledge/variables-exposures-outcomes.md",
            "knowledge/measurement-and-data-quality.md",
            "knowledge/bias-confounding-and-validity.md",
            "knowledge/analysis-and-statistical-interpretation.md",
            "knowledge/ethics-and-human-subjects.md",
            "knowledge/clinical-and-health-interpretation.md",
        ),
        review_guidance: embedded_documents!(
            "domain.med";
            "review/medical-review-policy.md",
            "review/design-and-population-review.md",
            "review/measurement-and-variables-review.md",
            "review/validity-and-bias-review.md",
            "review/analysis-and-results-review.md",
            "review/discussion-and-conclusion-review.md",
            "review/ethics-review.md",
        ),
    },
    EmbeddedPackSpec {
        manifest_path: "authority-packs/artifact.master-thesis/pack.yaml",
        manifest: include_str!("../assets/authority-packs/artifact.master-thesis/pack.yaml"),
        knowledge: embedded_documents!(
            "artifact.master-thesis";
            "knowledge/thesis-purpose-and-depth.md",
            "knowledge/semantic-structure.md",
            "knowledge/literature-and-conceptual-foundation.md",
            "knowledge/methods-results-discussion-roles.md",
            "knowledge/contribution-and-completion.md",
        ),
        review_guidance: embedded_documents!(
            "artifact.master-thesis";
            "review/artifact-review-policy.md",
            "review/structure-and-completeness.md",
            "review/literature-foundation-review.md",
            "review/depth-and-independence-review.md",
            "review/thesis-synthesis-review.md",
        ),
    },
];

#[derive(Clone, Debug, Deserialize)]
struct PackManifest {
    id: String,
    version: String,
    kind: String,
    title: String,
    description: String,
    #[serde(default)]
    applicability: BTreeMap<String, Vec<String>>,
    content: PackContentManifest,
    #[serde(default)]
    provenance: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize)]
struct PackContentManifest {
    #[serde(default)]
    knowledge: Vec<String>,
    #[serde(default)]
    review_guidance: Vec<String>,
    #[serde(default)]
    machine_facts: Vec<serde_json::Value>,
}

fn load_embedded_pack(spec: &EmbeddedPackSpec) -> Result<AuthorityPack, ResearchError> {
    let manifest: PackManifest = serde_yaml::from_str(spec.manifest)
        .map_err(|error| loader_error(format!("cannot parse {}: {error}", spec.manifest_path)))?;

    for (field, value) in [
        ("id", &manifest.id),
        ("version", &manifest.version),
        ("kind", &manifest.kind),
        ("title", &manifest.title),
        ("description", &manifest.description),
    ] {
        if value.trim().is_empty() {
            return Err(loader_error(format!(
                "{} has empty {field}",
                spec.manifest_path
            )));
        }
    }

    let applicability = RegulationApplicability {
        facets: manifest.applicability.clone(),
    };
    applicability.validate_context_facets()?;

    let knowledge =
        load_embedded_documents("knowledge", &manifest.content.knowledge, spec.knowledge)?;
    let review_guidance = load_embedded_documents(
        "review_guidance",
        &manifest.content.review_guidance,
        spec.review_guidance,
    )?;

    Ok(AuthorityPack {
        id: manifest.id.clone(),
        version: manifest.version,
        kind: manifest.kind,
        title: manifest.title,
        description: manifest.description,
        applicability: manifest.applicability,
        knowledge,
        review_guidance,
        machine_facts: manifest.content.machine_facts,
        provenance: manifest.provenance,
        source: AuthorityPackSource {
            manifest_path: spec.manifest_path.to_owned(),
            manifest_hash: sha256_hash(spec.manifest.as_bytes()),
        },
    })
}

fn load_embedded_documents(
    field: &str,
    references: &[String],
    assets: &[(&str, &str)],
) -> Result<Vec<AuthorityPackDocument>, ResearchError> {
    references
        .iter()
        .map(|reference| {
            let Some((_, content)) = assets.iter().find(|(path, _)| *path == reference) else {
                return Err(loader_error(format!(
                    "{field} reference is missing: {reference}"
                )));
            };
            if content.trim().is_empty() {
                return Err(loader_error(format!(
                    "{field} reference is empty: {reference}"
                )));
            }

            Ok(AuthorityPackDocument {
                path: reference.clone(),
                content_hash: sha256_hash(content.as_bytes()),
                content: (*content).to_owned(),
            })
        })
        .collect()
}

fn loader_error(message: String) -> ResearchError {
    ResearchError::Invalid(format!("authority pack loader: {message}"))
}

fn sha256_hash(value: &[u8]) -> ContentHash {
    let digest = Sha256::digest(value);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        write!(&mut hex, "{byte:02x}").expect("writing to String cannot fail");
    }
    ContentHash {
        algorithm: HashAlgorithm::Sha256,
        value: hex,
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResolvedReviewStack {
    pub research_context: ResearchContext,
    pub authority_packs: Vec<AuthorityPack>,
    pub regulation_requirements: Vec<RegulationRequirement>,
}

pub fn resolve_authority_packs(
    packs: &[AuthorityPack],
    context: &ResearchContext,
) -> Result<Vec<AuthorityPack>, ResearchError> {
    context.validate()?;
    Ok(packs
        .iter()
        .filter(|pack| pack.matches(context))
        .cloned()
        .collect())
}

pub fn resolve_review_stack(
    context: &ResearchContext,
    packs: &[AuthorityPack],
    regulation_requirements: &[RegulationRequirement],
    as_of_ms: i64,
) -> Result<ResolvedReviewStack, ResearchError> {
    Ok(ResolvedReviewStack {
        research_context: context.clone(),
        authority_packs: resolve_authority_packs(packs, context)?,
        regulation_requirements: resolve_effective_regulation_requirements(
            regulation_requirements,
            context,
            as_of_ms,
        ),
    })
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewExecutorMode {
    Deterministic,
    Semantic,
    Hybrid,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewSectionRole {
    Methodology,
    Results,
    Discussion,
    Conclusion,
    Unclassified,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewTaskTarget {
    pub document_map_contract_version: String,
    pub document_id: String,
    pub document_version: i64,
    pub section_ids: Vec<String>,
    pub locators: Vec<DocumentMapLocator>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegulationRequirementReference {
    pub requirement_id: RegulationRequirementId,
    pub source_id: ResearchSourceId,
    pub source_snapshot_id: ResearchSourceSnapshotId,
    pub authority_locator: Option<EvidenceLocator>,
    pub normalized_requirement: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReviewAuthorityReference {
    AuthorityPack {
        pack_id: String,
        version: String,
        source: AuthorityPackSource,
        content_paths: Vec<String>,
    },
    RegulationRequirement {
        reference: RegulationRequirementReference,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewTask {
    pub contract_version: String,
    pub id: String,
    pub kind: String,
    pub executor_mode: ReviewExecutorMode,
    pub target: ReviewTaskTarget,
    pub instruction: String,
    pub authority_references: Vec<ReviewAuthorityReference>,
}

pub fn classify_heading_role(heading: &str) -> ReviewSectionRole {
    let normalized = normalize_heading(heading);
    if matches_alias(
        &normalized,
        &[
            "phương pháp nghiên cứu",
            "đối tượng và phương pháp",
            "đối tượng và phương pháp nghiên cứu",
        ],
    ) {
        return ReviewSectionRole::Methodology;
    }
    if matches_alias(&normalized, &["kết quả", "kết quả nghiên cứu"]) {
        return ReviewSectionRole::Results;
    }
    if matches_alias(&normalized, &["bàn luận", "thảo luận"]) {
        return ReviewSectionRole::Discussion;
    }
    if matches_alias(&normalized, &["kết luận"]) {
        return ReviewSectionRole::Conclusion;
    }
    ReviewSectionRole::Unclassified
}

fn normalize_heading(value: &str) -> String {
    let words = value
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || character.is_whitespace() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut start = 0;
    if words.first().is_some_and(|word| word == "chương") {
        start = 1;
    }
    while words
        .get(start)
        .is_some_and(|word| word.chars().all(|character| character.is_ascii_digit()))
    {
        start += 1;
    }
    words[start..].join(" ")
}

fn matches_alias(normalized: &str, aliases: &[&str]) -> bool {
    aliases.contains(&normalized)
}

pub fn plan_review_tasks(
    context: &ResearchContext,
    map: &DocumentMap,
    stack: &ResolvedReviewStack,
) -> Result<Vec<ReviewTask>, ResearchError> {
    context.validate()?;
    if map.contract_version != crate::DOCUMENT_MAP_CONTRACT_VERSION {
        return Err(ResearchError::Invalid(format!(
            "unsupported document map contract version: {}",
            map.contract_version
        )));
    }
    if stack.research_context != *context {
        return Err(ResearchError::Invalid(
            "resolved review stack context does not match planner context".to_owned(),
        ));
    }

    let visible_blocks = map.blocks.iter().filter(|block| !block.is_deleted);
    if visible_blocks.clone().next().is_none() {
        return Ok(Vec::new());
    }

    let mut tasks = Vec::new();
    let document_scope = ReviewTaskTarget {
        document_map_contract_version: map.contract_version.clone(),
        document_id: map.document_id.clone(),
        document_version: map.version,
        section_ids: map
            .sections
            .iter()
            .filter(|section| !section.is_deleted)
            .map(|section| section.id.clone())
            .collect(),
        locators: map
            .sections
            .iter()
            .filter(|section| !section.is_deleted)
            .map(|section| section.locator.clone())
            .collect(),
    };

    tasks.push(make_task(
        "review.manuscript.research-coherence",
        "Review research purpose, evidence, interpretation, limitations, and cross-section coherence.",
        ReviewExecutorMode::Semantic,
        "review.manuscript.research-coherence",
        document_scope.clone(),
        pack_references(stack, &["research.core", "artifact.master-thesis", "domain.med"]),
    ));

    let mut routed_sections = Vec::new();
    for section in &map.sections {
        if section.is_deleted || section.level > 1 {
            continue;
        }
        let role = classify_heading_role(&section.heading_text);
        let Some((kind, instruction)) = section_task_kind(&role) else {
            continue;
        };
        let locators = section
            .block_ids
            .iter()
            .filter_map(|block_id| {
                map.blocks
                    .iter()
                    .find(|block| block.id == *block_id && !block.is_deleted)
                    .map(|block| block.locator.clone())
            })
            .collect::<Vec<_>>();
        if locators.is_empty() {
            continue;
        }
        routed_sections.push(section);
        tasks.push(make_task(
            &format!("review.section.{kind}.{}", section.id),
            instruction,
            ReviewExecutorMode::Semantic,
            &format!("review.section.{kind}"),
            ReviewTaskTarget {
                document_map_contract_version: map.contract_version.clone(),
                document_id: map.document_id.clone(),
                document_version: map.version,
                section_ids: vec![section.id.clone()],
                locators,
            },
            pack_references(
                stack,
                &["research.core", "domain.med", "artifact.master-thesis"],
            ),
        ));
    }

    if routed_sections.len() > 1 {
        tasks.push(make_task(
            "review.manuscript.cross-section",
            "Review consistency of purpose, methodology, results, discussion, and conclusion across identified sections.",
            ReviewExecutorMode::Semantic,
            "review.manuscript.cross-section",
            ReviewTaskTarget {
                document_map_contract_version: map.contract_version.clone(),
                document_id: map.document_id.clone(),
                document_version: map.version,
                section_ids: routed_sections.iter().map(|section| section.id.clone()).collect(),
                locators: routed_sections
                    .iter()
                    .map(|section| section.locator.clone())
                    .collect(),
            },
            pack_references(stack, &["research.core", "artifact.master-thesis", "domain.med"]),
        ));
    }

    if context
        .language
        .as_deref()
        .is_some_and(|language| language.trim().eq_ignore_ascii_case("vi"))
    {
        tasks.push(make_task(
            "review.vi.language",
            "Review Vietnamese academic clarity, precision, coherence, and readability across the manuscript scope.",
            ReviewExecutorMode::Semantic,
            "review.vi.language",
            document_scope.clone(),
            pack_references(stack, &["editorial.vi", "artifact.master-thesis"]),
        ));
        tasks.push(make_task(
            "review.vi.terminology",
            "Review Vietnamese terminology, abbreviations, numbers, units, and usage consistency.",
            ReviewExecutorMode::Semantic,
            "review.vi.terminology",
            document_scope.clone(),
            pack_references(stack, &["editorial.vi", "artifact.master-thesis"]),
        ));
    }

    if !stack.regulation_requirements.is_empty() {
        tasks.push(make_task(
            "review.regulation.presentation",
            "Review manuscript presentation against applicable institutional requirements.",
            ReviewExecutorMode::Semantic,
            "review.regulation.presentation",
            document_scope,
            regulation_references(stack),
        ));
    }

    Ok(tasks)
}

fn section_task_kind(role: &ReviewSectionRole) -> Option<(&'static str, &'static str)> {
    match role {
        ReviewSectionRole::Methodology => Some((
            "methodology",
            "Review methodology against research design, population, variables, measurement, validity, ethics, and thesis method roles.",
        )),
        ReviewSectionRole::Results => Some((
            "results",
            "Review results reporting and interpretation against the identified methodology.",
        )),
        ReviewSectionRole::Discussion => Some((
            "discussion",
            "Review discussion, limitations, and interpretation against results and claims.",
        )),
        ReviewSectionRole::Conclusion => Some((
            "conclusion",
            "Review conclusion claims, contribution, limitations, and completion against the manuscript evidence.",
        )),
        ReviewSectionRole::Unclassified => None,
    }
}

fn make_task(
    id: &str,
    instruction: &str,
    executor_mode: ReviewExecutorMode,
    kind: &str,
    target: ReviewTaskTarget,
    authority_references: Vec<ReviewAuthorityReference>,
) -> ReviewTask {
    ReviewTask {
        contract_version: REVIEW_TASK_CONTRACT_VERSION.to_owned(),
        id: id.to_owned(),
        kind: kind.to_owned(),
        executor_mode,
        target,
        instruction: instruction.to_owned(),
        authority_references,
    }
}

fn pack_references(
    stack: &ResolvedReviewStack,
    pack_ids: &[&str],
) -> Vec<ReviewAuthorityReference> {
    pack_ids
        .iter()
        .filter_map(|pack_id| {
            stack
                .authority_packs
                .iter()
                .find(|pack| pack.id == *pack_id)
                .map(AuthorityPack::reference)
        })
        .collect()
}

fn regulation_references(stack: &ResolvedReviewStack) -> Vec<ReviewAuthorityReference> {
    stack
        .regulation_requirements
        .iter()
        .map(
            |requirement| ReviewAuthorityReference::RegulationRequirement {
                reference: RegulationRequirementReference {
                    requirement_id: requirement.id.clone(),
                    source_id: requirement.source_id.clone(),
                    source_snapshot_id: requirement.source_snapshot_id.clone(),
                    authority_locator: requirement.authority_locator.clone(),
                    normalized_requirement: requirement.text.clone(),
                },
            },
        )
        .collect()
}
