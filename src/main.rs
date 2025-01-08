use std::path::PathBuf;

use clap::Parser;
use obsidian_copy::{Copier, CopyError};

/// Copies part of an obsidian vault according to filters.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Root of vault to copy
    #[arg(short, long)]
    root: PathBuf,

    /// Destination to copy to
    #[arg(short, long)]
    destination: PathBuf,

    /// Tags to include in copied vault
    #[arg(short, long)]
    include_tags: Vec<String>,

    /// Tags to exclude in copied vault
    #[arg(short, long)]
    exclude_tags: Vec<String>,
}

fn main() -> Result<(), CopyError> {
    let args = Args::parse();

    let mut copier = Copier::new(args.root, args.destination);

    copier
        .include_tags(args.include_tags)
        .exclude_tags(args.exclude_tags);

    copier.index()?;
    copier.copy()?;

    Ok(())
}
