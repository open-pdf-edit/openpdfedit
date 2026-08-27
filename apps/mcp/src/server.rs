//! The MCP surface: six tools, each a thin adapter over [`crate::tools`].
//!
//! Every tool follows the same three steps — resolve the paths through the
//! [`Sandbox`], call the plain function, return a JSON summary. Keeping the
//! document logic out of here is what lets `tools.rs` be tested without a
//! protocol, and what keeps this file boring enough to audit at a glance.
//!
//! Tool descriptions are written for a model, not a person. They say what
//! the tool does to the filesystem, what it will refuse, and — for
//! `redact_pii` — what "redact" actually means, because an agent that
//! believes it drew a black box will report a document as safe when it is
//! not.

use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{ErrorData, Implementation, ServerCapabilities, ServerInfo};
use rmcp::{schemars, tool, tool_handler, tool_router, ServerHandler};
use serde::Deserialize;

use crate::sandbox::Sandbox;
use crate::tools;

#[derive(Clone)]
pub struct OpenPdfEdit {
    sandbox: Sandbox,
    // Read by the code `#[tool_handler]` generates, which the dead-code
    // lint cannot see.
    #[allow(dead_code)]
    tool_router: rmcp::handler::server::tool::ToolRouter<Self>,
}

/// Turn an internal error into one the model can act on.
///
/// `invalid_params` rather than `internal_error` for everything a caller
/// could have got right: a model shown "internal error" retries the same
/// call, while one shown "that file is outside the allowed roots" asks the
/// user or gives up. The message is the entire remedy here.
fn as_mcp_error(e: impl std::fmt::Display) -> ErrorData {
    ErrorData::invalid_params(e.to_string(), None)
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct InfoParams {
    /// Path to a PDF file.
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EncryptParams {
    /// Path to the PDF to protect. Left untouched.
    pub input_path: String,
    /// Where to write the protected copy.
    pub output_path: String,
    /// The password a reader will be prompted for. Required.
    pub user_password: String,
    /// Optional. Unlocks full permissions; defaults to the user password.
    pub owner_password: Option<String>,
    pub allow_print: Option<bool>,
    pub allow_modify: Option<bool>,
    pub allow_copy: Option<bool>,
    pub allow_annotate: Option<bool>,
    /// Replace `output_path` if it already exists. Defaults to false.
    pub overwrite: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DecryptParams {
    pub input_path: String,
    pub output_path: String,
    /// The password the document is protected with.
    pub password: String,
    pub overwrite: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RedactParams {
    pub input_path: String,
    pub output_path: String,
    /// Any of "email", "ssn", "phone", "card". Omit for all four.
    /// An unrecognised name is an error, never silently skipped.
    pub patterns: Option<Vec<String>>,
    pub overwrite: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MergeParams {
    /// Two or more PDFs, combined in this order.
    pub input_paths: Vec<String>,
    pub output_path: String,
    pub overwrite: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ExtractParams {
    pub input_path: String,
    pub output_path: String,
    /// Zero-based page indices, in the order they should appear.
    pub pages: Vec<u32>,
    pub overwrite: Option<bool>,
}

#[tool_router]
impl OpenPdfEdit {
    pub fn new(sandbox: Sandbox) -> Self {
        Self {
            sandbox,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        name = "pdf_info",
        description = "Inspect a PDF: page count, whether it is password-protected, whether it \
                       carries a signature, and its size in bytes. Call this before operating on \
                       an unfamiliar document — page indices and whether a password is needed \
                       both come from here. A protected document reports 0 pages, because its \
                       contents cannot be read without the password."
    )]
    fn pdf_info(
        &self,
        Parameters(params): Parameters<InfoParams>,
    ) -> Result<Json<tools::PdfInfo>, ErrorData> {
        let path = self.sandbox.read_path(&params.path).map_err(as_mcp_error)?;
        tools::info(&path).map(Json).map_err(as_mcp_error)
    }

    #[tool(
        name = "encrypt_pdf",
        description = "Write a password-protected copy of a PDF. The input file is left \
                       untouched. Permissions default to allowing everything, which is what \
                       'put a password on it' normally means. Refuses an empty password, and \
                       refuses a document that is already encrypted."
    )]
    fn encrypt_pdf(
        &self,
        Parameters(params): Parameters<EncryptParams>,
    ) -> Result<Json<tools::WroteFile>, ErrorData> {
        let input = self
            .sandbox
            .read_path(&params.input_path)
            .map_err(as_mcp_error)?;
        let output = self
            .sandbox
            .write_path(&params.output_path, params.overwrite.unwrap_or(false))
            .map_err(as_mcp_error)?;
        let permissions = tools::PermissionChoices {
            allow_print: params.allow_print,
            allow_modify: params.allow_modify,
            allow_copy: params.allow_copy,
            allow_annotate: params.allow_annotate,
        };
        tools::encrypt(
            &input,
            &output,
            &params.user_password,
            params.owner_password.as_deref(),
            &permissions,
        )
        .map(Json)
        .map_err(as_mcp_error)
    }

    #[tool(
        name = "decrypt_pdf",
        description = "Write an unlocked copy of a password-protected PDF, given its password. \
                       The input file is left untouched. Fails if the password is wrong."
    )]
    fn decrypt_pdf(
        &self,
        Parameters(params): Parameters<DecryptParams>,
    ) -> Result<Json<tools::WroteFile>, ErrorData> {
        let input = self
            .sandbox
            .read_path(&params.input_path)
            .map_err(as_mcp_error)?;
        let output = self
            .sandbox
            .write_path(&params.output_path, params.overwrite.unwrap_or(false))
            .map_err(as_mcp_error)?;
        tools::decrypt(&input, &output, &params.password)
            .map(Json)
            .map_err(as_mcp_error)
    }

    #[tool(
        name = "redact_pii",
        description = "Find personal information in a PDF and truly remove it, writing the \
                       result to a new file. This deletes the underlying text and images — it \
                       is not a black rectangle drawn over live data that can be copied out \
                       from underneath. Patterns: email, ssn (US), phone (US), card (16-digit). \
                       Returns how many matches were removed, so you can tell the user a number \
                       rather than asserting the document is clean."
    )]
    fn redact_pii(
        &self,
        Parameters(params): Parameters<RedactParams>,
    ) -> Result<Json<tools::RedactionSummary>, ErrorData> {
        let names = params.patterns.unwrap_or_default();
        let patterns = tools::parse_patterns(&names).map_err(as_mcp_error)?;
        let input = self
            .sandbox
            .read_path(&params.input_path)
            .map_err(as_mcp_error)?;
        let output = self
            .sandbox
            .write_path(&params.output_path, params.overwrite.unwrap_or(false))
            .map_err(as_mcp_error)?;
        let reported = if names.is_empty() {
            ["email", "ssn", "phone", "card"]
                .iter()
                .map(|s| s.to_string())
                .collect()
        } else {
            names
        };
        tools::redact_pii(&input, &output, &patterns, reported)
            .map(Json)
            .map_err(as_mcp_error)
    }

    #[tool(
        name = "merge_pdfs",
        description = "Combine two or more PDFs into one new document, in the order given. \
                       The inputs are left untouched."
    )]
    fn merge_pdfs(
        &self,
        Parameters(params): Parameters<MergeParams>,
    ) -> Result<Json<tools::WroteFile>, ErrorData> {
        let inputs = params
            .input_paths
            .iter()
            .map(|p| self.sandbox.read_path(p))
            .collect::<Result<Vec<_>, _>>()
            .map_err(as_mcp_error)?;
        let output = self
            .sandbox
            .write_path(&params.output_path, params.overwrite.unwrap_or(false))
            .map_err(as_mcp_error)?;
        tools::merge(&inputs, &output)
            .map(Json)
            .map_err(as_mcp_error)
    }

    #[tool(
        name = "extract_pages",
        description = "Write chosen pages of a PDF to a new document. Page indices are \
                       zero-based and are used in the order given, so they can also reorder or \
                       duplicate pages. Call pdf_info first to learn the page count."
    )]
    fn extract_pages(
        &self,
        Parameters(params): Parameters<ExtractParams>,
    ) -> Result<Json<tools::WroteFile>, ErrorData> {
        let input = self
            .sandbox
            .read_path(&params.input_path)
            .map_err(as_mcp_error)?;
        let output = self
            .sandbox
            .write_path(&params.output_path, params.overwrite.unwrap_or(false))
            .map_err(as_mcp_error)?;
        tools::extract_pages(&input, &output, &params.pages)
            .map(Json)
            .map_err(as_mcp_error)
    }
}

