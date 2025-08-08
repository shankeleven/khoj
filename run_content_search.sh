#!/bin/bash

echo "🔍 Khoj Content Search Engine - TUI Demo"
echo "========================================"
echo ""
echo "✅ Now featuring REAL content search!"
echo ""
echo "📁 Indexed files include:"
find . -name "*.rs" -o -name "*.md" -o -name "*.txt" | head -5
echo "   ... and more"
echo ""
echo "🔍 Try searching for:"
echo "  • 'pub fn' - to find function definitions"
echo "  • 'struct' - to find struct definitions" 
echo "  • 'Result' - to find error handling"
echo "  • 'use std' - to find imports"
echo "  • 'impl' - to find implementations"
echo ""
echo "⌨️  Controls:"
echo "  • Type to search content (not just filenames!)"
echo "  • ↑/↓ arrows to navigate results"
echo "  • ESC to quit"
echo ""
echo "🚀 Starting TUI..."
echo ""

./target/debug/seroost
