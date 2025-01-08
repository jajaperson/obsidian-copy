mod frontmatter;
mod references;

use std::{
    collections::HashSet,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

pub use frontmatter::Frontmatter;
use ignore::WalkBuilder;
pub use pulldown_cmark;
use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, TagEnd};
use references::{ObsidianNoteReference, RefParser, RefParserState, RefType};
use serde_yaml::Value;
use snafu::{ResultExt, Snafu};
use unicode_normalization::UnicodeNormalization;

/// Represents all errors returned by this crate.
#[derive(Debug, Snafu)]
pub enum CopyError {
    #[snafu(display("failed to read from `{}`", path.display()))]
    /// This occurs when an IO read operation fails.
    ReadError {
        path: PathBuf,
        source: std::io::Error,
    },

    #[snafu(display("failed to copy `{}` to `{}`", from.display(), to.display()))]
    /// This occurs when copying a file fails.
    CopyError {
        from: PathBuf,
        to: PathBuf,
        source: std::io::Error,
    },

    #[snafu(display("No such file or directory: {}", path.display()))]
    /// This occurs when an operation is requested on a file or directory which doesn't exist.
    PathDoesNotExist { path: PathBuf },

    #[snafu(display("Encountered an error trying to walk `{}`", path.display()))]
    /// This occurs when an error is encountered while trying to walk a directory.
    WalkDirError {
        path: PathBuf,
        source: ignore::Error,
    },

    #[snafu(display("Failed to decode YAML frontmatter in `{}`", path.display()))]
    /// This occurs when an error is encountered parsing YAML frontmatter.
    FrontmatterDecodeError {
        path: PathBuf,
        #[snafu(source(from(serde_yaml::Error, Box::new)))]
        source: Box<serde_yaml::Error>,
    },
}

type Result<T, E = CopyError> = std::result::Result<T, E>;

pub struct Copier {
    root: PathBuf,
    destination: PathBuf,
    include_tags: HashSet<String>,
    exclude_tags: HashSet<String>,
    to_copy: HashSet<PathBuf>,
    vault_contents: HashSet<PathBuf>,
}

/// `Copier` provides the main interface to this library.
///
/// A `Copier` is created using [`Copier::new`], optionally followed by customization. Thereafter
/// calling [`Copier::index`] will find all the files to be copied. Finally, [`Copier::copy`]
/// copies the files to their destination.
impl Copier {
    /// Created a new copier which reads notes from `root` and copies these to `destination`.
    pub fn new(root: PathBuf, destination: PathBuf) -> Self {
        Self {
            root,
            destination,
            include_tags: HashSet::new(),
            exclude_tags: HashSet::new(),
            to_copy: HashSet::new(),
            vault_contents: HashSet::new(),
        }
    }

    /// Add tags to be included.
    pub fn include_tags(&mut self, tags: Vec<String>) -> &mut Self {
        self.include_tags.extend(tags);
        self
    }

    /// Add tags to be excluded.
    pub fn exclude_tags(&mut self, tags: Vec<String>) -> &mut Self {
        self.exclude_tags.extend(tags);
        self
    }

    /// Processes vault to determines files which should be copied.
    pub fn index(&mut self) -> Result<()> {
        self.vault_contents = vault_contents(&self.root)?;
        self.vault_contents
            .clone()
            .into_iter()
            .filter(|file| is_markdown_file(file))
            .try_for_each(|file| self.test_and_add_note(file))?;
        Ok(())
    }

