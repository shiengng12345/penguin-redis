//! Penguin Redis — `pr-render` (v2.1 §31.3).
//!
//! The Table design system and colour tokens. This layer has no network access (ADR-006):
//! it renders what it is given and asks the application for anything more.

pub mod generic;
pub mod table;
pub mod theme;
pub mod width;

pub use generic::{MAX_RENDER_DEPTH, render as render_generic};
pub use table::{Align, Cell, Column, DrawMode, Layout, Rendered, SAMPLE_ROWS, Table};
pub use theme::{ColorDepth, Rgb, Styled, Theme, ThemeKind, Token};
pub use width::{AmbiguousWidth, WidthPolicy};
