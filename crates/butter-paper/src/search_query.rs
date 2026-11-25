//! The query language of the search panel, and the literal query of the find
//! bar. A query is read into a tree of terms, compiled once, and then tested
//! against units: one page's text, or one markup. All terms of a query must
//! match in the same unit.
//!
//! The language is a subset of Obsidian's:
//!
//! | Syntax | Meaning |
//! | --- | --- |
//! | `fire door` | Both words, in any order |
//! | `"fire door"` | The phrase |
//! | `door OR window` | Either; `OR` binds looser than the implied AND |
//! | `-draft`, `-"for review"`, `-author:Sam` | Not |
//! | `(door OR window) fire` | Groups |
//! | `/D-\d{3}/` | A regular expression (Rust `regex` syntax) |
//! | `match-case:Fire`, `ignore-case:FIRE` | Case of one term (or group) |
//! | `text:(...)`, `content:(...)` | Page text only |
//! | `markup:(...)` | Markup text only |
//! | `author:Alex`, `subject:"Cloud+"` | A markup's author or subject |
//! | `page:3`, `page:3-10`, `page:12-` | Pages, as the page navigator numbers them |
//! | `file:level` | Documents whose name matches |
//!
//! Reading a query never fails: an unclosed quote or bracket closes at the
//! end of the query, an unknown filter is searched as literal text, and an
//! invalid regular expression matches nothing and is reported by
//! [`SearchQuery::problems`].

use std::{fmt, ops::Range};

use regex::{Regex, RegexBuilder};

use crate::search_text::expand_ligatures;

/// The size a regular expression can compile to, in bytes.
const REGEX_SIZE_LIMIT: usize = 1 << 20;

/// The toggles beside the query field.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SearchOptions {
    pub match_case: bool,
    pub whole_word: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TermKind {
    Word,
    Phrase,
    Regex,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Term {
    pub kind: TermKind,
    pub text: String,
    /// `Some` when `match-case:` or `ignore-case:` set the case of this
    /// term over the toggle.
    pub match_case: Option<bool>,
}

/// Where a filter looks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Field {
    /// A page's text.
    Text,
    /// A markup's text, label, caption and replies.
    Markup,
    Author,
    Subject,
}

/// A range of pages, counted from 1.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageRange {
    pub first: u32,
    pub last: Option<u32>,
}

impl PageRange {
    pub fn contains(&self, page_number: u32) -> bool {
        page_number >= self.first && self.last.is_none_or(|last| page_number <= last)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueryNode {
    Term(Term),
    And(Vec<QueryNode>),
    Or(Vec<QueryNode>),
    Not(Box<QueryNode>),
    Filter(Field, Box<QueryNode>),
    Page(PageRange),
    File(Term),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueryProblem {
    InvalidRegex { pattern: String, message: String },
    UnclosedQuote,
    UnclosedBracket,
}

impl fmt::Display for QueryProblem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRegex { .. } => formatter.write_str("Invalid regular expression"),
            Self::UnclosedQuote => formatter.write_str("Quote not closed"),
            Self::UnclosedBracket => formatter.write_str("Bracket not closed"),
        }
    }
}

const FILTERS: [&str; 10] = [
    "text",
    "content",
    "markup",
    "author",
    "subject",
    "page",
    "file",
    "match-case",
    "ignore-case",
    "path",
];

/// Reads `text` into a tree. An empty query has no tree.
pub fn parse_query(text: &str) -> (Option<QueryNode>, Vec<QueryProblem>) {
    let mut parser = Parser {
        chars: text.chars().collect(),
        position: 0,
        problems: Vec::new(),
    };
    let node = parser.or();
    // A stray bracket ends the group it is not in; read on.
    let mut nodes = vec![node];
    while parser.position < parser.chars.len() {
        parser.position += 1;
        nodes.push(parser.or());
    }
    let nodes = nodes.into_iter().flatten().collect::<Vec<_>>();
    let node = match nodes.len() {
        0 => None,
        1 => nodes.into_iter().next(),
        _ => Some(QueryNode::And(nodes)),
    };
    (node, parser.problems)
}

