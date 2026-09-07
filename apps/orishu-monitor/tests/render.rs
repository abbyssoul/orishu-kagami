//! Headless rendering tests.
//!
//! Every principal view state is drawn through ratatui's test backend, so the
//! frames the operator sees are exercised without a terminal. Assertions are
//! semantic — "the honest unavailable notice is on screen", "the overlay lists
//! every implemented key" — rather than whole-frame snapshots, which would fail
//! on any wording or spacing change without proving more.

use orishu_monitor::keys;
use orishu_monitor::message::Message;
use orishu_monitor::model::{MIN_COLUMNS, MIN_ROWS, Model, Section, TerminalSize};
use orishu_monitor::update::update;
use orishu_monitor::view;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

/// Draw `model` into a `columns` x `rows` terminal and return what it says.
///
/// Cells are joined per row, and the rows are also returned joined, because a
/// wrapped sentence is one message to a reader even though it is several rows
/// to the buffer.
fn draw(model: &Model, columns: u16, rows: u16) -> Screen {
    let mut terminal =
        Terminal::new(TestBackend::new(columns, rows)).expect("the test backend always builds");
    terminal
        .draw(|frame| view::render(frame, model))
        .expect("drawing to the test backend cannot fail");

    Screen::from_buffer(terminal.backend().buffer())
}

struct Screen {
    lines: Vec<String>,
}

impl Screen {
    fn from_buffer(buffer: &Buffer) -> Self {
        let area = buffer.area();
        let lines = (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buffer[(area.x + x, area.y + y)].symbol())
                    .collect::<String>()
            })
            .collect();

        Self { lines }
    }

    /// The whole frame as one string, with rows separated by spaces so that
    /// text wrapped across rows still reads as words.
    fn flattened(&self) -> String {
        self.lines
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn contains(&self, needle: &str) -> bool {
        self.flattened().contains(needle)
    }

    fn any_line_contains(&self, needle: &str) -> bool {
        self.lines.iter().any(|line| line.contains(needle))
    }
}

fn shell() -> Model {
    Model::new(TerminalSize::new(100, 30))
}

fn fold(model: Model, messages: &[Message]) -> Model {
    messages
        .iter()
        .fold(model, |model, message| update(model, *message))
}

// ── Overview ─────────────────────────────────────────────────────────────────

#[test]
fn the_overview_names_the_application_and_the_active_section() {
    let screen = draw(&shell(), 100, 30);

    assert!(screen.any_line_contains(view::APPLICATION_NAME));
    assert!(screen.any_line_contains(Section::Overview.title()));
    assert!(screen.any_line_contains(Section::Members.title()));
}

/// The default state must state the limitation, and must not read as a
/// connection that was attempted and failed.
#[test]
fn the_overview_reports_that_worker_integration_is_not_included() {
    let screen = draw(&shell(), 100, 30);

    assert!(screen.contains("Live worker integration is not part of this build"));
    assert!(screen.contains("No connection has been attempted"));

    let text = screen.flattened().to_lowercase();
    for forbidden in [
        "connection refused",
        "unauthorized",
        "authentication",
        "timed out",
    ] {
        assert!(!text.contains(forbidden), "{forbidden} in frame");
    }
}

/// No figure may be presented as though a worker reported it — including a
/// zero, which reads as "a worker answered, and the count is zero".
#[test]
fn the_overview_shows_no_cluster_figures() {
    let screen = draw(&shell(), 100, 30);
    let text = screen.flattened().to_lowercase();

    for forbidden in [
        "nodes",
        "members:",
        "alive",
        "formation",
        "workload",
        "locked",
    ] {
        assert!(!text.contains(forbidden), "{forbidden} in frame");
    }
}

#[test]
fn the_footer_hints_at_help_and_quitting() {
    let screen = draw(&shell(), 100, 30);

    assert!(screen.contains("? toggle this help"));
    assert!(screen.contains("q / Ctrl-C quit"));
}

// ── Members ──────────────────────────────────────────────────────────────────

#[test]
fn members_renders_an_honest_empty_state() {
    let model = update(shell(), Message::SelectSection(Section::Members));
    let screen = draw(&model, 100, 30);

    assert!(screen.contains("No member data"));
    assert!(screen.contains("Live worker integration is not part of this build"));
}

/// An empty list must not be dressed as data: no invented rows, no placeholder
/// node names.
#[test]
fn members_lists_no_rows_at_all() {
    let model = update(shell(), Message::SelectSection(Section::Members));
    let screen = draw(&model, 100, 30);
    let text = screen.flattened().to_lowercase();

    assert_eq!(model.section_row_count(), 0);
    for forbidden in ["node-", "member 0", "worker-", "127.0.0.1", "unknown"] {
        assert!(!text.contains(forbidden), "{forbidden} in frame");
    }
}

#[test]
fn switching_sections_changes_what_the_body_shows() {
    let overview = draw(&shell(), 100, 30);
    let members = draw(
        &update(shell(), Message::SelectSection(Section::Members)),
        100,
        30,
    );

    assert!(overview.contains("Use orishuctl"));
    assert!(!overview.contains("No member data"));
    assert!(members.contains("No member data"));
    assert!(!members.contains("Use orishuctl"));
}

