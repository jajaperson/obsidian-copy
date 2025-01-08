mod frontmatter;

use std::{
    collections::HashSet,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

pub use frontmatter::Frontmatter;
use ignore::WalkBuilder;
pub use pulldown_cmark;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use serde_yaml::Value;
use snafu::{ResultExt, Snafu};

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
        vault_contents(&self.root)?
            .into_iter()
            .filter(|file| is_markdown_file(file))
            .try_for_each(|file| {
                if self.test_note(&file)? {
                    self.to_copy.insert(file);
                }
                Ok(())
            })?;
        Ok(())
    }

    fn test_note(&self, src: &Path) -> Result<bool> {
        let content = fs::read_to_string(src).context(ReadSnafu { path: src })?;
        let mut frontmatter_str = String::new();
        // let mut found_attachments: HashSet<PathBuf> = HashSet::new();

        let parser_options = Options::ENABLE_MATH
            | Options::ENABLE_TABLES
            | Options::ENABLE_FOOTNOTES
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;

        let mut parser = Parser::new_ext(&content, parser_options);

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
        }

        let frontmatter = frontmatter::from_str(&frontmatter_str)
            .context(FrontmatterDecodeSnafu { path: src })?;

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

        Ok(include)
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
