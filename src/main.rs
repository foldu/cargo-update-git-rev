use anyhow::{bail, Context};
use std::{io::Write, path::Path};
use structopt::StructOpt;
use toml_edit::{Document, Item};

fn run() -> Result<(), anyhow::Error> {
    let Opt::UpdateGitRev { crates, all } = Opt::from_args();

    let cargo_toml = std::fs::read_to_string("Cargo.toml").context("Can't read Cargo.toml")?;
    let mut parsed = cargo_toml
        .parse::<toml_edit::Document>()
        .context("Can't parse Cargo.toml as toml")?;

    let crates_with_urls = if all {
        git_crates(&parsed)
    } else {
        depends_with_url(&parsed, crates)
    }?;

    for (krate, url) in crates_with_urls {
        let tmp_dir = tempfile::tempdir().context("Can't create temporary directory")?;
        println!("Cloning {}", url);
        let repo = git2::Repository::clone(&url, tmp_dir.path()).context("Can't clone git repo")?;
        let latest_rev = repo
            .head()?
            .target()
            .context("Can't get object id for head")?
            .to_string();
        println!("Latest git rev for {}: {}", krate, latest_rev);
        parsed["dependencies"][&krate]["rev"] = toml_edit::value(latest_rev);
    }

    write(
        "Cargo.toml",
        parsed.to_string_in_original_order().as_bytes(),
    )
    .context("Can't write to Cargo.toml")?;

    Ok(())
}

fn depends_with_url(
    doc: &Document,
    crates: Vec<String>,
) -> Result<Vec<(String, String)>, anyhow::Error> {
    crates
        .into_iter()
        .map(|krate| match &doc["dependencies"][&krate]["git"] {
            Item::Value(val) if val.is_str() => Ok((krate, val.as_str().unwrap().to_owned())),
            Item::None => bail!("Crate {} is not a git dependency", krate),
            _ => bail!("Crate {} git key is not a string", krate),
        })
        .collect()
}

fn git_crates(doc: &Document) -> Result<Vec<(String, String)>, anyhow::Error> {
    match &doc["dependencies"] {
        Item::Table(ref tab) => Ok(tab
            .iter()
            .filter_map(|(k, v)| v.as_table_like().map(|v| (k, v)))
            .filter_map(|(k, v)| {
                v.get("git")
                    .and_then(|url| url.as_value())
                    .and_then(|url| url.as_str())
                    .map(|v| (k.to_owned(), v.to_owned()))
            })
            .collect()),
        Item::None => Ok(Vec::new()),
        _ => anyhow::bail!("Invalid Cargo.toml: [dependencies] is not a table"),
    }
}

fn write(path: impl AsRef<Path>, cont: &[u8]) -> Result<(), std::io::Error> {
    let path = path.as_ref();
    let parent = path
        .parent()
        .expect("Trying to write to the root directory");
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write(cont)?;
    tmp.flush()?;
    // theoretically you'd also have to sync the dir inode

    // can't fail on linux
    let (_, new_path) = tmp.keep().unwrap();
    std::fs::rename(new_path, path)?;

    Ok(())
}

#[derive(StructOpt)]
enum Opt {
    UpdateGitRev {
        crates: Vec<String>,

        #[structopt(short, long)]
        all: bool,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}
