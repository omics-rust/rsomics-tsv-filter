use clap::Parser;
use rsomics_common::{CommonFlags, Result, RsomicsError, Tool, ToolMeta};
use rsomics_help::{Example, FlagSpec, HelpSpec, Origin, Section};
use serde::Serialize;

use rsomics_tsv_filter::{FilterOptions, filter};

pub const META: ToolMeta = ToolMeta {
    name: env!("CARGO_PKG_NAME"),
    version: env!("CARGO_PKG_VERSION"),
};

const TAGLINE: &str =
    "Filter CSV/TSV rows by an arithmetic condition on selected fields — csvtk filter port.";

#[derive(Parser, Debug)]
#[command(
    name = "rsomics-tsv-filter",
    version,
    about,
    long_about = None,
    disable_help_flag = true
)]
pub struct Cli {
    /// Input CSV/TSV file. `-` (or omitted) reads stdin.
    #[arg(default_value = "-")]
    input: String,

    /// Output file (`-` for stdout).
    #[arg(short = 'o', long = "out-file", default_value = "-")]
    output: String,

    /// Condition `<fields><op><value>`, e.g. `age>12`, `1,3<=2`, `-F c*!=0`.
    /// op ∈ `>` `<` `=` `>=` `<=` `!=`/`<>`.
    #[arg(
        short = 'f',
        long = "filter",
        default_value = "",
        allow_hyphen_values = true
    )]
    filter: String,

    /// Pass a row if ANY selected field satisfies (default: all must).
    #[arg(long = "any")]
    any: bool,

    /// Treat colnames as globs (`*`) matched against the header.
    #[arg(short = 'F', long = "fuzzy-fields")]
    fuzzy_fields: bool,

    /// Prepend a line-number column named `row` (1-based input data rows).
    #[arg(short = 'n', long = "line-number")]
    line_number: bool,

    /// Prepend a row-number column (same column as -n).
    #[arg(short = 'Z', long = "show-row-number")]
    show_row_number: bool,

    /// Input field delimiter.
    #[arg(short = 'd', long = "delimiter", default_value = ",")]
    delimiter: String,

    /// Output field delimiter.
    #[arg(short = 'D', long = "out-delimiter", default_value = ",")]
    out_delimiter: String,

    /// Treat input as tab-delimited (also drives tab output). (Long-only: -t is --threads.)
    #[arg(long = "tabs")]
    tabs: bool,

    /// Emit tab-delimited output.
    #[arg(short = 'T', long = "out-tabs")]
    out_tabs: bool,

    /// Comment-char: lines starting with it are ignored.
    #[arg(short = 'C', long = "comment-char", default_value = "#")]
    comment_char: String,

    /// Input has no header row.
    #[arg(short = 'H', long = "no-header-row")]
    no_header_row: bool,

    /// Do not output the header row.
    #[arg(short = 'U', long = "delete-header")]
    delete_header: bool,

    /// Ignore rows whose every field is empty.
    #[arg(short = 'E', long = "ignore-empty-row")]
    ignore_empty_row: bool,

    /// Skip rows whose field count differs from the header's (else: fail loud).
    #[arg(short = 'I', long = "ignore-illegal-row")]
    ignore_illegal_row: bool,

    /// Accept bare/unescaped quotes instead of failing loud on them.
    #[arg(short = 'l', long = "lazy-quotes")]
    lazy_quotes: bool,

    #[command(flatten)]
    pub common: CommonFlags,
}

#[derive(Serialize)]
pub struct FilterReport {
    pub input: String,
    pub output: String,
    pub rows: u64,
}

fn parse_delim(s: &str, flag: &str) -> Result<u8> {
    let bytes = s.as_bytes();
    if bytes.len() != 1 {
        return Err(RsomicsError::InvalidInput(format!(
            "value of flag --{flag} should have length of 1"
        )));
    }
    Ok(bytes[0])
}

impl Cli {
    pub fn execute(&self) -> Result<FilterReport> {
        let delimiter = parse_delim(&self.delimiter, "delimiter")?;
        let out_delimiter = parse_delim(&self.out_delimiter, "out-delimiter")?;
        let comment = if self.comment_char.is_empty() {
            None
        } else {
            Some(parse_delim(&self.comment_char, "comment-char")?)
        };

        let in_delim = if self.tabs { b'\t' } else { delimiter };
        let out_delim = if self.out_tabs || self.tabs {
            if out_delimiter == b',' {
                b'\t'
            } else {
                out_delimiter
            }
        } else {
            out_delimiter
        };

        let opts = FilterOptions {
            filter_str: self.filter.clone(),
            any: self.any,
            fuzzy_fields: self.fuzzy_fields,
            show_row_number: self.line_number || self.show_row_number,
            no_header_row: self.no_header_row,
            delete_header: self.delete_header,
            in_delim,
            out_delim,
            comment,
            ignore_empty_row: self.ignore_empty_row,
            lazy_quotes: self.lazy_quotes,
            ignore_illegal_row: self.ignore_illegal_row,
        };

        let mut out: Box<dyn std::io::Write> = if self.output == "-" && self.common.json {
            Box::new(std::io::sink())
        } else if self.output == "-" {
            Box::new(std::io::stdout().lock())
        } else {
            Box::new(std::fs::File::create(&self.output).map_err(RsomicsError::Io)?)
        };

        let stats = filter(&self.input, &opts, &mut out)?;

        Ok(FilterReport {
            input: self.input.clone(),
            output: self.output.clone(),
            rows: stats.rows,
        })
    }
}

