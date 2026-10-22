//! Auto-respond patterns: what the settings page accepts and how they reach the client.
//!
//! MCC builds every `regex=` rule of `matches.ini` with .NET's `new Regex(pattern)`. A pattern .NET
//! cannot parse throws while the bot loads (that rule and every later one are lost, with a stack
//! trace in the console), and .NET's backtracking engine runs without a time limit, so a pattern
//! like `(a+)+$` can freeze the client on one unlucky chat line. So only regular expressions in a
//! subset both engines read the same way pass (checked with `regex-syntax`, the parser of Rust's
//! `regex` crate): plain syntax, `\d \w \s`, `\p{..}` general categories, `\xNN` and `\uNNNN`
//! escapes, `(?<name>…)` groups and the `i`, `m` and `s` flags; no repetition inside a repetition,
//! counts up to 1000, at most [`MAX_PATTERN_CHARS`] characters. Lookarounds and backreferences are
//! refused (Rust's parser does not know them). Inside a repetition nothing may split a line more
//! than one way either (`(a|aa)+`, `(a?a)+`, `(.|\s)*`, `(a?b?)+`, `(a?){25}`): .NET would try
//! every split. Across the pattern, at most [`MAX_LINE_FACTORS`] loops may hand characters to what
//! follows them (`.*a.*a.*a.*a!` tries every way to cut a line in four), a start .NET may try at
//! every position counting as one more. These checks compare the characters parts can start
//! with, so they also refuse a few harmless shapes.
//!
//! The check runs when the settings are saved and again when the client's files are written, so
//! rules stored before it existed (or edited by hand) never reach the client unchecked. MCC's
//! `matches.ini` reader also cuts every line at `#`: the writer spells `#` as `\x23`, and a
//! plain-text pattern containing `#` is written as the equivalent case-insensitive regex.

use regex_syntax::ast::parse::ParserBuilder;
use regex_syntax::ast::{
    AssertionKind, Ast, ClassSet, ClassSetItem, ClassUnicode, ClassUnicodeKind, Flag, Flags,
    FlagsItemKind, GroupKind, HexLiteralKind, Literal, LiteralKind, RepetitionKind,
    RepetitionRange,
};
use regex_syntax::hir::translate::TranslatorBuilder;
use regex_syntax::hir::{self, ClassUnicodeRange, HirKind};
use serde::{Deserialize, Serialize};

/// Longest pattern, in characters (a chat line is at most 256).
pub const MAX_PATTERN_CHARS: usize = 256;
/// Largest repetition count (`{n}`, `{n,}`, `{n,m}`).
const MAX_REPEAT: u32 = 1000;
/// Deepest nesting of groups, repetitions and classes.
const NEST_LIMIT: u32 = 32;
/// Most factors of the line length .NET may multiply its work by on a line that does not match:
/// one per loop that can give characters back to what follows it, one more when the match may
/// start anywhere. Measured with .NET 10 (CR2): three take ~0.1 s on a 320-character line, four
/// over two seconds on 256 characters.
const MAX_LINE_FACTORS: u32 = 3;

/// Unicode general categories .NET knows by these exact names (`\p{L}`, `\P{Nd}`, ...).
const GENERAL_CATEGORIES: &[&str] = &[
    "L", "Lu", "Ll", "Lt", "Lm", "Lo", "M", "Mn", "Mc", "Me", "N", "Nd", "Nl", "No", "P", "Pc",
    "Pd", "Ps", "Pe", "Pi", "Pf", "Po", "S", "Sm", "Sc", "Sk", "So", "Z", "Zs", "Zl", "Zp", "C",
    "Cc", "Cf", "Cs", "Co", "Cn",
];

/// Why an auto-respond pattern can not be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum PatternProblem {
    /// Longer than [`MAX_PATTERN_CHARS`] characters.
    TooLong,
    /// Not a regular expression the client reads (or a control character in the text).
    Invalid,
    /// A repetition inside a repetition, a repetition that can split a line more than one way
    /// (rounds that may be empty or start like each other), more than three loops that can hand
    /// characters to what follows them, or a count above 1000: the client could take forever on
    /// some chat lines.
    TooComplex,
}

