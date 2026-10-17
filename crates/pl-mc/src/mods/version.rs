//! Version semantics of the mod loaders, used by the pre-launch mod check ([`super::check`]):
//!
//! - Fabric and Quilt: semantic versions and version predicates (`>=1.2 <2`, `~1.20`, `^3`,
//!   `1.20.x`, `*`; a JSON array accepts any of its predicates), read the way Fabric Loader's
//!   `SemanticVersionImpl` and `VersionPredicateParser` read them.
//! - Forge and NeoForge: Maven version ranges (`[1.20,1.21)`, `[47,)`, `(,2.0]`, unions; a bare
//!   version accepts everything) ordered like Maven's `ComparableVersion`.
//! - Minecraft version ids: releases, pre-releases, release candidates, snapshots and the
//!   year-based scheme (`26.1`, `26.1.2`, `26.1-snapshot-1`).
//!
//! Every question answers `None` when the answer is not certain (a version or predicate that
//! does not parse cleanly, a pre-release right at a bound): the check reports only what the
//! loader would certainly reject.

use std::cmp::Ordering;
use std::sync::LazyLock;

use regex::Regex;

use crate::loaders::LoaderKind;

// --- Fabric / Quilt semantic versions ---------------------------------------------------------

/// A version as Fabric Loader's `SemanticVersionImpl` reads it. Build metadata (`+...`) is
/// dropped: it never takes part in comparisons. Equality follows the ordering (`1.0` equals
/// `1.0.0`).
#[derive(Debug, Clone)]
pub struct FabricVersion {
    components: Vec<u32>,
    /// `None` for a release; `Some(empty)` for the empty pre-release of `1.21-`, which sorts
    /// before every other pre-release of that version.
    pre: Option<Vec<String>>,
}

/// A parsed version that may end in a wildcard (`1.20.x`); only predicates allow those.
struct ParsedSemver {
    version: FabricVersion,
    /// Number of components before the first wildcard.
    wildcard_at: Option<usize>,
}

impl FabricVersion {
    /// Parse a mod, game or loader version. `None` when Fabric would not read it as a semantic
    /// version (it then only supports string equality, which the check does not rely on).
    pub fn parse(text: &str) -> Option<Self> {
        let parsed = parse_semver(text, false)?;
        parsed.wildcard_at.is_none().then_some(parsed.version)
    }

    fn component(&self, index: usize) -> u32 {
        self.components.get(index).copied().unwrap_or(0)
    }

    /// Same version core as `other` (missing components count as zero).
    fn same_core(&self, other: &Self) -> bool {
        let len = self.components.len().max(other.components.len());
        (0..len).all(|i| self.component(i) == other.component(i))
    }

    /// A pre-release of `release` that lies strictly between `release-` and `release`.
    fn probe_below(release: &Self) -> Self {
        Self {
            components: release.components.clone(),
            pre: Some(vec!["0".to_owned()]),
        }
    }
}

impl Ord for FabricVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        let len = self.components.len().max(other.components.len());
        for i in 0..len {
            match self.component(i).cmp(&other.component(i)) {
                Ordering::Equal => {}
                unequal => return unequal,
            }
        }
        match (&self.pre, &other.pre) {
            (None, None) => Ordering::Equal,
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (Some(a), Some(b)) => compare_prerelease(a, b),
        }
    }
}

