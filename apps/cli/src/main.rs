//! Headless batch CLI (PLAN.md M9) — a thin argument-parsing shell over
//! `openpdfedit-batch`/`openpdfedit-doc`/`openpdfedit-pages`. No PDFium,
//! no GUI, no live render thread: every operation here is pure
//! object-graph manipulation, so it runs as a plain, fast, scriptable
//! process.
//!
//! Hand-rolled argument parsing rather than a `clap` dependency —
//! two subcommands each with 2-4 positional/flag arguments doesn't need
//! a parsing framework, and it keeps this binary's dependency footprint
//! (and `cargo-deny` license surface) to exactly what the operations
//! themselves need.

use std::path::PathBuf;
use std::process::ExitCode;

use openpdfedit_batch::PiiPattern;
use openpdfedit_doc::Document;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("redact-pii") => run_redact_pii(&args[1..]),
        Some("merge") => run_merge(&args[1..]),
        Some("compare") => run_compare(&args[1..]),
        Some("help") | Some("--help") | Some("-h") | None => {
            print_usage();
            Ok(())
        }
        Some(other) => Err(format!(
            "unknown subcommand {other:?} — run `openpdfedit help`"
        )),
    }
}

fn print_usage() {
    println!(
        "openpdfedit — headless batch CLI\n\n\
         USAGE:\n\
         \x20   openpdfedit redact-pii <input.pdf> <output.pdf> [--patterns email,ssn,phone,card]\n\
         \x20       Finds and true-removes text matching the given PII patterns\n\
         \x20       (default: all four — see openpdfedit-batch's module doc for\n\
         \x20       what each pattern does and does not catch).\n\n\
         \x20   openpdfedit merge <output.pdf> <input1.pdf> <input2.pdf> [...]\n\
         \x20       Merges two or more PDFs into one new file, in argument order.\n\n\
         \x20   openpdfedit compare <a.pdf> <b.pdf>\n\
         \x20       Reports per-page text-run differences between two PDFs (see\n\
         \x20       openpdfedit-compare's module doc — this is a line-of-runs diff,\n\
         \x20       not word-level). Text-only: no PDFium/rendering here, so\n\
         \x20       pixel-mode compare isn't available from this headless CLI.\n"
    );
}

fn parse_patterns(spec: &str) -> Result<Vec<PiiPattern>, String> {
    spec.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|token| match token {
            "email" => Ok(PiiPattern::Email),
            "ssn" => Ok(PiiPattern::UsSsn),
            "phone" => Ok(PiiPattern::UsPhone),
            "card" => Ok(PiiPattern::CardNumber16),
            other => Err(format!(
                "unknown PII pattern {other:?} — expected one of: email, ssn, phone, card"
            )),
        })
        .collect()
}

fn run_redact_pii(args: &[String]) -> Result<(), String> {
    let mut positional: Vec<&str> = Vec::new();
    let mut patterns_spec = "email,ssn,phone,card".to_string();

    let mut i = 0;
    while i < args.len() {
        if args[i] == "--patterns" {
            patterns_spec = args
                .get(i + 1)
                .ok_or("--patterns needs a value, e.g. --patterns email,ssn")?
                .clone();
            i += 2;
        } else {
            positional.push(&args[i]);
            i += 1;
        }
    }

    let [input, output] = positional.as_slice() else {
        return Err(
            "usage: openpdfedit redact-pii <input.pdf> <output.pdf> [--patterns ...]".to_string(),
        );
    };

    let patterns = parse_patterns(&patterns_spec)?;
    let mut doc = Document::open(input).map_err(|e| format!("failed to open {input:?}: {e}"))?;
    let report = openpdfedit_batch::redact_pii(&mut doc, &patterns)
        .map_err(|e| format!("redaction failed: {e}"))?;
    // Full rewrite, not an incremental save. An incremental save appends
    // the redaction while keeping every original byte, so the text this
    // command just removed stayed in the output file one revision back —
    // recoverable with `strings`. See Document::save_full.
    let saved = doc.save_full().map_err(|e| format!("save failed: {e}"))?;
    std::fs::write(output, saved).map_err(|e| format!("failed to write {output:?}: {e}"))?;

    println!(
        "redacted {} match(es) across {} page(s) -> {}",
        report.matches_redacted, report.pages_processed, output
    );
    Ok(())
}

fn run_merge(args: &[String]) -> Result<(), String> {
    let [output, sources @ ..] = args else {
        return Err(
            "usage: openpdfedit merge <output.pdf> <input1.pdf> <input2.pdf> [...]".to_string(),
        );
    };
    if sources.len() < 2 {
        return Err("merge needs at least two input files".to_string());
    }

    let source_paths: Vec<PathBuf> = sources.iter().map(PathBuf::from).collect();
    openpdfedit_batch::merge_files(&source_paths, output.as_ref())
        .map_err(|e| format!("merge failed: {e}"))?;

    println!("merged {} file(s) -> {}", sources.len(), output);
    Ok(())
}

