use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use rsomics_common::{Result, RsomicsError};

static RE_INTEGERS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[\-\+\d]+$").unwrap());
static RE_INTEGER_RANGE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([\-\d]+?)\-([\-\d]*?)$").unwrap());

/// A parsed field spec (csvtk's `fieldStr`). csvtk decides index-mode vs
/// colname-mode from the first comma-separated part: all `[-+0-9]` means
/// indices, anything else means column names matched against the header.
pub enum FieldSpec {
    /// 1-based signed indices. `x2ends` records which entries were open ranges
    /// (`3-`) keyed by their position in `fields`, mirroring csvtk's `x2ends`.
    Index {
        fields: Vec<i64>,
        x2ends: HashMap<usize, i64>,
        negative: bool,
    },
    /// Column names (or globs under `-F`); `-name` entries mean exclusion.
    Names { names: Vec<String>, negative: bool },
}

impl FieldSpec {
    pub fn is_names(&self) -> bool {
        matches!(self, FieldSpec::Names { .. })
    }
}

pub fn parse(field_str: &str) -> Result<FieldSpec> {
    let parts: Vec<&str> = field_str.split(',').collect();
    if parts[0].is_empty() {
        return Err(bad(format!(
            "the first field should not be empty: {field_str}"
        )));
    }

    if RE_INTEGERS.is_match(parts[0]) {
        parse_index(&parts)
    } else {
        parse_names(&parts)
    }
}

fn parse_index(parts: &[&str]) -> Result<FieldSpec> {
    let mut fields: Vec<i64> = Vec::new();
    let mut x2ends: HashMap<usize, i64> = HashMap::new();
    let mut j = 0usize;

    for s in parts {
        if let Some(caps) = RE_INTEGER_RANGE.captures(s) {
            let start: i64 = caps[1]
                .parse()
                .map_err(|_| bad(format!("fail to parse field range: {}", &caps[1])))?;
            let end_str = &caps[2];
            if end_str.is_empty() {
                fields.push(start);
                x2ends.insert(j, start);
                continue;
            }
            let end: i64 = end_str
                .parse()
                .map_err(|_| bad(format!("fail to parse field range: {end_str}")))?;
            if start == 0 || end == 0 {
                return Err(bad(format!("no 0 allowed in field range: {s}")));
            }
            if start < 0 && end < 0 {
                let (lo, hi) = if start < end {
                    (start, end)
                } else {
                    (end, start)
                };
                for i in lo..=hi {
                    fields.push(i);
                    j += 1;
                }
            } else if start > 0 && end > 0 {
                if start >= end {
                    return Err(bad(format!(
                        "invalid field range: {s}. start ({start}) should be less than end ({end})"
                    )));
                }
                for i in start..=end {
                    fields.push(i);
                    j += 1;
                }
            } else {
                return Err(bad(format!(
                    "invalid field range: {s}. start ({start}) and end ({end}) should be both > 0 or < 0"
                )));
            }
        } else {
            let f: i64 = s
                .parse()
                .map_err(|_| bad(format!("failed to parse {s} as a field number")))?;
            fields.push(f);
            j += 1;
        }
    }

    let mut negative = false;
    for &f in &fields {
        if f == 0 {
            return Err(bad("field should not be 0".into()));
        } else if f < 0 {
            negative = true;
        } else if negative {
            return Err(bad(
                "fields should not be mixed with positive and negative fields".into(),
            ));
        }
    }
    if negative {
        for &f in &fields {
            if f > 0 {
                return Err(bad(
                    "fields should not be mixed with positive and negative fields".into(),
                ));
            }
        }
    }

    Ok(FieldSpec::Index {
        fields,
        x2ends,
        negative,
    })
}

fn parse_names(parts: &[&str]) -> Result<FieldSpec> {
    let mut negative = false;
    for (i, f) in parts.iter().enumerate() {
        if f.is_empty() {
            return Err(bad(format!("field #{} should not be empty", i + 1)));
        } else if f.starts_with('-') {
            negative = true;
        } else if negative {
            return Err(bad(
                "field should not be mixed with positive and negative fields".into(),
            ));
        }
    }
    if negative {
        for f in parts {
            if !f.starts_with('-') {
                return Err(bad(
                    "field should not be mixed with positive and negative fields".into(),
                ));
            }
        }
    }
    Ok(FieldSpec::Names {
        names: parts.iter().map(|s| (*s).to_string()).collect(),
        negative,
    })
}

