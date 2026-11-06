//! Prompt autocompletion from the Danbooru and e621 tag lists.
//!
//! The four CSVs in `tags/` (`tag,type,count,aliases`) are deflated at build time and embedded
//! compressed; the first search inflates and parses them into one index. The index holds every
//! tag and every alias, sorted by its normalised form (lower case, `_` turned into a space), so an
//! alias's prefix is one binary search; tags themselves are matched anywhere in them, as
//! krita-ai-diffusion does ("cat" finds "black cat" and "cat ears"). The tag typed exactly comes
//! first, then every tag holding the word, then those found through an alias, each by how much of
//! its site's most-used tag an entry has, which mixes Danbooru's and e621's very different counts
//! fairly; the same tag in both lists appears once, by its higher score. NSFW lists are searched
//! only when asked for.
//!
//! What comes back is the tag as it should be typed (spaces for underscores, except for emoticon
//! tags, which have no letters and are kept as they are), its kind, a short count for the chip and
//! the byte range in the prompt it replaces. [`complete`] inserts it with the separator the prompt
//! expects.

use std::collections::HashMap;
use std::collections::hash_map::Entry as MapEntry;
use std::sync::OnceLock;

use miniz_oxide::inflate::decompress_to_vec;

/// The site a tag's count comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    Danbooru,
    E621,
}

impl Source {
    fn slot(self) -> usize {
        match self {
            Source::Danbooru => 0,
            Source::E621 => 1,
        }
    }
}

/// Danbooru's tag categories.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TagKind {
    General,
    Artist,
    Copyright,
    Character,
    Meta,
}

/// One tag a search found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    /// The tag as it should be inserted.
    pub tag: String,
    pub kind: TagKind,
    /// A short label of how often it is used ("12k", "1.2M", "850").
    pub count: String,
    /// The byte range of the prompt this suggestion replaces.
    pub from: usize,
    pub to: usize,
    pub source: Source,
    pub nsfw: bool,
    /// The alias the typed word matched, when it matched one rather than the tag itself ("house
    /// cat" for "domestic cat"), so the UIs can show why it's there.
    pub alias: Option<String>,
}

/// What a search found: the range it covers and the suggestions, best first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Suggestions {
    pub from: usize,
    pub to: usize,
    pub items: Vec<Suggestion>,
}

/// A tag in the index.
#[derive(Clone, Debug)]
struct Tag {
    /// As inserted: spaces for underscores unless it has no letters.
    tag: String,
    /// Lower case, underscores as spaces.
    key: String,
    kind: TagKind,
    count: u64,
    source: Source,
    nsfw: bool,
}

/// A searchable key: a tag's own key or one of its aliases.
#[derive(Clone, Debug)]
struct Key {
    key: String,
    tag: u32,
    /// One of the tag's aliases rather than the tag itself.
    alias: bool,
}

/// Every tag and alias, sorted by key.
#[derive(Default)]
struct Index {
    tags: Vec<Tag>,
    keys: Vec<Key>,
    /// The largest count of each source, for ranking.
    largest: [u64; 2],
}

static INDEX: OnceLock<Index> = OnceLock::new();

const DANBOORU: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/danbooru.bin"));
const DANBOORU_NSFW: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/danbooru-nsfw.bin"));
const E621: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/e621.bin"));
const E621_NSFW: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/e621-nsfw.bin"));

/// Suggests tags for the word being typed at `caret` (a byte offset into `text`). The word runs
/// from the last comma to the caret and must be at least two characters; `most` caps the list.
/// NSFW entries are left out unless `nsfw`.
pub fn suggest(text: &str, caret: usize, nsfw: bool, most: usize) -> Suggestions {
    index().suggest(text, caret, nsfw, most)
}

