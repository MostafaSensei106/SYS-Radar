use ratatui::{Frame, layout::Rect, text::Line, widgets::Paragraph};

use crate::ui::theme;

pub fn render_footer(frame: &mut Frame, area: Rect, filtering: bool, confirming_kill: bool) {
    let spans = if confirming_kill {
        vec![
            theme::keycap("Y", "confirm terminate", theme::RED),
            theme::keycap("N / ESC", "cancel", theme::MUTED),
        ]
    } else if filtering {
        vec![
            theme::keycap("ENTER", "apply", theme::GREEN),
            theme::keycap("ESC", "cancel", theme::MUTED),
            theme::keycap("BKSP", "edit", theme::BLUE),
        ]
    } else {
        vec![
            theme::keycap("↑↓", "select", theme::BLUE),
            theme::keycap("/", "filter", theme::CYAN),
            theme::keycap("C M T", "sort", theme::PURPLE),
            theme::keycap("K", "terminate", theme::RED),
            theme::keycap("Q", "quit", theme::MUTED),
        ]
    };
    let line = Line::from(spans.into_iter().flatten().collect::<Vec<_>>());
    frame.render_widget(Paragraph::new(line), area);
}