/// Resolve the spec to concrete 1-based column positions against the first
/// record. `fuzzy` (csvtk `-F`) turns every colname into a `^…$` regex with
/// `*` → `.*?`, matched against the header; index specs ignore it.
pub fn resolve(spec: &FieldSpec, first: &[String], fuzzy: bool) -> Result<Vec<usize>> {
    match spec {
        FieldSpec::Index {
            fields,
            x2ends,
            negative,
        } => resolve_index(fields, x2ends, *negative, first.len()),
        FieldSpec::Names { names, negative } => {
            if fuzzy {
                resolve_names_fuzzy(names, *negative, first)
            } else {
                resolve_names(names, *negative, first)
            }
        }
    }
}

fn resolve_index(
    fields: &[i64],
    x2ends: &HashMap<usize, i64>,
    negative: bool,
    n: usize,
) -> Result<Vec<usize>> {
    let n = n as i64;

    let mut expanded: Vec<i64> = Vec::new();
    for (i, &f) in fields.iter().enumerate() {
        if x2ends.get(&i) == Some(&f) {
            if negative {
                let mut k = -f;
                while k <= n {
                    expanded.push(-k);
                    k += 1;
                }
            } else {
                let mut k = f;
                while k <= n {
                    expanded.push(k);
                    k += 1;
                }
            }
        } else {
            expanded.push(f);
        }
    }

    for &f in &expanded {
        if f > n {
            return Err(bad(format!("field ({f}) out of range ({n})")));
        }
    }

    if negative {
        let mut drop = std::collections::HashSet::new();
        for &f in &expanded {
            drop.insert(-f);
        }
        Ok((1..=n)
            .filter(|i| !drop.contains(i))
            .map(|i| i as usize)
            .collect())
    } else {
        Ok(expanded.iter().map(|&f| f as usize).collect())
    }
}

fn colname_index(header: &[String]) -> HashMap<&str, Vec<usize>> {
    let mut map: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, col) in header.iter().enumerate() {
        map.entry(col.as_str()).or_default().push(i + 1);
    }
    map
}

fn resolve_names(names: &[String], negative: bool, header: &[String]) -> Result<Vec<usize>> {
    let name2cols = colname_index(header);

    if negative {
        let mut exclude = std::collections::HashSet::new();
        for name in names {
            let bare = &name[1..];
            if !name2cols.contains_key(bare) {
                return Err(bad(format!("column \"{bare}\" not existed in file")));
            }
            exclude.insert(bare);
        }
        Ok(header
            .iter()
            .enumerate()
            .filter(|(_, col)| !exclude.contains(col.as_str()))
            .map(|(i, _)| i + 1)
            .collect())
    } else {
        let mut out = Vec::new();
        for name in names {
            match name2cols.get(name.as_str()) {
                Some(cols) => {
                    // filter sets DoNotAllowDuplicatedColumnName: an ambiguous
                    // (duplicated) header name fails loud.
                    if cols.len() > 1 {
                        return Err(bad(format!(
                            "the selected colname is duplicated in the input data: {name}"
                        )));
                    }
                    out.extend(cols.iter().copied());
                }
                None => return Err(bad(format!("column \"{name}\" not existed in file"))),
            }
        }
        Ok(out)
    }
}

