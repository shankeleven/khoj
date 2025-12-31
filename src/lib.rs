use std::fs::{self, File};
use std::path::{Path};
use xml::reader::{XmlEvent, EventReader};
use xml::common::{Position, TextPosition};
use std::env;
use std::result::Result;
use std::process::ExitCode;
use std::str;
use std::io::{BufReader, BufWriter};
use std::sync::{Arc, Mutex};
use std::thread;

pub mod model;
use model::*;
mod server;
mod lexer;
pub mod snowball;
pub mod ignore_rules;

fn parse_entire_txt_file(file_path: &Path) -> Result<String, ()> {
    fs::read_to_string(file_path).map_err(|err| {
        eprintln!("ERROR: coult not open file {file_path}: {err}", file_path = file_path.display());
    })
}

fn parse_entire_pdf_file(file_path: &Path) -> Result<String, ()> {
    use poppler::Document;
    use std::io::Read;

    let mut content = Vec::new();
    File::open(file_path)
        .and_then(|mut file| file.read_to_end(&mut content))
        .map_err(|err| {
            eprintln!("ERROR: could not read file {file_path}: {err}", file_path = file_path.display());
        })?;

    let pdf = Document::from_data(&content, None).map_err(|err| {
        eprintln!("ERROR: could not read file {file_path}: {err}",
                  file_path = file_path.display());
    })?;

    let mut result = String::new();

    let n = pdf.n_pages();
    for i in 0..n {
        let page = pdf.page(i).expect(&format!("{i} is within the bounds of the range of the page"));
        if let Some(content) = page.text() {
            result.push_str(content.as_str());
            result.push(' ');
        }
    }

    Ok(result)
}

fn parse_entire_xml_file(file_path: &Path) -> Result<String, ()> {
    let file = File::open(file_path).map_err(|err| {
        eprintln!("ERROR: could not open file {file_path}: {err}", file_path = file_path.display());
    })?;
    let er = EventReader::new(BufReader::new(file));
    let mut content = String::new();
    for event in er.into_iter() {
        let event = event.map_err(|err| {
            let TextPosition {row, column} = err.position();
            let msg = err.msg();
            eprintln!("{file_path}:{row}:{column}: ERROR: {msg}", file_path = file_path.display());
        })?;

        if let XmlEvent::Characters(text) = event {
            content.push_str(&text);
            content.push(' ');
        }
    }
    Ok(content)
}

pub fn parse_entire_file_by_extension(file_path: &Path) -> Result<String, ()> {
    let extension = match file_path.extension() {
        Some(ext) => ext.to_string_lossy().to_ascii_lowercase(),
        None => return Err(()),
    };
    match extension.as_str() {
        "xhtml" | "xml" => parse_entire_xml_file(file_path),
        // Treat common source and config files as plain UTF-8 text
        "txt" | "md"
        | "rs" | "js" | "jsx" | "ts" | "tsx"
        | "json" | "toml" | "yaml" | "yml"
        | "py" | "go" | "java" | "kt" | "kts"
        | "c" | "h" | "hpp" | "hh" | "cpp" | "cc" | "cxx"
        | "cs" | "rb" | "php"
        | "html" | "htm" | "css" | "scss" | "less"
        | "mdx" | "ini" | "cfg" | "conf"
        | "sh" | "bash" | "zsh" | "fish"
        | "pl" | "sql" | "gradle" | "properties"
        | "r" | "tex" | "rst"
        | "vue" | "svelte" | "dart" | "erl" | "ex" | "exs" | "lua" | "nim"
            => parse_entire_txt_file(file_path),
        "pdf" => parse_entire_pdf_file(file_path),
        _ => Err(()),
    }
}

fn save_model_as_json(model: &Model, index_path: &Path) -> Result<(), ()> {
    println!("Saving {index_path}...", index_path = index_path.display());

    let index_file = File::create(index_path).map_err(|err| {
        eprintln!("ERROR: could not create index file {index_path}: {err}",
                  index_path = index_path.display());
    })?;

    serde_json::to_writer(BufWriter::new(index_file), &model).map_err(|err| {
        eprintln!("ERROR: could not serialize index into file {index_path}: {err}",
                  index_path = index_path.display());
    })?;

    Ok(())
}

use walkdir::WalkDir;
use rayon::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};