impl PartialOrd for FabricVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for FabricVersion {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for FabricVersion {}

fn compare_prerelease(a: &[String], b: &[String]) -> Ordering {
    for (i, left) in a.iter().enumerate() {
        let Some(right) = b.get(i) else {
            return Ordering::Greater;
        };
        let order = match (is_unsigned_int(left), is_unsigned_int(right)) {
            (true, true) => left
                .len()
                .cmp(&right.len())
                .then_with(|| left.as_str().cmp(right)),
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            (false, false) => left.as_str().cmp(right),
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    if b.len() > a.len() {
        Ordering::Less
    } else {
        Ordering::Equal
    }
}

/// `0|[1-9][0-9]*`
fn is_unsigned_int(text: &str) -> bool {
    !text.is_empty()
        && text.bytes().all(|b| b.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'))
}

/// Empty, or dot-separated non-empty identifiers of `[-0-9A-Za-z]`.
fn is_dot_separated_id(text: &str) -> bool {
    text.is_empty()
        || text.split('.').all(|part| {
            !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

fn parse_semver(text: &str, allow_wildcards: bool) -> Option<ParsedSemver> {
    if text.is_empty() {
        return None;
    }
    let (rest, build) = match text.split_once('+') {
        Some((rest, build)) => (rest, Some(build)),
        None => (text, None),
    };
    let (core, pre) = match rest.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (rest, None),
    };
    if !pre.is_none_or(is_dot_separated_id) || !build.is_none_or(is_dot_separated_id) {
        return None;
    }
    if core.starts_with('.') || core.ends_with('.') {
        return None;
    }
    let mut components = Vec::new();
    let mut wildcard_at = None;
    for part in core.split('.') {
        if allow_wildcards && matches!(part, "x" | "X" | "*") {
            if pre.is_some() {
                return None;
            }
            wildcard_at.get_or_insert(components.len());
            continue;
        }
        if wildcard_at.is_some() || part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        // Fabric stores components as Java ints.
        let value: i32 = part.parse().ok()?;
        components.push(u32::try_from(value).ok()?);
    }
    if components.is_empty() {
        return None;
    }
    let pre = pre.map(|pre| {
        pre.split('.')
            .filter(|token| !token.is_empty())
            .map(str::to_owned)
            .collect()
    });
    Some(ParsedSemver {
        version: FabricVersion { components, pre },
        wildcard_at,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Ge,
    Le,
    Gt,
    Lt,
    Eq,
    /// `~`: at least the version, same major and minor.
    Tilde,
    /// `^`: at least the version, same major.
    Caret,
}

/// Operators in the order Fabric tries them (`>=` before `>`).
const OPERATORS: [(&str, Op); 7] = [
    (">=", Op::Ge),
    ("<=", Op::Le),
    (">", Op::Gt),
    ("<", Op::Lt),
    ("=", Op::Eq),
    ("~", Op::Tilde),
    ("^", Op::Caret),
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct Term {
    op: Op,
    version: FabricVersion,
}

impl Term {
    fn test(&self, version: &FabricVersion) -> bool {
        let order = version.cmp(&self.version);
        let same = |n: usize| (0..n).all(|i| version.component(i) == self.version.component(i));
        match self.op {
            Op::Ge => order != Ordering::Less,
            Op::Le => order != Ordering::Greater,
            Op::Gt => order == Ordering::Greater,
            Op::Lt => order == Ordering::Less,
            Op::Eq => order == Ordering::Equal,
            Op::Tilde => order != Ordering::Less && same(2),
            Op::Caret => order != Ordering::Less && same(1),
        }
    }

    /// The version fails this term because it is below the term's lower bound.
    fn below(&self, version: &FabricVersion) -> bool {
        matches!(self.op, Op::Ge | Op::Gt | Op::Eq | Op::Tilde | Op::Caret)
            && !self.test(version)
            && version.cmp(&self.version) != Ordering::Greater
    }

    /// The version fails this term because it is above the term's upper bound.
    fn above(&self, version: &FabricVersion) -> bool {
        match self.op {
            Op::Le | Op::Lt | Op::Eq => {
                !self.test(version) && version.cmp(&self.version) != Ordering::Less
            }
            Op::Tilde | Op::Caret => {
                !self.test(version) && version.cmp(&self.version) == Ordering::Greater
            }
            Op::Ge | Op::Gt => false,
        }
    }
}

/// A Fabric or Quilt version requirement: any of its alternatives, each of which needs all of
/// its terms. An alternative without terms (`*`, empty text) accepts every version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FabricPredicate {
    alternatives: Vec<Vec<Term>>,
}

impl FabricPredicate {
    /// One predicate string (`>=1.2 <2`). `None` when it does not parse with certainty.
    pub fn parse(text: &str) -> Option<Self> {
        Some(Self {
            alternatives: vec![parse_alternative(text)?],
        })
    }

    /// Any of several predicate strings (a JSON array in `fabric.mod.json`). `None` for an
    /// empty list (Fabric then accepts no version at all) or when one does not parse.
    pub fn parse_any<'a>(texts: impl IntoIterator<Item = &'a str>) -> Option<Self> {
        let alternatives = texts
            .into_iter()
            .map(parse_alternative)
            .collect::<Option<Vec<_>>>()?;
        (!alternatives.is_empty()).then_some(Self { alternatives })
    }

    /// True when every version is accepted.
    pub fn is_any(&self) -> bool {
        self.alternatives.iter().any(Vec::is_empty)
    }

    pub fn matches(&self, version: &FabricVersion) -> bool {
        self.alternatives
            .iter()
            .any(|terms| terms.iter().all(|term| term.test(version)))
    }

    /// Whether a pre-release, release candidate or snapshot of `release` matches. The exact
    /// pre-release form the loader uses is not needed: the answer is given only when it is the
    /// same for every pre-release of `release` (no bound lies among them).
    pub fn matches_prerelease_of(&self, release: &FabricVersion) -> Option<bool> {
        let bound_inside = self.alternatives.iter().flatten().any(|term| {
            term.version.same_core(release)
                && term.version.pre.as_ref().is_some_and(|p| !p.is_empty())
        });
        (!bound_inside).then(|| self.matches(&FabricVersion::probe_below(release)))
    }

    fn mismatch(&self, version: &FabricVersion) -> Mismatch {
        if self.matches(version) {
            return Mismatch::Unclear;
        }
        let failing = || self.alternatives.iter().filter(|terms| !terms.is_empty());
        if failing().all(|terms| terms.iter().any(|t| t.below(version))) {
            Mismatch::TooOld
        } else if failing().all(|terms| terms.iter().any(|t| t.above(version))) {
            Mismatch::TooNew
        } else {
            Mismatch::Unclear
        }
    }
}

fn parse_alternative(text: &str) -> Option<Vec<Term>> {
    let mut terms = Vec::new();
    for raw in text.split(' ') {
        // Java's String.trim(): every character up to and including the space.
        let token = raw.trim_matches(|c: char| c <= ' ');
        if token.is_empty() || token == "*" {
            continue;
        }
        let (op, rest) = OPERATORS
            .iter()
            .find_map(|(prefix, op)| token.strip_prefix(prefix).map(|rest| (Some(*op), rest)))
            .unwrap_or((None, token));
        let parsed = parse_semver(rest, true)?;
        let term = match parsed.wildcard_at {
            None => Term {
                op: op.unwrap_or(Op::Eq),
                version: parsed.version,
            },
            // `1.20.x` → `~1.20-`, `1.x` → `^1-` (wildcards only with `=` or no operator).
            Some(count) if matches!(op, None | Some(Op::Eq)) && (1..=2).contains(&count) => Term {
                op: if count == 1 { Op::Caret } else { Op::Tilde },
                version: FabricVersion {
                    components: parsed.version.components[..count].to_vec(),
                    pre: Some(Vec::new()),
                },
            },
            Some(_) => return None,
        };
        terms.push(term);
    }
    Some(terms)
}

// --- Forge / NeoForge Maven versions -----------------------------------------------------------

/// A version ordered like Maven's `ComparableVersion`, limited to the forms every Maven release
/// orders the same way: numbers separated by `.` or `-` and the well-known qualifiers (`alpha`,
/// `beta`, `milestone`, `rc`/`cr`, `snapshot`, `ga`/`final`/`release`, `sp`). Equality follows
/// the ordering.
#[derive(Debug, Clone)]
pub struct MavenVersion {
    items: Vec<MavenItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MavenItem {
    /// Digits without leading zeros (`0` for zero), compared by length then text.
    Int(String),
    /// Qualifier rank: alpha 0, beta 1, milestone 2, rc 3, snapshot 4, release 5, sp 6.
    Qualifier(u8),
    List(Vec<MavenItem>),
}

const RELEASE_RANK: u8 = 5;

impl MavenVersion {
    /// `None` for versions outside the certain subset (unknown qualifiers, `+`, spaces, ...).
    pub fn parse(text: &str) -> Option<Self> {
        let lower = text.to_ascii_lowercase();
        if lower.is_empty()
            || !lower
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        {
            return None;
        }
        let bytes = lower.as_bytes();
        // Each '-' or letter/digit switch opens a list nested as the last item of the current
        // one; `levels[k + 1]` is that list for `levels[k]`.
        let mut levels: Vec<Vec<MavenItem>> = vec![Vec::new()];
        let mut is_digit = false;
        let mut start = 0;
        for (i, &c) in bytes.iter().enumerate() {
            let current = levels.len() - 1;
            if c == b'.' || c == b'-' {
                let item = if i == start {
                    MavenItem::Int("0".to_owned())
                } else {
                    maven_item(is_digit, &lower[start..i], false)?
                };
                levels[current].push(item);
                is_digit = false;
                start = i + 1;
                if c == b'-' {
                    levels.push(Vec::new());
                }
            } else if c.is_ascii_digit() {
                if !is_digit && i > start {
                    levels[current].push(maven_item(false, &lower[start..i], true)?);
                    start = i;
                    levels.push(Vec::new());
                }
                is_digit = true;
            } else {
                if is_digit && i > start {
                    levels[current].push(maven_item(true, &lower[start..i], false)?);
                    start = i;
                    levels.push(Vec::new());
                }
                is_digit = false;
            }
        }
        if bytes.len() > start {
            let current = levels.len() - 1;
            levels[current].push(maven_item(is_digit, &lower[start..], false)?);
        }
        // Normalise from the innermost list outwards, as Maven does when it unwinds its stack.
        let mut child: Option<Vec<MavenItem>> = None;
        for mut items in levels.into_iter().rev() {
            if let Some(list) = child.take() {
                items.push(MavenItem::List(list));
            }
            normalize_maven(&mut items);
            child = Some(items);
        }
        Some(Self {
            items: child.unwrap_or_default(),
        })
    }
}

fn maven_item(is_digit: bool, text: &str, followed_by_digit: bool) -> Option<MavenItem> {
    if is_digit {
        let trimmed = text.trim_start_matches('0');
        return Some(MavenItem::Int(if trimmed.is_empty() {
            "0".to_owned()
        } else {
            trimmed.to_owned()
        }));
    }
    let name = match text {
        "a" if followed_by_digit => "alpha",
        "b" if followed_by_digit => "beta",
        "m" if followed_by_digit => "milestone",
        other => other,
    };
    let rank = match name {
        "alpha" => 0,
        "beta" => 1,
        "milestone" => 2,
        "rc" | "cr" => 3,
        "snapshot" => 4,
        "" | "ga" | "final" | "release" => RELEASE_RANK,
        "sp" => 6,
        _ => return None,
    };
    Some(MavenItem::Qualifier(rank))
}

fn maven_is_null(item: &MavenItem) -> bool {
    match item {
        MavenItem::Int(digits) => digits == "0",
        MavenItem::Qualifier(rank) => *rank == RELEASE_RANK,
        MavenItem::List(items) => items.is_empty(),
    }
}

/// Drop trailing "null" items (`0`, release qualifiers, empty lists), looking past lists.
fn normalize_maven(items: &mut Vec<MavenItem>) {
    let mut i = items.len();
    while i > 0 {
        i -= 1;
        if maven_is_null(&items[i]) {
            items.remove(i);
        } else if !matches!(items[i], MavenItem::List(_)) {
            break;
        }
    }
}

fn compare_int(a: &str, b: &str) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

/// `ComparableVersion` item order; `None` on the right is a missing item.
fn compare_maven_item(left: &MavenItem, right: Option<&MavenItem>) -> Ordering {
    match (left, right) {
        (MavenItem::Int(a), None) => {
            if a == "0" {
                Ordering::Equal
            } else {
                Ordering::Greater
            }
        }
        (MavenItem::Int(a), Some(MavenItem::Int(b))) => compare_int(a, b),
        (MavenItem::Int(_), Some(_)) => Ordering::Greater,
        (MavenItem::Qualifier(a), None) => a.cmp(&RELEASE_RANK),
        (MavenItem::Qualifier(a), Some(MavenItem::Qualifier(b))) => a.cmp(b),
        (MavenItem::Qualifier(_), Some(_)) => Ordering::Less,
        (MavenItem::List(items), None) => items
            .iter()
            .map(|item| compare_maven_item(item, None))
            .find(|order| *order != Ordering::Equal)
            .unwrap_or(Ordering::Equal),
        (MavenItem::List(_), Some(MavenItem::Int(_))) => Ordering::Less,
        (MavenItem::List(_), Some(MavenItem::Qualifier(_))) => Ordering::Greater,
        (MavenItem::List(a), Some(MavenItem::List(b))) => compare_maven_lists(a, b),
    }
}

fn compare_maven_lists(a: &[MavenItem], b: &[MavenItem]) -> Ordering {
    for i in 0..a.len().max(b.len()) {
        let order = match (a.get(i), b.get(i)) {
            (Some(left), right) => compare_maven_item(left, right),
            (None, Some(right)) => compare_maven_item(right, None).reverse(),
            (None, None) => Ordering::Equal,
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    Ordering::Equal
}

impl Ord for MavenVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        compare_maven_lists(&self.items, &other.items)
    }
}

impl PartialOrd for MavenVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for MavenVersion {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for MavenVersion {}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Restriction {
    lower: Option<(MavenVersion, bool)>,
    upper: Option<(MavenVersion, bool)>,
}

impl Restriction {
    fn contains(&self, version: &MavenVersion) -> bool {
        if let Some((lower, inclusive)) = &self.lower {
            match lower.cmp(version) {
                Ordering::Greater => return false,
                Ordering::Equal if !inclusive => return false,
                _ => {}
            }
        }
        if let Some((upper, inclusive)) = &self.upper {
            match upper.cmp(version) {
                Ordering::Less => return false,
                Ordering::Equal if !inclusive => return false,
                _ => {}
            }
        }
        true
    }
}

/// A Maven version range as FML reads `versionRange`. No restrictions = any version (a bare
/// version such as `1.0` is only a recommendation in Maven and accepts everything).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MavenRange {
    restrictions: Vec<Restriction>,
}

impl MavenRange {
    /// `None` when the range is malformed or a bound is not a certain version.
    pub fn parse(spec: &str) -> Option<Self> {
        let spec = spec.trim();
        if spec.is_empty() {
            return None;
        }
        if !spec.starts_with(['[', '(']) {
            return Some(Self {
                restrictions: Vec::new(),
            });
        }
        let mut restrictions: Vec<Restriction> = Vec::new();
        let mut rest = spec;
        while rest.starts_with(['[', '(']) {
            let end = rest.find([')', ']'])?;
            let restriction = parse_restriction(&rest[..=end])?;
            if let Some(previous) = restrictions.last() {
                // Maven rejects overlapping or unordered sets.
                let (Some((upper, _)), Some((lower, _))) = (&previous.upper, &restriction.lower)
                else {
                    return None;
                };
                if lower < upper {
                    return None;
                }
            }
            restrictions.push(restriction);
            rest = rest[end + 1..].trim_start();
            if let Some(after) = rest.strip_prefix(',') {
                rest = after.trim_start();
            }
        }
        (rest.is_empty() && !restrictions.is_empty()).then_some(Self { restrictions })
    }

    pub fn is_any(&self) -> bool {
        self.restrictions.is_empty()
    }

    pub fn contains(&self, version: &MavenVersion) -> bool {
        self.is_any() || self.restrictions.iter().any(|r| r.contains(version))
    }

    fn mismatch(&self, version: &MavenVersion) -> Mismatch {
        if self.contains(version) {
            return Mismatch::Unclear;
        }
        let below = |r: &Restriction| {
            r.lower
                .as_ref()
                .is_some_and(|(lower, inclusive)| match version.cmp(lower) {
                    Ordering::Less => true,
                    Ordering::Equal => !inclusive,
                    Ordering::Greater => false,
                })
        };
        if self.restrictions.iter().all(below) {
            Mismatch::TooOld
        } else if self.restrictions.iter().all(|r| !below(r)) {
            Mismatch::TooNew
        } else {
            Mismatch::Unclear
        }
    }
}

fn parse_restriction(spec: &str) -> Option<Restriction> {
    let lower_inclusive = spec.starts_with('[');
    let upper_inclusive = spec.ends_with(']');
    let inner = spec.get(1..spec.len() - 1)?.trim();
    let Some((low, high)) = inner.split_once(',') else {
        if !lower_inclusive || !upper_inclusive {
            return None;
        }
        let version = MavenVersion::parse(inner)?;
        return Some(Restriction {
            lower: Some((version.clone(), true)),
            upper: Some((version, true)),
        });
    };
    let (low, high) = (low.trim(), high.trim());
    if low == high || high.contains(',') {
        return None;
    }
    let lower = if low.is_empty() {
        None
    } else {
        Some((MavenVersion::parse(low)?, lower_inclusive))
    };
    let upper = if high.is_empty() {
        None
    } else {
        Some((MavenVersion::parse(high)?, upper_inclusive))
    };
    if let (Some((l, _)), Some((u, _))) = (&lower, &upper)
        && u < l
    {
        return None;
    }
    Some(Restriction { lower, upper })
}

// --- Requirements ---------------------------------------------------------------------------

/// How a present version fails a requirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mismatch {
    /// Older than every accepted version: a newer one would do.
    TooOld,
    /// Newer than every accepted version.
    TooNew,
    /// Neither (or the version matches).
    Unclear,
}

/// A dependency's accepted versions, in the syntax of the descriptor that declared it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionReq {
    /// No requirement (or `*`).
    Any,
    Fabric(FabricPredicate),
    Maven(MavenRange),
    /// Present but not understood with certainty; never used to report a problem.
    Unknown,
}

impl VersionReq {
    /// A Fabric/Quilt predicate string.
    pub fn fabric(text: &str) -> Self {
        match FabricPredicate::parse(text) {
            Some(p) if p.is_any() => Self::Any,
            Some(p) => Self::Fabric(p),
            None => Self::Unknown,
        }
    }

    /// Any of several Fabric/Quilt predicate strings.
    pub fn fabric_any<'a>(texts: impl IntoIterator<Item = &'a str>) -> Self {
        match FabricPredicate::parse_any(texts) {
            Some(p) if p.is_any() => Self::Any,
            Some(p) => Self::Fabric(p),
            None => Self::Unknown,
        }
    }

    /// A Forge/NeoForge `versionRange` (missing = any).
    pub fn maven(spec: Option<&str>) -> Self {
        match spec.map(MavenRange::parse) {
            None => Self::Any,
            Some(Some(range)) if range.is_any() => Self::Any,
            Some(Some(range)) => Self::Maven(range),
            Some(None) => Self::Unknown,
        }
    }

    /// The requirement as the loader of `loader` would write it.
    pub fn parse_for(loader: LoaderKind, text: &str) -> Self {
        match loader {
            LoaderKind::Forge | LoaderKind::NeoForge => Self::maven(Some(text)),
            _ => Self::fabric(text),
        }
    }

    pub fn is_any(&self) -> bool {
        matches!(self, Self::Any)
    }

    /// Whether `version` (a mod, loader or Java version) is accepted. `None` when unsure.
    pub fn matches(&self, version: &str) -> Option<bool> {
        match self {
            Self::Any => Some(true),
            Self::Fabric(p) => Some(p.matches(&FabricVersion::parse(version)?)),
            Self::Maven(r) => Some(r.contains(&MavenVersion::parse(version)?)),
            Self::Unknown => None,
        }
    }

    /// Whether the game version is accepted. Releases are compared directly; for
    /// pre-releases, release candidates and year-based snapshots a Fabric/Quilt answer is given
    /// only when it holds for every pre-release of the target release. Forge and NeoForge do
    /// not support pre-releases, so their ranges are judged on releases only.
    pub fn matches_minecraft(&self, minecraft: &MinecraftVersion) -> Option<bool> {
        match self {
            Self::Any => Some(true),
            Self::Unknown => None,
            Self::Fabric(p) => {
                let release = FabricVersion::parse(minecraft.release.as_deref()?)?;
                if minecraft.kind == MinecraftKind::Release {
                    Some(p.matches(&release))
                } else {
                    p.matches_prerelease_of(&release)
                }
            }
            Self::Maven(r) => {
                if minecraft.kind != MinecraftKind::Release {
                    return None;
                }
                Some(r.contains(&MavenVersion::parse(&minecraft.id)?))
            }
        }
    }

    /// Why `version` fails the requirement, when that can be said.
    pub fn mismatch(&self, version: &str) -> Mismatch {
        match self {
            Self::Fabric(p) => {
                FabricVersion::parse(version).map_or(Mismatch::Unclear, |v| p.mismatch(&v))
            }
            Self::Maven(r) => {
                MavenVersion::parse(version).map_or(Mismatch::Unclear, |v| r.mismatch(&v))
            }
            Self::Any | Self::Unknown => Mismatch::Unclear,
        }
    }

    /// Whether some far newer version escapes the requirement (`<0.5` does, `*` does not).
    pub fn has_upper_bound(&self) -> bool {
        self.matches("999999.0.0") == Some(false)
    }
}

/// Whether `version` satisfies a requirement written for `loader` (`>=0.5` for Fabric/Quilt,
/// `[1,2)` for Forge/NeoForge). For phase two's Modrinth candidates; `None` when unsure.
pub fn requirement_matches(loader: LoaderKind, requirement: &str, version: &str) -> Option<bool> {
    VersionReq::parse_for(loader, requirement).matches(version)
}

/// The lowest Java major version a Fabric/Quilt `java` requirement accepts, if one in 8..=40.
pub fn minimum_java(req: &VersionReq) -> Option<u32> {
    (8..=40).find(|major| req.matches(&major.to_string()) == Some(true))
}

// --- Minecraft versions -----------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MinecraftKind {
    Release,
    PreRelease,
    ReleaseCandidate,
    Snapshot,
    /// April fools versions, old alpha/beta versions, unknown ids.
    Other,
}

/// A Minecraft version id and what is known about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinecraftVersion {
    pub id: String,
    pub kind: MinecraftKind,
    /// The release this version is or leads up to (`1.21` for `1.21-pre1`, `26.1` for
    /// `26.1-snapshot-2`). `None` for week snapshots (`24w14a`) and unknown ids.
    pub release: Option<String>,
    /// The semantic version Fabric Loader turns the id into (`1.21-beta.1`), when known for
    /// certain; the year-based pre-release forms are left out on purpose.
    pub normalized: Option<String>,
}

static RELEASE_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d+\.\d+(?:\.\d+)?$").expect("valid regex"));
static CLASSIC_PRE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\d+\.\d+(?:\.\d+)?)(?:-pre| Pre-[Rr]elease )(\d+)$").expect("valid regex")
});
static CLASSIC_RC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\d+\.\d+(?:\.\d+)?)(?:-rc| [Rr]elease Candidate )(\d+)$").expect("valid regex")
});
static YEAR_BASED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\d+\.\d+(?:\.\d+)?)-(pre|rc|snapshot)-(\d+)$").expect("valid regex")
});
static WEEK_SNAPSHOT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d{2}w\d{2}[a-z]$").expect("valid regex"));

