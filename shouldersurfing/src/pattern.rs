//! A small regular-expression matcher for the id lists (`java.util.regex` subset): literals,
//! `.`, classes `[a-z_]` / `[^...]`, escapes, groups, alternation and `* + ?`. A pattern must
//! match the whole text, as `Pattern.matches` does. A bad pattern compiles to `None`.

#[derive(Clone, Debug, PartialEq)]
enum Node {
    Char(char),
    Any,
    Class {
        negated: bool,
        ranges: Vec<(char, char)>,
    },
    Group(Vec<Vec<Node>>),
    Repeat {
        node: Box<Node>,
        min: usize,
        max: usize,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pattern {
    alternatives: Vec<Vec<Node>>,
}

const MAX_DEPTH: usize = 32;
const STEP_BUDGET: usize = 200_000;

struct Parser<'a> {
    chars: &'a [char],
    pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn alternatives(&mut self, depth: usize) -> Option<Vec<Vec<Node>>> {
        if depth > MAX_DEPTH {
            return None;
        }
        let mut out = vec![Vec::new()];
        while let Some(c) = self.peek() {
            match c {
                ')' => break,
                '|' => {
                    self.pos += 1;
                    out.push(Vec::new());
                }
                _ => {
                    let atom = self.atom(depth)?;
                    let atom = self.quantified(atom)?;
                    out.last_mut()?.push(atom);
                }
            }
        }
        Some(out)
    }

    fn atom(&mut self, depth: usize) -> Option<Node> {
        let c = self.peek()?;
        self.pos += 1;
        match c {
            '.' => Some(Node::Any),
            '(' => {
                if self.peek() == Some('?') {
                    // `(?:` is the only group flavour supported.
                    if self.chars.get(self.pos + 1) == Some(&':') {
                        self.pos += 2;
                    } else {
                        return None;
                    }
                }
                let inner = self.alternatives(depth + 1)?;
                if self.peek() != Some(')') {
                    return None;
                }
                self.pos += 1;
                Some(Node::Group(inner))
            }
            '[' => self.class(),
            '\\' => {
                let e = self.peek()?;
                self.pos += 1;
                Some(match e {
                    'd' => Node::Class {
                        negated: false,
                        ranges: vec![('0', '9')],
                    },
                    'w' => Node::Class {
                        negated: false,
                        ranges: vec![('a', 'z'), ('A', 'Z'), ('0', '9'), ('_', '_')],
                    },
                    other => Node::Char(other),
                })
            }
            '*' | '+' | '?' | ')' | '{' => None,
            other => Some(Node::Char(other)),
        }
    }

    fn class(&mut self) -> Option<Node> {
        let negated = if self.peek() == Some('^') {
            self.pos += 1;
            true
        } else {
            false
        };
        let mut ranges = Vec::new();
        let mut first = true;
        loop {
            let c = self.peek()?;
            self.pos += 1;
            if c == ']' && !first {
                break;
            }
            first = false;
            let lo = if c == '\\' {
                let e = self.peek()?;
                self.pos += 1;
                e
            } else {
                c
            };
            if self.peek() == Some('-') && self.chars.get(self.pos + 1).is_some_and(|n| *n != ']') {
                self.pos += 1;
                let mut hi = self.peek()?;
                self.pos += 1;
                if hi == '\\' {
                    hi = self.peek()?;
                    self.pos += 1;
                }
                if hi < lo {
                    return None;
                }
                ranges.push((lo, hi));
            } else {
                ranges.push((lo, lo));
            }
        }
        Some(Node::Class { negated, ranges })
    }

    fn quantified(&mut self, atom: Node) -> Option<Node> {
        let (min, max) = match self.peek() {
            Some('*') => (0, usize::MAX),
            Some('+') => (1, usize::MAX),
            Some('?') => (0, 1),
            _ => return Some(atom),
        };
        self.pos += 1;
        // Lazy and possessive suffixes match the same strings for a whole-text match.
        if matches!(self.peek(), Some('?' | '+')) {
            self.pos += 1;
        }
        Some(Node::Repeat {
            node: Box::new(atom),
            min,
            max,
        })
    }
}

struct Matcher<'a> {
    text: &'a [char],
    steps: usize,
}

