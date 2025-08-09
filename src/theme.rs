
use ratatui::style::Color;

pub struct Theme {
    pub background: Color,
    pub foreground: Color,
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub highlight_bg: Color,
    pub highlight_fg: Color,
    pub border: Color,
    pub border_highlight: Color,
}

impl Default for Theme {
    fn default() -> Self {
        // Using a modern color palette (Catppuccin Macchiato)
        Self {
            background: Color::Rgb(36, 39, 58),    // Base
            foreground: Color::Rgb(202, 211, 245), // Text
            primary: Color::Rgb(138, 173, 244),    // Blue
            secondary: Color::Rgb(183, 189, 248),  // Lavender
            accent: Color::Rgb(245, 194, 231),     // Pink
            highlight_bg: Color::Rgb(87, 91, 118), // Surface1
            highlight_fg: Color::Rgb(202, 211, 245), // Text
            border: Color::Rgb(69, 73, 94),        // Overlay0
            border_highlight: Color::Rgb(138, 173, 244), // Blue
        }
    }
}