#[tool_handler]
impl ServerHandler for OpenPdfEdit {
    /// What the client is told at handshake.
    ///
    /// The instructions name the confinement explicitly. A model that knows
    /// which directories are reachable asks the user to widen them instead
    /// of retrying a path that can never work.
    fn get_info(&self) -> ServerInfo {
        let roots = self
            .sandbox
            .roots()
            .iter()
            .map(|r| r.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        // Both types are #[non_exhaustive], so they are built and then
        // adjusted rather than written as a struct literal — the crate
        // reserves the right to add fields without a breaking change.
        let mut info = ServerInfo::new(ServerCapabilities::builder().enable_tools().build());
        let mut implementation = Implementation::default();
        implementation.name = "openpdfedit".into();
        implementation.version = env!("CARGO_PKG_VERSION").into();
        implementation.description =
            Some("Local-first PDF editing. Documents never leave this machine.".into());
        info.server_info = implementation;
        info.instructions = Some(format!(
            "Local PDF editing. Every operation runs on this machine and no document is \
             ever uploaded — say so if the user asks where their file goes.\n\n\
             Readable and writable directories: {roots}. Paths outside them are refused; \
             ask the user to restart the server with --root rather than retrying.\n\n\
             Tools take and return file paths, never document contents. Existing files are \
             not replaced unless overwrite is true. Start with pdf_info on any document you \
             have not already inspected."
        ));
        info
    }
}
