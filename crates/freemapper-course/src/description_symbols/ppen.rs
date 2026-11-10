//! From an IOF 2024 number back to the symbol id Purple Pen stores (the inverse of [`iof_2024`]).

use std::collections::BTreeMap;
use std::sync::OnceLock;

use super::db::{Standard, db, exists, symbol_kind};
use super::iof_2024::{BEND_2024, iof_2024};

/// Why a 2024 symbol has no Purple Pen id to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Purple Pen has no symbol for it (2024 5.25 and 5.26, 13.9, an unknown number).
    NoPurplePenId,
    /// Purple Pen has the symbol, but not in this standard (2.10, 15.6 and Purple Pen's 5.25 and 5.26 are
    /// 2018 symbols).
    NotInStandard(Standard),
    /// Purple Pen has the symbol, but its description popup does not offer it in this column; the bend of a
    /// 2004 file belongs in column G, and a column D symbol does not go in column H.
    WrongColumn,
}

/// The Purple Pen id of a 2024 symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inverse {
    /// The id to write.
    pub id: String,
    /// Purple Pen has several ids for the 2024 symbol and the writer picked one: "height or depth" (9.1) and
    /// "height on slope" (9.3) are `9.1high` and `9.3high` here, while Purple Pen also has `…deep`.
    pub chosen: bool,
}

/// What `ppen_columns` did not write as given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnNote {
    /// The symbol in `column` (`'C'` to `'H'`) was left out.
    Refused {
        column: char,
        id: String,
        reason: Refusal,
    },
    /// `written` stands for `id` in `column`, which could also be another Purple Pen id.
    VariantChosen {
        column: char,
        id: String,
        written: String,
    },
}

/// The Purple Pen ids for columns C to H, and what was left out or chosen on the way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpenColumns {
    pub columns: [Option<String>; 6],
    pub notes: Vec<ColumnNote>,
}

/// Order in which Purple Pen ids compete for one 2024 number: the plain id first, then the direction forms,
/// then the variants.
fn rank(id: &str) -> u8 {
    let suffix = id.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.');
    match suffix {
        "" => 0,
        "high" => 2,
        "deep" => 3,
        "single" => 4,
        "control" => 5,
        _ => 1,
    }
}

/// The Purple Pen id of each 2024 number the forward mapping can produce, built from the database. Where two
/// Purple Pen ids give one number (`10.1` and `10.1single`, `9.1high` and `9.1deep`), the lowest `rank` wins.
fn table() -> &'static BTreeMap<String, String> {
    static TABLE: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut ids: Vec<&str> = db()
            .map(|db| db.entries.iter().map(|e| e.id.as_str()).collect())
            .unwrap_or_default();
        ids.retain(|id| id.starts_with(|c: char| c.is_ascii_digit()));
        ids.sort_by_key(|id| (rank(id), *id));
        ids.dedup();
        let mut table = BTreeMap::new();
        for id in ids {
            if let Some(mapped) = iof_2024(id) {
                table.entry(mapped.id).or_insert_with(|| id.to_owned());
            }
        }
        table
    })
}

/// 2024 numbers `0.1` and `0.2` are the cardinal and the diagonal form of one symbol ("which of similar
/// features"); Purple Pen has `0.1N`, `0.1E`, `0.1S`, `0.1W` and `0.2NE`, `0.2SE`, `0.2SW`, `0.2NW`.
fn normalise_direction(id: &str) -> String {
    let cardinal = |d: &str| matches!(d, "N" | "E" | "S" | "W");
    if let Some(d) = id.strip_prefix("0.1")
        && !d.is_empty()
        && !cardinal(d)
    {
        return format!("0.2{d}");
    }
    if let Some(d) = id.strip_prefix("0.2")
        && cardinal(d)
    {
        return format!("0.1{d}");
    }
    id.to_owned()
}

/// The Purple Pen id of the IOF 2024 symbol `iof_2024_id` in a file of `standard` (`Iof2024` counts as 2018,
/// as Purple Pen knows only two standards).
pub fn ppen_symbol(iof_2024_id: &str, standard: Standard) -> Result<Inverse, Refusal> {
    let key = normalise_direction(iof_2024_id);
    let id = table().get(&key).ok_or(Refusal::NoPurplePenId)?;
    if !exists(id, standard) {
        return Err(Refusal::NotInStandard(standard));
    }
    Ok(Inverse {
        id: id.clone(),
        chosen: id.ends_with("high"),
    })
}

/// Whether Purple Pen offers a symbol of `kind` in column `index` (0 is C). Column E takes D symbols too.
/// Column C also takes the instruction rows (`V` to `Z`), which a finish, a crossing point or a map issue
/// point keeps in its one box.
fn accepts(index: usize, kind: char) -> bool {
    match index {
        0 => kind == 'C' || ('V'..='Z').contains(&kind),
        1 => kind == 'D',
        2 => kind == 'E' || kind == 'D',
        3 => kind == 'F',
        4 => kind == 'G',
        _ => kind == 'H',
    }
}

/// Purple Pen's combinations for "the feature in D is the feature in E": the variant `single` of the
/// combination in column F. 2024 `12.14` ("between") has no such writing, see [`ppen_columns`].
const SINGLE_CAPABLE: [&str; 2] = ["10.1", "10.2"];

/// The Purple Pen ids for columns C to H of a description given in 2024 numbers.
///
/// Rules that run the import backwards. In a 2004 file the bend (2024 `11.1`, column F) goes to column G as
/// `11.7` when G is empty, as 2004 has the bend in G; from 2018 on it stays in F. When D equals E and F holds
/// `10.1` or `10.2`, D keeps the feature, E stays empty and F gets `10.1single` or `10.2single`. For `12.14`
/// (Purple Pen `11.15`) both columns stay and G gets the plain id, because `11.15single` in column G makes
/// Purple Pen throw. A symbol that has no id, that the standard lacks or that does not fit the column is left
/// out and named in `notes`.
pub fn ppen_columns(columns: &[Option<String>; 6], standard: Standard) -> PpenColumns {
    let mut out: [Option<String>; 6] = Default::default();
    let mut notes = Vec::new();
    for (i, id) in columns.iter().enumerate() {
        let Some(id) = id else { continue };
        let column = char::from(b'C' + i as u8);
        let target =
            if standard == Standard::Iof2004 && i == 3 && id == BEND_2024 && columns[4].is_none() {
                4
            } else {
                i
            };
        let inverse =
            ppen_symbol(id, standard).and_then(|inv| match symbol_kind(&inv.id, standard) {
                Some(kind) if accepts(target, kind) => Ok(inv),
                _ => Err(Refusal::WrongColumn),
            });
        match inverse {
            Ok(inv) => {
                if inv.chosen {
                    notes.push(ColumnNote::VariantChosen {
                        column,
                        id: id.clone(),
                        written: inv.id.clone(),
                    });
                }
                out[target] = Some(inv.id);
            }
            Err(reason) => notes.push(ColumnNote::Refused {
                column,
                id: id.clone(),
                reason,
            }),
        }
    }
    if columns[1].is_some()
        && columns[1] == columns[2]
        && columns[3]
            .as_deref()
            .is_some_and(|f| SINGLE_CAPABLE.contains(&f))
        && out[1].is_some()
        && let Some(f) = out[3].as_deref()
    {
        let single = format!("{f}single");
        if exists(&single, standard) {
            out[2] = None;
            out[3] = Some(single);
        }
    }
    PpenColumns {
        columns: out,
        notes,
    }
}