impl Matcher<'_> {
    fn seq(
        &mut self,
        nodes: &[Node],
        pos: usize,
        k: &mut dyn FnMut(&mut Self, usize) -> bool,
    ) -> bool {
        self.steps += 1;
        if self.steps > STEP_BUDGET {
            return false;
        }
        let Some((first, rest)) = nodes.split_first() else {
            return k(self, pos);
        };
        self.node(first, pos, &mut |m, p| m.seq(rest, p, k))
    }

    fn node(
        &mut self,
        node: &Node,
        pos: usize,
        k: &mut dyn FnMut(&mut Self, usize) -> bool,
    ) -> bool {
        match node {
            Node::Char(c) => self.text.get(pos) == Some(c) && k(self, pos + 1),
            Node::Any => pos < self.text.len() && k(self, pos + 1),
            Node::Class { negated, ranges } => match self.text.get(pos) {
                Some(c) => {
                    let inside = ranges.iter().any(|(lo, hi)| lo <= c && c <= hi);
                    inside != *negated && k(self, pos + 1)
                }
                None => false,
            },
            Node::Group(alts) => {
                for alt in alts {
                    if self.seq(alt, pos, k) {
                        return true;
                    }
                }
                false
            }
            Node::Repeat { node, min, max } => self.repeat(node, *min, *max, 0, pos, k),
        }
    }

    fn repeat(
        &mut self,
        node: &Node,
        min: usize,
        max: usize,
        count: usize,
        pos: usize,
        k: &mut dyn FnMut(&mut Self, usize) -> bool,
    ) -> bool {
        self.steps += 1;
        if self.steps > STEP_BUDGET {
            return false;
        }
        if count < max {
            let more = self.node(node, pos, &mut |m, p| {
                // An empty iteration cannot make progress.
                p != pos && m.repeat(node, min, max, count + 1, p, k)
            });
            if more {
                return true;
            }
        }
        count >= min && k(self, pos)
    }
}

impl Pattern {
    pub fn new(source: &str) -> Option<Self> {
        let chars: Vec<char> = source.chars().collect();
        let mut parser = Parser {
            chars: &chars,
            pos: 0,
        };
        let alternatives = parser.alternatives(0)?;
        (parser.pos == chars.len()).then_some(Self { alternatives })
    }

    pub fn matches(&self, text: &str) -> bool {
        let chars: Vec<char> = text.chars().collect();
        let end = chars.len();
        let mut matcher = Matcher {
            text: &chars,
            steps: 0,
        };
        for alt in &self.alternatives {
            if matcher.seq(alt, 0, &mut |_, p| p == end) {
                return true;
            }
        }
        false
    }
}

/// A list of patterns; entries that do not compile are dropped.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PatternList(Vec<Pattern>);

impl PatternList {
    pub fn new(sources: &[String]) -> Self {
        Self(sources.iter().filter_map(|s| Pattern::new(s)).collect())
    }

    pub fn matches(&self, text: &str) -> bool {
        self.0.iter().any(|p| p.matches(text))
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(p: &str, t: &str) -> bool {
        Pattern::new(p).is_some_and(|p| p.matches(t))
    }

    #[test]
    fn literals_match_the_whole_text() {
        assert!(m("minecraft:stone", "minecraft:stone"));
        assert!(!m("minecraft:stone", "minecraft:stone_bricks"));
        assert!(!m("minecraft:stone", "x"));
        assert!(m("", ""));
    }

    #[test]
    fn wildcards_and_repeats() {
        assert!(m("minecraft:.*leaves", "minecraft:oak_leaves"));
        assert!(m("minecraft:.*_leaves", "minecraft:oak_leaves"));
        assert!(!m("minecraft:.*_leaves", "minecraft:oak_log"));
        assert!(m("a+b?", "aaa"));
        assert!(m("a+b?", "aab"));
        assert!(!m("a+b?", "b"));
        assert!(m("(ab)*", ""));
        assert!(m("(ab)*", "abab"));
        assert!(!m("(ab)*", "aba"));
    }

    #[test]
    fn classes_groups_and_alternation() {
        assert!(m("[a-c]+", "abcabc"));
        assert!(!m("[a-c]+", "abd"));
        assert!(m("[^a-c]+", "xyz"));
        assert!(m("minecraft:(glass|ice)", "minecraft:ice"));
        assert!(m("a|b", "b"));
        assert!(m("\\w+:\\d", "mod_1:5"));
        assert!(m("a\\.b", "a.b"));
        assert!(!m("a\\.b", "axb"));
        assert!(m("[]a]+", "]a"));
    }

    #[test]
    fn bad_patterns_do_not_compile() {
        for bad in ["(", ")", "*a", "[a", "[b-a]", "a{2}", "(?=a)", "\\"] {
            assert!(Pattern::new(bad).is_none(), "{bad}");
        }
        let deep = "(".repeat(100) + &")".repeat(100);
        assert!(Pattern::new(&deep).is_none());
    }

    #[test]
    fn pathological_patterns_stop() {
        let p = Pattern::new("(a*)*b").unwrap();
        assert!(!p.matches(&"a".repeat(64)));
        let p = Pattern::new("(a|aa)+b").unwrap();
        assert!(!p.matches(&"a".repeat(2000)));
    }

    #[test]
    fn lists_drop_bad_entries() {
        let list = PatternList::new(&["(".to_owned(), "a.c".to_owned()]);
        assert!(list.matches("abc"));
        assert!(!list.matches("abd"));
        assert!(!list.is_empty());
        assert!(PatternList::new(&[]).is_empty());
    }
}
