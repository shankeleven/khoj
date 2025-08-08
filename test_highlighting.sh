#!/bin/bash

echo "Testing TUI with color highlighting..."
echo ""
echo "Instructions:"
echo "1. The TUI will launch"
echo "2. Try searching for terms like 'test', 'search', 'TUI', or 'preview'"
echo "3. Navigate with Up/Down arrows"
echo "4. Notice the highlighted terms in the preview pane (yellow background)"
echo "5. Press Escape to exit"
echo ""
echo "Starting TUI in 3 seconds..."
sleep 1
echo "2..."
sleep 1  
echo "1..."
sleep 1

cargo run --bin seroost