struct Parser {
    chars: Vec<char>,
    position: usize,
    problems: Vec<QueryProblem>,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.position).copied()
    }

    fn skip_space(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.position += 1;
        }
    }

    /// Whether the `OR` operator is next.
    fn at_or(&self) -> bool {
        self.chars.get(self.position) == Some(&'O')
            && self.chars.get(self.position + 1) == Some(&'R')
            && self
                .chars
                .get(self.position + 2)
                .is_none_or(|next| next.is_whitespace() || *next == '(' || *next == '"')
    }

    fn or(&mut self) -> Option<QueryNode> {
        let mut alternatives = Vec::new();
        alternatives.extend(self.and());
        while self.at_or() {
            self.position += 2;
            alternatives.extend(self.and());
        }
        match alternatives.len() {
            0 => None,
            1 => alternatives.pop(),
            _ => Some(QueryNode::Or(alternatives)),
        }
    }

    fn and(&mut self) -> Option<QueryNode> {
        let mut parts = Vec::new();
        loop {
            self.skip_space();
            if self.peek().is_none_or(|c| c == ')') || self.at_or() {
                break;
            }
            parts.extend(self.unary());
        }
        match parts.len() {
            0 => None,
            1 => parts.pop(),
            _ => Some(QueryNode::And(parts)),
        }
    }

    fn unary(&mut self) -> Option<QueryNode> {
        if self.peek() == Some('-')
            && self
                .chars
                .get(self.position + 1)
                .is_some_and(|next| !next.is_whitespace() && *next != ')')
        {
            self.position += 1;
            return self.unary().map(|node| QueryNode::Not(Box::new(node)));
        }
        self.atom()
    }

    /// The characters from here to the next space or closing bracket.
    fn bare_word(&mut self) -> String {
        let start = self.position;
        while self.peek().is_some_and(|c| !c.is_whitespace() && c != ')') {
            self.position += 1;
        }
        self.chars[start..self.position].iter().collect()
    }

    fn atom(&mut self) -> Option<QueryNode> {
        match self.peek()? {
            '(' => {
                self.position += 1;
                let node = self.or();
                if self.peek() == Some(')') {
                    self.position += 1;
                } else {
                    self.problems.push(QueryProblem::UnclosedBracket);
                }
                node
            }
            '"' => Some(QueryNode::Term(self.phrase())),
            '/' => Some(match self.regex() {
                Some(term) => QueryNode::Term(term),
                None => QueryNode::Term(word(self.bare_word())),
            }),
            _ => self.filter_or_word(),
        }
    }

    fn phrase(&mut self) -> Term {
        self.position += 1;
        let start = self.position;
        while self.peek().is_some_and(|c| c != '"') {
            self.position += 1;
        }
        let text = self.chars[start..self.position].iter().collect();
        if self.peek() == Some('"') {
            self.position += 1;
        } else {
            self.problems.push(QueryProblem::UnclosedQuote);
        }
        Term {
            kind: TermKind::Phrase,
            text,
            match_case: None,
        }
    }

    /// A `/pattern/` followed by a space, a closing bracket or the end.
    fn regex(&mut self) -> Option<Term> {
        let mut end = self.position + 1;
        while let Some(&c) = self.chars.get(end) {
            match c {
                '\\' => end += 2,
                '/' => break,
                _ => end += 1,
            }
        }
        if self.chars.get(end) != Some(&'/') || end == self.position + 1 {
            return None;
        }
        if self
            .chars
            .get(end + 1)
            .is_some_and(|next| !next.is_whitespace() && *next != ')')
        {
            return None;
        }
        let text = self.chars[self.position + 1..end].iter().collect();
        self.position = end + 1;
        Some(Term {
            kind: TermKind::Regex,
            text,
            match_case: None,
        })
    }

    fn filter_or_word(&mut self) -> Option<QueryNode> {
        let start = self.position;
        let mut end = start;
        while self
            .chars
            .get(end)
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '-')
        {
            end += 1;
        }
        let name = self.chars[start..end]
            .iter()
            .collect::<String>()
            .to_ascii_lowercase();
        let operand_follows = self.chars.get(end) == Some(&':')
            && self
                .chars
                .get(end + 1)
                .is_some_and(|next| !next.is_whitespace() && *next != ')');
        if !operand_follows || !FILTERS.contains(&name.as_str()) {
            return Some(QueryNode::Term(word(self.bare_word())));
        }
        self.position = end + 1;
        match name.as_str() {
            "page" => {
                let operand = self.bare_word();
                Some(match page_range(&operand) {
                    Some(range) => QueryNode::Page(range),
                    None => QueryNode::Term(word(format!("page:{operand}"))),
                })
            }
            "file" | "path" => {
                let operand = self.atom()?;
                Some(map_terms(operand, &|term| QueryNode::File(term)))
            }
            "match-case" | "ignore-case" => {
                let case = name == "match-case";
                let operand = self.atom()?;
                Some(map_terms(operand, &|mut term| {
                    term.match_case = Some(case);
                    QueryNode::Term(term)
                }))
            }
            _ => {
                let field = match name.as_str() {
                    "markup" => Field::Markup,
                    "author" => Field::Author,
                    "subject" => Field::Subject,
                    _ => Field::Text,
                };
                let operand = self.atom()?;
                Some(QueryNode::Filter(field, Box::new(operand)))
            }
        }
    }
}

fn word(text: String) -> Term {
    Term {
        kind: TermKind::Word,
        text,
        match_case: None,
    }
}

/// `3`, `3-10`, `12-` and `-5`, counted from 1.
fn page_range(text: &str) -> Option<PageRange> {
    let number = |part: &str| part.parse::<u32>().ok().filter(|number| *number > 0);
    match text.split_once('-') {
        None => number(text).map(|page| PageRange {
            first: page,
            last: Some(page),
        }),
        Some(("", last)) => number(last).map(|last| PageRange {
            first: 1,
            last: Some(last),
        }),
        Some((first, "")) => number(first).map(|first| PageRange { first, last: None }),
        Some((first, last)) => {
            let (first, last) = (number(first)?, number(last)?);
            (first <= last).then_some(PageRange {
                first,
                last: Some(last),
            })
        }
    }
}

/// `node` with each of its terms replaced.
fn map_terms(node: QueryNode, replace: &impl Fn(Term) -> QueryNode) -> QueryNode {
    match node {
        QueryNode::Term(term) | QueryNode::File(term) => replace(term),
        QueryNode::And(nodes) => {
            QueryNode::And(nodes.into_iter().map(|n| map_terms(n, replace)).collect())
        }
        QueryNode::Or(nodes) => {
            QueryNode::Or(nodes.into_iter().map(|n| map_terms(n, replace)).collect())
        }
        QueryNode::Not(node) => QueryNode::Not(Box::new(map_terms(*node, replace))),
        QueryNode::Filter(field, node) => {
            QueryNode::Filter(field, Box::new(map_terms(*node, replace)))
        }
        page @ QueryNode::Page(_) => page,
    }
}

// Compiling

#[derive(Clone, Debug)]
struct CompiledTerm {
    regex: Regex,
}

#[derive(Clone, Debug)]
enum Compiled {
    Term(CompiledTerm),
    /// A term that cannot match: an invalid regular expression.
    Never,
    And(Vec<Compiled>),
    Or(Vec<Compiled>),
    Not(Box<Compiled>),
    Filter(Field, Box<Compiled>),
    Page(PageRange),
    File(CompiledTerm),
}

fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// The expression for `term`, or the problem with it.
fn compile_term(
    term: &Term,
    options: SearchOptions,
    whole_word: bool,
) -> Result<CompiledTerm, QueryProblem> {
    let case_sensitive = term.match_case.unwrap_or(options.match_case);
    let pattern = match term.kind {
        TermKind::Regex => term.text.clone(),
        TermKind::Word | TermKind::Phrase => {
            let text = expand_ligatures(term.text.trim());
            // Spaces in the query match any run of spaces in the page, but
            // not the break between paragraphs.
            let mut pattern = text
                .split_whitespace()
                .map(regex::escape)
                .collect::<Vec<_>>()
                .join(r"[^\S\n]+");
            if whole_word && options.whole_word {
                // A boundary only where the term's edge is a letter or
                // digit: `C++` has none at its end.
                if text.chars().next().is_some_and(is_word_character) {
                    pattern.insert_str(0, r"\b");
                }
                if text.chars().next_back().is_some_and(is_word_character) {
                    pattern.push_str(r"\b");
                }
            }
            pattern
        }
    };
    RegexBuilder::new(&pattern)
        .case_insensitive(!case_sensitive)
        .multi_line(true)
        .size_limit(REGEX_SIZE_LIMIT)
        .dfa_size_limit(REGEX_SIZE_LIMIT)
        .build()
        .map(|regex| CompiledTerm { regex })
        .map_err(|error| QueryProblem::InvalidRegex {
            pattern: term.text.clone(),
            message: error.to_string(),
        })
}

fn compile(node: &QueryNode, options: SearchOptions, problems: &mut Vec<QueryProblem>) -> Compiled {
    match node {
        QueryNode::Term(term) => match compile_term(term, options, true) {
            // A term with no characters to find matches nothing.
            Ok(_) if term.kind != TermKind::Regex && term.text.trim().is_empty() => Compiled::Never,
            Ok(term) => Compiled::Term(term),
            Err(problem) => {
                problems.push(problem);
                Compiled::Never
            }
        },
        QueryNode::File(term) => match compile_term(term, options, false) {
            Ok(term) => Compiled::File(term),
            Err(problem) => {
                problems.push(problem);
                Compiled::Never
            }
        },
        QueryNode::And(nodes) => Compiled::And(
            nodes
                .iter()
                .map(|node| compile(node, options, problems))
                .collect(),
        ),
        QueryNode::Or(nodes) => Compiled::Or(
            nodes
                .iter()
                .map(|node| compile(node, options, problems))
                .collect(),
        ),
        QueryNode::Not(node) => Compiled::Not(Box::new(compile(node, options, problems))),
        QueryNode::Filter(field, node) => {
            Compiled::Filter(*field, Box::new(compile(node, options, problems)))
        }
        QueryNode::Page(range) => Compiled::Page(*range),
    }
}

// Matching

/// What a unit belongs to, for `page:` and `file:`.
#[derive(Clone, Copy, Debug)]
pub struct UnitContext<'a> {
    pub file_name: &'a str,
    /// The page's number as the page navigator shows it, counted from 1.
    pub page_number: u32,
}

/// The text of a unit, by field.
#[derive(Clone, Copy, Debug)]
enum Unit<'a> {
    Page {
        text: &'a str,
    },
    Markup {
        body: &'a str,
        author: &'a str,
        subject: &'a str,
    },
}

/// A compiled query, ready to test units.
#[derive(Clone, Debug)]
pub struct SearchQuery {
    tree: Option<QueryNode>,
    compiled: Option<Compiled>,
    problems: Vec<QueryProblem>,
}

impl SearchQuery {
    /// The query a panel field holds.
    pub fn parse(text: &str, options: SearchOptions) -> Self {
        let (tree, mut problems) = parse_query(text);
        let compiled = tree
            .as_ref()
            .map(|tree| compile(tree, options, &mut problems));
        Self {
            tree,
            compiled,
            problems,
        }
    }

    /// The query of the find bar: the text as it is, with no operators.
    pub fn literal(text: &str, options: SearchOptions) -> Self {
        let term = Term {
            kind: TermKind::Phrase,
            text: text.to_owned(),
            match_case: None,
        };
        let tree = (!text.trim().is_empty()).then_some(QueryNode::Term(term));
        let mut problems = Vec::new();
        let compiled = tree
            .as_ref()
            .map(|tree| compile(tree, options, &mut problems));
        Self {
            tree,
            compiled,
            problems,
        }
    }

    /// Whether the query has nothing to find: no results, and no search.
    pub fn is_empty(&self) -> bool {
        self.tree.is_none()
    }

    pub fn tree(&self) -> Option<&QueryNode> {
        self.tree.as_ref()
    }

    pub fn problems(&self) -> &[QueryProblem] {
        &self.problems
    }

    /// The query in plain words, for the line under the field.
    pub fn explain(&self) -> String {
        if let Some(problem) = self.problems.first() {
            return problem.to_string();
        }
        match &self.tree {
            None => String::new(),
            Some(tree) => format!("Finds {}", explain_node(tree, true)),
        }
    }

    /// Whether any page or markup of the document called `file_name` can
    /// match, by its `file:` terms alone: false lets a search skip the
    /// document's pages.
    pub fn file_may_match(&self, file_name: &str) -> bool {
        self.compiled
            .as_ref()
            .is_some_and(|compiled| file_may_match(compiled, file_name))
    }

    /// Whether the query has a term that gives text to highlight (not only
    /// filters and exclusions).
    pub fn has_positive_terms(&self) -> bool {
        self.compiled.as_ref().is_some_and(|compiled| {
            let mut terms = Vec::new();
            positive_terms(compiled, None, true, &mut terms);
            !terms.is_empty()
        })
    }

