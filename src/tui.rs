use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use std::sync::{Arc, Mutex};
use std::{
    collections::VecDeque,
    env,
    error::Error,
    fs::File,
    io,
    io::{BufRead, BufReader, BufWriter},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use std::process::{Command, Stdio};

use crate::model::{Model};
use crate::add_folder_to_model;
use crate::theme::Theme;

const PREVIEW_FILL_LIMIT: usize = 100; // number of results to prefill preview for

/// Represents a single search result.
#[derive(Debug, Clone)]
struct SearchResult {
    /// The path to the file.
    file_path: PathBuf,
    /// A snippet from the file where the match was found.
    preview_line: String,
    /// Score from the fuzzy matcher.
    score: i64,
    /// Whether this result came from a filename match (not content)
    is_filename_match: bool,
}

/// Represents your search index.
struct Index {
    model: Model,
    /// Cached filename index for fast filename searches
    filename_cache: Vec<(PathBuf, String)>, // (path, lowercase_filename)
}

impl Index {
    fn new() -> Self {
        Self {
            model: Model::default(),
            filename_cache: Vec::new(),
        }
    }

    /// Build the filename cache once during initialization
    fn build_filename_cache(&mut self) {
        if let Ok(current_dir) = std::env::current_dir() {
            self.collect_filenames(&current_dir);
        }
    }

    fn collect_filenames(&mut self, dir: &Path) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();

                if path.is_file() {
                    if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                        self.filename_cache.push((path.clone(), filename.to_lowercase()));
                    }
                } else if path.is_dir() && !path.file_name().unwrap_or_default().to_str().unwrap_or("").starts_with('.') {
                    // Recursively collect from subdirectories (skip hidden dirs)
                    self.collect_filenames(&path);
                }
            }
        }
    }

    fn search(&self, query: &str) -> Vec<SearchResult> {
        if query.is_empty() || query.len() < 2 { return Vec::new(); }

        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();
        let query_chars: Vec<char> = query.chars().collect();

        // Content search first (no file I/O here)
        let content_search_results = self.model.search_query(&query_chars);
        let mut results = Vec::new();
        let mut processed_paths = std::collections::HashSet::new();

        for (path, score) in content_search_results.iter() {
            processed_paths.insert(path.clone());
            results.push(SearchResult {
                file_path: path.clone(),
                preview_line: String::new(),
                score: (score * 1000.0) as i64,
                is_filename_match: false,
            });
        }

        // Filename search (also no file I/O here)
        self.add_filename_search_results_fast(&mut results, &mut processed_paths, &query_words);

        // Sort by score (highest first). Do NOT truncate; keep all results.
        results.sort_by(|a, b| b.score.cmp(&a.score));

        // Fill previews only for the top results (perform file I/O now)
        self.fill_result_previews(&mut results, query);
        results
    }

    fn add_filename_search_results_fast(&self, results: &mut Vec<SearchResult>, processed_paths: &mut std::collections::HashSet<PathBuf>, query_words: &[&str]) {
        for (path, filename_lower) in &self.filename_cache {
            if processed_paths.contains(path) { continue; }

            let mut filename_score = 0;
            for word in query_words {
                if filename_lower.contains(word) {
                    filename_score += if filename_lower == *word { 100 } else { 50 };
                }
            }

            if filename_score > 0 {
                processed_paths.insert(path.clone());
                results.push(SearchResult {
                    file_path: path.clone(),
                    preview_line: String::new(), // filled later
                    score: filename_score,
                    is_filename_match: true,
                });
            }
        }
    }

    /// After sorting, populate preview lines with minimal I/O for only the first PREVIEW_FILL_LIMIT results
    fn fill_result_previews(&self, results: &mut [SearchResult], query: &str) {
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().filter(|w| !w.is_empty()).collect();
        for res in results.iter_mut().take(PREVIEW_FILL_LIMIT) {
            let file = match std::fs::File::open(&res.file_path) {
                Ok(f) => f,
                Err(_) => { res.preview_line = "Could not read file".to_string(); continue; }
            };
            let reader = BufReader::new(file);

            let mut first_non_empty: Option<String> = None;
            let mut chosen: Option<String> = None;
            // Scan at most N lines for performance
            let mut scanned = 0usize;
            for line in reader.lines() {
                scanned += 1;
                if scanned > 1000 { break; }
                let Ok(line) = line else { continue };
                if first_non_empty.is_none() && !line.trim().is_empty() {
                    first_non_empty = Some(line.trim().to_string());
                }
                let ll = line.to_lowercase();
                if query_words.iter().any(|w| ll.contains(w)) {
                    chosen = Some(line.trim().to_string());
                    break;
                }
            }

            let line = chosen
                .or(first_non_empty)
                .unwrap_or_else(|| "No preview available".to_string());

            res.preview_line = if res.is_filename_match {
                format!("[FILENAME MATCH] {}", line)
            } else { line };
        }
    }
}