/// Checks a rule's pattern as it is saved (trimmed). An empty pattern is not a problem here: such
/// a rule is dropped on save.
pub fn check_pattern(pattern: &str, is_regex: bool) -> Option<PatternProblem> {
    let pattern = pattern.trim();
    if pattern.chars().count() > MAX_PATTERN_CHARS {
        return Some(PatternProblem::TooLong);
    }
    if pattern.chars().any(char::is_control) {
        return Some(PatternProblem::Invalid);
    }
    if !is_regex || pattern.is_empty() {
        return None;
    }
    let Ok(ast) = ParserBuilder::new()
        .nest_limit(NEST_LIMIT)
        .build()
        .parse(pattern)
    else {
        return Some(PatternProblem::Invalid);
    };
    if TranslatorBuilder::new()
        .build()
        .translate(pattern, &ast)
        .is_err()
    {
        return Some(PatternProblem::Invalid);
    }
    let starts = Starts::new(pattern, &ast);
    if let Err(problem) = walk(&ast, false, &starts) {
        return Some(problem);
    }
    let factors = starts.loops(&ast, &Start::nothing()) + u32::from(!anchored(&ast));
    (factors > MAX_LINE_FACTORS).then_some(PatternProblem::TooComplex)
}

/// The match can only begin at the start of the line (`^`, `\A`), or begins with an unbounded
/// loop, after which .NET skips the positions the loop already covered. A capturing group around
/// that loop turns the skip off (measured).
fn anchored(ast: &Ast) -> bool {
    fn first(asts: &[Ast]) -> Option<&Ast> {
        asts.iter().find(|ast| !matches!(ast, Ast::Flags(_)))
    }
    fn at_line_start(ast: &Ast) -> bool {
        match ast {
            Ast::Assertion(assertion) => matches!(
                assertion.kind,
                AssertionKind::StartLine | AssertionKind::StartText
            ),
            Ast::Group(group) => at_line_start(&group.ast),
            Ast::Concat(concat) => first(&concat.asts).is_some_and(at_line_start),
            Ast::Alternation(alternation) => alternation.asts.iter().all(at_line_start),
            _ => false,
        }
    }
    fn leading_loop(ast: &Ast) -> bool {
        match ast {
            Ast::Repetition(repetition) => matches!(
                repetition.op.kind,
                RepetitionKind::ZeroOrMore
                    | RepetitionKind::OneOrMore
                    | RepetitionKind::Range(RepetitionRange::AtLeast(_))
            ),
            Ast::Group(group) => {
                matches!(group.kind, GroupKind::NonCapturing(_)) && leading_loop(&group.ast)
            }
            Ast::Concat(concat) => first(&concat.asts).is_some_and(leading_loop),
            _ => false,
        }
    }
    at_line_start(ast) || leading_loop(ast)
}

/// The pattern line of a `[Match]` block for a pattern [`check_pattern`] accepted (trimmed):
/// `match=` (MCC compares it case-insensitively) for plain text, `regex=` for a regular
/// expression. MCC's `matches.ini` reader cuts every line at `#`, so a regex spells `#` as `\x23`
/// and plain text containing `#` becomes the equivalent case-insensitive `regex=(?i)…`.
pub fn matches_line(pattern: &str, is_regex: bool) -> String {
    let pattern = pattern.trim();
    if is_regex {
        format!("regex={}", escape_hash(pattern))
    } else if pattern.contains('#') {
        // `match=` compares case-insensitively; the regex does the same without a `#`.
        format!("regex=(?i){}", escape_literal(pattern))
    } else {
        format!("match={pattern}")
    }
}

type Check = Result<(), PatternProblem>;

const INVALID: Check = Err(PatternProblem::Invalid);

