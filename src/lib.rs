mod fields;
mod filter;

use std::io::{Read, Write};

use rsomics_common::{Result, RsomicsError};
use rsomics_csvio::{CsvWriter, check_strict, normalize_crlf};

pub struct FilterOptions {
    /// Raw `-f/--filter` condition, e.g. `age>12`, `1,3<=2`, `c*!=0`.
    pub filter_str: String,
    /// `--any`: pass a row if any selected field satisfies (default: all must).
    pub any: bool,
    /// `-F`: treat colnames as globs matched against the header.
    pub fuzzy_fields: bool,
    /// `-n`/`-Z`: prepend a row-number column (header `row`).
    pub show_row_number: bool,
    pub no_header_row: bool,
    pub delete_header: bool,
    pub in_delim: u8,
    pub out_delim: u8,
    pub comment: Option<u8>,
    pub ignore_empty_row: bool,
    pub lazy_quotes: bool,
    pub ignore_illegal_row: bool,
}

pub struct FilterStats {
    pub rows: u64,
}

/// Filter the rows of `input` by an arithmetic condition on selected fields and
/// write survivors to `out`, mirroring `csvtk filter`.
///
/// The whole output row (`record.All`) is emitted for passing rows — the
/// selected fields only drive the pass/fail decision, they are not projected.
/// The header (unless `-H` in index mode, or `-U`) passes through unfiltered.
/// Unless `-l`, quoting is validated Go-strict first; unless `-I`, a
/// wrong-field-count row fails loud.
pub fn filter(input: &str, opts: &FilterOptions, out: &mut dyn Write) -> Result<FilterStats> {
    let cond = filter::parse_condition(&opts.filter_str)?;
    let spec = fields::parse(&cond.field_str)?;

    // csvtk parses a header row for colname specs even under `-H` (the flag is
    // ignored); index specs honour `-H`.
    let has_header = !opts.no_header_row || spec.is_names();

    let data = normalize_crlf(read_all(input)?);
    if !opts.lazy_quotes {
        check_strict(&data, opts.in_delim, opts.comment)?;
    }

    let mut reader = csv::ReaderBuilder::new()
        .delimiter(opts.in_delim)
        .comment(opts.comment)
        .flexible(opts.ignore_illegal_row)
        .has_headers(false)
        .from_reader(&data[..]);

    // Stage output in memory and flush only after the whole input parses
    // cleanly. csvtk's buffered writer never flushes on a mid-stream fatal
    // error (Go's os.Exit skips the deferred Flush), so a malformed row must
    // leave stdout empty rather than emitting the survivors seen before it.
    let mut buffer: Vec<u8> = Vec::new();
    let mut kept: u64 = 0;
    {
        let mut w = CsvWriter::new(&mut buffer, opts.out_delim);
        let mut rec = csv::StringRecord::new();

        let mut cols: Vec<usize> = Vec::new();
        let mut resolved = false;
        let mut expected_len = 0usize;
        let mut first_record = true;
        let mut row: u64 = 0;

        while reader
            .read_record(&mut rec)
            .map_err(|e| RsomicsError::InvalidInput(format!("reading record: {e}")))?
        {
            if opts.ignore_empty_row && rec.iter().all(str::is_empty) {
                continue;
            }
            if first_record {
                expected_len = rec.len();
            } else if opts.ignore_illegal_row && rec.len() != expected_len {
                continue;
            }

            if !resolved {
                let reference: Vec<String> = rec.iter().map(str::to_owned).collect();
                cols = fields::resolve(&spec, &reference, opts.fuzzy_fields)?;
                resolved = true;
                // csvtk: an empty field set (e.g. a fuzzy glob matching nothing)
                // yields no records at all — not even the header.
                if cols.is_empty() {
                    break;
                }
            }

            if first_record {
                first_record = false;
                if has_header {
                    if !opts.delete_header {
                        write_row(&mut w, &rec, opts.show_row_number, "row")?;
                    }
                    continue;
                }
            }

            row += 1;
            let values: Vec<&str> = cols.iter().map(|&c| rec.get(c - 1).unwrap_or("")).collect();
            if filter::row_passes(&values, &cond, opts.any)? {
                kept += 1;
                let label = row.to_string();
                write_row(&mut w, &rec, opts.show_row_number, &label)?;
            }
        }

        w.flush()?;
    }

    out.write_all(&buffer).map_err(RsomicsError::Io)?;
    out.flush().map_err(RsomicsError::Io)?;
    Ok(FilterStats { rows: kept })
}

fn write_row<W: Write>(
    w: &mut CsvWriter<W>,
    rec: &csv::StringRecord,
    show_row_number: bool,
    label: &str,
) -> Result<()> {
    if show_row_number {
        let mut fields_out: Vec<&str> = Vec::with_capacity(rec.len() + 1);
        fields_out.push(label);
        fields_out.extend(rec.iter());
        w.write_record(&fields_out)
    } else {
        let fields_out: Vec<&str> = rec.iter().collect();
        w.write_record(&fields_out)
    }
}

fn read_all(input: &str) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    if input == "-" {
        std::io::stdin()
            .lock()
            .read_to_end(&mut buf)
            .map_err(RsomicsError::Io)?;
    } else {
        std::fs::File::open(input)
            .map_err(|e| RsomicsError::InvalidInput(format!("{input}: {e}")))?
            .read_to_end(&mut buf)
            .map_err(RsomicsError::Io)?;
    }
    Ok(buf)
}
