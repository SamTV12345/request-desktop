//! The scale. Every measurement in the app comes from here.
//!
//! Split out of `theme` because a token is not a colour: these are the numbers the
//! design system is built on, and they change far less often than the palette. A
//! value that has no token yet is named as a `const` next to its use with a comment
//! saying it is the design's own number — that is a signal it may belong here, not
//! a licence to inline it.
/// `--space-*`, verbatim. Not a 4-px grid — see the module docs.
pub mod space {
    use gpui::{Pixels, px};
    pub const S1: Pixels = px(4.4);
    pub const S2: Pixels = px(8.8);
    pub const S3: Pixels = px(13.2);
    pub const S4: Pixels = px(17.6);
    /// 5 × 4.4. Absent from `--space-*` but used by the mockups (the title
    /// bar's left inset), so it is a real step rather than a rounded 22.
    pub const S5: Pixels = px(22.);
    pub const S6: Pixels = px(26.4);
    pub const S8: Pixels = px(35.2);

    // Migration shim: the previous scale was Ant's XXS..XXL. Mapped onto the
    // nearest Organic step so views keep building; delete as each file moves.
    pub const XXS: Pixels = S1;
    pub const XS: Pixels = S2;
    pub const SM: Pixels = S3;
    pub const MD: Pixels = S4;
    pub const LG: Pixels = px(22.);
    pub const XL: Pixels = S6;
    pub const XXL: Pixels = S8;
}
/// `--radius-*`, plus the two overrides `styles.css` applies at the end.
pub mod radius {
    use gpui::{Pixels, px};
    pub const SM: Pixels = px(8.);
    pub const MD: Pixels = px(16.);
    pub const LG: Pixels = px(28.);
    /// Buttons, tags, segmented controls and inputs.
    pub const PILL: Pixels = px(999.);
    /// Cards and dialogs: `--radius-lg * 1.15`.
    pub const CARD: Pixels = px(32.2);

    // Migration shim. NOTE: `XS` was Ant's 2-px corner; Organic puts every
    // small control on a pill, so a view still using XS looks like the old
    // system even once its colours are right. This is the alias that must go
    // first, not last.
    pub const XS: Pixels = PILL;
}
/// Type sizes from the system's element rules.
pub mod text {
    use gpui::{Pixels, px};
    pub const H1: Pixels = px(42.);
    pub const H2: Pixels = px(32.);
    pub const H3: Pixels = px(25.);
    pub const H4: Pixels = px(20.);
    pub const H5: Pixels = px(16.);
    /// `h6`: uppercase and letter-spaced in the system — a section label.
    pub const H6: Pixels = px(13.);
    /// `body`: 15px / 1.55.
    pub const BODY: Pixels = px(15.);
    pub const BODY_LINE: Pixels = px(23.25);
    /// Buttons and inputs share 14px so they align side by side.
    pub const CONTROL: Pixels = px(14.);
    pub const CARD_TITLE: Pixels = px(17.);
    /// `.card-body`, `.dialog-body`, `.seg-opt`.
    pub const SMALL: Pixels = px(13.);
    /// `.field > label`.
    pub const LABEL: Pixels = px(12.);
    /// `.tag`, `.card-meta`, `figcaption`.
    pub const TINY: Pixels = px(11.);
    /// `.card-kicker`.
    pub const KICKER: Pixels = px(10.);

    // Migration shim: the previous set carried a `_LINE` partner per size and a
    // DISPLAY step. Organic states leading as a ratio (1.12 headings, 1.55
    // body), so these are those ratios applied.
    pub const H3_LINE: Pixels = px(28.);
    pub const H4_LINE: Pixels = px(22.4);
    pub const H5_LINE: Pixels = px(18.);
    pub const SMALL_LINE: Pixels = px(20.);
    pub const DISPLAY: Pixels = px(48.);
    pub const DISPLAY_LINE: Pixels = px(54.);
}
/// Fixed panel and column widths. App layout, not part of the design system.
pub mod size {
    use gpui::{Pixels, px};
    /// Icon rail. 104 in the mockups — was 176 before the design was read.
    pub const RAIL: Pixels = px(104.);
    /// A rail tile and its corner.
    pub const TILE_W: Pixels = px(76.);
    pub const TILE_H: Pixels = px(74.);
    pub const TILE_RADIUS: Pixels = px(24.);
    pub const RAIL_ICON: Pixels = px(22.);
    /// Custom title bar, its app mark, and its window controls.
    pub const TITLEBAR: Pixels = px(52.);
    pub const MARK: Pixels = px(22.);
    pub const WINDOW_CTRL_W: Pixels = px(40.);
    pub const WINDOW_CTRL_H: Pixels = px(32.);
    /// The Time screen's left column: a *starting* width, capped by the
    /// container. [`SIDEBAR`] doubles as its minimum.
    pub const COLUMN: Pixels = px(392.);

    // -- floors and starting sizes for flexible layout --
    // Measured, not guessed: a layout probe mounted each pane at 1280, 1000,
    // 760, 560, 420 and 300 px and read the real geometry back.

    /// Floor under a card or editor block. The old floors summed to more than a
    /// narrow pane had, which pushed siblings off screen.
    pub const CARD_MIN_H: Pixels = px(70.4);
    /// Minimum width of a URL or search field before its row is allowed to wrap.
    pub const URL_MIN: Pixels = px(140.8);
    /// Floor under a flexible table column. Without it the task column
    /// collapsed to zero at 760 px.
    pub const TABLE_COL_MIN_W: Pixels = px(70.);
    /// Starting size of a grid cell, with its shrink and growth bounds.
    pub const GRID_CELL: Pixels = px(246.4);
    pub const GRID_CELL_MIN: Pixels = px(140.8);
    pub const GRID_CELL_MAX: Pixels = px(316.8);
    pub const SIDEBAR: Pixels = px(264.);
    pub const SIDEBAR_WIDE: Pixels = px(316.);
    pub const METHOD: Pixels = px(62.);
    pub const LABEL: Pixels = px(158.);
    pub const TAB_TITLE: Pixels = px(140.);
}
/// Control geometry from `.input`, `.btn` and `.btn-icon`.
pub mod control {
    use gpui::{Pixels, px};
    /// `.input` and `.btn` minimum height.
    pub const MD: Pixels = px(36.);
    /// `.btn-icon` is a 36-px square.
    pub const ICON: Pixels = px(36.);

    // Migration shim: Organic states one control height, not three.
    pub const SM: Pixels = px(28.);
    pub const LG: Pixels = px(44.);
}