/// `repeated`: inside a repetition that matches more than once. There, every way to split a line
/// between the rounds is one more path for .NET's backtracking (exponentially many on some lines):
/// alternatives must not start alike, an optional part must not start like what follows it, and
/// a round must not end in an optional part that starts like the next round.
fn walk(ast: &Ast, repeated: bool, starts: &Starts<'_>) -> Check {
    match ast {
        Ast::Empty(_) | Ast::Dot(_) | Ast::ClassPerl(_) => Ok(()),
        Ast::Flags(set) => flags(&set.flags),
        Ast::Literal(literal) => self::literal(literal),
        Ast::Assertion(assertion) => match assertion.kind {
            AssertionKind::StartLine
            | AssertionKind::EndLine
            | AssertionKind::StartText
            | AssertionKind::EndText
            | AssertionKind::WordBoundary
            | AssertionKind::NotWordBoundary => Ok(()),
            // `\<`, `\>` and `\b{start}`-style boundaries mean other things to .NET.
            _ => INVALID,
        },
        Ast::ClassUnicode(class) => unicode_class(class),
        Ast::ClassBracketed(class) => class_set(&class.kind),
        Ast::Repetition(repetition) => {
            let many = match &repetition.op.kind {
                RepetitionKind::ZeroOrOne => false,
                RepetitionKind::ZeroOrMore | RepetitionKind::OneOrMore => true,
                RepetitionKind::Range(range) => {
                    let (count, many) = match *range {
                        RepetitionRange::Exactly(n) => (n, n > 1),
                        RepetitionRange::AtLeast(n) => (n, true),
                        RepetitionRange::Bounded(_, n) => (n, n > 1),
                    };
                    if count > MAX_REPEAT {
                        return Err(PatternProblem::TooComplex);
                    }
                    many
                }
            };
            if many && repeated {
                return Err(PatternProblem::TooComplex);
            }
            // Counted rounds that may be empty (`(a?){25}`): .NET tries every way to spread the
            // characters over them. `*`, `+` and `{0,n}` stop at an empty round instead.
            if min_rounds(&repetition.op.kind) >= 2 && starts.of(&repetition.ast).nullable {
                return Err(PatternProblem::TooComplex);
            }
            walk(&repetition.ast, repeated || many, starts)?;
            if many {
                starts.check_rounds(&repetition.ast)?;
            }
            Ok(())
        }
        Ast::Group(group) => {
            match &group.kind {
                GroupKind::CaptureIndex(_) => {}
                GroupKind::CaptureName {
                    starts_with_p,
                    name,
                } => {
                    // .NET has no `(?P<…>`; its names are word characters (Rust also allows
                    // `.`, `[` and `]`).
                    let plain = name
                        .name
                        .starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
                        && name
                            .name
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '_');
                    if *starts_with_p || !plain {
                        return INVALID;
                    }
                }
                GroupKind::NonCapturing(group_flags) => flags(group_flags)?,
            }
            walk(&group.ast, repeated, starts)
        }
        Ast::Alternation(alternation) => {
            alternation
                .asts
                .iter()
                .try_for_each(|ast| walk(ast, repeated, starts))?;
            if repeated {
                starts.check_branches(&alternation.asts)?;
            }
            Ok(())
        }
        Ast::Concat(concat) => {
            concat
                .asts
                .iter()
                .try_for_each(|ast| walk(ast, repeated, starts))?;
            if repeated {
                starts.check_sequence(&concat.asts)?;
            }
            Ok(())
        }
    }
}

/// What a part of a pattern can match first.
#[derive(Clone)]
struct Start {
    /// The characters a non-empty match can start with.
    chars: hir::ClassUnicode,
    /// It can match nothing.
    nullable: bool,
}

impl Start {
    fn nothing() -> Self {
        Self {
            chars: hir::ClassUnicode::empty(),
            nullable: true,
        }
    }

    /// This part followed by `rest`.
    fn then(mut self, rest: &Start) -> Self {
        if self.nullable {
            self.chars.union(&rest.chars);
            self.nullable = rest.nullable;
        }
        self
    }
}

/// The start sets of one pattern's parts, for the checks inside repetitions (see [`walk`]). A set
/// may be larger than exact (letter case is folded when the pattern turns `i` on anywhere, `.`
/// takes `\n` when it turns `s` on anywhere): a larger set only refuses more.
struct Starts<'a> {
    pattern: &'a str,
    /// Flags that hold for the whole pattern when its classes are read on their own.
    flags: &'static str,
}