fn run_compare(args: &[String]) -> Result<(), String> {
    let [path_a, path_b] = args else {
        return Err("usage: openpdfedit compare <a.pdf> <b.pdf>".to_string());
    };

    let doc_a = Document::open(path_a).map_err(|e| format!("failed to open {path_a:?}: {e}"))?;
    let doc_b = Document::open(path_b).map_err(|e| format!("failed to open {path_b:?}: {e}"))?;
    let report = openpdfedit_compare::compare_text(&doc_a, &doc_b)
        .map_err(|e| format!("compare failed: {e}"))?;

    println!(
        "{path_a} ({} page(s)) vs {path_b} ({} page(s))",
        report.page_count_a, report.page_count_b
    );
    if report.pages.is_empty() {
        println!("no text differences found");
        return Ok(());
    }
    for page in &report.pages {
        println!(
            "page {}: -{} run(s), +{} run(s)",
            page.page_index + 1,
            page.removed.len(),
            page.added.len()
        );
        for line in &page.removed {
            println!("  - {line}");
        }
        for line in &page.added {
            println!("  + {line}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_patterns_accepts_known_names() {
        let patterns = parse_patterns("email, ssn,phone,card").expect("should parse");
        assert_eq!(patterns.len(), 4);
    }

    #[test]
    fn parse_patterns_rejects_unknown_names() {
        assert!(parse_patterns("email,bogus").is_err());
    }

    #[test]
    fn run_redact_pii_requires_input_and_output_args() {
        assert!(run_redact_pii(&[]).is_err());
        assert!(run_redact_pii(&["only_one.pdf".to_string()]).is_err());
    }

    #[test]
    fn run_merge_requires_at_least_two_sources() {
        assert!(run_merge(&["out.pdf".to_string()]).is_err());
        assert!(run_merge(&["out.pdf".to_string(), "one.pdf".to_string()]).is_err());
    }

    #[test]
    fn run_compare_requires_exactly_two_paths() {
        assert!(run_compare(&[]).is_err());
        assert!(run_compare(&["only_one.pdf".to_string()]).is_err());
        assert!(run_compare(&[
            "a.pdf".to_string(),
            "b.pdf".to_string(),
            "c.pdf".to_string()
        ])
        .is_err());
    }

    fn one_line_pdf_bytes(text: &str) -> Vec<u8> {
        use lopdf::content::{Content, Operation};
        use lopdf::{dictionary, Object, Stream};

        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        });
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 12.0.into()]),
                Operation::new("Td", vec![50.0.into(), 700.0.into()]),
                Operation::new("Tj", vec![Object::string_literal(text)]),
                Operation::new("ET", vec![]),
            ],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
            }),
        );
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    #[test]
    fn compare_end_to_end_via_the_cli_entry_point_succeeds_on_real_files() {
        let path_a = std::env::temp_dir().join(format!(
            "openpdfedit-cli-compare-a-{}.pdf",
            std::process::id()
        ));
        let path_b = std::env::temp_dir().join(format!(
            "openpdfedit-cli-compare-b-{}.pdf",
            std::process::id()
        ));
        std::fs::write(&path_a, one_line_pdf_bytes("Hello World")).unwrap();
        std::fs::write(&path_b, one_line_pdf_bytes("Goodbye World")).unwrap();

        let result = run_compare(&[
            path_a.to_string_lossy().into_owned(),
            path_b.to_string_lossy().into_owned(),
        ]);
        assert!(result.is_ok(), "compare should succeed: {result:?}");

        let _ = std::fs::remove_file(&path_a);
        let _ = std::fs::remove_file(&path_b);
    }

    #[test]
    fn redact_pii_end_to_end_via_the_cli_entry_point() {
        use lopdf::content::{Content, Operation};
        use lopdf::{dictionary, Object, Stream};

        let mut raw = lopdf::Document::with_version("1.5");
        let pages_id = raw.new_object_id();
        let font_id = raw.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        });
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 12.0.into()]),
                Operation::new("Td", vec![50.0.into(), 700.0.into()]),
                Operation::new("Tj", vec![Object::string_literal("mail me: a@b.com")]),
                Operation::new("ET", vec![]),
            ],
        };
        let content_id = raw.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page_id = raw.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
        });
        raw.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
            }),
        );
        let catalog_id = raw.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        raw.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        raw.save_to(&mut bytes).unwrap();

        let input_path = std::env::temp_dir().join(format!(
            "openpdfedit-cli-test-in-{}.pdf",
            std::process::id()
        ));
        let output_path = std::env::temp_dir().join(format!(
            "openpdfedit-cli-test-out-{}.pdf",
            std::process::id()
        ));
        std::fs::write(&input_path, bytes).unwrap();

        run_redact_pii(&[
            input_path.to_string_lossy().into_owned(),
            output_path.to_string_lossy().into_owned(),
            "--patterns".to_string(),
            "email".to_string(),
        ])
        .expect("redact-pii should succeed");

        assert!(output_path.exists());
        let out_bytes = std::fs::read(&output_path).unwrap();
        let reopened = lopdf::Document::load_mem(&out_bytes).unwrap();
        let page = reopened.get_dictionary(reopened.get_pages()[&1]).unwrap();
        let content_id = page.get(b"Contents").unwrap().as_reference().unwrap();
        let stream = reopened
            .get_object(content_id)
            .unwrap()
            .as_stream()
            .unwrap();
        let ops = Content::decode(&stream.content).unwrap();
        assert!(
            !ops.operations.iter().any(|op| op.operator == "Tj"),
            "the email-bearing Tj must be gone"
        );

        let _ = std::fs::remove_file(&input_path);
        let _ = std::fs::remove_file(&output_path);
    }
}
