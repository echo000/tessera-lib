use porter_utils::HashExt;
use std::time::Duration;

const HASH_MASK: u64 = 0x7FFF_FFFF_FFFF_FFFF;

/// The searchable data for an asset.
pub struct SearchAsset {
    bone_count: u32,
    mesh_count: u32,
    frame_count: u32,
    frame_rate: u32,
    width: u32,
    height: u32,
    channels: u32,
    duration: Duration,
    /// Stable kind tag matching the order used by the asset list / sort
    /// (see `AssetKind::index` on the producing side). The `kind:` field
    /// in queries resolves names to these tags via `kind_alias_to_index`.
    kind_index: u8,
    name: String,
    hash: u64,
    bone_names: Vec<String>,
}

impl SearchAsset {
    pub const fn new(name: String) -> Self {
        Self {
            bone_count: 0,
            mesh_count: 0,
            frame_count: 0,
            frame_rate: 0,
            width: 0,
            height: 0,
            channels: 0,
            duration: Duration::ZERO,
            kind_index: u8::MAX,
            name,
            hash: 0,
            bone_names: Vec::new(),
        }
    }

    pub const fn hash(mut self, hash: u64) -> Self {
        self.hash = hash;
        self
    }

    pub const fn kind_index(mut self, index: u8) -> Self {
        self.kind_index = index;
        self
    }

    pub const fn bone_count(mut self, count: u32) -> Self {
        self.bone_count = count;
        self
    }

    pub fn bone_names(mut self, names: &[String]) -> Self {
        self.bone_names = names.to_vec();
        self
    }

    pub const fn mesh_count(mut self, count: u32) -> Self {
        self.mesh_count = count;
        self
    }

    pub const fn frame_count(mut self, count: u32) -> Self {
        self.frame_count = count;
        self
    }

    pub const fn frame_rate(mut self, rate: u32) -> Self {
        self.frame_rate = rate;
        self
    }

    pub const fn width(mut self, width: u32) -> Self {
        self.width = width;
        self
    }

    pub const fn height(mut self, height: u32) -> Self {
        self.height = height;
        self
    }

    pub const fn channels(mut self, channels: u32) -> Self {
        self.channels = channels;
        self
    }

    pub const fn duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }
}

/// Visual token kind used to render syntax hints in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchSyntaxTokenKind {
    Field,
    Operator,
    Logic,
    Number,
    Text,
    Grouping,
}

/// A token generated from the search input for syntax hint rendering.
#[derive(Debug, Clone)]
pub struct SearchSyntaxToken {
    pub text: String,
    pub kind: SearchSyntaxTokenKind,
}

/// A classified syntax span with range information.
#[derive(Debug, Clone, Copy)]
pub struct SearchSyntaxSpan {
    pub start: usize,
    pub end: usize,
    pub kind: SearchSyntaxTokenKind,
}