impl<'a> Starts<'a> {
    fn new(pattern: &'a str, ast: &Ast) -> Self {
        let flags = match (
            turns_on(ast, Flag::CaseInsensitive),
            turns_on(ast, Flag::DotMatchesNewLine),
        ) {
            (true, true) => "(?is)",
            (true, false) => "(?i)",
            (false, true) => "(?s)",
            (false, false) => "",
        };
        Self { pattern, flags }
    }

    fn of(&self, ast: &Ast) -> Start {
        match ast {
            Ast::Empty(_) | Ast::Flags(_) | Ast::Assertion(_) => Start::nothing(),
            Ast::Literal(_)
            | Ast::Dot(_)
            | Ast::ClassPerl(_)
            | Ast::ClassUnicode(_)
            | Ast::ClassBracketed(_) => Start {
                chars: self.class(ast),
                nullable: false,
            },
            Ast::Repetition(repetition) => {
                let mut start = self.of(&repetition.ast);
                start.nullable |= min_rounds(&repetition.op.kind) == 0;
                start
            }
            Ast::Group(group) => self.of(&group.ast),
            Ast::Alternation(alternation) => alternation.asts.iter().fold(
                Start {
                    chars: hir::ClassUnicode::empty(),
                    nullable: false,
                },
                |mut all, ast| {
                    let branch = self.of(ast);
                    all.chars.union(&branch.chars);
                    all.nullable |= branch.nullable;
                    all
                },
            ),
            Ast::Concat(concat) => concat
                .asts
                .iter()
                .rev()
                .fold(Start::nothing(), |rest, ast| self.of(ast).then(&rest)),
        }
    }

    /// The characters a match can go on with after it has matched something and could stop: an
    /// optional ending (`b?` in `ab?`) or more rounds of a repetition. An ending skipped after an
    /// empty match does not count: .NET ends a loop on an empty round.
    fn optional_tail(&self, ast: &Ast) -> hir::ClassUnicode {
        match ast {
            Ast::Repetition(repetition) => {
                let mut tail = self.optional_tail(&repetition.ast);
                if more_than_once(&repetition.op.kind) {
                    tail.union(&self.of(&repetition.ast).chars);
                }
                tail
            }
            Ast::Group(group) => self.optional_tail(&group.ast),
            Ast::Alternation(alternation) => {
                let mut tail = hir::ClassUnicode::empty();
                for ast in &alternation.asts {
                    tail.union(&self.optional_tail(ast));
                }
                tail
            }
            Ast::Concat(concat) => {
                let mut tail = hir::ClassUnicode::empty();
                for (index, ast) in concat.asts.iter().enumerate().rev() {
                    tail.union(&self.optional_tail(ast));
                    let part = self.of(ast);
                    if !part.nullable {
                        break;
                    }
                    // Skipping this part ends the match early only after something before it.
                    if concat.asts[..index]
                        .iter()
                        .any(|before| !self.of(before).chars.ranges().is_empty())
                    {
                        tail.union(&part.chars);
                    }
                }
                tail
            }
            _ => hir::ClassUnicode::empty(),
        }
    }

    /// How many loops on the worst path through `ast` can hand characters to what follows them
    /// (`follow`: what can come next). On a line that does not match, .NET tries every cut at
    /// each of them, so each multiplies its work by up to the line length. Loops inside loops
    /// are refused before this is asked.
    fn loops(&self, ast: &Ast, follow: &Start) -> u32 {
        match ast {
            Ast::Repetition(repetition) if more_than_once(&repetition.op.kind) => {
                u32::from(overlaps(&self.of(&repetition.ast).chars, &follow.chars))
            }
            Ast::Repetition(repetition) => self.loops(&repetition.ast, follow),
            Ast::Group(group) => self.loops(&group.ast, follow),
            Ast::Alternation(alternation) => alternation
                .asts
                .iter()
                .map(|ast| self.loops(ast, follow))
                .max()
                .unwrap_or(0),
            Ast::Concat(concat) => {
                let mut rest = follow.clone();
                let mut total = 0;
                for part in concat.asts.iter().rev() {
                    total += self.loops(part, &rest);
                    rest = self.of(part).then(&rest);
                }
                total
            }
            _ => 0,
        }
    }

