//! The IOF control description symbols and their German and English texts, from Purple Pen's `symbols.xml`.
//! See `db` for the file format and the reader.

mod db;
#[cfg(test)]
mod tests;

pub use db::{
    Circle, FLATNESS, MAX_DEPTH, MAX_POINTS_PER_SYMBOL, Standard, Stroke, SymbolGeometry, TextLang,
    exists, symbol, symbol_kind, symbol_text,
};
