//! `openpdfedit-mcp` — OpenPdfEdit's document tools, over the Model
//! Context Protocol.
//!
//! One static binary, stdio transport, and every operation runs here on
//! this machine. That last part is the point: the PDF-over-MCP servers that
//! exist are almost all wrappers over a cloud API, so using them means
//! uploading the document. This one reads and writes files locally and
//! hands the agent back a summary — the bytes never leave.
//!
//! ```text
//! openpdfedit-mcp --root ~/Documents --root ~/Downloads
//! ```
//!
//! With no `--root`, the working directory is the only reachable place.
mod sandbox;
mod server;
mod tools;

use std::path::PathBuf;

use rmcp::ServiceExt;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // stderr, never stdout: stdout *is* the protocol channel, and a stray
    // log line there is a parse error at the other end rather than a
    // cosmetic problem.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_usage();
        return Ok(());
    }
    let roots = parse_roots(&args)?;

    let sandbox = sandbox::Sandbox::new(roots)?;
    tracing::info!(
        roots = ?sandbox.roots().iter().map(|r| r.display().to_string()).collect::<Vec<_>>(),
        "openpdfedit-mcp ready",
    );

    let service = server::OpenPdfEdit::new(sandbox)
        .serve(rmcp::transport::stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}

fn parse_roots(args: &[String]) -> anyhow::Result<Vec<PathBuf>> {
    let mut roots = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                let value = args
                    .get(i + 1)
                    .ok_or_else(|| anyhow::anyhow!("--root needs a directory"))?;
                roots.push(PathBuf::from(value));
                i += 2;
            }
            other => anyhow::bail!("unexpected argument {other:?} — run with --help"),
        }
    }
    Ok(roots)
}

fn print_usage() {
    println!(
        "openpdfedit-mcp — local-first PDF tools for AI agents (MCP, stdio)\n\n\
         USAGE:\n\
         \x20   openpdfedit-mcp [--root DIR]...\n\n\
         \x20   --root DIR   A directory the server may read and write.\n\
         \x20                Repeatable. Defaults to the working directory.\n\n\
         TOOLS:\n\
         \x20   pdf_info        pages, encrypted, signed, size\n\
         \x20   encrypt_pdf     write a password-protected copy\n\
         \x20   decrypt_pdf     write an unlocked copy\n\
         \x20   redact_pii      truly remove emails, SSNs, phones, card numbers\n\
         \x20   merge_pdfs      combine documents in order\n\
         \x20   extract_pages   write chosen pages to a new document\n\n\
         Every operation runs on this machine. Documents are never uploaded.\n"
    );
}

#[cfg(test)]
mod tests {
    use super::parse_roots;

    #[test]
    fn roots_accumulate() {
        let args = vec![
            "--root".to_string(),
            "/a".to_string(),
            "--root".to_string(),
            "/b".to_string(),
        ];
        assert_eq!(parse_roots(&args).unwrap().len(), 2);
    }

    #[test]
    fn no_roots_is_fine_and_means_the_working_directory() {
        assert!(parse_roots(&[]).unwrap().is_empty());
    }

    #[test]
    fn a_dangling_root_flag_is_an_error() {
        assert!(parse_roots(&["--root".to_string()]).is_err());
    }

    #[test]
    fn an_unknown_flag_is_refused_rather_than_ignored() {
        assert!(parse_roots(&["--wat".to_string()]).is_err());
    }
}