pub fn add_folder_to_model(dir_path: &Path, model: Arc<Mutex<Model>>, processed: &mut usize) -> Result<(), ()> {
    let files: Vec<_> = WalkDir::new(dir_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_owned())
        .collect();

    let processed_count = AtomicUsize::new(0);

    files.par_iter().for_each(|file_path| {
        // Skip if matched by .khojignore (checked inside is_ignored)
        if ignore_rules::is_ignored(file_path, false) {
            return;
        }

        let dot_file = file_path
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.starts_with("."))
            .unwrap_or(false);

        if dot_file {
            return;
        }

        let extension = match file_path.extension() {
            Some(ext) => ext.to_string_lossy().to_ascii_lowercase(),
            None => return,
        };

        match extension.as_str() {
            // Allowlist: text, markup, source code, configs
            "txt" | "md" | "xml" | "xhtml" | "pdf"
            | "rs" | "js" | "jsx" | "ts" | "tsx"
            | "json" | "toml" | "yaml" | "yml"
            | "py" | "go" | "java" | "kt" | "kts"
            | "c" | "h" | "hpp" | "hh" | "cpp" | "cc" | "cxx"
            | "cs" | "rb" | "php"
            | "html" | "htm" | "css" | "scss" | "less"
            | "mdx" | "ini" | "cfg" | "conf"
            | "sh" | "bash" | "zsh" | "fish"
            | "pl" | "sql" | "gradle" | "properties"
            | "r" | "tex" | "rst"
            | "vue" | "svelte" | "dart" | "erl" | "ex" | "exs" | "lua" | "nim"
                => { /* supported */ }
            _ => return,
        }

        let last_modified = match file_path.metadata().and_then(|m| m.modified()) {
            Ok(time) => time,
            Err(err) => {
                eprintln!("ERROR: could not get metadata for {}: {}", file_path.display(), err);
                return;
            }
        };

        // Check if reindexing is needed - requires lock, but quick check
        let needs_reindexing = {
            let mut model = model.lock().unwrap();
            model.requires_reindexing(file_path, last_modified)
        };

        if needs_reindexing {
             // Parse content WITHOUT lock
             let content = match parse_entire_file_by_extension(file_path) {
                Ok(content) => content.chars().collect::<Vec<_>>(),
                Err(()) => return,
            };

            // Compute search data (tokenization) WITHOUT lock, in parallel
            let (count, tf, positions) = Model::compute_search_data(&content);

            // Add to model WITH lock - minimal critical section
            {
                let mut model = model.lock().unwrap();
                model.add_document_precomputed(file_path.clone(), last_modified, count, tf, positions);
            }
            
            processed_count.fetch_add(1, Ordering::SeqCst);
        }
    });

    *processed += processed_count.load(Ordering::SeqCst);
    Ok(())
}

fn usage(program: &str) {
    eprintln!("Usage: {program} [SUBCOMMAND] [OPTIONS]");
    eprintln!("Subcommands:");
    eprintln!("    serve <folder> [address]       start local HTTP server with Web Interface");
}

pub fn entry() -> Result<(), ()> {
    let mut args = env::args();
    let program = args.next().expect("path to program is provided");

    let subcommand = args.next().ok_or_else(|| {
        usage(&program);
        eprintln!("ERROR: no subcommand is provided");
    })?;

    match subcommand.as_str() {
        "serve" => {
            let dir_path = args.next().ok_or_else(|| {
                usage(&program);
                eprintln!("ERROR: no directory is provided for {subcommand} subcommand");
            })?;

            // Initialize ignore rules from .khojignore
            ignore_rules::init(Path::new(&dir_path));

            let mut index_path = Path::new(&dir_path).to_path_buf();
            index_path.push(".finder.json");

            let address = args.next().unwrap_or("127.0.0.1:6969".to_string());

            let exists = index_path.try_exists().map_err(|err| {
                eprintln!("ERROR: could not check the existence of file {index_path}: {err}",
                          index_path = index_path.display());
            })?;

            let model: Arc<Mutex<Model>>;
            if exists {
                let index_file = File::open(&index_path).map_err(|err| {
                    eprintln!("ERROR: could not open index file {index_path}: {err}",
                              index_path = index_path.display());
                })?;

                model = Arc::new(Mutex::new(serde_json::from_reader(index_file).map_err(|err| {
                    eprintln!("ERROR: could not parse index file {index_path}: {err}",
                              index_path = index_path.display());
                })?));
            } else {
                model = Arc::new(Mutex::new(Default::default()));
            }

            {
                let model = Arc::clone(&model);
                thread::spawn(move || {
                    let mut processed = 0;
                    // TODO: what should we do in case indexing thread crashes
                    add_folder_to_model(Path::new(&dir_path), Arc::clone(&model), &mut processed).unwrap();
                    if processed > 0 {
                        let model = model.lock().unwrap();
                        save_model_as_json(&model, &index_path).unwrap();
                    }
                    println!("Finished indexing");
                });
            }

            server::start(&address, Arc::clone(&model))
        }

        _ => {
            usage(&program);
            eprintln!("ERROR: unknown subcommand {subcommand}");
            Err(())
        }
    }
}

fn main() -> ExitCode {
    match entry() {
        Ok(()) => ExitCode::SUCCESS,
        Err(()) => ExitCode::FAILURE,
    }
}

// TODO: search result must consist of clickable links
// TODO: synonym terms