/// Application state
struct App {
    /// The user's current search query.
    query: String,
    /// The list of search results to display.
    results: Vec<SearchResult>,
    /// The application's search index.
    index: Index,
    /// The state for the results list (handles selection and scrolling).
    results_state: ListState,
    /// The content for the file preview pane.
    preview_content: String,
    /// Styled preview content for highlighting
    preview_spans: Vec<Line<'static>>,
    /// Last search query to avoid redundant searches
    last_search_query: String,
    /// Debounce control: last input time and whether a search is pending
    last_input_time: Option<Instant>,
    needs_search: bool,
}

impl App {
    /// Creates a new App instance with the given index.
    fn new(index: Index) -> Self {
        Self {
            query: String::new(),
            results: Vec::new(),
            index,
            results_state: ListState::default(),
            preview_content: "Type to search files...".to_string(),
            preview_spans: vec![Line::from("Type to search files...")],
            last_search_query: String::new(),
            last_input_time: None,
            needs_search: false,
        }
    }

    /// Called when the user types a character. Updates the query and schedules a debounced search.
    fn on_key(&mut self, c: char) {
        self.query.push(c);
        self.last_input_time = Some(Instant::now());
        self.needs_search = true;
    }

    /// Called on backspace. Schedules a debounced search.
    fn on_backspace(&mut self) {
        self.query.pop();
        self.last_input_time = Some(Instant::now());
        self.needs_search = true;
    }

    /// Navigates to the next item in the results list.
    fn next_result(&mut self) {
        if self.results.is_empty() {
            return;
        }
        let i = match self.results_state.selected() {
            Some(i) => (i + 1) % self.results.len(),
            None => 0,
        };
        self.results_state.select(Some(i));
        self.update_preview();
    }

    /// Navigates to the previous item in the results list.
    fn previous_result(&mut self) {
        if self.results.is_empty() {
            return;
        }
        let i = match self.results_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.results.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.results_state.select(Some(i));
        self.update_preview();
    }

    /// Updates the search results based on the current query.
    fn update_search_results(&mut self) {
        if self.query == self.last_search_query {
            return;
        }
        self.last_search_query = self.query.clone();
        self.results = self.index.search(&self.query);
        if !self.results.is_empty() { self.results_state.select(Some(0)); } else { self.results_state.select(None); }
        self.update_preview();
    }

    /// Updates the preview pane with the content of the selected file.
    fn update_preview(&mut self) {
        if let Some(selected_index) = self.results_state.selected() {
            if let Some(selected_result) = self.results.get(selected_index) {
                // Enhanced file preview with highlighting
                let (content, spans) = get_enhanced_preview_with_styling(&selected_result.file_path, &self.query)
                    .unwrap_or_else(|e| (format!("Error reading file: {}", e), vec![Line::from("Error reading file")]));
                self.preview_content = content;
                self.preview_spans = spans;
            }
        } else {
            self.preview_content = "Type to search files...".to_string();
            self.preview_spans = vec![Line::from("Type to search files...")];
        }
    }
}