    /// The ranges of `text` to highlight when the page whose text it is
    /// matches the query, or `None` when it does not. The ranges are empty
    /// when the match comes from filters and exclusions alone.
    pub fn match_page(&self, context: UnitContext<'_>, text: &str) -> Option<Vec<Range<usize>>> {
        let compiled = self.compiled.as_ref()?;
        if !evaluate(compiled, context, Unit::Page { text }, None) {
            return None;
        }
        let mut terms = Vec::new();
        positive_terms(compiled, None, true, &mut terms);
        let mut ranges = Vec::new();
        for (term, field) in terms {
            if matches!(field, None | Some(Field::Text)) {
                ranges.extend(
                    term.regex
                        .find_iter(text)
                        .filter(|found| !found.is_empty())
                        .map(|found| found.range()),
                );
            }
        }
        ranges.sort_by_key(|range| (range.start, range.end));
        Some(ranges)
    }

    /// For a markup: `None` when it does not match, else the range of its
    /// `body` to show in a snippet (the first match), if a term matched
    /// there.
    pub fn match_markup(
        &self,
        context: UnitContext<'_>,
        body: &str,
        author: &str,
        subject: &str,
    ) -> Option<Option<Range<usize>>> {
        let compiled = self.compiled.as_ref()?;
        let unit = Unit::Markup {
            body,
            author,
            subject,
        };
        if !evaluate(compiled, context, unit, None) {
            return None;
        }
        let mut terms = Vec::new();
        positive_terms(compiled, None, true, &mut terms);
        Some(
            terms
                .into_iter()
                .filter(|(_, field)| matches!(field, None | Some(Field::Markup)))
                .filter_map(|(term, _)| term.regex.find(body))
                .filter(|found| !found.is_empty())
                .map(|found| found.range())
                .min_by_key(|range| (range.start, range.end)),
        )
    }
}

fn evaluate(
    node: &Compiled,
    context: UnitContext<'_>,
    unit: Unit<'_>,
    field: Option<Field>,
) -> bool {
    match node {
        Compiled::Never => false,
        Compiled::Term(term) => term_matches(term, unit, field),
        Compiled::And(nodes) => nodes
            .iter()
            .all(|node| evaluate(node, context, unit, field)),
        Compiled::Or(nodes) => nodes
            .iter()
            .any(|node| evaluate(node, context, unit, field)),
        Compiled::Not(node) => !evaluate(node, context, unit, field),
        Compiled::Filter(inner, node) => evaluate(node, context, unit, Some(*inner)),
        Compiled::Page(range) => range.contains(context.page_number),
        Compiled::File(term) => term.regex.is_match(context.file_name),
    }
}

fn term_matches(term: &CompiledTerm, unit: Unit<'_>, field: Option<Field>) -> bool {
    let is_match = |text: &str| term.regex.is_match(text);
    match (unit, field) {
        (Unit::Page { text }, None | Some(Field::Text)) => is_match(text),
        (Unit::Page { .. }, Some(_)) => false,
        (
            Unit::Markup {
                body,
                author,
                subject,
            },
            None,
        ) => is_match(body) || is_match(author) || is_match(subject),
        (Unit::Markup { body, .. }, Some(Field::Markup)) => is_match(body),
        (Unit::Markup { author, .. }, Some(Field::Author)) => is_match(author),
        (Unit::Markup { subject, .. }, Some(Field::Subject)) => is_match(subject),
        (Unit::Markup { .. }, Some(Field::Text)) => false,
    }
}

/// The terms whose matches count for a highlight: those not under a `-`,
/// with the field their filter names.
fn positive_terms<'a>(
    node: &'a Compiled,
    field: Option<Field>,
    positive: bool,
    terms: &mut Vec<(&'a CompiledTerm, Option<Field>)>,
) {
    match node {
        Compiled::Term(term) if positive => terms.push((term, field)),
        Compiled::And(nodes) | Compiled::Or(nodes) => {
            for node in nodes {
                positive_terms(node, field, positive, terms);
            }
        }
        Compiled::Not(node) => positive_terms(node, field, false, terms),
        Compiled::Filter(inner, node) => positive_terms(node, Some(*inner), positive, terms),
        _ => {}
    }
}

fn file_may_match(node: &Compiled, file_name: &str) -> bool {
    match node {
        Compiled::File(term) => term.regex.is_match(file_name),
        Compiled::And(nodes) => nodes.iter().all(|node| file_may_match(node, file_name)),
        Compiled::Or(nodes) => nodes.iter().any(|node| file_may_match(node, file_name)),
        Compiled::Filter(_, node) => file_may_match(node, file_name),
        // A document is out only when the exclusion is of files alone.
        Compiled::Not(node) if only_files(node) => !file_may_match(node, file_name),
        Compiled::Never => false,
        _ => true,
    }
}

fn only_files(node: &Compiled) -> bool {
    match node {
        Compiled::File(_) => true,
        Compiled::And(nodes) | Compiled::Or(nodes) => nodes.iter().all(only_files),
        Compiled::Not(node) => only_files(node),
        _ => false,
    }
}

// Explaining

fn case_note(term: &Term) -> &'static str {
    match term.match_case {
        Some(true) => " (match case)",
        Some(false) => " (ignore case)",
        None => "",
    }
}

fn explain_term(term: &Term) -> String {
    match term.kind {
        TermKind::Word => format!("\"{}\"{}", term.text, case_note(term)),
        TermKind::Phrase => format!("the phrase \"{}\"{}", term.text, case_note(term)),
        TermKind::Regex => format!("the pattern /{}/{}", term.text, case_note(term)),
    }
}