/// Inserts `suggestion` over the range it replaces, followed by ", " unless a comma is already
/// there; returns the new text and where the caret goes.
pub fn complete(text: &str, _caret: usize, suggestion: &Suggestion) -> (String, usize) {
    let from = boundary_at_or_before(text, suggestion.from.min(text.len()));
    let to = boundary_at_or_before(text, suggestion.to.min(text.len()).max(from));
    let mut new = String::with_capacity(text.len().saturating_add(suggestion.tag.len()).saturating_add(2));
    if let Some(head) = text.get(..from) {
        new.push_str(head);
    }
    new.push_str(&suggestion.tag);
    let tail = text.get(to..).unwrap_or_default();
    let mut rest = 0;
    // Keep a comma already after the word, with the spaces around it, rather than adding another.
    while tail.get(rest..).is_some_and(|s| s.starts_with(' ') || s.starts_with('\t')) {
        rest += 1;
    }
    if tail.get(rest..).is_some_and(|s| s.starts_with(',')) {
        rest += 1;
        while tail.get(rest..).is_some_and(|s| s.starts_with(' ') || s.starts_with('\t')) {
            rest += 1;
        }
        if let Some(separator) = tail.get(..rest) {
            new.push_str(separator);
        }
    } else {
        new.push_str(", ");
    }
    let caret = new.len();
    if let Some(rest) = tail.get(rest..) {
        new.push_str(rest);
    }
    (new, caret)
}

/// The tag list built once, on first use, whichever thread asks first.
fn index() -> &'static Index {
    INDEX.get_or_init(|| {
        let mut index = Index::default();
        for (packed, source, nsfw) in [
            (DANBOORU, Source::Danbooru, false),
            (DANBOORU_NSFW, Source::Danbooru, true),
            (E621, Source::E621, false),
            (E621_NSFW, Source::E621, true),
        ] {
            match decompress_to_vec(packed) {
                Ok(data) => match std::str::from_utf8(&data) {
                    Ok(text) => index.read_csv(text, source, nsfw),
                    Err(e) => log::error!("tags: {source:?}: {e}"),
                },
                Err(e) => log::error!("tags: inflating {source:?}: {e:?}"),
            }
        }
        index.finish();
        index
    })
}

impl Index {
    /// Adds a tag and its aliases.
    fn add(&mut self, tag: &str, kind: TagKind, count: u64, source: Source, nsfw: bool, aliases: &[String]) {
        let tag = tag.trim();
        let key = normalise(tag);
        if key.is_empty() {
            return;
        }
        let Ok(at) = u32::try_from(self.tags.len()) else { return };
        self.tags.push(Tag { tag: inserted(tag), key: key.clone(), kind, count, source, nsfw });
        self.keys.push(Key { key, tag: at, alias: false });
        for alias in aliases {
            let key = normalise(alias);
            if !key.is_empty() {
                self.keys.push(Key { key, tag: at, alias: true });
            }
        }
    }

    /// Parses one CSV (without its header).
    fn read_csv(&mut self, text: &str, source: Source, nsfw: bool) {
        for line in text.lines() {
            let line = line.trim_end_matches('\r');
            if line.is_empty() {
                continue;
            }
            let fields = fields(line);
            let Some(tag) = fields.first().map(String::as_str).map(str::trim) else { continue };
            if tag.is_empty() || tag.eq_ignore_ascii_case("tag") {
                continue;
            }
            let kind = kind_of(fields.get(1).map(String::as_str).unwrap_or_default());
            let count = fields.get(2).and_then(|c| c.trim().parse::<u64>().ok()).unwrap_or(0);
            let aliases: Vec<String> = fields
                .get(3)
                .map(|a| a.split(',').map(str::trim).filter(|a| !a.is_empty()).map(ToOwned::to_owned).collect())
                .unwrap_or_default();
            self.add(tag, kind, count, source, nsfw, &aliases);
        }
    }

    /// Sorts the keys and notes each source's largest count.
    fn finish(&mut self) {
        for tag in &self.tags {
            let largest = self.largest.get_mut(tag.source.slot());
            if let Some(largest) = largest {
                *largest = (*largest).max(tag.count);
            }
        }
        self.keys.sort_unstable_by(|a, b| a.key.cmp(&b.key).then_with(|| a.tag.cmp(&b.tag)));
    }

    fn score(&self, tag: &Tag) -> f64 {
        let largest = self.largest.get(tag.source.slot()).copied().unwrap_or(0).max(1);
        tag.count as f64 / largest as f64
    }