    /// Alternatives inside a repetition must not start alike (`(a|aa)+`, `(.|\s)*`).
    fn check_branches(&self, branches: &[Ast]) -> Check {
        let mut seen = hir::ClassUnicode::empty();
        for branch in branches {
            let chars = self.of(branch).chars;
            if overlaps(&chars, &seen) {
                return Err(PatternProblem::TooComplex);
            }
            seen.union(&chars);
        }
        Ok(())
    }

    /// In a sequence inside a repetition, an optional part must not start like what follows it
    /// (`(a?a)+`).
    fn check_sequence(&self, parts: &[Ast]) -> Check {
        let mut rest = Start::nothing();
        for part in parts.iter().rev() {
            let start = self.of(part);
            if start.nullable && overlaps(&start.chars, &rest.chars) {
                return Err(PatternProblem::TooComplex);
            }
            rest = start.then(&rest);
        }
        Ok(())
    }

    /// A round must not be able to stop before an optional ending that starts like the next
    /// round (`(aa?)+`, `(ab?|b)+`, `(a?b?)+`): those characters could go either way.
    fn check_rounds(&self, body: &Ast) -> Check {
        if overlaps(&self.optional_tail(body), &self.of(body).chars) {
            return Err(PatternProblem::TooComplex);
        }
        Ok(())
    }

    /// The characters a one-character part (literal, `.`, class) matches.
    fn class(&self, ast: &Ast) -> hir::ClassUnicode {
        match ast {
            Ast::Literal(literal) => {
                let mut class =
                    hir::ClassUnicode::new([ClassUnicodeRange::new(literal.c, literal.c)]);
                if self.flags.contains('i') {
                    class.case_fold_simple();
                }
                class
            }
            Ast::Dot(_) if !self.flags.contains('s') => hir::ClassUnicode::new([
                ClassUnicodeRange::new('\0', '\t'),
                ClassUnicodeRange::new('\u{b}', char::MAX),
            ]),
            _ => {
                // A class reads the same on its own: translate just its text.
                let span = ast.span();
                let text = format!(
                    "{}{}",
                    self.flags,
                    &self.pattern[span.start.offset..span.end.offset]
                );
                let translated = ParserBuilder::new()
                    .build()
                    .parse(&text)
                    .ok()
                    .and_then(|ast| TranslatorBuilder::new().build().translate(&text, &ast).ok());
                match translated.as_ref().map(hir::Hir::kind) {
                    Some(HirKind::Class(hir::Class::Unicode(class))) => class.clone(),
                    // Not expected: any character (refuses more, never less).
                    _ => everything(),
                }
            }
        }
    }
}

/// Whether the pattern turns `flag` on anywhere.
fn turns_on(ast: &Ast, flag: Flag) -> bool {
    let sets = |flags: &Flags| {
        let mut on = true;
        flags.items.iter().any(|item| match item.kind {
            FlagsItemKind::Negation => {
                on = false;
                false
            }
            FlagsItemKind::Flag(set) => on && set == flag,
        })
    };
    match ast {
        Ast::Flags(set) => sets(&set.flags),
        Ast::Group(group) => {
            matches!(&group.kind, GroupKind::NonCapturing(group_flags) if sets(group_flags))
                || turns_on(&group.ast, flag)
        }
        Ast::Repetition(repetition) => turns_on(&repetition.ast, flag),
        Ast::Alternation(alternation) => alternation.asts.iter().any(|ast| turns_on(ast, flag)),
        Ast::Concat(concat) => concat.asts.iter().any(|ast| turns_on(ast, flag)),
        _ => false,
    }
}

fn min_rounds(kind: &RepetitionKind) -> u32 {
    match kind {
        RepetitionKind::ZeroOrOne | RepetitionKind::ZeroOrMore => 0,
        RepetitionKind::OneOrMore => 1,
        RepetitionKind::Range(
            RepetitionRange::Exactly(n)
            | RepetitionRange::AtLeast(n)
            | RepetitionRange::Bounded(n, _),
        ) => *n,
    }
}