pub fn main() -> Result<(), Box<dyn Error>> {
    // Parse CLI args for --refresh
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("Usage: khoj [--refresh|-r]\n  --refresh  Rebuild index even if .finder.json exists");
        return Ok(());
    }
    let refresh = args.iter().any(|a| a == "--refresh" || a == "-r");

    // Determine working directory and index path
    let current_dir = env::current_dir()?;
    let index_path = current_dir.join(".finder.json");

    // Prepare model, either by loading existing index or indexing afresh
    let wrapped_model: Arc<Mutex<Model>> = if !refresh && index_path.try_exists().unwrap_or(false) {
        // Load existing index
        match File::open(&index_path) {
            Ok(file) => {
                let reader = BufReader::new(file);
                let model: Model = serde_json::from_reader(reader)?;
                Arc::new(Mutex::new(model))
            }
            Err(_) => Arc::new(Mutex::new(Model::default())),
        }
    } else {
        // Build a new index and save it
        let wrapped = Arc::new(Mutex::new(Model::default()));
        let mut processed = 0;
        add_folder_to_model(&current_dir, Arc::clone(&wrapped), &mut processed).map_err(|_| "Failed to index folder")?;
        if processed > 0 {
            if let Ok(file) = File::create(&index_path) {
                let writer = BufWriter::new(file);
                let model = wrapped.lock().unwrap();
                serde_json::to_writer(writer, &*model)?;
            }
        }
        wrapped
    };

    // Extract the model from the Arc<Mutex<>>
    let final_model = match Arc::try_unwrap(wrapped_model) {
        Ok(mutex) => match mutex.into_inner() {
            Ok(model) => model,
            Err(_) => return Err("Failed to extract model from mutex".into()),
        },
        Err(_) => return Err("Failed to extract model from Arc".into()),
    };

    // Create index with the populated model
    let mut index = Index::new();
    index.model = final_model;

    // Build filename cache for fast filename searches
    index.build_filename_cache();

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app and run it
    let app = App::new(index);
    let res = run_app(&mut terminal, app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    match res {
        Ok(RunOutcome::Quit) => {}
        Ok(RunOutcome::Open(path)) => {
            // After clean terminal restore, open editor then exit.
            open_file_external(&path);
        }
        Err(err) => println!("Error: {:?}", err),
    }

    Ok(())
}


/// The main application loop.
enum RunOutcome { Quit, Open(PathBuf) }

fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App) -> io::Result<RunOutcome> {
    let tick_rate = Duration::from_millis(50);
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Esc => return Ok(RunOutcome::Quit),
                        KeyCode::Char(c) => app.on_key(c),
                        KeyCode::Backspace => app.on_backspace(),
                        KeyCode::Down => app.next_result(),
                        KeyCode::Up => app.previous_result(),
                        KeyCode::Enter => {
                            if let Some(sel) = app.results_state.selected() {
                                if let Some(res) = app.results.get(sel) {
                                    return Ok(RunOutcome::Open(res.file_path.clone()));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Debounced search trigger
        if app.needs_search {
            if let Some(t) = app.last_input_time {
                if t.elapsed() >= Duration::from_millis(90) { // ~90ms debounce
                    app.needs_search = false;
                    app.update_search_results();
                }
            }
        }

        if last_tick.elapsed() >= tick_rate { last_tick = Instant::now(); }
    }
}

/// Renders the user interface.
fn ui(f: &mut Frame, app: &mut App) {
    let theme = Theme::default();
    let size = f.size();
    // Paint background
    let bg_block = Block::default().style(Style::default().bg(theme.background));
    f.render_widget(bg_block, size);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(size);

    // Header
    let header = Paragraph::new("  Khoj • ↑↓ navigate • Enter open • Esc quit")
        .style(Style::default().fg(theme.foreground).bg(theme.highlight_bg).add_modifier(Modifier::BOLD));
    f.render_widget(header, layout[0]);

    // Search bar
    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(Span::styled("Search", Style::default().fg(theme.secondary).add_modifier(Modifier::BOLD))); // BorderType removed to simplify
    let input = Paragraph::new(app.query.as_str())
        .style(Style::default().fg(theme.accent))
        .block(search_block);
    f.render_widget(input, layout[1]);
    f.set_cursor(layout[1].x + app.query.len() as u16 + 1, layout[1].y + 1);

    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)].as_ref())
        .split(layout[2]);

    // Prepare query words
    let lowered_query = app.query.to_lowercase();
    let q_words: Vec<&str> = lowered_query.split_whitespace().filter(|w| !w.is_empty()).collect();

    // Results items with theme
    let results_items: Vec<ListItem> = app.results.iter().map(|res| {
        let file_name = res.file_path.file_name().and_then(|n| n.to_str()).unwrap_or("Unknown");
        let dir_path = res.file_path.parent().and_then(|p| p.to_str()).unwrap_or("");
        let trimmed_preview = if res.preview_line.is_empty() {"(preview on select)".to_string()} else if res.preview_line.len()>80 {format!("{}…", &res.preview_line[..77])} else {res.preview_line.clone()};
        let filename_line = create_highlighted_line(file_name, &q_words, "");
        let preview_line = create_highlighted_line(&trimmed_preview, &q_words, "  → ");
        let path_line = Line::from(vec![Span::styled("  ", Style::default()), Span::styled(dir_path.to_string(), Style::default().fg(theme.secondary))]);
        ListItem::new(vec![filename_line, path_line, preview_line]).style(Style::default().fg(theme.foreground))
    }).collect();

    let results_title = format!("Results ({})", app.results.len());
    let results_list = List::new(results_items)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(theme.border)).title(Span::styled(results_title, Style::default().fg(theme.secondary).add_modifier(Modifier::BOLD))))
        .highlight_style(Style::default().bg(theme.highlight_bg).fg(theme.highlight_fg).add_modifier(Modifier::BOLD))
        .highlight_symbol("› ");
    f.render_stateful_widget(results_list, content_chunks[0], &mut app.results_state);

    let preview_block = Block::default().borders(Borders::ALL).border_style(Style::default().fg(theme.border)).title(Span::styled("Preview", Style::default().fg(theme.secondary).add_modifier(Modifier::BOLD)));
    let preview = Paragraph::new(app.preview_spans.clone()).wrap(Wrap { trim: true }).block(preview_block).style(Style::default().fg(theme.foreground));
    f.render_widget(preview, content_chunks[1]);

    let footer_text = format!("  Query len: {}  •  Results: {}  ", app.query.chars().count(), app.results.len());
    let footer = Paragraph::new(footer_text).style(Style::default().fg(theme.foreground).bg(theme.highlight_bg));
    f.render_widget(footer, layout[3]);
}