    /// The search behind [`suggest`].
    fn suggest(&self, text: &str, caret: usize, nsfw: bool, most: usize) -> Suggestions {
        let Some((from, to, word)) = word(text, caret) else { return Suggestions::default() };
        let query = normalise(word);
        // a word of symbols (":", "^") is looked up from its first character, letters from two;
        // one symbol only finds tags starting with it (":" gives ":d" and ":3", not "re:zero")
        let symbols = !query.chars().any(char::is_alphanumeric);
        let length = query.chars().count();
        if length < if symbols { 1 } else { 2 } {
            return Suggestions { from, to, items: Vec::new() };
        }
        let anywhere = !(symbols && length == 1);
        let start = self.keys.partition_point(|key| key.key.as_str() < query.as_str());
        // per tag, its best match: the lowest tier (an exact match, then the tag holding the word,
        // then one of its aliases starting with it), then the higher score
        let mut best: HashMap<&str, Match> = HashMap::new();
        let tail = self.keys.get(start..).unwrap_or_default();
        for key in tail.iter().take_while(|key| key.key.starts_with(&query)) {
            let at = key.tag as usize;
            let Some(tag) = self.tags.get(at) else { continue };
            if tag.nsfw && !nsfw {
                continue;
            }
            let tier = if key.key == query {
                Tier::Exact
            } else if key.alias {
                Tier::Alias
            } else {
                Tier::Contains
            };
            let found = Match { tier, score: self.score(tag), at, alias: key.alias.then(|| key.key.clone()) };
            keep_best(&mut best, tag.key.as_str(), found);
        }
        // the tags holding the word anywhere (a pass over them all: a few milliseconds)
        for (at, tag) in self.tags.iter().enumerate().filter(|_| anywhere) {
            if (tag.nsfw && !nsfw) || !tag.key.contains(query.as_str()) {
                continue;
            }
            let tier = if tag.key == query { Tier::Exact } else { Tier::Contains };
            keep_best(&mut best, tag.key.as_str(), Match { tier, score: self.score(tag), at, alias: None });
        }
        let mut found: Vec<Match> = best.into_values().collect();
        found.sort_unstable_by(|a, b| {
            a.tier.cmp(&b.tier).then_with(|| b.score.total_cmp(&a.score)).then_with(|| {
                let a = self.tags.get(a.at).map_or("", |t| t.tag.as_str());
                let b = self.tags.get(b.at).map_or("", |t| t.tag.as_str());
                a.cmp(b)
            })
        });
        found.truncate(most);
        let items = found
            .into_iter()
            .filter_map(|m| {
                self.tags.get(m.at).map(|tag| Suggestion {
                    tag: tag.tag.clone(),
                    kind: tag.kind,
                    count: count_label(tag.count),
                    from,
                    to,
                    source: tag.source,
                    nsfw: tag.nsfw,
                    // an exact alias ("houses" for "house") needs no explaining
                    alias: m.alias.filter(|_| m.tier == Tier::Alias),
                })
            })
            .collect();
        Suggestions { from, to, items }
    }
}

/// How a typed word matched a tag; earlier tiers rank first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    /// The word is the tag, or one of its aliases, exactly.
    Exact,
    /// The tag holds the word, anywhere in it.
    Contains,
    /// One of the tag's aliases starts with the word.
    Alias,
}

/// A tag a word matched: how, its score, where it is, and the alias when through one.
struct Match {
    tier: Tier,
    score: f64,
    at: usize,
    alias: Option<String>,
}

/// Keeps `found` for the tag `key` if it is the tag's best match so far.
fn keep_best<'a>(best: &mut HashMap<&'a str, Match>, key: &'a str, found: Match) {
    match best.entry(key) {
        MapEntry::Vacant(v) => {
            v.insert(found);
        }
        MapEntry::Occupied(mut o) => {
            if found.better_than(o.get()) {
                o.insert(found);
            }
        }
    }
}

impl Match {
    fn better_than(&self, other: &Match) -> bool {
        self.tier < other.tier || (self.tier == other.tier && self.score > other.score)
    }
}

/// The word at `caret`: after the last comma before it, trimmed; none if it is empty. `from` and
/// `to` are the trimmed word's byte range.
fn word(text: &str, caret: usize) -> Option<(usize, usize, &str)> {
    let caret = boundary_at_or_before(text, caret.min(text.len()));
    let start = text.get(..caret)?.rfind(',').map_or(0, |comma| comma + 1);
    let segment = text.get(start..caret)?;
    let trimmed = segment.trim();
    if trimmed.is_empty() {
        return None;
    }
    let leading = segment.len().saturating_sub(segment.trim_start().len());
    let from = start.saturating_add(leading);
    let to = from.saturating_add(trimmed.len());
    Some((from, to, trimmed))
}

