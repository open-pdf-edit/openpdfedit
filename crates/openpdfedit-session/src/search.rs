//! Document-wide text search.
//!
//! Lives here rather than in the desktop crate for the usual reason (see
//! [`crate::signatures`]): the same logic has to drive the desktop's
//! thread-wrapped `EngineHandle` and the extension's bare in-process
//! engine. Search is generic over `E: Engine` because it is *entirely*
//! an engine operation — it never reads the working copy, never touches
//! `docs`, and never mutates, so there is nothing here to gate behind
//! `WorkingStore` or `#[cfg(not(target_arch = "wasm32"))]`.
//!
//! The matching rules — and why they collapse whitespace rather than
//! matching PDFium's raw character stream — live in
//! `openpdfedit-engine`'s `find_matches`.

use openpdfedit_engine::{DocHandle, Engine, SearchOptions};
use serde::Serialize;

use crate::SessionError;

/// Ceiling on hits returned for one query.
///
/// On the desktop this runs as a single request on the shared render
/// thread (see `Engine::search_document`), so an unbounded search of a
/// long document stalls tile rendering for as long as it takes — a
/// one-letter query on a 2,000-page contract would freeze scrolling and
/// then push a six-figure array across the IPC boundary to build a list
/// nobody can read. In the extension it runs on the page's only thread,
/// where the same cost is a frozen tab. The front-end reports a
/// truncated result set rather than silently showing a short list.
const MAX_SEARCH_HITS: usize = 500;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHitDto {
    pub page_index: u32,
    /// Inclusive character range of the match on its page, in the same
    /// index space the annotation commands use — which is what would let
    /// "highlight every match" turn a hit into a real highlight over the
    /// matched glyphs rather than an approximated box.
    pub char_start: u32,
    pub char_end: u32,
    /// One `[x0, y0, x1, y1]` per visual line the match spans, in PDF
    /// page-space points (origin bottom-left).
    pub quads: Vec<[f32; 4]>,
    pub context_before: String,
    pub context_match: String,
    pub context_after: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResultsDto {
    pub hits: Vec<SearchHitDto>,
    /// True when the search stopped at [`MAX_SEARCH_HITS`] and the
    /// document holds more matches than are listed.
    pub truncated: bool,
}

/// The logic behind the desktop's `search_document_cmd` and the
/// extension's `WasmSession::search_document`.
pub fn search_document_impl<E: Engine>(
    engine: &E,
    handle: DocHandle,
    query: &str,
    match_case: bool,
    whole_word: bool,
) -> Result<SearchResultsDto, SessionError> {
    let options = SearchOptions {
        match_case,
        whole_word,
    };
    let hits = engine.search_document(handle, query, options, MAX_SEARCH_HITS)?;
    let truncated = hits.len() >= MAX_SEARCH_HITS;

    Ok(SearchResultsDto {
        truncated,
        hits: hits
            .into_iter()
            .map(|hit| SearchHitDto {
                page_index: hit.page_index,
                char_start: hit.char_start,
                char_end: hit.char_end,
                quads: hit.quads,
                context_before: hit.context_before,
                context_match: hit.context_match,
                context_after: hit.context_after,
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_corpus_path() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("testdata/corpus/hello_world_rotated.pdf")
    }

    #[test]
    fn search_finds_hits_and_reports_truncation_honestly() {
        let Some(engine) = crate::test_support::shared_handle() else {
            return;
        };
        let corpus = text_corpus_path();
        if !corpus.exists() {
            eprintln!("skipping: {} not present", corpus.display());
            return;
        }
        let handle = engine.open(&corpus).expect("engine should open the corpus");

        let results = search_document_impl(engine, handle, "hello", false, false)
            .expect("search should succeed");
        assert!(!results.hits.is_empty());
        assert!(
            !results.truncated,
            "a handful of hits must not be reported as a truncated result set"
        );
        for hit in &results.hits {
            assert_eq!(hit.context_match.to_lowercase(), "hello");
            assert!(!hit.quads.is_empty());
        }

        // A query that matches nothing is an empty result, not an error.
        let none = search_document_impl(engine, handle, "zzzznotpresent", false, false)
            .expect("a query with no matches should still succeed");
        assert!(none.hits.is_empty());
        assert!(!none.truncated);

        engine.close(handle);
    }

    #[test]
    fn an_unknown_handle_is_an_error_not_a_panic() {
        let Some(engine) = crate::test_support::shared_handle() else {
            return;
        };
        assert!(search_document_impl(engine, 999_999, "anything", false, false).is_err());
    }
}