/// A syntax issue found while scanning the search input.
#[derive(Debug, Clone)]
pub struct SearchSyntaxIssue {
    pub message: String,
    /// Character start position in the original query.
    pub start: usize,
    /// Character end position in the original query.
    pub end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchValueKind {
    Text,
    Number,
    Duration,
    Hash,
    Kind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchField {
    Name,
    Hash,
    BoneName,
    BoneCount,
    MeshCount,
    FrameCount,
    FrameRate,
    Width,
    Height,
    Channels,
    Duration,
    Kind,
}

#[derive(Debug, Clone, Copy)]
struct SearchFieldSpec {
    field: SearchField,
    value_kind: SearchValueKind,
    aliases: &'static [&'static str],
}

// Centralized field registry for easy extension.
const SEARCH_FIELD_SPECS: &[SearchFieldSpec] = &[
    SearchFieldSpec {
        field: SearchField::Duration,
        value_kind: SearchValueKind::Duration,
        aliases: &["duration"],
    },
    SearchFieldSpec {
        field: SearchField::Name,
        value_kind: SearchValueKind::Text,
        aliases: &["name", "n"],
    },
    SearchFieldSpec {
        field: SearchField::Hash,
        value_kind: SearchValueKind::Hash,
        aliases: &["hash", "h"],
    },
    SearchFieldSpec {
        field: SearchField::BoneName,
        value_kind: SearchValueKind::Text,
        aliases: &["bonename", "bone"],
    },
    SearchFieldSpec {
        field: SearchField::BoneCount,
        value_kind: SearchValueKind::Number,
        aliases: &["bonecount"],
    },
    SearchFieldSpec {
        field: SearchField::MeshCount,
        value_kind: SearchValueKind::Number,
        aliases: &["meshcount"],
    },
    SearchFieldSpec {
        field: SearchField::FrameCount,
        value_kind: SearchValueKind::Number,
        aliases: &["framecount"],
    },
    SearchFieldSpec {
        field: SearchField::FrameRate,
        value_kind: SearchValueKind::Number,
        aliases: &["framerate", "fps"],
    },
    SearchFieldSpec {
        field: SearchField::Width,
        value_kind: SearchValueKind::Number,
        aliases: &["width"],
    },
    SearchFieldSpec {
        field: SearchField::Height,
        value_kind: SearchValueKind::Number,
        aliases: &["height"],
    },
    SearchFieldSpec {
        field: SearchField::Channels,
        value_kind: SearchValueKind::Number,
        aliases: &["channels"],
    },
    SearchFieldSpec {
        field: SearchField::Kind,
        value_kind: SearchValueKind::Kind,
        aliases: &["kind", "type", "k"],
    },
];

fn resolve_field(alias: &str) -> Option<SearchFieldSpec> {
    SEARCH_FIELD_SPECS.iter().copied().find(|spec| {
        spec.aliases
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(alias))
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ComparisonOperator {
    Eq,
    NotEq,
    Lt,
    Lte,
    Gt,
    Gte,
}

impl ComparisonOperator {
    const fn as_text(self) -> &'static str {
        match self {
            Self::Eq => ":",
            Self::NotEq => "!=",
            Self::Lt => "<",
            Self::Lte => "<=",
            Self::Gt => ">",
            Self::Gte => ">=",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextMatchMode {
    Contains,
    Exact,
    StartsWith,
    EndsWith,
}

#[derive(Debug, Clone)]
struct TextPredicate {
    mode: TextMatchMode,
    needle: String,
    negate: bool,
    hashed_needle: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
struct NumberPredicate<T> {
    op: ComparisonOperator,
    value: T,
}

impl<T: PartialEq + PartialOrd + Copy> NumberPredicate<T> {
    fn evaluate(&self, current: T) -> bool {
        match self.op {
            ComparisonOperator::Eq => current == self.value,
            ComparisonOperator::NotEq => current != self.value,
            ComparisonOperator::Lt => current < self.value,
            ComparisonOperator::Lte => current <= self.value,
            ComparisonOperator::Gt => current > self.value,
            ComparisonOperator::Gte => current >= self.value,
        }
    }
}

#[derive(Debug, Clone)]
enum SearchPredicate {
    Name(TextPredicate),
    BoneName(TextPredicate),
    Hash(NumberPredicate<u64>),
    BoneCount(NumberPredicate<u32>),
    MeshCount(NumberPredicate<u32>),
    FrameCount(NumberPredicate<u32>),
    FrameRate(NumberPredicate<u32>),
    Width(NumberPredicate<u32>),
    Height(NumberPredicate<u32>),
    Channels(NumberPredicate<u32>),
    Duration(NumberPredicate<Duration>),
    Kind(KindPredicate),
}

/// Predicate over `SearchAsset::kind_index`. Allowed operators: `:` (eq) and
/// `!=` (negate). The query value is one of the canonical kind aliases below.
#[derive(Debug, Clone)]
struct KindPredicate {
    indices: Vec<u8>,
    negate: bool,
}

/// Map a query kind alias (case-insensitive) to a stable kind index that
/// matches `AssetKind::index` on the producing side. Multiple aliases map to
/// the same index.
fn kind_alias_to_index(alias: &str) -> Option<u8> {
    match alias.to_ascii_lowercase().as_str() {
        "animation" | "anim" | "a" => Some(0),
        "image" | "img" | "i" | "texture" | "tex" => Some(1),
        "model" | "mdl" | "m" | "mesh" => Some(2),
        "sound" | "snd" | "audio" | "s" => Some(3),
        "material" | "mat" => Some(4),
        "rawfile" | "raw" | "file" | "r" => Some(5),
        "movie" | "mov" | "video" => Some(6),
        "map" => Some(7),
        "camo" => Some(8),
        "weapon" | "wep" => Some(9),
        "sndbank" | "bank" => Some(10),
        "animpkg" | "pkg" => Some(11),
        _ => None,
    }
}

#[derive(Debug, Clone)]
enum SearchExpr {
    True,
    False,
    Predicate(SearchPredicate),
    Not(Box<SearchExpr>),
    And(Vec<SearchExpr>),
    Or(Vec<SearchExpr>),
}

impl SearchExpr {
    fn and(lhs: SearchExpr, rhs: SearchExpr) -> SearchExpr {
        match (lhs, rhs) {
            (SearchExpr::False, _) | (_, SearchExpr::False) => SearchExpr::False,
            (SearchExpr::True, other) | (other, SearchExpr::True) => other,
            (SearchExpr::And(mut left), SearchExpr::And(mut right)) => {
                left.append(&mut right);
                SearchExpr::And(left)
            }
            (SearchExpr::And(mut left), right) => {
                left.push(right);
                SearchExpr::And(left)
            }
            (left, SearchExpr::And(mut right)) => {
                let mut all = vec![left];
                all.append(&mut right);
                SearchExpr::And(all)
            }
            (left, right) => SearchExpr::And(vec![left, right]),
        }
    }

    fn or(lhs: SearchExpr, rhs: SearchExpr) -> SearchExpr {
        match (lhs, rhs) {
            (SearchExpr::True, _) | (_, SearchExpr::True) => SearchExpr::True,
            (SearchExpr::False, other) | (other, SearchExpr::False) => other,
            (SearchExpr::Or(mut left), SearchExpr::Or(mut right)) => {
                left.append(&mut right);
                SearchExpr::Or(left)
            }
            (SearchExpr::Or(mut left), right) => {
                left.push(right);
                SearchExpr::Or(left)
            }
            (left, SearchExpr::Or(mut right)) => {
                let mut all = vec![left];
                all.append(&mut right);
                SearchExpr::Or(all)
            }
            (left, right) => SearchExpr::Or(vec![left, right]),
        }
    }

    fn not(expr: SearchExpr) -> SearchExpr {
        match expr {
            SearchExpr::True => SearchExpr::False,
            SearchExpr::False => SearchExpr::True,
            SearchExpr::Not(inner) => *inner,
            other => SearchExpr::Not(Box::new(other)),
        }
    }

    fn evaluate(&self, asset: &EvaluatedAsset) -> bool {
        match self {
            SearchExpr::True => true,
            SearchExpr::False => false,
            SearchExpr::Predicate(predicate) => predicate.evaluate(asset),
            SearchExpr::Not(inner) => !inner.evaluate(asset),
            SearchExpr::And(parts) => parts.iter().all(|part| part.evaluate(asset)),
            SearchExpr::Or(parts) => parts.iter().any(|part| part.evaluate(asset)),
        }
    }

    fn prepare_hash_seed(&mut self, seed: u64) {
        match self {
            SearchExpr::Predicate(predicate) => predicate.prepare_hash_seed(seed),
            SearchExpr::Not(inner) => inner.prepare_hash_seed(seed),
            SearchExpr::And(parts) | SearchExpr::Or(parts) => {
                for part in parts {
                    part.prepare_hash_seed(seed);
                }
            }
            SearchExpr::True | SearchExpr::False => {}
        }
    }

    fn collect_usage(&self, usage: &mut SearchUsage) {
        match self {
            SearchExpr::Predicate(predicate) => predicate.collect_usage(usage),
            SearchExpr::Not(inner) => inner.collect_usage(usage),
            SearchExpr::And(parts) | SearchExpr::Or(parts) => {
                for part in parts {
                    part.collect_usage(usage);
                }
            }
            SearchExpr::True | SearchExpr::False => {}
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct SearchUsage {
    needs_name: bool,
    needs_bone_name: bool,
    needs_bone_hash: bool,
    needs_kind: bool,
}

/// A compiled search term.
#[derive(Debug, Clone)]
#[repr(align(64))]
pub struct SearchTerm {
    expression: SearchExpr,
    usage: SearchUsage,
    hash_seed: Option<u64>,
}

impl SearchTerm {
    pub fn compile(search: &str) -> Self {
        let tokens = tokenize(search);

        let expression = if tokens.is_empty() {
            SearchExpr::True
        } else {
            SearchParser::new(tokens).parse()
        };

        let mut term = Self {
            expression,
            usage: SearchUsage::default(),
            hash_seed: None,
        };

        term.refresh_usage();
        term
    }

    pub fn syntax_tokens(search: &str) -> Vec<SearchSyntaxToken> {
        let tokens = tokenize(search);
        classify_syntax_tokens(&tokens)
    }

    pub fn syntax_spans(search: &str) -> Vec<SearchSyntaxSpan> {
        let tokens = tokenize(search);
        classify_syntax_spans(&tokens)
    }

    pub fn syntax_issues(search: &str) -> Vec<SearchSyntaxIssue> {
        let tokens = tokenize(search);
        analyze_syntax_issues(&tokens)
    }

    pub fn with_hash_seed(mut self, seed: u64) -> Self {
        self.hash_seed = Some(seed);
        self.expression.prepare_hash_seed(seed);
        self.refresh_usage();
        self
    }

    #[inline(always)]
    pub fn matches(&self, asset: SearchAsset) -> bool {
        if matches!(self.expression, SearchExpr::True) {
            return true;
        }

        if matches!(self.expression, SearchExpr::False) {
            return false;
        }

        let prepared = EvaluatedAsset::new(asset, self.usage, self.hash_seed);
        self.expression.evaluate(&prepared)
    }

    #[inline(always)]
    pub fn matches_with_hash(&self, asset: SearchAsset, offset: u64) -> bool {
        if self.hash_seed == Some(offset) {
            return self.matches(asset);
        }

        self.clone().with_hash_seed(offset).matches(asset)
    }

    fn refresh_usage(&mut self) {
        let mut usage = SearchUsage::default();
        self.expression.collect_usage(&mut usage);
        self.usage = usage;
    }
}

struct EvaluatedAsset {
    bone_count: u32,
    mesh_count: u32,
    frame_count: u32,
    frame_rate: u32,
    width: u32,
    height: u32,
    channels: u32,
    duration: Duration,
    kind_index: u8,
    hash_masked: u64,
    name_lower: String,
    bone_names_lower: Vec<String>,
    bone_hashes: Option<Vec<u64>>,
}

impl EvaluatedAsset {
    fn new(asset: SearchAsset, usage: SearchUsage, hash_seed: Option<u64>) -> Self {
        let SearchAsset {
            bone_count,
            mesh_count,
            frame_count,
            frame_rate,
            width,
            height,
            channels,
            duration,
            kind_index,
            name,
            hash,
            bone_names,
        } = asset;

        let name_lower = if usage.needs_name {
            // Asset names are effectively ASCII; the `to_ascii_lowercase`
            // path avoids the full Unicode case-folding tables and roughly
            // halves time spent here for the typical ~100k-asset filter.
            name.to_ascii_lowercase()
        } else {
            String::new()
        };

        let bone_names_lower = if usage.needs_bone_name {
            bone_names
                .iter()
                .map(|bone| bone.to_ascii_lowercase())
                .collect()
        } else {
            Vec::new()
        };

        let bone_hashes = if usage.needs_bone_hash {
            hash_seed.map(|seed| {
                bone_names
                    .iter()
                    .map(|bone| bone.hash_fnv1a64_with_seed(seed) & HASH_MASK)
                    .collect()
            })
        } else {
            None
        };

        Self {
            bone_count,
            mesh_count,
            frame_count,
            frame_rate,
            width,
            height,
            channels,
            duration,
            kind_index,
            hash_masked: hash & HASH_MASK,
            name_lower,
            bone_names_lower,
            bone_hashes,
        }
    }
}

impl SearchPredicate {
    fn evaluate(&self, asset: &EvaluatedAsset) -> bool {
        match self {
            SearchPredicate::Name(predicate) => predicate.evaluate_name(asset),
            SearchPredicate::BoneName(predicate) => predicate.evaluate_bone_name(asset),
            SearchPredicate::Hash(predicate) => predicate.evaluate(asset.hash_masked),
            SearchPredicate::BoneCount(predicate) => predicate.evaluate(asset.bone_count),
            SearchPredicate::MeshCount(predicate) => predicate.evaluate(asset.mesh_count),
            SearchPredicate::FrameCount(predicate) => predicate.evaluate(asset.frame_count),
            SearchPredicate::FrameRate(predicate) => predicate.evaluate(asset.frame_rate),
            SearchPredicate::Width(predicate) => predicate.evaluate(asset.width),
            SearchPredicate::Height(predicate) => predicate.evaluate(asset.height),
            SearchPredicate::Channels(predicate) => predicate.evaluate(asset.channels),
            SearchPredicate::Duration(predicate) => predicate.evaluate(asset.duration),
            SearchPredicate::Kind(predicate) => predicate.evaluate(asset.kind_index),
        }
    }

    fn prepare_hash_seed(&mut self, seed: u64) {
        match self {
            SearchPredicate::Name(predicate) | SearchPredicate::BoneName(predicate) => {
                predicate.hashed_needle =
                    Some(predicate.needle.hash_fnv1a64_with_seed(seed) & HASH_MASK);
            }
            _ => {}
        }
    }

    fn collect_usage(&self, usage: &mut SearchUsage) {
        match self {
            SearchPredicate::Name(_) => {
                usage.needs_name = true;
            }
            SearchPredicate::BoneName(predicate) => {
                usage.needs_bone_name = true;
                if predicate.hashed_needle.is_some() {
                    usage.needs_bone_hash = true;
                }
            }
            SearchPredicate::Kind(_) => {
                usage.needs_kind = true;
            }
            _ => {}
        }
    }
}

impl KindPredicate {
    fn evaluate(&self, current: u8) -> bool {
        let matched = self.indices.contains(&current);
        if self.negate { !matched } else { matched }
    }
}

impl TextPredicate {
    fn evaluate_name(&self, asset: &EvaluatedAsset) -> bool {
        let mut matched = evaluate_text(self.mode, &asset.name_lower, &self.needle);

        if !matched && let Some(hash) = self.hashed_needle {
            matched = asset.hash_masked == hash;
        }

        if self.negate { !matched } else { matched }
    }

    fn evaluate_bone_name(&self, asset: &EvaluatedAsset) -> bool {
        let mut matched = asset
            .bone_names_lower
            .iter()
            .any(|bone| evaluate_text(self.mode, bone, &self.needle));

        if !matched && let Some(hash) = self.hashed_needle {
            matched = asset
                .bone_hashes
                .as_ref()
                .map(|hashes| hashes.contains(&hash))
                .unwrap_or(false);
        }

        if self.negate { !matched } else { matched }
    }
}

fn evaluate_text(mode: TextMatchMode, haystack: &str, needle: &str) -> bool {
    match mode {
        TextMatchMode::Contains => haystack.contains(needle),
        TextMatchMode::Exact => haystack == needle,
        TextMatchMode::StartsWith => haystack.starts_with(needle),
        TextMatchMode::EndsWith => haystack.ends_with(needle),
    }
}

fn build_plain_name_expression(word: String) -> SearchExpr {
    match build_text_predicate(word, ComparisonOperator::Eq) {
        Some(predicate) => SearchExpr::Predicate(SearchPredicate::Name(predicate)),
        None => SearchExpr::False,
    }
}

fn build_predicate_or_fallback(key: String, op: ComparisonOperator, value: String) -> SearchExpr {
    let fallback_text = format!("{key}{}{}", op.as_text(), value);
    let fallback = || build_plain_name_expression(fallback_text.clone());

    let Some(spec) = resolve_field(&key) else {
        return fallback();
    };

    match spec.value_kind {
        SearchValueKind::Text => {
            let Some(predicate) = build_text_predicate(value, op) else {
                return fallback();
            };

            match spec.field {
                SearchField::Name => SearchExpr::Predicate(SearchPredicate::Name(predicate)),
                SearchField::BoneName => {
                    SearchExpr::Predicate(SearchPredicate::BoneName(predicate))
                }
                _ => fallback(),
            }
        }
        SearchValueKind::Number => match build_u32_predicate(spec.field, op, value) {
            Some(predicate) => SearchExpr::Predicate(predicate),
            None => fallback(),
        },
        SearchValueKind::Duration => match parse_duration(&value) {
            Some(value) => {
                SearchExpr::Predicate(SearchPredicate::Duration(NumberPredicate { op, value }))
            }
            None => fallback(),
        },
        SearchValueKind::Hash => match build_u64_predicate(spec.field, op, value) {
            Some(predicate) => SearchExpr::Predicate(predicate),
            None => fallback(),
        },
        SearchValueKind::Kind => match build_kind_predicate(op, value) {
            Some(predicate) => SearchExpr::Predicate(SearchPredicate::Kind(predicate)),
            None => fallback(),
        },
    }
}

fn build_text_predicate(raw_value: String, op: ComparisonOperator) -> Option<TextPredicate> {
    if !matches!(op, ComparisonOperator::Eq | ComparisonOperator::NotEq) {
        return None;
    }

    let (mode, needle, invert) = parse_text_pattern(raw_value)?;

    let mut negate = matches!(op, ComparisonOperator::NotEq);
    if invert {
        negate = !negate;
    }

    Some(TextPredicate {
        mode,
        needle,
        negate,
        hashed_needle: None,
    })
}

fn parse_text_pattern(raw_value: String) -> Option<(TextMatchMode, String, bool)> {
    let mut value = raw_value.trim().to_owned();

    if value.is_empty() {
        return None;
    }

    let mut invert = false;
    while let Some(rest) = value.strip_prefix('-') {
        invert = !invert;
        value = rest.to_owned();
    }

    let mut exact = false;
    while let Some(rest) = value.strip_prefix('+') {
        exact = true;
        value = rest.to_owned();
    }

    let ends_with = value.starts_with("...");
    if ends_with {
        value = value.chars().skip(3).collect();
    }

    let starts_with = value.ends_with("...");
    if starts_with {
        let trimmed_len = value.chars().count().saturating_sub(3);
        value = value.chars().take(trimmed_len).collect();
    }

    let mode = if starts_with && ends_with {
        TextMatchMode::Exact
    } else if starts_with {
        TextMatchMode::StartsWith
    } else if ends_with {
        TextMatchMode::EndsWith
    } else if exact {
        TextMatchMode::Exact
    } else {
        TextMatchMode::Contains
    };

    let needle = value.trim().to_ascii_lowercase();

    if needle.is_empty() {
        None
    } else {
        Some((mode, needle, invert))
    }
}

fn parse_duration(value: &str) -> Option<Duration> {
    let value = value.to_ascii_lowercase();
    let (number, multiplier) = if let Some(value) = value.strip_suffix("ms") {
        (value, 1)
    } else if let Some(value) = value.strip_suffix('s') {
        (value, 1_000)
    } else if let Some(value) = value.strip_suffix('m') {
        (value, 60_000)
    } else if let Some(value) = value.strip_suffix('h') {
        (value, 3_600_000)
    } else {
        (value.as_str(), 1)
    };
    Some(Duration::from_millis(
        number.parse::<u64>().ok()?.checked_mul(multiplier)?,
    ))
}

fn build_u32_predicate(
    field: SearchField,
    op: ComparisonOperator,
    value: String,
) -> Option<SearchPredicate> {
    let value = parse_u32(value.as_str())?;
    let predicate = NumberPredicate { op, value };

    match field {
        SearchField::BoneCount => Some(SearchPredicate::BoneCount(predicate)),
        SearchField::MeshCount => Some(SearchPredicate::MeshCount(predicate)),
        SearchField::FrameCount => Some(SearchPredicate::FrameCount(predicate)),
        SearchField::FrameRate => Some(SearchPredicate::FrameRate(predicate)),
        SearchField::Width => Some(SearchPredicate::Width(predicate)),
        SearchField::Height => Some(SearchPredicate::Height(predicate)),
        SearchField::Channels => Some(SearchPredicate::Channels(predicate)),
        _ => None,
    }
}

fn build_u64_predicate(
    field: SearchField,
    op: ComparisonOperator,
    value: String,
) -> Option<SearchPredicate> {
    let value = parse_u64(value.as_str())? & HASH_MASK;
    let predicate = NumberPredicate { op, value };

    match field {
        SearchField::Hash => Some(SearchPredicate::Hash(predicate)),
        _ => None,
    }
}

fn build_kind_predicate(op: ComparisonOperator, value: String) -> Option<KindPredicate> {
    if !matches!(op, ComparisonOperator::Eq | ComparisonOperator::NotEq) {
        return None;
    }
    let raw = value.trim();
    if raw.is_empty() {
        return None;
    }
    // Allow comma-separated kind list: kind:model,anim
    let mut indices: Vec<u8> = raw
        .split(',')
        .filter_map(|alias| kind_alias_to_index(alias.trim()))
        .collect();
    if indices.is_empty() {
        return None;
    }
    indices.sort_unstable();
    indices.dedup();
    Some(KindPredicate {
        indices,
        negate: matches!(op, ComparisonOperator::NotEq),
    })
}

fn parse_u32(value: &str) -> Option<u32> {
    value.trim().parse().ok()
}

fn parse_u64(value: &str) -> Option<u64> {
    let value = value.trim();

    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16).ok()
    } else {
        value.parse::<u64>().ok()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LexTokenKind {
    Word,
    LParen,
    RParen,
    And,
    Or,
    Not,
    Comparator(ComparisonOperator),
}

#[derive(Debug, Clone)]
struct LexToken {
    kind: LexTokenKind,
    text: String,
    start: usize,
    end: usize,
}

fn tokenize(input: &str) -> Vec<LexToken> {
    let chars: Vec<char> = input.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0usize;

    while index < chars.len() {
        let ch = chars[index];

        if ch.is_whitespace() {
            index += 1;
            continue;
        }

        match ch {
            '(' => {
                tokens.push(LexToken {
                    kind: LexTokenKind::LParen,
                    text: String::from("("),
                    start: index,
                    end: index + 1,
                });
                index += 1;
                continue;
            }
            ')' => {
                tokens.push(LexToken {
                    kind: LexTokenKind::RParen,
                    text: String::from(")"),
                    start: index,
                    end: index + 1,
                });
                index += 1;
                continue;
            }
            ',' => {
                tokens.push(LexToken {
                    kind: LexTokenKind::Or,
                    text: String::from(","),
                    start: index,
                    end: index + 1,
                });
                index += 1;
                continue;
            }
            '&' => {
                let double = chars.get(index + 1).copied() == Some('&');
                let token_len = if double { 2 } else { 1 };
                tokens.push(LexToken {
                    kind: LexTokenKind::And,
                    text: if double {
                        String::from("&&")
                    } else {
                        String::from("&")
                    },
                    start: index,
                    end: index + token_len,
                });
                index += token_len;
                continue;
            }
            '|' => {
                let double = chars.get(index + 1).copied() == Some('|');
                let token_len = if double { 2 } else { 1 };
                tokens.push(LexToken {
                    kind: LexTokenKind::Or,
                    text: if double {
                        String::from("||")
                    } else {
                        String::from("|")
                    },
                    start: index,
                    end: index + token_len,
                });
                index += token_len;
                continue;
            }
            _ => {}
        }

        if let Some((operator, text, len)) = try_read_comparator(&chars, index) {
            tokens.push(LexToken {
                kind: LexTokenKind::Comparator(operator),
                text: String::from(text),
                start: index,
                end: index + len,
            });
            index += len;
            continue;
        }

        if ch == '!' {
            tokens.push(LexToken {
                kind: LexTokenKind::Not,
                text: String::from("!"),
                start: index,
                end: index + 1,
            });
            index += 1;
            continue;
        }

        if ch == '"' || ch == '\'' {
            let quote = ch;
            let quoted_start = index;
            index += 1;

            let mut value = String::new();

            while index < chars.len() {
                let current = chars[index];

                if current == '\\' {
                    if index + 1 < chars.len() {
                        index += 1;
                        value.push(chars[index]);
                        index += 1;
                    } else {
                        index += 1;
                    }
                } else if current == quote {
                    index += 1;
                    break;
                } else {
                    value.push(current);
                    index += 1;
                }
            }

            if !value.is_empty() {
                tokens.push(LexToken {
                    kind: LexTokenKind::Word,
                    text: value.to_ascii_lowercase(),
                    start: quoted_start,
                    end: index,
                });
            }

            continue;
        }

        let start = index;

        while index < chars.len() && !is_word_delimiter(chars[index]) {
            index += 1;
        }

        if start == index {
            index += 1;
            continue;
        }

        let word: String = chars[start..index].iter().collect();

        if word.eq_ignore_ascii_case("and") {
            tokens.push(LexToken {
                kind: LexTokenKind::And,
                text: String::from("AND"),
                start,
                end: index,
            });
        } else if word.eq_ignore_ascii_case("or") {
            tokens.push(LexToken {
                kind: LexTokenKind::Or,
                text: String::from("OR"),
                start,
                end: index,
            });
        } else if word.eq_ignore_ascii_case("not") {
            tokens.push(LexToken {
                kind: LexTokenKind::Not,
                text: String::from("NOT"),
                start,
                end: index,
            });
        } else {
            tokens.push(LexToken {
                kind: LexTokenKind::Word,
                text: word.to_ascii_lowercase(),
                start,
                end: index,
            });
        }
    }

    tokens
}

fn classify_syntax_tokens(tokens: &[LexToken]) -> Vec<SearchSyntaxToken> {
    let mut output = Vec::with_capacity(tokens.len());

    for (index, token) in tokens.iter().enumerate() {
        let kind = classify_token_kind(tokens, index);

        output.push(SearchSyntaxToken {
            text: token.text.clone(),
            kind,
        });
    }

    output
}

fn classify_syntax_spans(tokens: &[LexToken]) -> Vec<SearchSyntaxSpan> {
    let mut output = Vec::with_capacity(tokens.len());

    for (index, token) in tokens.iter().enumerate() {
        output.push(SearchSyntaxSpan {
            start: token.start,
            end: token.end,
            kind: classify_token_kind(tokens, index),
        });
    }

    output
}

fn classify_token_kind(tokens: &[LexToken], index: usize) -> SearchSyntaxTokenKind {
    let token = &tokens[index];

    match token.kind {
        LexTokenKind::LParen | LexTokenKind::RParen => SearchSyntaxTokenKind::Grouping,
        LexTokenKind::And | LexTokenKind::Or | LexTokenKind::Not => SearchSyntaxTokenKind::Logic,
        LexTokenKind::Comparator(_) => SearchSyntaxTokenKind::Operator,
        LexTokenKind::Word => {
            let is_field = matches!(
                tokens.get(index + 1).map(|next| next.kind),
                Some(LexTokenKind::Comparator(_))
            ) && resolve_field(token.text.as_str()).is_some();

            if is_field {
                SearchSyntaxTokenKind::Field
            } else if parse_u64(token.text.as_str()).is_some()
                || parse_u32(token.text.as_str()).is_some()
            {
                SearchSyntaxTokenKind::Number
            } else {
                SearchSyntaxTokenKind::Text
            }
        }
    }
}

fn try_read_comparator(
    chars: &[char],
    index: usize,
) -> Option<(ComparisonOperator, &'static str, usize)> {
    let current = chars[index];
    let next = chars.get(index + 1).copied();

    match current {
        '!' => match next {
            Some('=') => Some((ComparisonOperator::NotEq, "!=", 2)),
            Some(':') => Some((ComparisonOperator::NotEq, "!:", 2)),
            _ => None,
        },
        '<' => match next {
            Some('=') => Some((ComparisonOperator::Lte, "<=", 2)),
            Some(':') => Some((ComparisonOperator::Lte, "<:", 2)),
            _ => Some((ComparisonOperator::Lt, "<", 1)),
        },
        '>' => match next {
            Some('=') => Some((ComparisonOperator::Gte, ">=", 2)),
            Some(':') => Some((ComparisonOperator::Gte, ">:", 2)),
            _ => Some((ComparisonOperator::Gt, ">", 1)),
        },
        '=' => match next {
            Some('=') => Some((ComparisonOperator::Eq, "==", 2)),
            _ => Some((ComparisonOperator::Eq, "=", 1)),
        },
        ':' => Some((ComparisonOperator::Eq, ":", 1)),
        _ => None,
    }
}

fn is_word_delimiter(ch: char) -> bool {
    ch.is_whitespace()
        || matches!(
            ch,
            '(' | ')' | ',' | '&' | '|' | '!' | '<' | '>' | '=' | ':' | '"' | '\''
        )
}

struct SearchParser {
    tokens: Vec<LexToken>,
    index: usize,
}

impl SearchParser {
    fn new(tokens: Vec<LexToken>) -> Self {
        Self { tokens, index: 0 }
    }

    fn parse(&mut self) -> SearchExpr {
        let mut expression = self.parse_or().unwrap_or(SearchExpr::True);

        while self.index < self.tokens.len() {
            if self.consume(LexTokenKind::RParen) {
                continue;
            }

            if let Some(next) = self.parse_or() {
                expression = SearchExpr::and(expression, next);
            } else {
                self.index += 1;
            }
        }

        expression
    }

    fn parse_or(&mut self) -> Option<SearchExpr> {
        let mut expression = self.parse_and()?;

        while self.consume(LexTokenKind::Or) {
            let right = self.parse_and().unwrap_or(SearchExpr::False);
            expression = SearchExpr::or(expression, right);
        }

        Some(expression)
    }

    fn parse_and(&mut self) -> Option<SearchExpr> {
        let mut expression = self.parse_unary()?;

        loop {
            if self.consume(LexTokenKind::And) {
                let right = self.parse_unary().unwrap_or(SearchExpr::False);
                expression = SearchExpr::and(expression, right);
                continue;
            }

            if self.peek_starts_term() {
                let right = self.parse_unary().unwrap_or(SearchExpr::False);
                expression = SearchExpr::and(expression, right);
                continue;
            }

            break;
        }

        Some(expression)
    }

    fn parse_unary(&mut self) -> Option<SearchExpr> {
        if self.consume(LexTokenKind::Not) {
            let inner = self.parse_unary().unwrap_or(SearchExpr::False);
            return Some(SearchExpr::not(inner));
        }

        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Option<SearchExpr> {
        if self.consume(LexTokenKind::LParen) {
            let expression = self.parse_or().unwrap_or(SearchExpr::True);
            let _ = self.consume(LexTokenKind::RParen);
            return Some(expression);
        }

        let word = self.consume_word()?;
        Some(self.parse_predicate(word))
    }

    fn parse_predicate(&mut self, key: String) -> SearchExpr {
        let Some(mut operator) = self.consume_comparator() else {
            return build_plain_name_expression(key);
        };

        if matches!(operator, ComparisonOperator::Eq | ComparisonOperator::NotEq)
            && let Some(next_operator) = self.peek_comparator()
            && matches!(
                next_operator,
                ComparisonOperator::Lt
                    | ComparisonOperator::Lte
                    | ComparisonOperator::Gt
                    | ComparisonOperator::Gte
            )
        {
            operator = next_operator;
            self.index += 1;
        }

        let Some(value) = self.consume_value_token() else {
            return SearchExpr::False;
        };

        build_predicate_or_fallback(key, operator, value)
    }

    fn consume_value_token(&mut self) -> Option<String> {
        if let Some(word) = self.consume_word() {
            return Some(word);
        }

        if self.consume(LexTokenKind::Not) {
            if let Some(word) = self.consume_word() {
                return Some(format!("!{word}"));
            }

            return Some(String::from("!"));
        }

        if let Some(operator) = self.consume_comparator() {
            let mut value = String::from(operator.as_text());

            if let Some(word) = self.consume_word() {
                value.push_str(word.as_str());
            }

            return Some(value);
        }

        None
    }

    fn peek_starts_term(&self) -> bool {
        matches!(
            self.peek_kind(),
            Some(LexTokenKind::Word | LexTokenKind::LParen | LexTokenKind::Not)
        )
    }

    fn consume_word(&mut self) -> Option<String> {
        if self.peek_kind() == Some(LexTokenKind::Word) {
            let token = self.tokens.get(self.index).cloned()?;
            self.index += 1;
            Some(token.text)
        } else {
            None
        }
    }

    fn consume_comparator(&mut self) -> Option<ComparisonOperator> {
        let comparator = self.peek_comparator()?;
        self.index += 1;
        Some(comparator)
    }

    fn peek_comparator(&self) -> Option<ComparisonOperator> {
        match self.peek_kind()? {
            LexTokenKind::Comparator(operator) => Some(operator),
            _ => None,
        }
    }

    fn consume(&mut self, expected: LexTokenKind) -> bool {
        if self.peek_kind() == Some(expected) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn peek_kind(&self) -> Option<LexTokenKind> {
        self.tokens.get(self.index).map(|token| token.kind)
    }
}

fn analyze_syntax_issues(tokens: &[LexToken]) -> Vec<SearchSyntaxIssue> {
    let mut issues = Vec::new();
    let mut group_stack: Vec<&LexToken> = Vec::new();

    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            LexTokenKind::LParen => {
                group_stack.push(token);
            }
            LexTokenKind::RParen => {
                if group_stack.pop().is_none() {
                    issues.push(SearchSyntaxIssue {
                        message: String::from("Unmatched ')'"),
                        start: token.start,
                        end: token.end,
                    });
                } else if tokens
                    .get(index.saturating_sub(1))
                    .is_some_and(|previous| previous.kind == LexTokenKind::LParen)
                {
                    issues.push(SearchSyntaxIssue {
                        message: String::from("Empty group '()'"),
                        start: token.start,
                        end: token.end,
                    });
                }
            }
            LexTokenKind::Comparator(_) => {
                let left_ok = tokens
                    .get(index.saturating_sub(1))
                    .is_some_and(is_term_end_token);
                if !left_ok {
                    issues.push(SearchSyntaxIssue {
                        message: String::from("Missing left side before comparator"),
                        start: token.start,
                        end: token.end,
                    });
                }

                let right_ok = tokens.get(index + 1).is_some_and(is_value_start_token);
                if !right_ok {
                    issues.push(SearchSyntaxIssue {
                        message: String::from("Missing value after comparator"),
                        start: token.start,
                        end: token.end,
                    });
                }
            }
            LexTokenKind::And | LexTokenKind::Or => {
                let left_ok = tokens
                    .get(index.saturating_sub(1))
                    .is_some_and(is_term_end_token);
                let right_ok = tokens.get(index + 1).is_some_and(is_term_start_token);

                if !left_ok || !right_ok {
                    issues.push(SearchSyntaxIssue {
                        message: String::from("Logical operator is missing one side"),
                        start: token.start,
                        end: token.end,
                    });
                }
            }
            LexTokenKind::Not => {
                let right_ok = tokens.get(index + 1).is_some_and(is_term_start_token);
                if !right_ok {
                    issues.push(SearchSyntaxIssue {
                        message: String::from("NOT must be followed by a term"),
                        start: token.start,
                        end: token.end,
                    });
                }
            }
            LexTokenKind::Word => {}
        }
    }

    for token in group_stack {
        issues.push(SearchSyntaxIssue {
            message: String::from("Unmatched '('"),
            start: token.start,
            end: token.end,
        });
    }

    issues
}

fn is_term_start_token(token: &LexToken) -> bool {
    matches!(
        token.kind,
        LexTokenKind::Word | LexTokenKind::LParen | LexTokenKind::Not
    )
}

fn is_term_end_token(token: &LexToken) -> bool {
    matches!(token.kind, LexTokenKind::Word | LexTokenKind::RParen)
}

fn is_value_start_token(token: &LexToken) -> bool {
    matches!(
        token.kind,
        LexTokenKind::Word | LexTokenKind::Not | LexTokenKind::Comparator(_)
    )
}
