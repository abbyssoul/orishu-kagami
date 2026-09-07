//! The shell's application state.
//!
//! Everything here is a plain value: no terminal handle, no client, no IO. A
//! [`Model`] can be built, folded through [`crate::update::update`], and handed
//! to [`crate::view::render`] in a test that never opens a terminal.
//!
//! The types are app-local presentation state. They are never serialized, and
//! they deliberately do not mirror any Orishu client or domain model — this
//! slice has no worker integration to project.

/// A top-level section of the shell.
///
/// The names describe durable operator concepts rather than wire resources, so
/// adopting a real membership projection later does not rename the navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Section {
    /// Cluster-wide summary.
    #[default]
    Overview,
    /// The nodes making up the cluster formation.
    Members,
}

impl Section {
    /// Every section, in the order the header presents them.
    pub const ALL: [Section; 2] = [Section::Overview, Section::Members];

    /// The section's title, as shown in the header and panel borders.
    pub const fn title(self) -> &'static str {
        match self {
            Section::Overview => "Overview",
            Section::Members => "Members",
        }
    }

    /// The section after this one, wrapping at the end.
    pub const fn next(self) -> Self {
        match self {
            Section::Overview => Section::Members,
            Section::Members => Section::Overview,
        }
    }

    /// The section before this one, wrapping at the start.
    pub const fn previous(self) -> Self {
        match self {
            Section::Overview => Section::Members,
            Section::Members => Section::Overview,
        }
    }
}

/// A clamped index into a list of `len` rows.
///
/// Movement is expressed here, once, so no caller has to remember to bound a
/// `+ 1` against the list it is indexing. Every constructor and mutator leaves
/// the index at `0` when the list is empty, which is what makes an empty
/// section — the only kind this slice has — navigate without a special case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Selection {
    index: usize,
}

impl Selection {
    /// The first row.
    pub const fn first() -> Self {
        Self { index: 0 }
    }

    /// The selected row, or `None` when the list is empty.
    ///
    /// Callers index with this rather than with [`Selection::index`], so a
    /// stale selection left over from a longer list cannot address a row that
    /// is not there.
    pub fn resolved(self, len: usize) -> Option<usize> {
        (len > 0).then(|| self.index.min(len - 1))
    }

    /// The raw index. Prefer [`Selection::resolved`] when indexing.
    pub const fn index(self) -> usize {
        self.index
    }

    /// Move one row towards the start, stopping at the first row.
    #[must_use]
    pub fn move_up(self, len: usize) -> Self {
        match self.resolved(len) {
            Some(current) => Self {
                index: current.saturating_sub(1),
            },
            None => Self::first(),
        }
    }

    /// Move one row towards the end, stopping at the last row.
    #[must_use]
    pub fn move_down(self, len: usize) -> Self {
        match self.resolved(len) {
            Some(current) => Self {
                index: (current + 1).min(len - 1),
            },
            None => Self::first(),
        }
    }
}

/// The narrowest terminal the full shell is laid out for.
pub const MIN_COLUMNS: u16 = 56;

/// The shortest terminal the full shell is laid out for.
pub const MIN_ROWS: u16 = 14;

/// A terminal's size in character cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TerminalSize {
    /// Width in columns.
    pub columns: u16,
    /// Height in rows.
    pub rows: u16,
}

impl TerminalSize {
    /// A size of `columns` by `rows` cells.
    pub const fn new(columns: u16, rows: u16) -> Self {
        Self { columns, rows }
    }

    /// Whether the full shell fits, or only the compact fallback does.
    pub const fn class(self) -> SizeClass {
        if self.columns >= MIN_COLUMNS && self.rows >= MIN_ROWS {
            SizeClass::Usable
        } else {
            SizeClass::TooSmall
        }
    }
}

/// Whether the terminal has room for the shell's panels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SizeClass {
    /// Big enough for the header, body, status, and footer.
    #[default]
    Usable,
    /// Too small to lay out; the view draws a compact explanation instead.
    TooSmall,
}

