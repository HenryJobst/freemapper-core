//! The IOF control description symbols and their German and English texts, from Purple Pen's `symbols.xml`.
//! See `db` for the file format and the reader.

mod db;
mod iof_2024;
mod ppen;
#[cfg(test)]
mod tests;

pub use db::{
    Circle, FLATNESS, MAX_DEPTH, MAX_POINTS_PER_SYMBOL, Standard, Stroke, SymbolGeometry, TextLang,
    exists, ids, symbol, symbol_kind, symbol_text,
};
pub use iof_2024::{BEND_2024, DIRECTIONS, Mapped, iof_2024};
pub use ppen::{ColumnNote, Inverse, PpenColumns, Refusal, ppen_columns, ppen_symbol};
