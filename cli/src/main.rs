//! wasm-search command line: build a bundle from documents.jsonl, query it.
//!
//!     wasm-search build --documents docs.jsonl [--profile profile.json] --out bundle/
//!     wasm-search query --bundle bundle/ --query "转专业" [--limit 10]
mod bundle;

use anyhow::{bail, Context, Result};
use bundle::{load_engine, write_bundle};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use wasm_search_core::{compile_search_bundle, IndexDocument, Profile, Query};

fn option(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|value| value == name)
        .and_then(|index| args.get(index + 1).cloned())
}

fn required(args: &[String], name: &str) -> Result<String> {
    option(args, name).with_context(|| format!("missing {name}"))
}

fn build(args: &[String]) -> Result<()> {
    let started = Instant::now();
    let text = fs::read_to_string(required(args, "--documents")?)?;
    let documents = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .enumerate()
        .map(|(index, line)| {
            serde_json::from_str::<IndexDocument>(line)
                .with_context(|| format!("document line {}", index + 1))
        })
        .collect::<Result<Vec<_>>>()?;
    let profile: Profile = match option(args, "--profile") {
        Some(path) => serde_json::from_slice(&fs::read(path)?).context("invalid profile")?,
        None => Profile::default(),
    };
    let output = PathBuf::from(required(args, "--out")?);
    let bundle = compile_search_bundle(documents, profile).map_err(anyhow::Error::msg)?;
    write_bundle(&output, &bundle)?;
    println!(
        "{}",
        serde_json::json!({
            "documents": bundle.document_count,
            "lexicon_terms": bundle.lexicon_terms,
            "files": bundle.artifacts.len() + 1,
            "bytes": bundle.total_artifact_bytes(),
            "elapsed_ms": started.elapsed().as_millis(),
        })
    );
    Ok(())
}

fn query(args: &[String]) -> Result<()> {
    let engine = load_engine(&PathBuf::from(required(args, "--bundle")?))?;
    let request = Query {
        query: required(args, "--query")?,
        limit: option(args, "--limit")
            .and_then(|value| value.parse().ok())
            .unwrap_or(10),
        sort: Default::default(),
        filters: Default::default(),
    };
    let response = engine.search(&request).map_err(anyhow::Error::msg)?;
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("build") => build(&args[2..]),
        Some("query") => query(&args[2..]),
        _ => bail!("usage: wasm-search <build|query> ..."),
    }
}