fn explain_node(node: &QueryNode, top: bool) -> String {
    match node {
        QueryNode::Term(term) => explain_term(term),
        QueryNode::And(nodes) => {
            let parts = nodes
                .iter()
                .map(|node| explain_node(node, false))
                .collect::<Vec<_>>();
            if top {
                parts.join(" and ")
            } else {
                format!("({})", parts.join(" and "))
            }
        }
        QueryNode::Or(nodes) => {
            let parts = nodes
                .iter()
                .map(|node| explain_node(node, false))
                .collect::<Vec<_>>();
            let text = format!("either {}", parts.join(" or "));
            if top { text } else { format!("({text})") }
        }
        QueryNode::Not(node) => format!("not {}", explain_node(node, false)),
        QueryNode::Filter(field, node) => {
            let place = match field {
                Field::Text => "in page text",
                Field::Markup => "in markups",
                Field::Author => "with author",
                Field::Subject => "with subject",
            };
            format!("{place} {}", explain_node(node, false))
        }
        QueryNode::Page(range) => match (range.first, range.last) {
            (first, Some(last)) if first == last => format!("on page {first}"),
            (first, Some(last)) => format!("on pages {first} to {last}"),
            (first, None) => format!("on page {first} or later"),
        },
        QueryNode::File(term) => format!("in a file named {}", explain_term(term)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn term(text: &str) -> QueryNode {
        QueryNode::Term(word(text.to_owned()))
    }

    fn phrase(text: &str) -> QueryNode {
        QueryNode::Term(Term {
            kind: TermKind::Phrase,
            text: text.to_owned(),
            match_case: None,
        })
    }

    fn tree(text: &str) -> QueryNode {
        parse_query(text).0.expect("a tree")
    }

    fn not(node: QueryNode) -> QueryNode {
        QueryNode::Not(Box::new(node))
    }

    #[test]
    fn words_are_joined_by_and() {
        assert_eq!(
            tree("fire door"),
            QueryNode::And(vec![term("fire"), term("door")])
        );
        assert_eq!(tree("fire"), term("fire"));
    }

    #[test]
    fn a_phrase_in_quotes_is_one_term() {
        assert_eq!(tree("\"fire door\""), phrase("fire door"));
        assert_eq!(
            tree("a \"b  c\" d"),
            QueryNode::And(vec![term("a"), phrase("b  c"), term("d")])
        );
    }

    #[test]
    fn or_binds_looser_than_the_implied_and() {
        assert_eq!(
            tree("a b OR c d"),
            QueryNode::Or(vec![
                QueryNode::And(vec![term("a"), term("b")]),
                QueryNode::And(vec![term("c"), term("d")]),
            ])
        );
        assert_eq!(
            tree("a OR b OR c"),
            QueryNode::Or(vec![term("a"), term("b"), term("c")])
        );
    }

    #[test]
    fn only_a_capital_or_is_the_operator() {
        assert_eq!(
            tree("a or b"),
            QueryNode::And(vec![term("a"), term("or"), term("b")])
        );
        assert_eq!(
            tree("ORDER"),
            term("ORDER"),
            "a word that starts with OR is a word"
        );
    }

    #[test]
    fn brackets_group() {
        assert_eq!(
            tree("(door OR window) fire"),
            QueryNode::And(vec![
                QueryNode::Or(vec![term("door"), term("window")]),
                term("fire"),
            ])
        );
    }

    #[test]
    fn a_minus_excludes_a_term_a_phrase_a_group_and_a_filter() {
        assert_eq!(tree("-draft"), not(term("draft")));
        assert_eq!(tree("-\"for review\""), not(phrase("for review")));
        assert_eq!(
            tree("-(a b)"),
            not(QueryNode::And(vec![term("a"), term("b")]))
        );
        assert_eq!(
            tree("-author:Sam"),
            not(QueryNode::Filter(Field::Author, Box::new(term("Sam"))))
        );
    }

    #[test]
    fn a_minus_inside_a_word_or_alone_is_text() {
        assert_eq!(tree("fire-door"), term("fire-door"));
        assert_eq!(
            tree("a - b"),
            QueryNode::And(vec![term("a"), term("-"), term("b")])
        );
    }

    #[test]
    fn a_pattern_between_slashes_is_a_regular_expression() {
        assert_eq!(
            tree(r"/D-\d{3}/"),
            QueryNode::Term(Term {
                kind: TermKind::Regex,
                text: r"D-\d{3}".into(),
                match_case: None
            })
        );
        assert_eq!(
            tree(r"/a\/b/ x"),
            QueryNode::And(vec![
                QueryNode::Term(Term {
                    kind: TermKind::Regex,
                    text: r"a\/b".into(),
                    match_case: None
                }),
                term("x"),
            ])
        );
    }

    #[test]
    fn slashes_that_are_not_a_pattern_are_text() {
        assert_eq!(tree("/usr/bin"), term("/usr/bin"));
        assert_eq!(tree("//"), term("//"));
        assert_eq!(tree("/open"), term("/open"));
    }

    #[test]
    fn case_filters_set_the_case_of_a_term_or_a_group() {
        let case = |text: &str, match_case| {
            QueryNode::Term(Term {
                kind: TermKind::Word,
                text: text.into(),
                match_case: Some(match_case),
            })
        };
        assert_eq!(tree("match-case:Fire"), case("Fire", true));
        assert_eq!(tree("ignore-case:FIRE"), case("FIRE", false));
        assert_eq!(
            tree("match-case:(a OR b)"),
            QueryNode::Or(vec![case("a", true), case("b", true)])
        );
    }

    #[test]
    fn text_and_content_look_in_page_text_and_markup_in_markups() {
        let filter = |field, node| QueryNode::Filter(field, Box::new(node));
        assert_eq!(tree("text:door"), filter(Field::Text, term("door")));
        assert_eq!(tree("content:door"), filter(Field::Text, term("door")));
        assert_eq!(
            tree("markup:(a OR b)"),
            filter(Field::Markup, QueryNode::Or(vec![term("a"), term("b")]))
        );
        assert_eq!(
            tree("author:Alex door"),
            QueryNode::And(vec![filter(Field::Author, term("Alex")), term("door")])
        );
        assert_eq!(
            tree("subject:\"Cloud+\""),
            filter(Field::Subject, phrase("Cloud+"))
        );
    }

    #[test]
    fn filter_names_ignore_case() {
        assert_eq!(
            tree("Author:Alex"),
            QueryNode::Filter(Field::Author, Box::new(term("Alex")))
        );
    }

    #[test]
    fn page_range_filters() {
        let range = |first, last| QueryNode::Page(PageRange { first, last });
        assert_eq!(tree("page:3"), range(3, Some(3)));
        assert_eq!(tree("page:3-10"), range(3, Some(10)));
        assert_eq!(tree("page:12-"), range(12, None));
        assert_eq!(tree("page:-5"), range(1, Some(5)));
        assert!(
            !PageRange {
                first: 3,
                last: Some(10)
            }
            .contains(2)
        );
        assert!(
            PageRange {
                first: 3,
                last: Some(10)
            }
            .contains(10)
        );
        assert!(
            PageRange {
                first: 12,
                last: None
            }
            .contains(900)
        );
    }

    #[test]
    fn a_bad_page_filter_is_text() {
        assert_eq!(tree("page:x"), term("page:x"));
        assert_eq!(tree("page:0"), term("page:0"));
        assert_eq!(tree("page:10-3"), term("page:10-3"));
    }

    #[test]
    fn file_filter_names_a_document() {
        assert_eq!(
            tree("file:level"),
            QueryNode::File(word("level".to_owned()))
        );
        assert_eq!(
            tree("file:\"level 2\" door"),
            QueryNode::And(vec![
                QueryNode::File(Term {
                    kind: TermKind::Phrase,
                    text: "level 2".into(),
                    match_case: None
                }),
                term("door"),
            ])
        );
    }

    #[test]
    fn an_unknown_filter_is_literal_text() {
        assert_eq!(tree("foo:bar"), term("foo:bar"));
        assert_eq!(tree("tag:x"), term("tag:x"));
    }

    #[test]
    fn a_filter_with_nothing_after_it_is_text() {
        assert_eq!(tree("author:"), term("author:"));
        assert_eq!(
            tree("author: Sam"),
            QueryNode::And(vec![term("author:"), term("Sam")])
        );
    }

    #[test]
    fn an_unclosed_quote_or_bracket_closes_at_the_end() {
        let (node, problems) = parse_query("a \"b c");
        assert_eq!(node, Some(QueryNode::And(vec![term("a"), phrase("b c")])));
        assert_eq!(problems, [QueryProblem::UnclosedQuote]);
        let (node, problems) = parse_query("(a OR b");
        assert_eq!(node, Some(QueryNode::Or(vec![term("a"), term("b")])));
        assert_eq!(problems, [QueryProblem::UnclosedBracket]);
    }

    #[test]
    fn a_stray_closing_bracket_does_not_stop_the_query() {
        assert_eq!(tree("a ) b"), QueryNode::And(vec![term("a"), term("b")]));
    }

    #[test]
    fn an_empty_query_has_no_tree() {
        assert_eq!(parse_query("").0, None);
        assert_eq!(parse_query("   ").0, None);
        assert!(SearchQuery::parse("  ", SearchOptions::default()).is_empty());
        assert!(SearchQuery::literal(" ", SearchOptions::default()).is_empty());
    }

    #[test]
    fn an_operator_with_nothing_on_a_side_is_dropped() {
        assert_eq!(tree("OR a"), term("a"));
        assert_eq!(tree("a OR"), term("a"));
        assert_eq!(parse_query("OR").0, None);
    }

    // The matcher

    const CONTEXT: UnitContext<'static> = UnitContext {
        file_name: "Level 2 plan.pdf",
        page_number: 3,
    };

    fn page_ranges(query: &str, options: SearchOptions, text: &str) -> Option<Vec<String>> {
        SearchQuery::parse(query, options)
            .match_page(CONTEXT, text)
            .map(|ranges| ranges.into_iter().map(|r| text[r].to_owned()).collect())
    }

    fn found(query: &str, text: &str) -> Option<Vec<String>> {
        page_ranges(query, SearchOptions::default(), text)
    }

    const TEXT: &str = "Fire door 60 min\nsee Fire Door schedule, door D-101 and D-22";

    #[test]
    fn matching_ignores_case_by_default_and_gives_every_match() {
        // Each word is highlighted where it is, in page order.
        assert_eq!(
            found("fire door", TEXT),
            Some(
                ["Fire", "door", "Fire", "Door", "door"]
                    .map(str::to_owned)
                    .to_vec()
            )
        );
        assert_eq!(
            found("fire", TEXT),
            Some(vec!["Fire".to_owned(), "Fire".to_owned()])
        );
    }

    #[test]
    fn the_match_case_toggle_makes_matching_exact() {
        let options = SearchOptions {
            match_case: true,
            whole_word: false,
        };
        assert_eq!(
            page_ranges("Fire", options, TEXT),
            Some(vec!["Fire".to_owned(), "Fire".to_owned()])
        );
        assert_eq!(page_ranges("fire", options, TEXT), None);
    }

    #[test]
    fn case_filters_beat_the_toggle() {
        let options = SearchOptions {
            match_case: true,
            whole_word: false,
        };
        assert_eq!(
            page_ranges("ignore-case:fire", options, TEXT).map(|v| v.len()),
            Some(2)
        );
        assert_eq!(found("match-case:fire", TEXT), None);
        assert_eq!(found("match-case:Fire", TEXT).map(|v| v.len()), Some(2));
    }

    #[test]
    fn whole_word_does_not_match_inside_a_word() {
        let options = SearchOptions {
            match_case: false,
            whole_word: true,
        };
        assert_eq!(
            page_ranges("door", options, "door doors outdoor door."),
            Some(vec!["door".to_owned(), "door".to_owned()])
        );
        assert_eq!(page_ranges("oor", options, "door"), None);
        assert_eq!(found("oor", "door").map(|v| v.len()), Some(1));
    }

    #[test]
    fn whole_word_leaves_out_a_boundary_at_a_symbol() {
        let options = SearchOptions {
            match_case: false,
            whole_word: true,
        };
        // A boundary applies on the letter side only: `xC++` does not
        // start at a word, but `C+++` still has `C++` in it.
        assert_eq!(
            page_ranges("C++", options, "C++ and C+++ and xC++"),
            Some(vec!["C++".to_owned(), "C++".to_owned()])
        );
    }

    #[test]
    fn whole_word_works_on_non_ascii_letters() {
        let options = SearchOptions {
            match_case: false,
            whole_word: true,
        };
        assert_eq!(
            page_ranges("Über", options, "Über überall über"),
            Some(vec!["Über".to_owned(), "über".to_owned()])
        );
    }

    #[test]
    fn unicode_case_folding_matches_greek_and_umlauts() {
        assert_eq!(found("ΣΟΦΟΣ", "σοφος"), Some(vec!["σοφος".to_owned()]));
        assert_eq!(found("äpfel", "ÄPFEL und Äpfel").map(|v| v.len()), Some(2));
    }

    #[test]
    fn a_phrase_matches_across_any_run_of_spaces_but_not_across_paragraphs() {
        assert_eq!(
            found("\"fire door\"", "a fire   door b"),
            Some(vec!["fire   door".to_owned()])
        );
        assert_eq!(found("\"fire door\"", "fire\ndoor"), None);
        // Both words as separate terms still match across paragraphs.
        assert_eq!(found("fire door", "fire\ndoor").map(|v| v.len()), Some(2));
    }

    #[test]
    fn a_literal_query_is_one_phrase_with_no_operators() {
        let query = SearchQuery::literal("fire OR (door", SearchOptions::default());
        assert_eq!(
            query
                .match_page(CONTEXT, "x fire OR (door y")
                .map(|r| r.len()),
            Some(1)
        );
        assert!(query.match_page(CONTEXT, "fire door").is_none());
        assert!(query.problems().is_empty());
    }

    #[test]
    fn or_matches_either_and_highlights_what_matched() {
        assert_eq!(
            found("window OR schedule", TEXT),
            Some(vec!["schedule".to_owned()])
        );
        assert_eq!(found("window OR garage", TEXT), None);
    }

    #[test]
    fn all_words_must_be_in_the_unit() {
        assert_eq!(found("fire garage", TEXT), None);
        assert!(found("fire schedule", TEXT).is_some());
    }

    #[test]
    fn a_minus_rejects_units_with_the_term_and_is_not_highlighted() {
        assert_eq!(found("fire -garage", TEXT).map(|v| v.len()), Some(2));
        assert_eq!(found("fire -schedule", TEXT), None);
        assert_eq!(found("fire -\"60 min\"", TEXT), None);
        assert_eq!(found("-garage", TEXT), Some(Vec::new()));
        assert_eq!(found("-fire", TEXT), None);
    }

    #[test]
    fn groups_combine() {
        assert!(found("(garage OR schedule) fire", TEXT).is_some());
        assert!(found("(garage OR window) fire", TEXT).is_none());
        assert!(found("garage OR (fire schedule)", TEXT).is_some());
    }

    #[test]
    fn a_regular_expression_matches_its_pattern() {
        assert_eq!(found(r"/D-\d{3}/", TEXT), Some(vec!["D-101".to_owned()]));
        assert_eq!(
            found(r"/D-\d+/", TEXT),
            Some(vec!["D-101".to_owned(), "D-22".to_owned()])
        );
    }

    #[test]
    fn a_regular_expression_ignores_case_unless_asked() {
        assert_eq!(found(r"/d-\d{3}/", TEXT).map(|v| v.len()), Some(1));
        assert_eq!(found(r"match-case:/d-\d{3}/", TEXT), None);
    }

    #[test]
    fn regular_expression_anchors_work_per_line() {
        assert_eq!(found("/^see/", TEXT).map(|v| v.len()), Some(1));
        assert_eq!(found("/min$/", TEXT).map(|v| v.len()), Some(1));
    }

    #[test]
    fn an_invalid_regular_expression_matches_nothing_and_is_reported() {
        let query = SearchQuery::parse("/(unclosed/ door", SearchOptions::default());
        assert_eq!(query.problems().len(), 1);
        assert_eq!(
            query.problems()[0].to_string(),
            "Invalid regular expression"
        );
        assert!(query.match_page(CONTEXT, "door (unclosed").is_none());
        assert_eq!(query.explain(), "Invalid regular expression");
    }

    #[test]
    fn a_regular_expression_too_big_to_compile_is_a_problem_not_a_hang() {
        let query = SearchQuery::parse("/(a{1000}){1000}/", SearchOptions::default());
        assert_eq!(query.problems().len(), 1);
        assert!(query.match_page(CONTEXT, "aaaa").is_none());
    }

    #[test]
    fn page_and_file_filters_use_the_unit_context() {
        let query = |text: &str| SearchQuery::parse(text, SearchOptions::default());
        let on = |page_number| UnitContext {
            file_name: "Level 2 plan.pdf",
            page_number,
        };
        let q = query("door page:3-10");
        assert!(q.match_page(on(2), "door").is_none());
        assert!(q.match_page(on(3), "door").is_some());
        assert!(q.match_page(on(11), "door").is_none());
        let q = query("door file:level");
        assert!(q.match_page(on(1), "door").is_some());
        let q = query("door file:sections");
        assert!(q.match_page(on(1), "door").is_none());
        let q = query("door -file:level");
        assert!(q.match_page(on(1), "door").is_none());
    }

    #[test]
    fn a_query_of_filters_alone_matches_whole_pages_with_nothing_to_highlight() {
        let query = SearchQuery::parse("page:3", SearchOptions::default());
        assert_eq!(query.match_page(CONTEXT, "anything"), Some(Vec::new()));
        assert!(!query.has_positive_terms());
        assert!(SearchQuery::parse("a -b", SearchOptions::default()).has_positive_terms());
        assert!(!SearchQuery::parse("-b", SearchOptions::default()).has_positive_terms());
    }

    #[test]
    fn a_document_is_skipped_only_when_its_name_rules_it_out() {
        let query = |text: &str| SearchQuery::parse(text, SearchOptions::default());
        assert!(query("door file:level").file_may_match("Level 2 plan.pdf"));
        assert!(!query("door file:level").file_may_match("Sections.pdf"));
        assert!(query("door").file_may_match("Sections.pdf"));
        assert!(!query("door -file:level").file_may_match("Level 2 plan.pdf"));
        assert!(query("file:level OR file:sections").file_may_match("Sections.pdf"));
        assert!(query("-door").file_may_match("Sections.pdf"));
    }

    #[test]
    fn page_text_ignores_markup_and_author_filters() {
        let query = |text: &str| SearchQuery::parse(text, SearchOptions::default());
        assert!(query("author:Alex").match_page(CONTEXT, "Alex").is_none());
        assert!(query("markup:door").match_page(CONTEXT, "door").is_none());
        assert!(query("text:door").match_page(CONTEXT, "door").is_some());
        assert!(
            query("-author:Alex door")
                .match_page(CONTEXT, "door")
                .is_some()
        );
    }

    fn markup(query: &str, body: &str, author: &str, subject: &str) -> Option<Option<String>> {
        SearchQuery::parse(query, SearchOptions::default())
            .match_markup(CONTEXT, body, author, subject)
            .map(|range| range.map(|range| body[range].to_owned()))
    }

    #[test]
    fn a_markup_unit_covers_its_text_author_and_subject() {
        let (body, author, subject) = ("Check fire door rating", "Alex", "Cloud+");
        assert_eq!(
            markup("fire", body, author, subject),
            Some(Some("fire".to_owned()))
        );
        assert_eq!(markup("alex", body, author, subject), Some(None));
        assert_eq!(markup("cloud+", body, author, subject), Some(None));
        assert_eq!(markup("garage", body, author, subject), None);
    }

    #[test]
    fn author_and_subject_filters_look_only_there() {
        let (body, author, subject) = ("Sam wrote this", "Alex", "Text Box");
        assert!(markup("author:Alex", body, author, subject).is_some());
        assert!(markup("author:Sam", body, author, subject).is_none());
        assert!(markup("subject:\"text box\"", body, author, subject).is_some());
        assert!(markup("author:Alex wrote", body, author, subject).is_some());
        assert!(markup("-author:Sam wrote", body, author, subject).is_some());
        assert!(markup("markup:Sam", body, author, subject).is_some());
        assert!(markup("markup:Alex", body, author, subject).is_none());
        assert!(markup("text:wrote", body, author, subject).is_none());
    }

    #[test]
    fn the_snippet_range_of_a_markup_is_its_first_match() {
        assert_eq!(
            markup("door OR rating", "door and rating", "", ""),
            Some(Some("door".to_owned()))
        );
        assert_eq!(
            markup("rating OR door", "door and rating", "", ""),
            Some(Some("door".to_owned()))
        );
    }

    #[test]
    fn ligatures_in_the_query_read_as_letters() {
        assert_eq!(found("\u{FB01}re", "a fire"), Some(vec!["fire".to_owned()]));
        assert_eq!(found("fire", "a fire"), Some(vec!["fire".to_owned()]));
    }

    #[test]
    fn a_phrase_of_only_spaces_matches_nothing() {
        assert_eq!(found("\"   \"", "a b"), None);
    }

    #[test]
    fn explain_reads_the_query_in_plain_words() {
        let explain = |text: &str| SearchQuery::parse(text, SearchOptions::default()).explain();
        assert_eq!(explain(""), "");
        assert_eq!(explain("fire door"), "Finds \"fire\" and \"door\"");
        assert_eq!(
            explain("\"fire door\" author:Alex"),
            "Finds the phrase \"fire door\" and with author \"Alex\""
        );
        assert_eq!(
            explain("a OR b -c"),
            "Finds either \"a\" or (\"b\" and not \"c\")"
        );
        assert_eq!(explain("page:3-10"), "Finds on pages 3 to 10");
        assert_eq!(explain("page:12-"), "Finds on page 12 or later");
        assert_eq!(explain("page:4"), "Finds on page 4");
        assert_eq!(explain("file:level"), "Finds in a file named \"level\"");
        assert_eq!(explain("match-case:Fire"), "Finds \"Fire\" (match case)");
        assert_eq!(explain("/D-\\d+/"), "Finds the pattern /D-\\d+/");
        assert_eq!(explain("\"open"), "Quote not closed");
    }
}