fn more_than_once(kind: &RepetitionKind) -> bool {
    match kind {
        RepetitionKind::ZeroOrOne => false,
        RepetitionKind::ZeroOrMore
        | RepetitionKind::OneOrMore
        | RepetitionKind::Range(RepetitionRange::AtLeast(_)) => true,
        RepetitionKind::Range(RepetitionRange::Exactly(n) | RepetitionRange::Bounded(_, n)) => {
            *n > 1
        }
    }
}

fn overlaps(a: &hir::ClassUnicode, b: &hir::ClassUnicode) -> bool {
    let mut both = a.clone();
    both.intersect(b);
    !both.ranges().is_empty()
}

fn everything() -> hir::ClassUnicode {
    hir::ClassUnicode::new([ClassUnicodeRange::new('\0', char::MAX)])
}

/// `i`, `m` and `s` mean the same in both engines; `x` would also change how `#` and spaces read.
fn flags(flags: &Flags) -> Check {
    flags.items.iter().try_for_each(|item| match item.kind {
        FlagsItemKind::Negation
        | FlagsItemKind::Flag(Flag::CaseInsensitive | Flag::MultiLine | Flag::DotMatchesNewLine) => {
            Ok(())
        }
        FlagsItemKind::Flag(_) => INVALID,
    })
}

fn literal(literal: &Literal) -> Check {
    match &literal.kind {
        LiteralKind::Verbatim | LiteralKind::Meta | LiteralKind::Special(_) => Ok(()),
        // `\_` is an unknown escape to .NET (`_` is a word character there).
        LiteralKind::Superfluous if literal.c != '_' => Ok(()),
        // .NET reads `\xNN` and `\uNNNN`, not `\U…` or `\x{…}`.
        LiteralKind::HexFixed(HexLiteralKind::X | HexLiteralKind::UnicodeShort) => Ok(()),
        _ => INVALID,
    }
}

fn unicode_class(class: &ClassUnicode) -> Check {
    match &class.kind {
        ClassUnicodeKind::Named(name) if GENERAL_CATEGORIES.contains(&name.as_str()) => Ok(()),
        // `\pL` (no braces), scripts and `name=value` forms are unknown to .NET.
        _ => INVALID,
    }
}

fn class_set(set: &ClassSet) -> Check {
    match set {
        ClassSet::Item(item) => class_item(item),
        // `&&`, `--` and `~~` are plain characters to .NET.
        ClassSet::BinaryOp(_) => INVALID,
    }
}

fn class_item(item: &ClassSetItem) -> Check {
    match item {
        ClassSetItem::Empty(_) | ClassSetItem::Perl(_) => Ok(()),
        ClassSetItem::Literal(literal) => self::literal(literal),
        ClassSetItem::Range(range) => {
            self::literal(&range.start)?;
            self::literal(&range.end)
        }
        ClassSetItem::Unicode(class) => unicode_class(class),
        ClassSetItem::Union(union) => union.items.iter().try_for_each(class_item),
        // `[[:alpha:]]` and nested classes read differently in .NET.
        ClassSetItem::Ascii(_) | ClassSetItem::Bracketed(_) => INVALID,
    }
}

/// Writes every `#` (bare or escaped) as `\x23`, which both engines read as `#`.
fn escape_hash(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('#') => out.push_str(r"\x23"),
                Some(next) => {
                    out.push('\\');
                    out.push(next);
                }
                None => out.push('\\'),
            },
            '#' => out.push_str(r"\x23"),
            c => out.push(c),
        }
    }
    out
}