// --- Helper Functions ---

/// Enhanced preview function that returns both plain text and styled spans for highlighting
fn get_enhanced_preview_with_styling(file_path: &Path, query: &str) -> Result<(String, Vec<Line<'static>>), Box<dyn Error>> {
    let file = std::fs::File::open(file_path)?;
    let mut reader = BufReader::new(file);

    let query_lower = query.to_lowercase();
    let query_words: Vec<&str> = query_lower.split_whitespace().filter(|w| !w.is_empty()).collect();

    if query.is_empty() {
        return get_simple_preview_with_styling(file_path);
    }

    let mut preview_lines: Vec<String> = Vec::new();
    let mut styled_lines: Vec<Line<'static>> = Vec::new();

    // Keep last 3 lines for context before match
    let mut prev_lines: VecDeque<(usize, String)> = VecDeque::with_capacity(3);
    let mut line_num = 0usize;
    let mut match_found = false;

    // Also collect first 15 lines for fallback
    let mut first_lines: Vec<String> = Vec::new();

    // Read and search, limit scanning to avoid huge files stalling the UI
    let mut buf = String::new();
    while {
        buf.clear();
        match reader.read_line(&mut buf) {
            Ok(0) => false,
            Ok(_) => true,
            Err(_) => false,
        }
    } {
        line_num += 1;
        let line = buf.trim_end_matches(['\n', '\r']).to_string();
        if first_lines.len() < 15 { first_lines.push(format!("    {:3}: {}", line_num, &line)); }

        let ll = line.to_lowercase();
        if !match_found && query_words.iter().any(|w| ll.contains(w)) {
            // Emit previous context lines
            for (n, pline) in prev_lines.iter() {
                let plain = format!("    {:3}: {}", n, pline);
                preview_lines.push(plain.clone());
                styled_lines.push(Line::from(format!("    {:3}: {}", n, pline)));
            }
            // Emit the matching line with highlight
            let prefix = format!(">>> {:3}: ", line_num);
            preview_lines.push(format!("{}{}", &prefix, &line));
            styled_lines.push(create_highlighted_line(&line, &query_words, &prefix));

            // Emit up to 10 lines after match
            for i in 0..10 {
                buf.clear();
                match reader.read_line(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let next_line = buf.trim_end_matches(['\n','\r']).to_string();
                        let ln = line_num + i + 1;
                        let plain = format!("    {:3}: {}", ln, &next_line);
                        preview_lines.push(plain.clone());
                        styled_lines.push(Line::from(plain));
                    }
                }
            }

            match_found = true;
            break;
        }

        // Maintain rolling prev context
        if prev_lines.len() == 3 { prev_lines.pop_front(); }
        prev_lines.push_back((line_num, line));

        // Safety: hard limit on lines scanned
        if line_num >= 5000 { break; }
    }

    if !match_found {
        // Fallback to first 15 lines
        if first_lines.is_empty() {
            first_lines.push("(empty file)".to_string());
        }
        let styled: Vec<Line<'static>> = first_lines.iter().map(|l| Line::from(l.clone())).collect();
        return Ok((first_lines.join("\n"), styled));
    }

    Ok((preview_lines.join("\n"), styled_lines))
}