// ── Help ─────────────────────────────────────────────────────────────────────

#[test]
fn the_help_overlay_lists_every_implemented_key() {
    let model = update(shell(), Message::ToggleHelp);
    let screen = draw(&model, 100, 30);

    assert!(screen.contains(view::help::TITLE));
    for binding in keys::BINDINGS {
        assert!(
            screen.contains(&binding.keys_label()),
            "{} missing from help",
            binding.keys_label()
        );
        assert!(
            screen.contains(binding.action()),
            "{} missing from help",
            binding.action()
        );
    }
}

#[test]
fn closing_the_help_overlay_reveals_the_section_again() {
    let model = fold(shell(), &[Message::ToggleHelp, Message::Dismiss]);
    let screen = draw(&model, 100, 30);

    assert!(!screen.contains(view::help::TITLE));
    assert!(screen.contains("Live worker integration is not part of this build"));
}

/// The overlay is opaque: text on the rows it occupies is covered, not blended
/// with the help behind it.
#[test]
fn the_help_overlay_is_drawn_over_the_section() {
    const COVERED: &str = "Use orishuctl for implemented cluster administration.";

    let closed = draw(&shell(), 100, 30);
    let open = draw(&update(shell(), Message::ToggleHelp), 100, 30);

    assert!(closed.contains(COVERED));
    assert!(open.contains(view::help::TITLE));
    assert!(!open.contains(COVERED));
}

/// In a terminal too short for the whole key list, scrolling must reach the
/// entries the first frame could not show.
#[test]
fn a_short_terminal_can_scroll_the_help_overlay_to_the_last_key() {
    let last = keys::BINDINGS
        .last()
        .expect("there is at least one binding");
    let model = update(
        Model::new(TerminalSize::new(MIN_COLUMNS, MIN_ROWS)),
        Message::ToggleHelp,
    );

    let mut scrolled = model;
    for _ in 0..keys::BINDINGS.len() {
        scrolled = update(scrolled, Message::MoveDown);
    }

    let screen = draw(&scrolled, MIN_COLUMNS, MIN_ROWS);
    assert!(screen.contains(&last.keys_label()));
    assert!(screen.contains(last.action()));
}

// ── Size handling ────────────────────────────────────────────────────────────

#[test]
fn a_terminal_below_the_minimum_gets_a_compact_explanation() {
    let model = update(shell(), Message::Resized(TerminalSize::new(20, 5)));
    let screen = draw(&model, 20, 5);

    assert!(screen.contains("too small"));
    assert!(screen.contains(&format!("needs {MIN_COLUMNS}x{MIN_ROWS}")));
    assert!(screen.contains("have 20x5"));
    assert!(screen.contains("q to quit"));
}

/// The fallback has to be readable in the terminal that triggered it, so no
/// row of it may be wider than the terminals it is shown in.
#[test]
fn the_too_small_fallback_is_not_itself_truncated() {
    for columns in 20..MIN_COLUMNS {
        let model = update(shell(), Message::Resized(TerminalSize::new(columns, 5)));
        let screen = draw(&model, columns, 5);

        assert!(screen.contains("too small"), "at {columns} columns");
        assert!(
            screen.contains(&format!("needs {MIN_COLUMNS}x{MIN_ROWS}")),
            "at {columns} columns"
        );
    }
}

/// Rendering must not panic or produce an invalid layout at any size, from a
/// single cell up, in either section and with the overlay open or closed.
#[test]
fn every_size_from_one_cell_up_renders_without_panicking() {
    for columns in [
        1u16,
        2,
        10,
        MIN_COLUMNS - 1,
        MIN_COLUMNS,
        MIN_COLUMNS + 1,
        200,
    ] {
        for rows in [1u16, 2, 5, MIN_ROWS - 1, MIN_ROWS, MIN_ROWS + 1, 60] {
            for section in Section::ALL {
                for help in [false, true] {
                    let mut model = fold(
                        shell(),
                        &[
                            Message::SelectSection(section),
                            Message::Resized(TerminalSize::new(columns, rows)),
                        ],
                    );
                    if help {
                        model = update(model, Message::ToggleHelp);
                    }
                    let _ = draw(&model, columns, rows);
                }
            }
        }
    }
}

#[test]
fn growing_back_above_the_minimum_restores_the_full_shell() {
    let model = fold(
        shell(),
        &[
            Message::SelectSection(Section::Members),
            Message::Resized(TerminalSize::new(20, 5)),
            Message::Resized(TerminalSize::new(100, 30)),
        ],
    );
    let screen = draw(&model, 100, 30);

    assert!(!screen.contains("too small"));
    assert!(screen.any_line_contains(view::APPLICATION_NAME));
    assert!(screen.contains("No member data"));
}

// ── Rendering is a projection ────────────────────────────────────────────────

/// Drawing must not change anything the next message will see, and must be
/// reproducible.
#[test]
fn rendering_is_repeatable_and_changes_nothing() {
    let model = fold(shell(), &[Message::SelectSection(Section::Members)]);
    let before = model.clone();

    let first = draw(&model, 100, 30);
    let second = draw(&model, 100, 30);

    assert_eq!(model, before);
    assert_eq!(first.lines, second.lines);
}
