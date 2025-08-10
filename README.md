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
- Content search using TF-IDF
- Preview with highlighted terms
- Smooth user interface

## Testing
We can search for words like "search", "preview", or "TUI" to see if highlighting works correctly.

The system should find these keywords and show them with → ← markers around the matching terms.
