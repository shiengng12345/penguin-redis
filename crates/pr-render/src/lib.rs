//! Penguin Redis — `pr-render` (v2.1 §31.3).
//!
//! The Table design system and colour tokens. This layer has no network access (ADR-006):
//! it renders what it is given and asks the application for anything more.

pub mod theme;

pub use theme::{ColorDepth, Rgb, Styled, Theme, ThemeKind, Token};