/// `at` moved back to a UTF-8 boundary (or the end).
fn boundary_at_or_before(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// Lower case, underscores as spaces; the form keys and prefixes are compared in.
fn normalise(text: &str) -> String {
    let mut key = String::with_capacity(text.len());
    for c in text.trim().chars() {
        if c == '_' {
            key.push(' ');
        } else {
            key.extend(c.to_lowercase());
        }
    }
    key
}

/// A tag as it should be typed: underscores become spaces, unless it has no letters (emoticons
/// such as `^_^` and `>_<` keep theirs).
fn inserted(tag: &str) -> String {
    if tag.chars().any(char::is_alphabetic) { tag.replace('_', " ") } else { tag.to_owned() }
}

/// Danbooru's category numbers.
fn kind_of(field: &str) -> TagKind {
    match field.trim() {
        "1" => TagKind::Artist,
        "3" => TagKind::Copyright,
        "4" => TagKind::Character,
        "5" => TagKind::Meta,
        _ => TagKind::General,
    }
}

/// A short count for a chip: "850", "12k", "1.2M".
fn count_label(count: u64) -> String {
    if count < 1_000 {
        count.to_string()
    } else if count < 1_000_000 {
        if count < 10_000 {
            one_decimal(count as f64 / 1_000.0, "k")
        } else {
            format!("{}k", count.saturating_add(500) / 1_000)
        }
    } else if count < 10_000_000 {
        one_decimal(count as f64 / 1_000_000.0, "M")
    } else {
        format!("{}M", count.saturating_add(500_000) / 1_000_000)
    }
}

fn one_decimal(value: f64, unit: &str) -> String {
    let text = format!("{value:.1}");
    let text = text.strip_suffix(".0").unwrap_or(&text);
    format!("{text}{unit}")
}

/// Splits one CSV line into its fields, honouring quotes and doubled quotes inside them.
fn fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
        } else if c == '"' && field.is_empty() {
            quoted = true;
        } else if c == ',' {
            fields.push(std::mem::take(&mut field));
        } else {
            field.push(c);
        }
    }
    fields.push(field);
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Index {
        let mut index = Index::default();
        index.add("cat", TagKind::General, 100, Source::Danbooru, false, &[]);
        index.add("cat", TagKind::General, 50, Source::E621, false, &[]);
        index.add("caterpillar", TagKind::General, 900, Source::E621, false, &[]);
        index.add("cathode", TagKind::Meta, 10, Source::Danbooru, false, &[]);
        index.add("breasts", TagKind::General, 800, Source::Danbooru, false, &["boobs".to_owned()]);
        index.add("long_hair", TagKind::General, 700, Source::Danbooru, false, &["long hair".to_owned()]);
        index.add("^_^", TagKind::General, 120, Source::Danbooru, false, &[]);
        index.add("dog", TagKind::General, 80, Source::Danbooru, false, &[]);
        index.add("black_cat", TagKind::General, 400, Source::Danbooru, false, &[]);
        index.add(":d", TagKind::General, 600, Source::Danbooru, false, &[]);
        index.add(":3", TagKind::General, 200, Source::Danbooru, false, &[]);
        index.add("re:zero", TagKind::Copyright, 700, Source::Danbooru, false, &[]);
        index.add("penis", TagKind::General, 300, Source::Danbooru, true, &[]);
        index.finish();
        index
    }

    #[test]
    fn the_word_comes_first_then_every_tag_holding_it_by_use() {
        let found = sample().suggest("cat", 3, false, 12);
        let tags: Vec<&str> = found.items.iter().map(|s| s.tag.as_str()).collect();
        // "black cat" holds the word inside it, and ranks by how much it is used
        assert_eq!(tags, ["cat", "caterpillar", "black cat", "cathode"], "the word itself first");
        let cat = found.items.iter().find(|s| s.tag == "cat").unwrap();
        assert_eq!(cat.count, "100");
        assert_eq!(cat.source, Source::Danbooru);
    }

    #[test]
    fn one_symbol_finds_the_tags_starting_with_it_and_one_letter_nothing() {
        let tags = |word: &str| -> Vec<String> {
            sample().suggest(word, word.len(), false, 12).items.into_iter().map(|s| s.tag).collect()
        };
        assert_eq!(tags(":"), [":d", ":3"], "the emoticons, not the title with a colon in it");
        assert_eq!(tags("^"), ["^_^"]);
        assert!(tags("c").is_empty(), "a letter waits for a second");
        // two symbols or more search anywhere again
        assert_eq!(tags(":3"), [":3"]);
    }

    #[test]
    fn aliases_resolve_to_their_tag() {
        let found = sample().suggest("boo", 3, false, 12);
        assert_eq!(found.items.first().map(|s| s.tag.as_str()), Some("breasts"));
    }

    #[test]
    fn the_word_itself_comes_first_then_tags_starting_with_it_then_aliases() {
        let found = suggest("house", 5, false, 12);
        let tags: Vec<&str> = found.items.iter().map(|s| s.tag.as_str()).collect();
        assert_eq!(tags.first(), Some(&"house"), "{tags:?}");
        assert!(found.items.iter().all(|s| s.alias.is_none()), "enough tags start with it: {tags:?}");
        // fewer direct matches: an alias's tag comes after them, saying which alias it was
        let found = suggest("housec", 6, false, 12);
        let cat = found.items.iter().find(|s| s.tag == "domestic cat").unwrap();
        assert_eq!(cat.alias.as_deref(), Some("housecat"));
        let mount = suggest("mounta", 6, false, 12);
        assert_eq!(mount.items.first().map(|s| s.tag.as_str()), Some("mountain"));
    }

    #[test]
    fn nsfw_tags_are_only_offered_when_asked_for() {
        let off = sample().suggest("pen", 3, false, 12);
        assert!(off.items.is_empty());
        let on = sample().suggest("pen", 3, true, 12);
        assert_eq!(on.items.first().map(|s| s.tag.as_str()), Some("penis"));
    }

    #[test]
    fn underscores_become_spaces_but_not_for_emoticons() {
        let found = sample().suggest("long", 4, false, 12);
        assert_eq!(found.items.first().map(|s| s.tag.as_str()), Some("long hair"));
        let found = sample().suggest("^_", 2, false, 12);
        assert_eq!(found.items.first().map(|s| s.tag.as_str()), Some("^_^"));
    }

    #[test]
    fn the_replaced_range_is_the_trimmed_word_at_start_middle_and_end() {
        let start = sample().suggest("cat", 3, false, 12);
        assert_eq!((start.from, start.to), (0, 3));
        let middle = sample().suggest("cat, cat, cathode", 8, false, 12);
        assert_eq!((middle.from, middle.to), (5, 8));
        let end = sample().suggest("cat, lo", 7, false, 12);
        assert_eq!((end.from, end.to), (5, 7));
        let too_short = sample().suggest("cat, l", 6, false, 12);
        assert!(too_short.items.is_empty());
        assert!(sample().suggest("cat, ", 5, false, 12).items.is_empty());
    }

    #[test]
    fn count_labels_stay_short() {
        assert_eq!(count_label(850), "850");
        assert_eq!(count_label(1_500), "1.5k");
        assert_eq!(count_label(12_000), "12k");
        assert_eq!(count_label(1_200_000), "1.2M");
        assert_eq!(count_label(5_596_576), "5.6M");
    }

    #[test]
    fn the_embedded_lists_load_and_find_tags() {
        let found = suggest("long_h", 6, false, 12);
        assert!(found.items.iter().any(|s| s.tag == "long hair"));
        let found = suggest("^_", 2, false, 12);
        assert!(found.items.iter().any(|s| s.tag == "^_^"));
    }

    #[test]
    fn complete_inserts_a_separator_but_never_doubles_one() {
        let end = sample().suggest("cat, do", 7, false, 12);
        let (text, caret) = complete("cat, do", 7, &end.items[0]);
        assert_eq!(text, "cat, dog, ");
        assert_eq!(caret, text.len());

        let end = sample().suggest("cat, do ", 7, false, 12);
        let (text, _) = complete("cat, do ", 7, &end.items[0]);
        assert_eq!(text, "cat, dog, ");

        let middle = sample().suggest("cat, cat, cathode", 8, false, 12);
        let caterpillar = middle.items.iter().find(|s| s.tag == "caterpillar").unwrap();
        let (text, caret) = complete("cat, cat, cathode", 8, caterpillar);
        assert_eq!(text, "cat, caterpillar, cathode");
        assert_eq!(caret, "cat, caterpillar, ".len());

        let emoticon = sample().suggest("^_", 2, false, 12);
        let (text, _) = complete("^_", 2, &emoticon.items[0]);
        assert_eq!(text, "^_^, ");
    }
}
