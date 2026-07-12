# rsomics-tsv-filter

Filter CSV/TSV rows by an arithmetic condition on selected fields — a
value-exact Rust port of
[`csvtk filter`](https://github.com/shenwei356/csvtk) v0.37.0 (MIT).

Output is **byte-identical** to csvtk, including field re-quoting, the
`row` line-number column, fuzzy-field expansion order, and the exact
non-numeric / `--any` short-circuit behaviour.

## Usage

```
rsomics-tsv-filter [OPTIONS] -f '<fields><op><value>' [input.csv|-]
```

- `-f, --filter` — the condition. `<fields>` is a 1-based index, a range
  (`2-3`, `2-`, `-1`), or a column name; multiple with commas (`1,3`).
  `<op>` is one of `>` `<` `=` `>=` `<=` `!=` (with `<>` as an alias for
  `!=`). Note `=` is equality — `==` is **rejected**, matching csvtk.
  `<value>` is a number (thousands separators in a field value are
  stripped, e.g. `1,000` → `1000`).
- `--any` — a row passes if **any** selected field satisfies the
  condition (default: **all** must).
- `-F, --fuzzy-fields` — treat colnames as globs (`*`) matched against
  the header, e.g. `-F -f 'c*>=0'`.
- `-n, --line-number` — prepend a 1-based line-number column named `row`
  (counts input **data** rows; the numbers are preserved, not renumbered).
- `-d/-D` in/out delimiter, `--tabs`/`-T` TSV modes, `-C` comment char
  (empty disables), `-H` no header, `-U` drop header on output,
  `-E` skip empty rows, `-I` skip ragged rows, `-l` lazy quotes.

Only the selected fields drive the pass/fail decision — the **whole**
matching row is emitted, never a projection.

```sh
rsomics-tsv-filter -f 'age>12' in.csv              # numeric threshold on a named column
rsomics-tsv-filter --any -f '2,3<=100' in.csv      # column 2 or 3 within bound
rsomics-tsv-filter -F -f 'c*!=0' in.csv            # every c-prefixed column non-zero
rsomics-tsv-filter --tabs -n -f 'depth>=30' in.tsv # TSV, prepend a row number
```

## Fidelity notes

- **Non-numeric values.** csvtk tests each selected value against
  `reDigitals`; the first value that fails rejects the whole row (and
  under `--any`, a non-numeric value seen *before* a satisfying one still
  rejects — a load-bearing ordering quirk this port reproduces).
- **`reDigitals` vs float parse.** A value can look numeric to
  `reDigitals` yet fail `strconv.ParseFloat` (e.g. `1.2.3`); csvtk treats
  that as a fatal error, so we fail loud too.
- **Equality is `=`.** `==` is an invalid expression in csvtk.

### Known edge divergence

Go's `strconv.ParseFloat` returns `ErrRange` (fatal in csvtk) for
decimal literals that **overflow** to ±Inf or **underflow** to 0. We
reproduce the overflow case (an infinite parse result is treated as an
error) but not the subnormal-underflow case (e.g. a threshold like
`1e-400` parses to `0.0` here instead of erroring). Such literals do not
occur in real tabular data.

Go treats CSV fields as opaque byte sequences, so csvtk copies an invalid
UTF-8 byte through unchanged. This port parses fields as UTF-8 (`String`),
so a field containing invalid UTF-8 fails loud rather than passing the
byte through. This is the project's fail-loud stance on malformed input;
well-formed (valid UTF-8) input is byte-exact.

## Origin

Goldens under `tests/golden/*.out` were frozen from `csvtk filter`
v0.37.0, e.g.:

```sh
csvtk filter -f "age>26"          people.csv    # age_gt.out
csvtk filter -f "2>26"            people.csv    # idx_gt.out
csvtk filter -f "score>=88"       people.csv    # score_ge.out
csvtk filter -f "age!=30"         people.csv    # age_ne.out
csvtk filter -f "age<=30"         people.csv    # age_ldeq.out
csvtk filter -f "age=30"          people.csv    # age_eq.out
csvtk filter -f "age<>30"         people.csv    # age_ltgt.out
csvtk filter -f "2-3>20"          people.csv    # range_all.out
csvtk filter --any -f "age,score>90" people.csv # any_score.out
csvtk filter -f "name>0"          people.csv    # nonnum.out
csvtk filter -n -f "age>26"       people.csv    # linenum.out
csvtk filter -U -f "age>26"       people.csv    # delhdr.out
csvtk filter -H -f "2>26"         nohdr.csv      # nohdr.out
csvtk filter -H -n -f "2>26"      nohdr.csv      # nohdr_ln.out
csvtk filter -t -f "age>26"       people.tsv     # tsv_gt.out
csvtk filter -d $'\t' -D $'\t' -f "age>26" people.tsv  # tsv_dD.out
csvtk filter -f "val>100"         quoted.csv     # quoted.out
csvtk filter -F -f "c*>8"         cols.csv       # fuzzy_cols.out
csvtk filter -F --any -f "c*>25"  cols.csv       # fuzzy_any.out
csvtk filter -n -f "age>26"       cmt.csv        # cmt_ln.out
csvtk filter -I -f "1>0"          ragged.csv     # ragged_ign.out
csvtk filter -l -f "1>0"          barequote.csv  # barequote_lz.out
```

This crate is an independent Rust reimplementation of `csvtk filter`,
which is MIT-licensed. Its `filter` command source (semantics, the
`reFilter`/`reDigitals` regexes, `removeComma`, and the CSV reader's
field-selection and fuzzy-field logic) was read and matched directly.

Go's `encoding/csv` writer quoting and strict-parse validation come from
the shared `rsomics-csvio` crate.

License: MIT OR Apache-2.0.
Upstream credit: [csvtk](https://github.com/shenwei356/csvtk) (MIT).
