use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use std::sync::{Arc, Mutex};
use std::{
    env,
    error::Error,
    io,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use crate::model::{Model};
use crate::add_folder_to_model;

/// Represents a single search result.
#[derive(Debug, Clone)]
struct SearchResult {
    /// The path to the file.
    file_path: PathBuf,
    /// A snippet from the file where the match was found.
    preview_line: String,
    /// The line number of the preview.
    line_number: usize,
    /// Score from the fuzzy matcher.
    score: i64,
}

/// Represents your search index.
struct Index {
    model: Model,
}

impl Index {
    fn new() -> Self {
        Self { model: Model::default() }
    }

    fn search(&self, query: &str) -> Vec<SearchResult> {
        if query.is_empty() {
            return Vec::new();
        }

        // Use the model's built-in search functionality for content search
        let query_chars: Vec<char> = query.chars().collect();
        let search_results = self.model.search_query(&query_chars);
        
        let mut results = Vec::new();
        
        for (path, score) in search_results.iter() {
            // Only include results with a meaningful score (filter out very low relevance)
            if *score < 0.001 {
                continue;
            }
            
            // Get a preview line from the file content
            let preview_line = if let Ok(content) = std::fs::read_to_string(path) {
                // Find the first line containing any of the query terms
                let query_lower = query.to_lowercase();
                let query_words: Vec<&str> = query_lower.split_whitespace().collect();
                let mut found_line = None;
                
                // Look for lines that contain the query terms
                for line in content.lines() {
                    let line_lower = line.to_lowercase();
                    // Check if the line contains any of the query words
                    if query_words.iter().any(|word| line_lower.contains(word)) {
                        found_line = Some(line.trim().to_string());
                        break;
                    }
                }
                
                // If no line contains the query directly, check if the file actually matches
                if found_line.is_none() {
                    let content_lower = content.to_lowercase();
                    if !query_words.iter().any(|word| content_lower.contains(word)) {
                        // Skip this file if it doesn't actually contain the search terms
                        continue;
                    }
                }
                
                // If no specific line found, use the first non-empty line
                found_line.unwrap_or_else(|| {
                    content.lines()
                        .find(|line| !line.trim().is_empty())
                        .unwrap_or("No preview available")
                        .trim()
                        .to_string()
                })
            } else {
                "Could not read file".to_string()
            };

            results.push(SearchResult {
                file_path: path.clone(),
                preview_line,
                line_number: 1,
                score: (score * 1000.0) as i64,
            });
        }

        // Sort by score (highest first) and limit results
        results.sort_by(|a, b| b.score.cmp(&a.score));
        results.truncate(20); // Limit to top 20 results
        results
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
        }
    }

    /// Called when the user types a character. Updates the query and search results.
    fn on_key(&mut self, c: char) {
        self.query.push(c);
        self.update_search_results();
    }

    /// Called on backspace.
    fn on_backspace(&mut self) {
        self.query.pop();
        self.update_search_results();
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
        self.results = self.index.search(&self.query);

        if !self.results.is_empty() {
            self.results_state.select(Some(0));
        } else {
            self.results_state.select(None);
        }
        self.update_preview();
    }

    /// Updates the preview pane with the content of the selected file.
    fn update_preview(&mut self) {
        if let Some(selected_index) = self.results_state.selected() {
            if let Some(selected_result) = self.results.get(selected_index) {
                // Simple file preview
                self.preview_content = get_simple_preview(&selected_result.file_path)
                    .unwrap_or_else(|e| format!("Error reading file: {}", e));
            }
        } else {
            self.preview_content = "Type to search files...".to_string();
        }
    }
}

pub fn main() -> Result<(), Box<dyn Error>> {
    // Perform indexing first
    let current_dir = env::current_dir()?;
    
    // Create the model and index it
    let wrapped_model = Arc::new(Mutex::new(Model::default()));
    add_folder_to_model(&current_dir, Arc::clone(&wrapped_model), &mut 0).map_err(|_| "Failed to index folder")?;
    
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

    if let Err(err) = res {
        println!("Error: {:?}", err);
    }

    Ok(())
}


/// The main application loop.
fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App) -> io::Result<()> {
    let tick_rate = Duration::from_millis(50); // Much faster tick rate for smooth UI
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
                        KeyCode::Esc => return Ok(()),
                        KeyCode::Char(c) => app.on_key(c),
                        KeyCode::Backspace => app.on_backspace(),
                        KeyCode::Down => app.next_result(),
                        KeyCode::Up => app.previous_result(),
                        KeyCode::Enter => {
                            // Open the file in the default editor
                            if let Some(selected_index) = app.results_state.selected() {
                                if let Some(_selected_result) = app.results.get(selected_index) {
                                    // You could implement file opening here
                                    // For now, just copy the path to clipboard or show a message
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }
}

/// Renders the user interface.
fn ui(f: &mut Frame, app: &mut App) {
    // Main layout: search bar and content
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Search bar
            Constraint::Min(0)     // Content
        ])
        .split(f.size());

    // Search bar
    let input = Paragraph::new(app.query.as_str())
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL).title("Search"));
    f.render_widget(input, chunks[0]);
    // Make the cursor visible
    f.set_cursor(chunks[0].x + app.query.len() as u16 + 1, chunks[0].y + 1);

    // Content layout: two vertical panes for results and preview.
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)].as_ref())
        .split(chunks[1]);

    // Results pane
    let results_items: Vec<ListItem> = app
        .results
        .iter()
        .map(|res| {
            let file_name = res.file_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Unknown");
            let dir_path = res.file_path
                .parent()
                .and_then(|p| p.to_str())
                .unwrap_or("");
            
            // Show filename, path, and preview of the matching content
            let preview = if res.preview_line.len() > 50 {
                format!("{}...", &res.preview_line[..47])
            } else {
                res.preview_line.clone()
            };
            
            ListItem::new(format!("{}\n  {}\n  → {}", file_name, dir_path, preview))
                .style(Style::default().fg(Color::White))
        })
        .collect();

    let results_list = List::new(results_items)
        .block(Block::default().borders(Borders::ALL).title("Results"))
        .highlight_style(
            Style::default()
                .bg(Color::LightGreen)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");
    f.render_stateful_widget(results_list, content_chunks[0], &mut app.results_state);

    // Preview pane
    let preview = Paragraph::new(app.preview_content.as_str())
        .wrap(Wrap { trim: true })
        .block(Block::default().borders(Borders::ALL).title("Preview"));
    f.render_widget(preview, content_chunks[1]);
}


// --- Helper Functions ---

/// Simple preview function that reads the first few lines of a file
fn get_simple_preview(file_path: &Path) -> Result<String, Box<dyn Error>> {
    let content = std::fs::read_to_string(file_path)?;
    let lines: Vec<&str> = content.lines().take(20).collect();
    Ok(lines.join("\n"))
}