/// Create a highlighted line with colored spans
fn create_highlighted_line(line: &str, query_words: &[&str], prefix: &str) -> Line<'static> {
    let theme = Theme::default();
    let mut spans = vec![Span::styled(prefix.to_string(), Style::default().fg(theme.secondary))];
    let mut remaining = line.to_string();
    while !remaining.is_empty() {
        let mut found_match = false; let mut earliest_pos = remaining.len(); let mut match_len = 0;
        for word in query_words { if !word.is_empty() && word.len()>1 { let rem_lower = remaining.to_lowercase(); let w_lower = word.to_lowercase(); if let Some(pos)=rem_lower.find(&w_lower) { if pos < earliest_pos { earliest_pos = pos; match_len = word.len(); found_match=true; } } } }
        if found_match { if earliest_pos>0 { spans.push(Span::raw(remaining[..earliest_pos].to_string())); }
            let matched_text = &remaining[earliest_pos..earliest_pos+match_len];
            spans.push(Span::styled(matched_text.to_string(), Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)));
            remaining = remaining[earliest_pos+match_len..].to_string();
        } else { spans.push(Span::raw(remaining.clone())); break; }
    }
    Line::from(spans)
}

/// Simple preview function with styling that reads the first few lines of a file
fn get_simple_preview_with_styling(file_path: &Path) -> Result<(String, Vec<Line<'static>>), Box<dyn Error>> {
    let file = std::fs::File::open(file_path)?;
    let reader = BufReader::new(file);
    let mut lines: Vec<String> = Vec::new();
    for (i, line) in reader.lines().enumerate() {
        if i >= 20 { break; }
        let line = line.unwrap_or_default();
        lines.push(format!("{:3}: {}", i + 1, line));
    }
    let styled_lines: Vec<Line<'static>> = lines.iter().map(|l| Line::from(l.clone())).collect();
    Ok((lines.join("\n"), styled_lines))
}

/// Temporarily leave the TUI to open the selected file in an external editor, then return.
/// Launch external editor after program exit (terminal already restored by main).
fn open_file_external(path: &Path) {
    // Best-effort ensure terminal is in normal mode
    let _ = disable_raw_mode();
    let mut stdout = io::stdout();
    let _ = execute!(stdout, DisableMouseCapture);
    // Launch editor
    let (program, mut args) = select_editor();
    args.push(path.to_string_lossy().to_string());
    // For GUI editors (code/code-insiders) launch detached (non-blocking). For terminal editors, block.
    if program == "code" || program == "code-insiders" {
    if let Ok(child) = Command::new(&program)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn() {
            // Immediately detach
            let _ = child.id();
        }
    } else {
        let _ = Command::new(&program).args(&args).status();
    }
    // After editor returns, re-assert sane terminal (raw already disabled). Leave screen as-is.
    let _ = disable_raw_mode();
    let mut stdout2 = io::stdout();
    let _ = execute!(stdout2, DisableMouseCapture);
    // Print a newline to ensure shell prompt appears cleanly
    println!("");
}

fn select_editor() -> (String, Vec<String>) {
    // Helper to find a binary in PATH
    fn in_path(bin: &str) -> bool {
        if let Ok(path_var) = env::var("PATH") {
            for p in env::split_paths(&path_var) {
                let candidate = p.join(bin);
                if candidate.is_file() { return true; }
            }
        }
        false
    }

    for candidate in ["code", "code-insiders"].iter() {
        if in_path(candidate) { return ((**candidate).to_string(), vec![]); }
    }

    if let Ok(ed) = env::var("KHOJ_EDITOR") { return (ed, vec![]); }
    if let Ok(ed) = env::var("EDITOR") { return (ed, vec![]); }
    if in_path("nano") { return ("nano".to_string(), vec![]); }
    ("vi".to_string(), vec![])
}