/// A regex matching `text` literally (.NET metacharacters escaped, `#` as `\x23`).
fn escape_literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    for c in text.chars() {
        match c {
            '#' => out.push_str(r"\x23"),
            '\\' | '*' | '+' | '?' | '|' | '{' | '}' | '[' | ']' | '(' | ')' | '^' | '$' | '.' => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn problem(pattern: &str) -> Option<PatternProblem> {
        check_pattern(pattern, true)
    }

    #[test]
    fn everyday_patterns_pass() {
        for ok in [
            r"^\[Özel\] (\w+) -> sen: (.*)$",
            r"(?i)satıyorum\s+\d+",
            r"selam|merhaba",
            r"^(?<oyuncu>[A-Za-z0-9_]{3,16}) katıldı$",
            r"\bAFK\b",
            r"\p{Lu}{2,}",
            r"[\p{L}\d]+",
            r"\x23ç",
            r"(?im:^hey)",
            r"fiyat: \$\d+(\.\d{1,2})?",
            r"a\%b\/c\-d",
            r"[#!]komut",
            r"(?s).*",
        ] {
            assert_eq!(problem(ok), None, "{ok}");
        }
    }

    #[test]
    fn syntax_dotnet_rejects_or_reads_differently_is_invalid() {
        for bad in [
            "[abc",
            "a)",
            "(?P<ad>x)",
            "(?<a.b>x)",
            r"\pL",
            r"\p{Greek}",
            r"\p{l}",
            r"\x{41}",
            r"\U0001F600",
            r"\<kelime\>",
            r"\b{start}x",
            "[[:alpha:]]",
            "[a[bc]]",
            "[a-z&&[^aeiou]]",
            "(?x)a b",
            "(?U)a+",
            "(?u)a",
            r"\_",
            r"(\w)\1",
            "(?=a)b",
            "(?<=a)b",
            "a{2,1}",
        ] {
            assert_eq!(problem(bad), Some(PatternProblem::Invalid), "{bad}");
        }
    }

    #[test]
    fn slow_shapes_are_too_complex() {
        for slow in [
            "(a+)+$",
            r"(\w+\s?)*$",
            "(a*)*b",
            "(.*a){10}",
            "(a{1,2})+",
            "a{1001}",
            "a{2,5000}",
            "(?:x+y)+",
        ] {
            assert_eq!(problem(slow), Some(PatternProblem::TooComplex), "{slow}");
        }
        // One level of repetition, optional parts and fixed counts are fine.
        for ok in ["(ab)+", "(a?)+", "a{1000}", "(?:x?y)*"] {
            assert_eq!(problem(ok), None, "{ok}");
        }
    }

    #[test]
    fn ambiguous_repetitions_are_too_complex() {
        // Measured with .NET 10's `new Regex(pattern)`, as MCC builds them: on a chat line of
        // 24-40 characters (`aaaa…!`, `a a a …`, `abab…!`) each takes over two seconds, and every
        // extra character doubles that.
        for slow in [
            "(a|aa)+$",
            "(a|a?)+$",
            r"(.|\s)*x",
            "(a?a)+$",
            "(aa?)+$",
            "(a|b|ab)+$",
            "(ab?|b)+$",
        ] {
            assert_eq!(problem(slow), Some(PatternProblem::TooComplex), "{slow}");
        }
        // Branches that never start alike, optional parts that can not be mistaken for what
        // follows them: linear in .NET too.
        for ok in [
            "(?:a|b)+$",
            r"(\s|\S)*x",
            r"(.|\n)*x",
            r"(\.|\w)+$",
            r"(\d|,)+$",
            "(ab|b)+$",
            "(a?)+$",
            "(?:x?y)*$",
            r"(\d,?)+$",
            r"(-?\d)+$",
            "(?:merhaba|selam)+$",
            r"^(\w+\s?)$",
            "(?:a|b)+(?:a|b)+$",
        ] {
            assert_eq!(problem(ok), None, "{ok}");
        }
        // The check is cheap rather than exact: these are refused although .NET copes with them,
        // and `\w+`, `h[ae]` or `x(aa?)` say the same.
        for refused in [r"(\w|\d)+", "(ha|he)+", "(x(a|aa))+"] {
            assert_eq!(
                problem(refused),
                Some(PatternProblem::TooComplex),
                "{refused}"
            );
        }
    }

    #[test]
    fn many_loops_that_can_give_back_are_too_complex() {
        // Measured with .NET 10's `new Regex(pattern)` (CR2): on a line of 'a's that never ends in
        // `!`, every loop that can hand characters to the next part multiplies the work by the
        // line length, and so does a start .NET can try anywhere on the line. Four such factors
        // take over two seconds on a 256-character chat line; three take ~0.1 s on 320 characters.
        for slow in [
            ".*a.*a.*a.*a!",
            "a.*a.*a.*a!",
            r"\w*a\w*a\w*a\w*a!",
            "(?m)^.*a.*a.*a.*a!",
            ".{0,100}a.{0,100}a.{0,100}a!",
            "(?:.*a)?(?:.*a)?(?:.*a)?!",
            ".*a.*b.*c.*d",
            // A capturing group around the leading loop turns .NET's start skip off.
            "(.*)a.*a.*a!",
        ] {
            assert_eq!(problem(slow), Some(PatternProblem::TooComplex), "{slow}");
        }
        for ok in [
            ".*a.*a.*a!",
            "(?:.*)a.*a.*a!",
            "(?i).*a.*a.*a!",
            "a.*a.*a!",
            "x.*a.*a!",
            r"^\[.*\] .* -> .*: .*$",
            "^.{0,100}a.{0,100}a.{0,100}a!",
            r"\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}",
            r"(?i)^\[.*\] (\w+): .*(satıyorum|alıyorum).*$",
            ".*selam.*",
            "[^:]*: [^>]*> .*!",
            r"\w+\s\w+\s\w+\s\w+\s\w+!",
            r"^(.*) -> sen: (.*)$",
        ] {
            assert_eq!(problem(ok), None, "{ok}");
        }
    }

    #[test]
    fn rounds_that_can_match_nothing_split_lines_too() {
        // Counted rounds that may be empty, and rounds whose optional ending starts like the next
        // round, try every split as well (measured: over two seconds on 25-64 characters).
        for slow in [
            "(a?){25}a{25}",
            "(?:a|b?){25}c",
            "(?:a?b?)+c",
            "(?:a?b?c?)+d",
        ] {
            assert_eq!(problem(slow), Some(PatternProblem::TooComplex), "{slow}");
        }
        // `*`, `+` and `{0,n}` stop at an empty round in .NET: these stay linear.
        for ok in [
            "(a?){0,20}a{20}",
            "(?:a|b?)+c",
            "(?:ab?)+c",
            "(a?)+c",
            r"(\d,?)+x",
        ] {
            assert_eq!(problem(ok), None, "{ok}");
        }
    }

    #[test]
    fn length_control_characters_and_plain_text() {
        let long = "a".repeat(MAX_PATTERN_CHARS + 1);
        assert_eq!(check_pattern(&long, false), Some(PatternProblem::TooLong));
        assert_eq!(problem(&long), Some(PatternProblem::TooLong));
        let ok = format!("  {}  ", "ğ".repeat(MAX_PATTERN_CHARS));
        assert_eq!(
            check_pattern(&ok, false),
            None,
            "counted in characters, trimmed"
        );
        assert_eq!(
            check_pattern("satır\nkesme", false),
            Some(PatternProblem::Invalid)
        );
        assert_eq!(
            check_pattern("[abc (yazı)", false),
            None,
            "plain text is literal"
        );
        assert_eq!(check_pattern("", true), None);
        assert_eq!(check_pattern("   ", true), None);
    }

    #[test]
    fn deep_nesting_is_refused_without_blowing_the_stack() {
        let deep = format!("{}a{}", "(".repeat(200), ")".repeat(200));
        assert_eq!(problem(&deep), Some(PatternProblem::TooLong));
        let nested = format!("{}a{}", "(".repeat(100), ")".repeat(100));
        assert_eq!(problem(&nested), Some(PatternProblem::Invalid));
    }

    #[test]
    fn hash_survives_mccs_comment_cut() {
        assert_eq!(matches_line("  #genel ", true), r"regex=\x23genel");
        assert_eq!(matches_line(r"[\#!]a\\#b", true), r"regex=[\x23!]a\\\x23b");
        assert_eq!(matches_line(" selam ", false), "match=selam");
        assert_eq!(
            matches_line("#1 (en iyi) oyuncu?", false),
            r"regex=(?i)\x231 \(en iyi\) oyuncu\?"
        );
        // What the writer produces is itself a valid pattern for both engines.
        for line in [
            matches_line(r"[\#!]a\\#b", true),
            matches_line("#1 (en iyi) oyuncu?", false),
        ] {
            let pattern = line.trim_start_matches("regex=");
            assert_eq!(problem(pattern), None, "{pattern}");
            assert!(!pattern.contains('#'));
        }
    }
}