/// Whether this build can consume live worker data.
///
/// One variant, because there is exactly one honest answer in this slice. It
/// exists as a named model value so the view states the limitation from the
/// model rather than hard-coding it in a panel, and so the follow-up
/// live-integration task extends a type that already reaches the view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Integration {
    /// No worker client is compiled into this build. Nothing has been
    /// requested from a worker, and no connection has been attempted.
    #[default]
    NotIncluded,
}

impl Integration {
    /// A short operator-facing sentence for the status area.
    pub const fn summary(self) -> &'static str {
        match self {
            Integration::NotIncluded => "shell only — no worker integration in this build",
        }
    }

    /// The full explanation a section's empty state shows.
    ///
    /// Worded so it cannot be read as a failed connection or a rejected
    /// credential: neither has been attempted.
    pub const fn explanation(self) -> &'static str {
        match self {
            Integration::NotIncluded => {
                "Live worker integration is not part of this build. No connection has been \
                 attempted and no cluster data has been requested or received."
            }
        }
    }
}

/// Whether the operator has asked the shell to exit.
///
/// This is the only effect the shell performs, so it is named in the model
/// rather than returned through an effect queue that would have nothing else
/// to carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExitIntent {
    /// Keep running.
    #[default]
    Running,
    /// Leave the event loop and restore the terminal.
    Requested,
}

impl ExitIntent {
    /// Whether the event loop should stop.
    pub const fn is_requested(self) -> bool {
        matches!(self, ExitIntent::Requested)
    }
}

/// The complete state of the shell.
///
/// Compared for equality by the runner to decide whether a fold changed
/// anything worth redrawing, so every field must be part of what the view can
/// show.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Model {
    section: Section,
    selection: Selection,
    help_visible: bool,
    help_scroll: Selection,
    size: TerminalSize,
    integration: Integration,
    exit: ExitIntent,
}

impl Model {
    /// A shell showing Overview at `size`, with no help overlay open.
    pub fn new(size: TerminalSize) -> Self {
        Self {
            section: Section::default(),
            selection: Selection::first(),
            help_visible: false,
            help_scroll: Selection::first(),
            size,
            integration: Integration::default(),
            exit: ExitIntent::default(),
        }
    }

    /// The section the body is showing.
    pub const fn section(&self) -> Section {
        self.section
    }

    /// The selected row within the active section.
    pub const fn selection(&self) -> Selection {
        self.selection
    }

    /// Whether the help overlay is open.
    pub const fn help_visible(&self) -> bool {
        self.help_visible
    }

    /// The help overlay's scroll position.
    pub const fn help_scroll(&self) -> Selection {
        self.help_scroll
    }

    /// The last size the terminal reported.
    pub const fn size(&self) -> TerminalSize {
        self.size
    }

    /// Whether the full shell fits in the current terminal.
    pub const fn size_class(&self) -> SizeClass {
        self.size.class()
    }

    /// What this build can say about live worker data.
    pub const fn integration(&self) -> Integration {
        self.integration
    }

    /// Whether the operator has asked to exit.
    pub const fn exit_intent(&self) -> ExitIntent {
        self.exit
    }

    /// The rows the active section has to show.
    ///
    /// Zero throughout this slice: with no worker integration there is nothing
    /// to list, and inventing rows would present fixture data as cluster
    /// state. Navigation is still driven through it so the follow-up task
    /// changes what this returns rather than how selection works.
    pub const fn section_row_count(&self) -> usize {
        match self.section {
            Section::Overview | Section::Members => 0,
        }
    }

    // ── Transitions, used by `update` ────────────────────────────────────────

    pub(crate) fn with_section(mut self, section: Section) -> Self {
        if section != self.section {
            self.section = section;
            self.selection = Selection::first();
        }
        self
    }

    pub(crate) fn with_selection(mut self, selection: Selection) -> Self {
        self.selection = selection;
        self
    }

    pub(crate) fn with_help_visible(mut self, visible: bool) -> Self {
        self.help_visible = visible;
        if !visible {
            self.help_scroll = Selection::first();
        }
        self
    }