impl Tool for Cli {
    fn meta() -> ToolMeta {
        META
    }

    fn common(&self) -> &CommonFlags {
        &self.common
    }

    fn execute(self) -> Result<()> {
        Cli::execute(&self)?;
        Ok(())
    }

    fn run(self) -> std::process::ExitCode {
        let common = self.common().clone();
        rsomics_common::run(&common, Self::meta(), move || Cli::execute(&self))
    }
}

pub static HELP: HelpSpec = HelpSpec {
    name: META.name,
    version: META.version,
    tagline: TAGLINE,
    origin: Some(Origin {
        upstream: "csvtk filter",
        upstream_license: "MIT",
        our_license: "MIT OR Apache-2.0",
        paper_doi: None,
    }),
    usage_lines: &[
        "[OPTIONS] -f <fields><op><value> [input.csv|-]",
        "[OPTIONS] -F -f 'c*>=0' [input.tsv|-]",
    ],
    sections: &[Section {
        title: "OPTIONS",
        flags: &[
            FlagSpec {
                short: Some('f'),
                long: "filter",
                aliases: &[],
                value: Some("<cond>"),
                type_hint: Some("String"),
                required: true,
                default: None,
                description: "Condition <fields><op><value>. op ∈ > < = >= <= !=/<>.",
                why_default: None,
            },
            FlagSpec {
                short: None,
                long: "any",
                aliases: &[],
                value: None,
                type_hint: None,
                required: false,
                default: None,
                description: "Pass a row if any selected field satisfies (default: all).",
                why_default: None,
            },
            FlagSpec {
                short: Some('F'),
                long: "fuzzy-fields",
                aliases: &[],
                value: None,
                type_hint: None,
                required: false,
                default: None,
                description: "Treat colnames as globs (`*`) matched against the header.",
                why_default: None,
            },
            FlagSpec {
                short: Some('n'),
                long: "line-number",
                aliases: &[],
                value: None,
                type_hint: None,
                required: false,
                default: None,
                description: "Prepend a line-number column named `row` (1-based input data rows).",
                why_default: None,
            },
            FlagSpec {
                short: Some('d'),
                long: "delimiter",
                aliases: &[],
                value: Some("<char>"),
                type_hint: Some("char"),
                required: false,
                default: Some(","),
                description: "Input field delimiter.",
                why_default: None,
            },
            FlagSpec {
                short: Some('D'),
                long: "out-delimiter",
                aliases: &[],
                value: Some("<char>"),
                type_hint: Some("char"),
                required: false,
                default: Some(","),
                description: "Output field delimiter.",
                why_default: None,
            },
            FlagSpec {
                short: None,
                long: "tabs",
                aliases: &[],
                value: None,
                type_hint: None,
                required: false,
                default: None,
                description: "Tab-delimited input and output. (No short flag: -t is --threads.)",
                why_default: None,
            },
            FlagSpec {
                short: Some('T'),
                long: "out-tabs",
                aliases: &[],
                value: None,
                type_hint: None,
                required: false,
                default: None,
                description: "Tab-delimited output only.",
                why_default: None,
            },
            FlagSpec {
                short: Some('C'),
                long: "comment-char",
                aliases: &[],
                value: Some("<char>"),
                type_hint: Some("char"),
                required: false,
                default: Some("#"),
                description: "Lines starting with this char are ignored (empty disables).",
                why_default: None,
            },
            FlagSpec {
                short: Some('H'),
                long: "no-header-row",
                aliases: &[],
                value: None,
                type_hint: None,
                required: false,
                default: None,
                description: "Input has no header row (index fields only).",
                why_default: None,
            },
            FlagSpec {
                short: Some('U'),
                long: "delete-header",
                aliases: &[],
                value: None,
                type_hint: None,
                required: false,
                default: None,
                description: "Do not output the header row.",
                why_default: None,
            },
            FlagSpec {
                short: Some('E'),
                long: "ignore-empty-row",
                aliases: &[],
                value: None,
                type_hint: None,
                required: false,
                default: None,
                description: "Skip rows whose every field is empty.",
                why_default: None,
            },
            FlagSpec {
                short: Some('I'),
                long: "ignore-illegal-row",
                aliases: &[],
                value: None,
                type_hint: None,
                required: false,
                default: None,
                description: "Skip rows whose field count differs from the header's (else: fail loud).",
                why_default: None,
            },
            FlagSpec {
                short: Some('l'),
                long: "lazy-quotes",
                aliases: &[],
                value: None,
                type_hint: None,
                required: false,
                default: None,
                description: "Accept bare/unescaped quotes instead of failing loud.",
                why_default: None,
            },
        ],
    }],
    examples: &[
        Example {
            description: "Keep rows where column `age` exceeds 12",
            command: "rsomics-tsv-filter -f 'age>12' in.csv",
        },
        Example {
            description: "Keep rows where column 2 or 3 is <= 100 (any)",
            command: "rsomics-tsv-filter --any -f '2,3<=100' in.csv",
        },
        Example {
            description: "Fuzzy fields: every column starting with `c` must be non-zero",
            command: "rsomics-tsv-filter -F -f 'c*!=0' in.csv",
        },
        Example {
            description: "Tab-delimited input, prepend a row number",
            command: "rsomics-tsv-filter --tabs -n -f 'depth>=30' in.tsv",
        },
    ],
    json_result_schema_doc: None,
};

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_debug_assert() {
        Cli::command().debug_assert();
    }
}
