use ratatui::style::{Color, Modifier, Style};

// Use terminal defaults and ANSI palette entries so the user's shell theme
// controls the actual colors, including custom palettes and light terminals.
pub const BG: Color = Color::Reset;
pub const SURFACE: Color = Color::Reset;
pub const BORDER: Color = Color::DarkGray;
pub const TEXT: Color = Color::Reset;
pub const MUTED: Color = Color::Gray;
pub const BLUE: Color = Color::Blue;
pub const CYAN: Color = Color::Cyan;
pub const GREEN: Color = Color::Green;
pub const AMBER: Color = Color::Yellow;
pub const RED: Color = Color::Red;
pub const PURPLE: Color = Color::Magenta;

pub fn body() -> Style {
    Style::default()
}

pub fn panel<'a>(title: impl Into<ratatui::text::Line<'a>>) -> ratatui::widgets::Block<'a> {
    ratatui::widgets::Block::bordered()
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .title(title)
        .title_style(Style::default().fg(BLUE).add_modifier(Modifier::BOLD))
}

pub fn keycap(
    label: &'static str,
    description: &'static str,
    color: Color,
) -> Vec<ratatui::text::Span<'static>> {
    use ratatui::text::Span;
    vec![
        Span::styled(
            format!(" {label} "),
            Style::default()
                .fg(color)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED),
        ),
        Span::styled(format!(" {description}  "), Style::default().fg(MUTED)),
    ]
}