impl MinecraftVersion {
    pub fn parse(id: &str) -> Self {
        let id = id.trim();
        let make = |kind, release: Option<&str>, normalized: Option<String>| Self {
            id: id.to_owned(),
            kind,
            release: release.map(str::to_owned),
            normalized,
        };
        if RELEASE_ID.is_match(id) {
            return make(MinecraftKind::Release, Some(id), Some(id.to_owned()));
        }
        if let Some(c) = CLASSIC_PRE.captures(id) {
            let (release, n) = (&c[1], &c[2]);
            // Fabric marks pre-releases of 1.16 and earlier as release candidates.
            let legacy = FabricVersion::parse(release)
                .zip(FabricVersion::parse("1.16"))
                .is_some_and(|(r, limit)| r <= limit);
            let tag = if legacy { "rc" } else { "beta" };
            let normalized = n
                .parse::<u32>()
                .ok()
                .map(|n| format!("{release}-{tag}.{n}"));
            return make(MinecraftKind::PreRelease, Some(release), normalized);
        }
        if let Some(c) = CLASSIC_RC.captures(id) {
            let release = &c[1];
            // Fabric numbers 1.16's release candidates after its eight pre-releases.
            let offset = if release == "1.16" { 8 } else { 0 };
            let normalized = c[2]
                .parse::<u32>()
                .ok()
                .map(|n| format!("{release}-rc.{}", n + offset));
            return make(MinecraftKind::ReleaseCandidate, Some(release), normalized);
        }
        if let Some(c) = YEAR_BASED.captures(id) {
            let kind = match &c[2] {
                "pre" => MinecraftKind::PreRelease,
                "rc" => MinecraftKind::ReleaseCandidate,
                _ => MinecraftKind::Snapshot,
            };
            return make(kind, Some(&c[1]), None);
        }
        if WEEK_SNAPSHOT.is_match(id) {
            return make(MinecraftKind::Snapshot, None, None);
        }
        make(MinecraftKind::Other, None, None)
    }