/// csvtk's `-F` path (csv.go `fuzzyFields`): each pattern becomes `^pat$` with
/// `*`→`.*?`. Positive selection iterates patterns then header columns, so a
/// column can be matched by several patterns and appear multiple times (no
/// dedup — filter does not set UniqColumn). Negative keeps header columns that
/// no exclusion pattern matches. No existence check in fuzzy mode.
fn resolve_names_fuzzy(names: &[String], negative: bool, header: &[String]) -> Result<Vec<usize>> {
    let name2cols = colname_index(header);

    if negative {
        let mut res = Vec::with_capacity(names.len());
        for name in names {
            res.push(fuzzy_regex(&name[1..])?);
        }
        let mut out = Vec::new();
        for col in header {
            if !res.iter().any(|re| re.is_match(col)) {
                out.extend(name2cols[col.as_str()].iter().copied());
            }
        }
        Ok(out)
    } else {
        let mut out = Vec::new();
        for name in names {
            let re = fuzzy_regex(name)?;
            for col in header {
                if re.is_match(col) {
                    out.extend(name2cols[col.as_str()].iter().copied());
                }
            }
        }
        Ok(out)
    }
}

fn fuzzy_regex(pattern: &str) -> Result<Regex> {
    let body = pattern.replace('*', ".*?");
    Regex::new(&format!("^{body}$"))
        .map_err(|e| bad(format!("invalid fuzzy field \"{pattern}\": {e}")))
}

fn bad(msg: String) -> RsomicsError {
    RsomicsError::InvalidInput(msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cols(field_str: &str, header: &[&str]) -> Vec<usize> {
        let spec = parse(field_str).unwrap();
        let row: Vec<String> = header.iter().map(|s| (*s).to_string()).collect();
        resolve(&spec, &row, false).unwrap()
    }

    fn fcols(field_str: &str, header: &[&str]) -> Vec<usize> {
        let spec = parse(field_str).unwrap();
        let row: Vec<String> = header.iter().map(|s| (*s).to_string()).collect();
        resolve(&spec, &row, true).unwrap()
    }

    #[test]
    fn single_index() {
        assert_eq!(cols("2", &["a", "b", "c"]), vec![2]);
    }

    #[test]
    fn multi_index() {
        assert_eq!(cols("1,3", &["a", "b", "c"]), vec![1, 3]);
    }

    #[test]
    fn closed_range() {
        assert_eq!(cols("1-3", &["a", "b", "c", "d"]), vec![1, 2, 3]);
    }

    #[test]
    fn open_range() {
        assert_eq!(cols("2-", &["a", "b", "c", "d"]), vec![2, 3, 4]);
    }

    #[test]
    fn negative_index_excludes() {
        assert_eq!(cols("-2", &["a", "b", "c"]), vec![1, 3]);
    }

    #[test]
    fn colname_single() {
        assert_eq!(cols("name", &["id", "name", "val"]), vec![2]);
    }

    #[test]
    fn negative_colname_excludes() {
        assert_eq!(cols("-name", &["id", "name", "val"]), vec![1, 3]);
    }

    #[test]
    fn fuzzy_prefix_glob() {
        // c* matches c1,c2 in header order.
        assert_eq!(fcols("c*", &["id", "c1", "c2", "x"]), vec![2, 3]);
    }

    #[test]
    fn fuzzy_suffix_glob() {
        assert_eq!(
            fcols("*name", &["id", "myname", "surname", "x"]),
            vec![2, 3]
        );
    }

    #[test]
    fn fuzzy_no_match_is_empty() {
        assert_eq!(fcols("zzz*", &["a", "b"]), Vec::<usize>::new());
    }

    #[test]
    fn fuzzy_negative_excludes_matches() {
        assert_eq!(fcols("-c*", &["id", "c1", "c2", "x"]), vec![1, 4]);
    }

    #[test]
    fn missing_colname_errors() {
        let spec = parse("nope").unwrap();
        let row: Vec<String> = ["id", "name"].iter().map(|s| (*s).to_string()).collect();
        assert!(resolve(&spec, &row, false).is_err());
    }

    #[test]
    fn out_of_range_errors() {
        let spec = parse("5").unwrap();
        let row: Vec<String> = ["a", "b"].iter().map(|s| (*s).to_string()).collect();
        assert!(resolve(&spec, &row, false).is_err());
    }

    #[test]
    fn zero_field_errors() {
        assert!(parse("0").is_err());
    }

    #[test]
    fn mixed_sign_errors() {
        assert!(parse("1,-2").is_err());
    }
}
