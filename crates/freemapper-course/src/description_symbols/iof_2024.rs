//! From a Purple Pen symbol id to the number in the IOF Control Descriptions 2024.

/// The IOF 2024 number of the bend (column F).
pub const BEND_2024: &str = "11.1";

/// The eight directions Purple Pen appends to the ids of direction symbols (`11.1NE`).
pub const DIRECTIONS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];

/// A Purple Pen symbol id mapped to IOF Control Descriptions 2024.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mapped {
    /// The 2024 number.
    pub id: String,
    /// The Purple Pen variant "objects of the same kind" (`10.1single`, `10.2single`, `11.15single`).
    pub single: bool,
}

/// The IOF Control Descriptions 2024 number of a Purple Pen symbol id, or `None` when 2024 has no such
/// symbol. Purple Pen stores the 2004 numbers (its `symbols.xml`: "Uses the 2004 ids only") with suffixes
/// for direction (`11.1NE`) and variant (`9.1high`, `10.1single`). Sections 0 to 10 kept their numbers in
/// 2018 and 2024, except that the four symbols Purple Pen numbers 5.23 to 5.26 (it has no 5.21 and 5.22)
/// are 5.21 to 5.24 in 2024; the bend (2004 11.7, column G) is 11.1 in column F; the locations of column G
/// moved from section 11 to 12, column H from 12 to 13, special instructions from 13 to 15 and the finish
/// rows from 14 to 16. 2004 12.3 (radio control) and 13.6 (map issue point) have no 2024 number.
///
/// The meaning of an id is the same in the 2004 and the 2018 definitions of Purple Pen (only 1.10 differs
/// in its drawing, 11.7 in its column), so the function takes no standard.
///
/// Source: International Specification for Control Descriptions 2024 (IOF), compared with Purple Pen's
/// `symbols.xml` at `201417f9`.
pub fn iof_2024(id: &str) -> Option<Mapped> {
    let split = id
        .char_indices()
        .find(|&(_, ch)| !(ch.is_ascii_digit() || ch == '.'))
        .map_or(id.len(), |(i, _)| i);
    let (base, suffix) = id.split_at(split);
    let (section, item) = base.split_once('.')?;
    let section: u32 = section.parse().ok()?;
    let item: u32 = item.parse().ok()?;
    let direction = DIRECTIONS.contains(&suffix);
    let plain = suffix.is_empty();
    let to = |s: u32, i: u32| format!("{s}.{i}");
    let new = match (section, item) {
        (0, 1 | 2) if direction => format!("{base}{suffix}"),
        (0, 3..=5) if plain => base.to_owned(),
        (1, 1..=16)
        | (2, 1..=10)
        | (3, 1..=11)
        | (4, 1..=10)
        | (5, 1..=20)
        | (6, 1 | 2)
        | (8, 1..=11)
            if plain =>
        {
            base.to_owned()
        }
        (5, 23..=26) if plain => to(5, item - 2),
        (9, 1 | 3) if plain || suffix == "high" || suffix == "deep" => base.to_owned(),
        (9, 2 | 4) if plain => base.to_owned(),
        (10, 1 | 2) if plain || suffix == "single" => base.to_owned(),
        (11, 7) if plain => BEND_2024.to_owned(),
        (11, 1..=6) if direction => format!("12.{item}{suffix}"),
        (11, 8) if direction => format!("12.7{suffix}"),
        (11, 14) if direction => format!("12.12{suffix}"),
        (11, 9) if plain => "12.8".to_owned(),
        (11, 10) if plain => "12.9".to_owned(),
        (11, 11) if plain => "12.10".to_owned(),
        (11, 12) if plain => "12.13".to_owned(),
        (11, 13) if plain => "12.11".to_owned(),
        (11, 15) if plain || suffix == "single" => "12.14".to_owned(),
        (12, 1 | 2) if plain => to(13, item),
        (12, 4) if plain => "13.3".to_owned(),
        (13, 1..=5) if plain => to(15, item),
        (13, 5) if suffix == "control" => "15.5".to_owned(),
        (15, 6) if plain => "15.6".to_owned(),
        (14, 1..=3) if plain => to(16, item),
        _ => return None,
    };
    Some(Mapped {
        id: new,
        single: suffix == "single",
    })
}