    /// Version with a major/minor/patch at least `(major, minor)` (releases and pre-releases).
    pub fn at_least(&self, major: u32, minor: u32, patch: u32) -> Option<bool> {
        let release = FabricVersion::parse(self.release.as_deref()?)?;
        let limit = FabricVersion {
            components: vec![major, minor, patch],
            pre: None,
        };
        Some(release >= limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> FabricVersion {
        FabricVersion::parse(text).unwrap_or_else(|| panic!("{text} should parse"))
    }

    fn pred(text: &str) -> FabricPredicate {
        FabricPredicate::parse(text).unwrap_or_else(|| panic!("{text} should parse"))
    }

    fn mv(text: &str) -> MavenVersion {
        MavenVersion::parse(text).unwrap_or_else(|| panic!("{text} should parse"))
    }

    #[test]
    fn fabric_versions_parse_like_fabric() {
        for ok in [
            "1",
            "1.2.3",
            "1.2.3-beta.1+build.5",
            "0.5.3+mc1.20.1",
            "1.21-",
            "1.0.0-beta-2",
            "2147483647",
        ] {
            assert!(FabricVersion::parse(ok).is_some(), "{ok}");
        }
        for bad in [
            "",
            "1..2",
            "1.2.",
            ".1",
            "mc1.20.1-0.5",
            "1.0_beta",
            "1.0-beta_1",
            "1.0+build_5",
            "2147483648",
            "1.x",
            "١.٢",
        ] {
            assert!(FabricVersion::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn fabric_ordering() {
        assert_eq!(v("1.0").cmp(&v("1.0.0")), Ordering::Equal);
        assert_eq!(v("1.0+a").cmp(&v("1.0+b")), Ordering::Equal);
        assert!(v("1.21-") < v("1.21-0"));
        assert!(v("1.21-0") < v("1.21-alpha.1"));
        assert!(v("1.21-alpha.1") < v("1.21-beta.1"));
        assert!(v("1.21-beta.2") < v("1.21-beta.10"));
        assert!(v("1.21-beta.10") < v("1.21-rc.1"));
        assert!(v("1.21-rc.1") < v("1.21"));
        assert!(v("1.21") < v("1.21.1-pre.1"));
        assert!(v("1.0.0-alpha") < v("1.0.0-alpha.1"));
        assert!(v("1.0.0-01") > v("1.0.0-1"), "leading zero is alphanumeric");
        assert!(v("0.14.21") < v("0.15"));
    }

    #[test]
    fn fabric_predicates() {
        let range = pred(">=1.2 <2");
        assert!(range.matches(&v("1.2")));
        assert!(range.matches(&v("1.9.9")));
        assert!(!range.matches(&v("2.0")));
        assert!(!range.matches(&v("1.1")));
        assert!(
            range.matches(&v("2.0-alpha")),
            "a pre-release of 2 sorts below 2"
        );
        assert!(
            !pred(">=1.2 <2-").matches(&v("2.0-alpha")),
            "`<2-` excludes them"
        );
        assert!(pred("~1.20").matches(&v("1.20.6")));
        assert!(!pred("~1.20").matches(&v("1.21")));
        assert!(!pred("~1.20.2").matches(&v("1.20.1")));
        assert!(pred("^3.1").matches(&v("3.9")));
        assert!(!pred("^3.1").matches(&v("4.0")));
        assert!(pred("1.20.x").matches(&v("1.20.6")));
        assert!(pred("1.20.x").matches(&v("1.20")));
        assert!(
            pred("1.20.x").matches(&v("1.20-beta.1")),
            "1.20.x is ~1.20-"
        );
        assert!(!pred("~1.20").matches(&v("1.20-beta.1")));
        assert!(!pred("1.20.x").matches(&v("1.21")));
        assert!(pred("1.x").matches(&v("1.99")));
        assert!(pred("1.x.x").matches(&v("1.99")));
        assert!(!pred("1.x").matches(&v("2.0")));
        assert!(pred("=1.20.x").matches(&v("1.20.4")));
        assert!(pred("1.20.1").matches(&v("1.20.1+build")));
        assert!(!pred("1.20.1").matches(&v("1.20.2")));
        assert!(pred("  >=0.5   ").matches(&v("0.6")));
        assert!(pred("*").is_any());
        assert!(pred("").is_any());
        assert!(pred(">=1.21- <1.21.2-").matches(&v("1.21.1")));
        assert!(pred(">=1.21- <1.21.2-").matches(&v("1.21-rc.1")));
        assert!(!pred(">=1.21- <1.21.2-").matches(&v("1.21.2-pre.1")));

        let any = FabricPredicate::parse_any(["1.20", "1.20.1"]).unwrap();
        assert!(any.matches(&v("1.20.1")));
        assert!(!any.matches(&v("1.20.2")));
        assert!(FabricPredicate::parse_any([]).is_none());

        for uncertain in [
            ">=1.20.x",
            "x",
            "1.x.2",
            "1.2.3.x",
            ">=mc1.20",
            "<abc",
            "1.20.x-pre",
        ] {
            assert!(FabricPredicate::parse(uncertain).is_none(), "{uncertain}");
        }
    }

    #[test]
    fn mismatch_direction() {
        let req = VersionReq::fabric(">=0.5.0");
        assert_eq!(req.mismatch("0.4.10"), Mismatch::TooOld);
        assert_eq!(req.mismatch("0.5.1"), Mismatch::Unclear);
        let req = VersionReq::fabric("<0.5");
        assert_eq!(req.mismatch("0.6"), Mismatch::TooNew);
        assert!(req.has_upper_bound());
        assert!(!VersionReq::fabric(">=1").has_upper_bound());
        let req = VersionReq::fabric("~1.20");
        assert_eq!(req.mismatch("1.19.4"), Mismatch::TooOld);
        assert_eq!(req.mismatch("1.21"), Mismatch::TooNew);
        let req = VersionReq::maven(Some("[47.1,)"));
        assert_eq!(req.mismatch("47.0.3"), Mismatch::TooOld);
        let req = VersionReq::maven(Some("[1.20.1,1.20.2)"));
        assert_eq!(req.mismatch("1.21"), Mismatch::TooNew);
    }

    #[test]
    fn maven_versions() {
        assert_eq!(mv("1.0").cmp(&mv("1")), Ordering::Equal);
        assert_eq!(mv("1.0.0").cmp(&mv("1")), Ordering::Equal);
        assert!(mv("1.0-beta") < mv("1.0"));
        assert!(mv("1.0-alpha") < mv("1.0-beta"));
        assert!(mv("1.0-rc1") < mv("1.0"));
        assert!(mv("1.0-rc1") < mv("1.0-rc2"));
        assert!(mv("1.0-SNAPSHOT") < mv("1.0"));
        assert!(mv("1.0-rc1") < mv("1.0-SNAPSHOT"));
        assert!(mv("1.0-sp") > mv("1.0"));
        assert!(mv("1.0-beta") < mv("1.0.1"));
        assert!(mv("20.4.80-beta") < mv("20.4.80"));
        assert!(mv("20.4.80-beta") > mv("20.4.79"));
        assert!(mv("47.2.0") > mv("47.1"));
        assert!(mv("1.20.1") < mv("1.21"));
        assert!(mv("26.1") > mv("1.21.11"));
        assert!(mv("1.20.1-47.2.0") > mv("1.20.1"));
        assert!(mv("1.0-final") == mv("1.0"));
        assert!(mv("1.0a1") == mv("1.0-alpha-1"));
        assert!(mv("12345678901234567890") > mv("9999999999"));
        for unsure in [
            "1.0-pre1",
            "1.0+build",
            "v1.0",
            "1.20.1-0.5.1.f",
            "1.0 beta",
            "",
        ] {
            assert!(MavenVersion::parse(unsure).is_none(), "{unsure}");
        }
    }

    #[test]
    fn maven_ranges() {
        let r = MavenRange::parse("[1.20,1.21)").unwrap();
        assert!(r.contains(&mv("1.20")));
        assert!(r.contains(&mv("1.20.6")));
        assert!(!r.contains(&mv("1.21")));
        assert!(!r.contains(&mv("1.19.4")));
        let r = MavenRange::parse("[47,)").unwrap();
        assert!(r.contains(&mv("47.2.0")));
        assert!(!r.contains(&mv("46.0.1")));
        let r = MavenRange::parse("(,2.0]").unwrap();
        assert!(r.contains(&mv("2.0")));
        assert!(!r.contains(&mv("2.0.1")));
        let r = MavenRange::parse("[1.0]").unwrap();
        assert!(r.contains(&mv("1.0.0")));
        assert!(!r.contains(&mv("1.0.1")));
        let r = MavenRange::parse("[1,2), [3,4)").unwrap();
        assert!(r.contains(&mv("3.5")));
        assert!(!r.contains(&mv("2.5")));
        assert!(MavenRange::parse("1.0").unwrap().is_any());
        assert!(MavenRange::parse("*").unwrap().is_any());
        for bad in [
            "",
            "[1.0",
            "(1.0)",
            "[2,1]",
            "[1,1]",
            "[1,3),[2,4)",
            "[1,2) x",
            "[1.0-pre1,)",
        ] {
            assert!(MavenRange::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn minecraft_ids() {
        let release = MinecraftVersion::parse("1.20.1");
        assert_eq!(release.kind, MinecraftKind::Release);
        assert_eq!(release.normalized.as_deref(), Some("1.20.1"));
        for year in ["26.1", "26.1.2"] {
            let parsed = MinecraftVersion::parse(year);
            assert_eq!(parsed.kind, MinecraftKind::Release, "{year}");
            assert_eq!(parsed.release.as_deref(), Some(year));
        }
        let pre = MinecraftVersion::parse("1.21-pre1");
        assert_eq!(pre.kind, MinecraftKind::PreRelease);
        assert_eq!(pre.release.as_deref(), Some("1.21"));
        assert_eq!(pre.normalized.as_deref(), Some("1.21-beta.1"));
        assert_eq!(
            MinecraftVersion::parse("1.16-pre3").normalized.as_deref(),
            Some("1.16-rc.3")
        );
        assert_eq!(
            MinecraftVersion::parse("1.16-rc1").normalized.as_deref(),
            Some("1.16-rc.9")
        );
        assert_eq!(
            MinecraftVersion::parse("1.20.5-rc2").normalized.as_deref(),
            Some("1.20.5-rc.2")
        );
        assert_eq!(
            MinecraftVersion::parse("1.14 Pre-Release 2")
                .normalized
                .as_deref(),
            Some("1.14-rc.2")
        );
        for (id, kind) in [
            ("26.1-pre-2", MinecraftKind::PreRelease),
            ("26.1-rc-1", MinecraftKind::ReleaseCandidate),
            ("26.1-snapshot-3", MinecraftKind::Snapshot),
        ] {
            let parsed = MinecraftVersion::parse(id);
            assert_eq!(parsed.kind, kind, "{id}");
            assert_eq!(parsed.release.as_deref(), Some("26.1"));
            assert_eq!(parsed.normalized, None);
        }
        let week = MinecraftVersion::parse("24w14a");
        assert_eq!(week.kind, MinecraftKind::Snapshot);
        assert_eq!(week.release, None);
        for other in ["b1.7.3", "20w14infinite", "", "1.21-pre", "rd-132211"] {
            assert_eq!(
                MinecraftVersion::parse(other).kind,
                MinecraftKind::Other,
                "{other}"
            );
        }
        assert_eq!(
            MinecraftVersion::parse("1.20.4").at_least(1, 20, 5),
            Some(false)
        );
        assert_eq!(
            MinecraftVersion::parse("1.20.5").at_least(1, 20, 5),
            Some(true)
        );
        assert_eq!(
            MinecraftVersion::parse("26.1").at_least(1, 20, 5),
            Some(true)
        );
        assert_eq!(MinecraftVersion::parse("24w14a").at_least(1, 20, 5), None);
    }

    #[test]
    fn minecraft_requirements() {
        let mc = |id: &str| MinecraftVersion::parse(id);
        assert_eq!(
            VersionReq::fabric("1.20.x").matches_minecraft(&mc("1.20.1")),
            Some(true)
        );
        assert_eq!(
            VersionReq::fabric("1.20.x").matches_minecraft(&mc("1.21.1")),
            Some(false)
        );
        assert_eq!(
            VersionReq::fabric(">=26.1").matches_minecraft(&mc("26.1.2")),
            Some(true)
        );
        assert_eq!(
            VersionReq::fabric("~1.21").matches_minecraft(&mc("26.1")),
            Some(false)
        );
        // Pre-releases: only answers that hold for every pre-release of the release.
        assert_eq!(
            VersionReq::fabric("~1.20").matches_minecraft(&mc("1.21-pre1")),
            Some(false)
        );
        assert_eq!(
            VersionReq::fabric(">=1.21").matches_minecraft(&mc("1.21-pre1")),
            Some(false)
        );
        assert_eq!(
            VersionReq::fabric("1.21.x").matches_minecraft(&mc("1.21-pre1")),
            Some(true)
        );
        assert_eq!(
            VersionReq::fabric(">=1.20").matches_minecraft(&mc("26.1-snapshot-1")),
            Some(true)
        );
        assert_eq!(
            VersionReq::fabric(">=1.21-beta.2").matches_minecraft(&mc("1.21-pre1")),
            None
        );
        assert_eq!(
            VersionReq::fabric(">=1.20").matches_minecraft(&mc("24w14a")),
            None
        );
        assert_eq!(
            VersionReq::fabric("*").matches_minecraft(&mc("24w14a")),
            Some(true)
        );
        let forge = VersionReq::maven(Some("[1.20.1,1.20.2)"));
        assert_eq!(forge.matches_minecraft(&mc("1.20.1")), Some(true));
        assert_eq!(forge.matches_minecraft(&mc("1.21.1")), Some(false));
        assert_eq!(forge.matches_minecraft(&mc("1.20.2-pre1")), None);
        assert_eq!(VersionReq::Unknown.matches_minecraft(&mc("1.20.1")), None);
    }

    #[test]
    fn helpers() {
        assert_eq!(
            requirement_matches(LoaderKind::Fabric, ">=0.15", "0.14.21"),
            Some(false)
        );
        assert_eq!(
            requirement_matches(LoaderKind::NeoForge, "[21.1.0,)", "21.1.77"),
            Some(true)
        );
        assert_eq!(
            requirement_matches(LoaderKind::Forge, "[1.0,)", "1.0+x"),
            None
        );
        assert_eq!(minimum_java(&VersionReq::fabric(">=21")), Some(21));
        assert_eq!(minimum_java(&VersionReq::fabric(">=17 <21")), Some(17));
        assert_eq!(minimum_java(&VersionReq::Unknown), None);
        assert_eq!(VersionReq::maven(None), VersionReq::Any);
        assert_eq!(VersionReq::maven(Some("*")), VersionReq::Any);
        assert_eq!(VersionReq::maven(Some("[1.0")), VersionReq::Unknown);
        assert_eq!(VersionReq::fabric("*"), VersionReq::Any);
        assert_eq!(VersionReq::fabric_any(["*", ">=2"]), VersionReq::Any);
        assert_eq!(VersionReq::fabric(">=1.x"), VersionReq::Unknown);
    }
}
