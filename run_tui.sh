#!/bin/bash

echo "Starting improved TUI search engine..."
echo "Features:"
echo "- Instant startup (no blocking indexing)"
echo "- Background indexing with progress"
echo "- Smooth search with debouncing (150ms)"
echo "- Fast file preview with caching"
echo "- Responsive UI (50ms tick rate)"
echo ""
echo "Controls:"
echo "- Type to search files"
echo "- Up/Down arrows to navigate"
echo "- Enter to select (placeholder)"
echo "- Esc to quit"
echo ""
echo "Starting TUI..."
sleep 2

./target/debug/seroost