    pub(crate) fn with_help_scroll(mut self, scroll: Selection) -> Self {
        self.help_scroll = scroll;
        self
    }

    pub(crate) fn with_size(mut self, size: TerminalSize) -> Self {
        self.size = size;
        self
    }

    pub(crate) fn requesting_exit(mut self) -> Self {
        self.exit = ExitIntent::Requested;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_cycle_in_both_directions() {
        assert_eq!(Section::Overview.next(), Section::Members);
        assert_eq!(Section::Members.next(), Section::Overview);
        assert_eq!(Section::Overview.previous(), Section::Members);
        assert_eq!(Section::Members.previous(), Section::Overview);
    }

    #[test]
    fn a_selection_never_leaves_the_list() {
        let mut selection = Selection::first();
        for _ in 0..10 {
            selection = selection.move_down(4);
        }
        assert_eq!(selection.resolved(4), Some(3));

        for _ in 0..10 {
            selection = selection.move_up(4);
        }
        assert_eq!(selection.resolved(4), Some(0));
    }

    #[test]
    fn a_selection_on_an_empty_list_resolves_to_nothing_and_cannot_move() {
        let selection = Selection::first();

        assert_eq!(selection.resolved(0), None);
        assert_eq!(selection.move_down(0).index(), 0);
        assert_eq!(selection.move_up(0).index(), 0);
    }

    /// A selection kept from a longer list must not address a row that is no
    /// longer there — the resolved index is what callers slice with.
    #[test]
    fn a_selection_left_over_from_a_longer_list_clamps_to_the_last_row() {
        let selection = Selection::first().move_down(9).move_down(9).move_down(9);

        assert_eq!(selection.resolved(9), Some(3));
        assert_eq!(selection.resolved(2), Some(1));
        assert_eq!(selection.move_down(2).resolved(2), Some(1));
        assert_eq!(selection.move_up(2).resolved(2), Some(0));
    }

    #[test]
    fn the_size_class_boundary_is_the_documented_minimum() {
        assert_eq!(
            TerminalSize::new(MIN_COLUMNS, MIN_ROWS).class(),
            SizeClass::Usable
        );
        assert_eq!(
            TerminalSize::new(MIN_COLUMNS - 1, MIN_ROWS).class(),
            SizeClass::TooSmall
        );
        assert_eq!(
            TerminalSize::new(MIN_COLUMNS, MIN_ROWS - 1).class(),
            SizeClass::TooSmall
        );
        assert_eq!(TerminalSize::new(0, 0).class(), SizeClass::TooSmall);
    }

    #[test]
    fn a_new_model_starts_on_overview_with_no_overlay_and_no_exit_intent() {
        let model = Model::new(TerminalSize::new(100, 30));

        assert_eq!(model.section(), Section::Overview);
        assert!(!model.help_visible());
        assert_eq!(model.exit_intent(), ExitIntent::Running);
        assert!(!model.exit_intent().is_requested());
        assert_eq!(model.integration(), Integration::NotIncluded);
        assert_eq!(model.section_row_count(), 0);
    }

    /// The default state must not claim a connection was tried and failed.
    #[test]
    fn the_integration_wording_does_not_imply_a_failed_connection() {
        let text = Integration::NotIncluded.explanation().to_lowercase();

        assert!(text.contains("not part of this build"));
        for forbidden in ["failed", "refused", "unauthorized", "disconnected", "error"] {
            assert!(!text.contains(forbidden), "{forbidden} in {text:?}");
        }
    }

    #[test]
    fn changing_section_resets_the_selection_but_re_selecting_the_same_one_does_not() {
        let model = Model::new(TerminalSize::new(100, 30))
            .with_selection(Selection::first().move_down(5))
            .with_section(Section::Overview);
        assert_eq!(model.selection().index(), 1);

        let moved = model.with_section(Section::Members);
        assert_eq!(moved.selection().index(), 0);
    }
}
