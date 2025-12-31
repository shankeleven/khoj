# KHOJ is a Local Search Engine written in Rust
Currently only supports basic text file formats like txt, md, and pdf
SOON this would support source code files like c, cpp, py, js, etc.
and is not optimized for large files.
#### Update: Now it does

## Quick Start

```console
$ cargo run or cargo build # to build the project and use the prebuilt index
$ cargo run -- refresh or cargo build -- refresh # to refresh the index
```

<img width="1920" height="1080" alt="image" src="https://github.com/user-attachments/assets/45943c57-003d-4c84-b1fc-f1c715fad997" />


## Features

### Performance
- Background indexing so the tool starts immediately.  
- Local index stored in `.finder.json` for faster subsequent runs.  
- Debounced input to keep the interface responsive.

### Search
- Fuzzy filename matching.  
- Full-text search across files.  
- Results ranked by relevance, with filename matches prioritized over content matches.

### Terminal UI
- Live file preview with query highlighting.  
- Syntax highlighting where applicable.  
- Keyboard navigation (Up/Down) and mouse support.

### Configuration
- Supports `.khojignore` for excluding files and directories, same format as .gitignore 
- Opens results in VS Code or the editor defined in environment variables.

---
### Options

| Option | Description |
|---------|-------------|
| `--refresh`, `-r` | Rebuilds the index and ignores any existing `.finder.json`. |

Example:


