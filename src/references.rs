use std::sync::LazyLock;

use regex::Regex;

static OBSIDIAN_NOTE_LINK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?P<file>[^#|]+)??(#(?P<section>.+?))??(\|(?P<label>.+?))??$").unwrap()
});

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
/// `ObsidianNoteReference` represents the structure of a `[[note]]` or `![[embed]]` reference.
pub struct ObsidianNoteReference<'a> {
    /// The file (note name or partial path) being referenced.
    /// This will be None in the case that the reference is to a section within the same document
    pub file: Option<&'a str>,
    /// If specific, a specific section/heading being referenced.
    pub section: Option<&'a str>,
    /// If specific, the custom label/text which was specified.
    pub label: Option<&'a str>,
}

#[derive(PartialEq, Eq)]
/// `RefParserState` enumerates all the possible parsing states [`RefParser`] may enter.
pub enum RefParserState {
    NoState,
    ExpectSecondOpenBracket,
    ExpectRefText,
    ExpectRefTextOrCloseBracket,
    ExpectFinalCloseBracket,
    Resetting,
}

/// `RefType` indicates whether a note reference is a link (`[[note]]`) or embed (`![[embed]]`).
pub enum RefType {
    Link,
    Embed,
}

/// `RefParser` holds state which is used to parse Obsidian `WikiLinks` (`[[note]]`, `![[embed]]`).
pub struct RefParser {
    pub state: RefParserState,
    pub ref_type: Option<RefType>,
    // References sometimes come in through multiple events. One example of this is when notes
    // start with an underscore (_), presumably because this is also the literal which starts
    // italic and bold text.
    //
    // ref_text concatenates the values from these partial events so that there's a fully-formed
    // string to work with by the time the final `]]` is encountered.
    pub ref_text: String,
}

impl RefParser {
    pub const fn new() -> Self {
        Self {
            state: RefParserState::NoState,
            ref_type: None,
            ref_text: String::new(),
        }
    }

    pub fn transition(&mut self, new_state: RefParserState) {
        self.state = new_state;
    }

    pub fn reset(&mut self) {
        self.state = RefParserState::NoState;
        self.ref_type = None;
        self.ref_text.clear();
    }
}

impl ObsidianNoteReference<'_> {
    pub fn from_str(text: &str) -> ObsidianNoteReference {
        let captures = OBSIDIAN_NOTE_LINK_RE
            .captures(text)
            .expect("note link regex didn't match - bad input?");
        let file = captures.name("file").map(|v| v.as_str().trim());
        let label = captures.name("label").map(|v| v.as_str());
        let section = captures.name("section").map(|v| v.as_str().trim());

        ObsidianNoteReference {
            file,
            section,
            label,
        }
    }
}
