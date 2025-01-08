use serde_yaml::Result;

/// YAML frontmatter from an Obsidian note.
///
/// See [`serde_yaml::Mapping`].
pub type Frontmatter = serde_yaml::Mapping;

pub fn from_str(mut s: &str) -> Result<Frontmatter> {
    if s.is_empty() {
        s = "{}";
    }
    let frontmatter: Frontmatter = serde_yaml::from_str(s)?;
    Ok(frontmatter)
}
