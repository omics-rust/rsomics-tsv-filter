use std::borrow::Cow;
use std::sync::LazyLock;

use regex::Regex;
use rsomics_common::{Result, RsomicsError};

// csvtk `reFilter`: `^(.+?)([!<=>]+)([\-\d\.e,E\+]+)$`. `\d` is RE2/ASCII, so we
// spell the digit class `0-9` to avoid Rust's Unicode `\d`.
static RE_FILTER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(.+?)([!<=>]+)([\-0-9\.e,E\+]+)$").unwrap());
// csvtk `reDigitals`: a value "looks numeric" iff it matches this.
static RE_DIGITALS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[\-\+]?[0-9\.,]+$|^[\-\+]?[0-9\.,]+[eE][\-\+0-9]+$").unwrap());

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op {
    Gt,
    Lt,
    Eq,
    Ge,
    Le,
    Ne,
}

impl Op {
    fn test(self, v: f64, t: f64) -> bool {
        match self {
            Op::Gt => v > t,
            Op::Lt => v < t,
            Op::Eq => v == t,
            Op::Ge => v >= t,
            Op::Le => v <= t,
            Op::Ne => v != t,
        }
    }
}

pub struct Condition {
    pub field_str: String,
    pub op: Op,
    pub threshold: f64,
}

/// Parse `-f "<fields><op><value>"`. csvtk validates the operator against a
/// closed set (`=`/`<>` are the equality/inequality spellings — `==` is
/// rejected), and parses the value with Go's `strconv.ParseFloat`.
pub fn parse_condition(filter_str: &str) -> Result<Condition> {
    if filter_str.is_empty() {
        return Err(bad("flag -f (--filter) needed".into()));
    }
    let caps = RE_FILTER
        .captures(filter_str)
        .ok_or_else(|| bad(format!("invalid filter: {filter_str}")))?;
    let op = match &caps[2] {
        ">" => Op::Gt,
        "<" => Op::Lt,
        "=" => Op::Eq,
        ">=" => Op::Ge,
        "<=" => Op::Le,
        "!=" | "<>" => Op::Ne,
        other => return Err(bad(format!("invalid expression: {other}"))),
    };
    let threshold = parse_float(&caps[3])?;
    Ok(Condition {
        field_str: caps[1].to_string(),
        op,
        threshold,
    })
}

/// Decide whether a data row passes, mirroring csvtk's loop over the selected
/// values: the first non-numeric value (per `reDigitals`) rejects the row
/// outright; under `--any` the first satisfying value accepts it; otherwise the
/// row passes only when every selected value satisfies the condition.
pub fn row_passes(values: &[&str], cond: &Condition, any: bool) -> Result<bool> {
    let mut n = 0usize;
    let mut flag = false;
    for val in values {
        if !RE_DIGITALS.is_match(val) {
            flag = false;
            break;
        }
        // reDigitals can match strings ParseFloat rejects (e.g. `1.2.3`);
        // csvtk treats that parse failure as fatal, so we surface it too.
        let v = parse_float(&remove_comma(val))?;
        if cond.op.test(v, cond.threshold) {
            n += 1;
        }
        if any && n == 1 {
            flag = true;
            break;
        }
    }
    if n == values.len() {
        flag = true;
    }
    Ok(flag)
}

fn remove_comma(s: &str) -> Cow<'_, str> {
    if s.contains(',') {
        Cow::Owned(s.replace(',', ""))
    } else {
        Cow::Borrowed(s)
    }
}

/// Go's `strconv.ParseFloat(s, 64)`. The value char set here can't spell
/// `inf`/`nan`/hex, so the only Go-specific behaviour left to match is the
/// overflow `ErrRange` — Go returns ±Inf *with* an error, which csvtk treats as
/// fatal.
fn parse_float(s: &str) -> Result<f64> {
    let v: f64 = s
        .parse()
        .map_err(|_| bad(format!("strconv.ParseFloat: parsing {s:?}: invalid syntax")))?;
    if v.is_infinite() {
        return Err(bad(format!(
            "strconv.ParseFloat: parsing {s:?}: value out of range"
        )));
    }
    Ok(v)
}

fn bad(msg: String) -> RsomicsError {
    RsomicsError::InvalidInput(msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ops() {
        assert_eq!(parse_condition("age>12").unwrap().op, Op::Gt);
        assert_eq!(parse_condition("age>=12").unwrap().op, Op::Ge);
        assert_eq!(parse_condition("age<=12").unwrap().op, Op::Le);
        assert_eq!(parse_condition("age=12").unwrap().op, Op::Eq);
        assert_eq!(parse_condition("age!=12").unwrap().op, Op::Ne);
        assert_eq!(parse_condition("age<>12").unwrap().op, Op::Ne);
    }

    #[test]
    fn double_equals_rejected() {
        assert!(parse_condition("age==12").is_err());
    }

    #[test]
    fn empty_filter_rejected() {
        assert!(parse_condition("").is_err());
    }

    #[test]
    fn no_operator_rejected() {
        assert!(parse_condition("age").is_err());
    }

    #[test]
    fn comma_threshold_rejected() {
        assert!(parse_condition("age>1,000").is_err());
    }

    #[test]
    fn negative_and_scientific_threshold() {
        assert_eq!(parse_condition("x>-5").unwrap().threshold, -5.0);
        assert_eq!(parse_condition("x>1e3").unwrap().threshold, 1000.0);
    }

    #[test]
    fn all_must_satisfy_by_default() {
        let c = parse_condition("f>10").unwrap();
        assert!(row_passes(&["20", "30"], &c, false).unwrap());
        assert!(!row_passes(&["20", "5"], &c, false).unwrap());
    }

    #[test]
    fn any_accepts_on_first_hit() {
        let c = parse_condition("f>10").unwrap();
        assert!(row_passes(&["5", "50"], &c, true).unwrap());
        assert!(!row_passes(&["5", "8"], &c, true).unwrap());
    }

    #[test]
    fn non_numeric_rejects_row() {
        let c = parse_condition("f>10").unwrap();
        assert!(!row_passes(&["abc", "50"], &c, false).unwrap());
        // Even under --any, a non-numeric value seen before a hit rejects.
        assert!(!row_passes(&["abc", "50"], &c, true).unwrap());
    }

    #[test]
    fn comma_grouped_value_stripped() {
        let c = parse_condition("f>800").unwrap();
        assert!(row_passes(&["1,000"], &c, false).unwrap());
    }

    #[test]
    fn redigitals_match_but_unparseable_errors() {
        let c = parse_condition("f>1").unwrap();
        assert!(row_passes(&["1.2.3"], &c, false).is_err());
    }
}