    fn test_and_add_note(&mut self, src: PathBuf) -> Result<()> {
        let content = fs::read_to_string(&src).context(ReadSnafu { path: &src })?;
        let mut frontmatter_str = String::new();
        // let mut found_attachments: HashSet<PathBuf> = HashSet::new();

        let parser_options = Options::ENABLE_MATH
            | Options::ENABLE_TABLES
            | Options::ENABLE_FOOTNOTES
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;

        let mut parser = Parser::new_ext(&content, parser_options);
        let mut ref_parser = RefParser::new();
        let mut events: Vec<Event> = Vec::new();
        let mut buffer = Vec::with_capacity(5);
        let mut found_attachments: HashSet<PathBuf> = HashSet::new();

        'outer: while let Some(event) = parser.next() {
            // Collect all frontmatter to string in one sweep
            if matches!(event, Event::Start(Tag::MetadataBlock(_kind))) {
                for event in parser.by_ref() {
                    match event {
                        Event::Text(cowstr) => frontmatter_str.push_str(&cowstr),
                        Event::End(TagEnd::MetadataBlock(_kind)) => {
                            continue 'outer;
                        }
                        _ => panic!(
                            "Encountered an unexpected event while processing frontmatter in {}.",
                            src.display()
                        ),
                    }
                }
            }
            if ref_parser.state == RefParserState::Resetting {
                events.append(&mut buffer);
                buffer.clear();
                ref_parser.reset();
            }
            buffer.push(event.clone());
            match ref_parser.state {
                RefParserState::NoState => match event {
                    Event::Text(CowStr::Borrowed("![")) => {
                        ref_parser.ref_type = Some(RefType::Embed);
                        ref_parser.transition(RefParserState::ExpectSecondOpenBracket);
                    }
                    Event::Text(CowStr::Borrowed("[")) => {
                        ref_parser.ref_type = Some(RefType::Link);
                        ref_parser.transition(RefParserState::ExpectSecondOpenBracket);
                    }
                    _ => {
                        events.push(event);
                        buffer.clear();
                    }
                },
                RefParserState::ExpectSecondOpenBracket => match event {
                    Event::Text(CowStr::Borrowed("[")) => {
                        ref_parser.transition(RefParserState::ExpectRefText);
                    }
                    _ => {
                        ref_parser.transition(RefParserState::Resetting);
                    }
                },
                RefParserState::ExpectRefText => match event {
                    Event::Text(CowStr::Borrowed("]")) => {
                        ref_parser.transition(RefParserState::Resetting);
                    }
                    Event::Text(text) => {
                        ref_parser.ref_text.push_str(&text);
                        ref_parser.transition(RefParserState::ExpectRefTextOrCloseBracket);
                    }
                    _ => {
                        ref_parser.transition(RefParserState::Resetting);
                    }
                },
                RefParserState::ExpectRefTextOrCloseBracket => match event {
                    Event::Text(CowStr::Borrowed("]")) => {
                        ref_parser.transition(RefParserState::ExpectFinalCloseBracket);
                    }
                    Event::Text(text) => {
                        ref_parser.ref_text.push_str(&text);
                    }
                    _ => {
                        ref_parser.transition(RefParserState::Resetting);
                    }
                },
                RefParserState::ExpectFinalCloseBracket => match event {
                    Event::Text(CowStr::Borrowed("]")) => {
                        let reference = ObsidianNoteReference::from_str(&ref_parser.ref_text);
                        if let Some(attachment) = self.reference_to_path(reference) {
                            found_attachments.insert(attachment);
                        }
                    },
                    _ => {
                        ref_parser.transition(RefParserState::Resetting);
                    }
                },
                RefParserState::Resetting => panic!("Reached Resetting state, but it should have been handled prior to this match block"),
            }
        }

        let frontmatter = frontmatter::from_str(&frontmatter_str)
            .context(FrontmatterDecodeSnafu { path: &src })?;

        let tags: Vec<String> = match frontmatter.get("tags") {
            Some(Value::Sequence(tags)) => tags
                .iter()
                .filter_map(|tag| {
                    if let Value::String(s) = tag {
                        Some(s.to_string())
                    } else {
                        None
                    }
                })
                .collect(),
            _ => Vec::new(),
        };

        let include = tags.iter().any(|tag| self.include_tags.contains(tag))
            && !tags.iter().any(|tag| self.exclude_tags.contains(tag));

        if include {
            self.to_copy.insert(src);
            self.to_copy.extend(found_attachments);
        }

        Ok(())
    }

    fn reference_to_path(&self, reference: ObsidianNoteReference) -> Option<PathBuf> {
        reference
            .file
            .and_then(|filename| lookup_filename_in_vault(filename, &self.vault_contents))
            .cloned()
    }

    pub fn copy(self) -> Result<()> {
        for file in self.to_copy {
            let relative_path = file
                .strip_prefix(self.root.clone())
                .expect("walked files should be nested under root")
                .to_path_buf();
            let destination = &self.destination.join(relative_path);
            fs::copy(&file, destination).context(CopySnafu {
                from: file,
                to: destination,
            })?;
        }
        Ok(())
    }
}

/// `vault_contents` returns all of the files in an Obsidian vault located at the root, except
/// those ignored.
pub fn vault_contents(root: &Path) -> Result<HashSet<PathBuf>> {
    let mut contents = HashSet::new();
    let walker = WalkBuilder::new(root).hidden(false).build();
    for entry in walker {
        let entry = entry.context(WalkDirSnafu { path: root })?;
        let path = entry.path();
        if !entry.metadata().context(WalkDirSnafu { path })?.is_dir() {
            contents.insert(path.to_path_buf());
        }
    }
    Ok(contents)
}

fn is_markdown_file(file: &Path) -> bool {
    let no_ext = OsString::new();
    let ext = file.extension().unwrap_or(&no_ext).to_string_lossy();
    ext == "md"
}

fn lookup_filename_in_vault<'a>(
    filename: &str,
    vault_contents: &'a HashSet<PathBuf>,
) -> Option<&'a PathBuf> {
    let filename = PathBuf::from(filename);
    let filename_normalized: String = filename.to_string_lossy().nfc().collect();

    vault_contents.iter().find(|path| {
        let path_normalized_str: String = path.to_string_lossy().nfc().collect();
        let path_normalized = PathBuf::from(&path_normalized_str);
        let path_normalized_lowered = PathBuf::from(&path_normalized_str.to_lowercase());

        path_normalized.ends_with(&filename_normalized)
            || path_normalized.ends_with(filename_normalized.clone() + ".md")
            || path_normalized_lowered.ends_with(filename_normalized.to_lowercase())
            || path_normalized_lowered.ends_with(filename_normalized.to_lowercase() + ".md")
    })
}
