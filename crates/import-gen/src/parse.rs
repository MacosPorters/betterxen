// SPDX-License-Identifier: GPL-3.0-or-later

//! CSV parser for the `missing_imports.csv` produced by `xenora probe`.
//!
//! Format: `"module","symbol_or_ordinal","iat_rva"`
//! An optional `import_kind` column distinguishes numeric ordinals from names.

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ImportRecord {
    /// DLL name in original casing, e.g. `"api-ms-win-crt-math-l1-1-0.dll"`
    pub module: String,
    /// Symbol name or `Ordinal#N`
    pub symbol: String,
    pub ordinal: Option<u16>,
    /// IAT RVA (informational only, not used for generation)
    pub iat_rva: u64,
}

/// Parse a CSV produced by `xenora probe`.
/// Returns records sorted by (module_lc, symbol).
pub fn parse_csv(path: &str) -> Result<Vec<ImportRecord>, String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let mut records = Vec::new();
    let mut kind_column = None;
    for (line_no, line) in content.lines().enumerate() {
        // Skip header (with or without quotes)
        let trimmed = line.trim();
        if line_no == 0 && (trimmed.starts_with("module") || trimmed.starts_with("\"module\"")) {
            kind_column = split_csv_line(trimmed)
                .iter()
                .position(|column| unquote(column) == "import_kind");
            continue;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let cols: Vec<&str> = split_csv_line(line);
        if cols.len() < 2 {
            continue;
        }
        let module = unquote(cols[0]).to_string();
        let symbol = unquote(cols[1]).to_string();
        let ordinal = match kind_column {
            None => None,
            Some(index) => match cols.get(index).map(|field| unquote(field)) {
                Some("name") => None,
                Some("ordinal") => Some(
                    symbol
                        .strip_prefix("Ordinal#")
                        .and_then(|value| value.parse().ok())
                        .ok_or_else(|| format!("{path}:{}: invalid ordinal", line_no + 1))?,
                ),
                _ => return Err(format!("{path}:{}: invalid import_kind", line_no + 1)),
            },
        };
        let iat_rva = if cols.len() >= 3 {
            u64::from_str_radix(unquote(cols[2]).trim_start_matches("0x"), 16).unwrap_or(0)
        } else {
            0
        };
        if !module.is_empty() && !symbol.is_empty() {
            records.push(ImportRecord {
                module,
                symbol,
                ordinal,
                iat_rva,
            });
        }
    }
    records.sort();
    records.dedup();
    Ok(records)
}

fn unquote(s: &str) -> &str {
    let s = s.trim();
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

/// Minimal CSV line splitter that handles quoted fields containing commas.
fn split_csv_line(line: &str) -> Vec<&str> {
    let mut fields = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            // Find closing quote
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' {
                i += 1;
            }
            if i < bytes.len() {
                i += 1; // skip closing quote
            }
            fields.push(&line[start..i]);
            // skip comma
            if i < bytes.len() && bytes[i] == b',' {
                i += 1;
            }
        } else {
            let start = i;
            while i < bytes.len() && bytes[i] != b',' {
                i += 1;
            }
            fields.push(&line[start..i]);
            if i < bytes.len() {
                i += 1;
            }
        }
    }
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_basic() {
        let cols = split_csv_line(r#""foo.dll","bar",0x1234"#);
        assert_eq!(cols, vec![r#""foo.dll""#, r#""bar""#, "0x1234"]);
    }

    #[test]
    fn unquote_strips() {
        assert_eq!(unquote(r#""hello""#), "hello");
        assert_eq!(unquote("world"), "world");
    }
}
