# Research Report 3 — Rust Tech Stack Evaluation

*Research date: 2026-08-01. Produced by a deep-research agent; citations inline.*

**Verdict up front: yes, Rust makes sense — but only as a hybrid stack built around PDFium (C++, BSD-licensed) for rendering plus Rust-native libraries for document manipulation. A pure-Rust renderer is 1–2 years away from production grade, and MuPDF's AGPL is a trap for an open-core product.**

## 1. PDF Rendering Engines (the hard part)

### PDFium via `pdfium-render` — the pragmatic default
- **Crate:** [`pdfium-render`](https://crates.io/crates/pdfium-render) v0.9.4, **MIT-licensed**, ~690 GitHub stars, 555+ commits, actively maintained through 2026 (0.9.4 fixed double-free errors in font/page-object handling). Also: maintained fork [`kreuzberg-pdfium-render`](https://crates.io/crates/kreuzberg-pdfium-render) (MIT) and newer independent binding [PDFium-rs](https://github.com/newinnovations/pdfium-rs) — insurance against single-maintainer risk (effectively one maintainer, ajrcarey).
- **PDFium itself:** BSD-3-Clause (Chromium project, originally Foxit). Battle-tested in Chrome; the accurate/high-performance choice for native apps ([Syncfusion comparison](https://www.syncfusion.com/blogs/post/pdf-rendering-engines-comparison)).
- **Linking:** dynamic linking against [bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries) (auto-built weekly since 2017; Win x64/arm64, mac universal). Static prebuilts from paulocoutinhox/pdfium-lib. Practical answer: **ship the ~10 MB dylib/DLL alongside the app**.
- **Form APIs:** field introspection and **filling** supported (FPDF_FORMFILL). **Form field *creation* is weak in PDFium** — create AcroForm dictionaries yourself with a writer library.
- **Text APIs:** char-level geometry (FPDFText: char boxes, rects, hit-testing — exactly what selection/highlighting needs), plus page-object-level text editing (`FPDFText_SetText`, font loading, object add/remove) — raw material for in-place editing, no layout intelligence.
- **Caveats:** PDFium is not thread-safe (crate serializes via `thread_safe` feature); marshal rendering onto one thread.

### MuPDF via `mupdf-rs` — best engine, worst license
- [`mupdf`](https://crates.io/crates/mupdf) crate bindings are **AGPL-3.0** matching [MuPDF's AGPL](https://mupdf.readthedocs.io/en/1.27.2/license.html). Technically excellent.
- **AGPL implications:** the whole distributed binary that links MuPDF must be AGPL. Paid proprietary features cannot live in the same process without an [Artifex commercial license](https://artifex.com/licensing) (~$1,500–$50,000+, negotiated; actively enforced). **Rule out unless prepared to buy the license.**

### Poppler, qpdf
- **Poppler:** GPL; Linux-oriented; no advantage over PDFium. Skip.
- **qpdf:** [Apache-2.0](https://qpdf.readthedocs.io/en/stable/license.html), **no rendering** — content-preserving PDF transformer (repair, linearize, encrypt, split/merge). Belongs in manipulation.

### Pure-Rust renderers (2025–2026 state)
- **[`hayro`](https://github.com/LaurenzV/hayro)** (LaurenzV of resvg/typst): Apache-2.0/MIT, ~730 stars, nine crates. Self-described "experimental" but **the most feature-complete pure-Rust PDF rasterizer ever** — passes 1,400+ PDFs from the PDFBOX/pdf.js suites; **Typst 0.14 adopted it** ([changelog](https://typst.app/docs/changelog/0.14.0/)). Gaps: knockout groups, non-embedded CID fonts, unoptimized performance.
- **pdf-rs / pdf_render**: parsing decent, rendering stalled.
- **oxidize-pdf**: parser/writer, not a real renderer; license ambiguity (see report 4).

**Honest assessment:** no pure-Rust renderer is ready as *primary* engine in 2026. Hayro is the one to watch — architect an engine-abstraction layer so PDFium→hayro can swap later.

## 2. PDF Manipulation / Writing

| Crate | License | Status | Role |
|---|---|---|---|
| [`lopdf`](https://github.com/J-F-Liu/lopdf) | MIT | ~2.2k stars, active | Object-level read/modify/write. **Incremental updates supported (since 0.28)** — critical for preserving digital signatures. Object/xref streams (PDF 1.5+), merge with bookmarks, content-stream parsing, font/image embedding. Weakness: limited encryption support; whole doc in memory. |
| [`pdf-writer`](https://crates.io/crates/pdf-writer) | MIT/Apache-2.0 | typst project | Low-level write-only PDF construction. Foundation layer. |
| [`krilla`](https://github.com/LaurenzV/krilla) | MIT/Apache-2.0 | ~425 stars, powers Typst export | High-level creation: fills/strokes/gradients/glyphs, **CFF+TTF font subsetting**, tagged PDF (PDF/UA-1), PDF/A 1–4, annotations, outlines. Creation-only. |
| [`printpdf`](https://github.com/fschutt/printpdf) | MIT | v0.9.1 | Read + write + experimental rendering, [WASM demo](https://fschutt.github.io/printpdf/). One maintainer; treat as secondary. |
| [`oxidize-pdf`](https://crates.io/crates/oxidize-pdf) | MIT claimed (verify) | ~185 stars, 7,900+ tests | Pure-Rust parse+generate: split/merge/rotate, AES-256, PKCS#7 verification, JBIG2. Young. |
| [`qpdf`/`qpdf-sys`](https://crates.io/crates/qpdf) | MIT/Apache bindings; qpdf Apache-2.0 | mature | Repair, linearization, encryption, robust split/merge. **Rewrites files (no incremental mode)** — don't route signed docs through it. |

**Architecture implication:** PDFium for rendering + interactive geometry; lopdf for object-level editing and incremental saves (annotations, AcroForm creation, page ops); krilla + typst's `subsetter` for new content/embedded fonts. Appearance streams (`/AP`) you generate yourself — no crate does WYSIWYG appearance generation; budget real effort here.

## 3. Why "Edit Text in Place" Is Hard — and What's Feasible

1. **No reflow information.** Content streams are positioned glyph runs (`Tj`/`TJ` with kerning arrays); paragraph/line/column structure doesn't exist unless Tagged (most aren't). Editors reverse-engineer layout by clustering glyph boxes heuristically.
2. **Subset fonts.** Nearly all PDFs embed [subsetted fonts containing only used glyphs](https://www.prepressure.com/pdf/basics/fonts) ([PDF Tools AG](https://blog.pdf-tools.com/2015/05/font-subsetting-how-it-works-and-where.html)). New characters have no glyph: refuse, substitute (visible mismatch), or merge a new subset.
3. **Encoding chaos.** Ad-hoc encodings; `ToUnicode` may be missing/wrong.
4. **Everything is baked.** Justification, ligatures, tracking — edit one word and you re-layout the line/paragraph; the original layout engine and its metrics are gone.

**How real editors do it:** Acrobat/Foxit parse content streams, group runs into paragraph boxes, run their *own* layout engine in that box, rewrite text objects; silently substitute fonts when subsets lack glyphs. LibreOffice Draw imports each *line* as a separate frame (why it feels broken). Solid Documents (licensed by Adobe for PDF→Word) does full reconstruction.

**Staged roadmap:**
- **Tier 1 (ship first):** annotation/overlay editing — highlights, notes, freehand, whiteout+retype, stamps, form filling. Doable with PDFium + lopdf today.
- **Tier 2 (moderate):** single-line/text-box in-place editing: PDFium char geometry to find runs, edit via `FPDFText_SetText` or content-stream rewrite in lopdf; bundled-font fallback (new subset via [`subsetter`](https://crates.io/crates/subsetter)) with an explicit "font substituted" indicator (honesty beats Acrobat's silent substitution).
- **Tier 3 (hard, differentiator):** paragraph-level re-layout with own line-breaking (cosmic-text/parley). Cross-page reflow: don't — that's PDF→Word territory.

## 4. OCR

- **Tesseract:** [`tesseract`](https://crates.io/crates/tesseract) v0.15.2 (Apache-2.0 engine 5.5.x) — standard, mediocre on photos, C build pain; [`tesseract-rs`](https://github.com/cafercangundogdu/tesseract-rs) vendors the build.
- **[`ocrs`](https://crates.io/crates/ocrs):** pure Rust neural, "early preview", Latin-focused. Not competitive yet.
- **RapidOCR** (PaddleOCR ONNX, Apache-2.0): run models under the `ort` crate — best route to strong **CJK** OCR.
- **Apple Vision (macOS):** `VNRecognizeTextRequest` via [`objc2-vision`](https://crates.io/crates/objc2-vision) — excellent accuracy, zero-install, free ([Tauri + Vision OCR devlog](https://dev.to/hiyoyok/calling-apple-vision-api-from-tauri-for-offline-ocr-pdf-devlog-2-3mkb)). [Memory-growth quirk](https://developer.apple.com/forums/thread/815812) — run OCR in a subprocess. Windows analog: `Windows.Media.Ocr` via the `windows` crate.
- **Searchable PDF:** copy [OCRmyPDF's "sandwich"](https://ocrmypdf.readthedocs.io/en/latest/advanced.html) — invisible text (render mode 3) positioned from OCR word boxes via incremental update. Fully feasible in Rust.

**Recommendation:** platform-native OCR first (Vision/Windows.Media.Ocr) + Tesseract/RapidOCR-via-ort as quality/CJK fallback. Skip ocrs for now.

## 5. Office Conversion (PDF→Word/Excel)

Document *reconstruction* — the hardest feature on the list; every open-source option is weak:
- **LibreOffice headless:** MPL-2.0, subprocess = no copyleft issue, but PDF import is line-by-line frames; [batch quality poor](https://pdf4.dev/blog/how-to-convert-pdf-to-word); ~300 MB dependency. Best-effort only.
- **[pdf2docx](https://github.com/ArtifexSoftware/pdf2docx):** best OSS quality, now MIT, but depends on **PyMuPDF (AGPL)** + Python runtime.
- **The pros:** Adobe licenses [Solid Framework](https://solidframework.net/sample/convert-pdf-to-word/) (proprietary); Apryse/Nutrient sell SDKs. Nothing in Rust comes close.
- **Realistic strategy:** make PDF→Office a **paid, cloud-side feature** (fits open-core; no desktop licensing entanglement), or license Solid Framework, or ship best-effort LibreOffice locally. Do not build reconstruction yourself in v1. Excel/table extraction: Camelot-style heuristics on PDFium text geometry is achievable for simple cases.

## 6. Digital Signatures (PAdES)

PAdES-B needs: SHA-256 over `/ByteRange`, detached **CMS SignedData** in `/Contents`, signature appearance annotation, RFC 3161 timestamp; B-LT adds DSS/VRI dictionaries — and **every added signature must be an incremental update** or prior signatures break (exactly why lopdf's incremental save matters; [discussion](https://github.com/OpenSignLabs/OpenSign/issues/2090)).

- **CMS:** RustCrypto [`cms`](https://docs.rs/cms/latest/cms/) (pure Rust, RFC 5652, active) + `x509-cert`, `rsa`/`p256`, `sha2`. Alternative: `cryptographic-message-syntax` (maintenance winding down). `openssl` crate as escape hatch.
- **PDF-level:** [`pdf_signing`](https://github.com/ralpha/pdf_signing) is WIP. **No turnkey PAdES-LT Rust library — assemble it yourself** (~weeks for B-B/B-T, more for LT/LTA). Study [pyHanko](https://github.com/MatthiasValvekens/pyHanko) as reference.
- **OS cert stores:** Windows — CNG `NCryptSignHash` via `windows` crate (smartcard/token identities for free); macOS — `security-framework` (`SecIdentity`, `SecKeyCreateSignature`). Verification UI chain validation: [`rustls-platform-verifier`](https://github.com/rustls/rustls-platform-verifier). A genuinely solid part of the Rust ecosystem.

## 7. GUI Frameworks

| Framework | Big-canvas perf | Text/IME | Accessibility | Binary | Shipped proof | Verdict |
|---|---|---|---|---|---|---|
| **Tauri 2** | Good (render in Rust, blit to webview) | Best-in-class (browser text stack) | Best-in-class (webview) | [3–15 MB installer, 30–40 MB RAM](https://www.digitalapplied.com/blog/desktop-apps-web-stack-tauri-electron-deno-wails-2026) | **Slate PDF editor (AGPL), [Open PDF Studio](https://github.com/OpenAEC-Foundation/open-pdf-studio)** — direct prior art | **Recommended** |
| **egui** | Excellent (GPU immediate mode) | Historically weak; [IME overhauled recently](https://linebender.org/blog/tmil-15/) | [AccessKit](https://github.com/emilk/egui/pull/2294) | ~10 MB | Rerun viewer | Viable; text-heavy chrome is a grind |
| **Iced** | Very good | [Not IME-correct](https://rust-pc.github.io/rust-windows-gui.html) | Largely missing | small | COSMIC desktop | A11y gap disqualifying today |
| **Slint** | Good | Correct IME | Good (incl. Narrator) | small | Mostly embedded | Solid #2 native option; royalty-free tier requires attribution, or GPLv3, or paid |
| **GPUI** | Excellent | Correct IME | Improving | mid | Zed 1.0 (Apr 2026) | Pre-1.0 churn, sparse docs; [Zed deprioritized GPUI standalone](https://news.ycombinator.com/item?id=47003569) — high risk |
| **Dioxus native (Blitz)** | promising | — | — | — | [Alpha](https://blitz.is/about) | Too early |
| **flutter_rust_bridge** | Excellent (Skia/Impeller) | Excellent | Excellent | 20–40 MB | [pdfrx proves Flutter+PDFium](https://github.com/espresso3389/pdfrx) | Dark horse; UI is Dart not Rust |

**Tauri 2 specifics for a PDF editor:** don't use pdf.js as the primary render path (fine for v0; slower on large files, annotation editing limited). Render tiles with PDFium in the Rust core, ship bitmaps via custom URI scheme/response (never JSON IPC for pixels), composite in canvas/WebGL with virtualized pages. Selection/hit-testing from PDFium char boxes in Rust. This is exactly the architecture Slate and Open PDF Studio validate. Risks: WebView2 vs WKWebView drift; keeping pixels off the JSON bridge.

## 8. Recommended Stack

### Primary recommendation
- **Shell/UI:** **Tauri 2** (TS front-end) — commercial-grade text editing, IME, accessibility for free; tiny installers; two shipped open-source Rust PDF editors as prior art.
- **Rendering:** **PDFium (BSD-3) via `pdfium-render` (MIT)**, dynamic bblanchon binaries; tile-based renderer behind an **engine trait** (hayro slot-in later; fork fallbacks contain bus-factor risk).
- **Document model/writing:** **`lopdf` (MIT)** for object edits, annotations, AcroForm creation, **incremental saves**; **`pdf-writer`/`krilla` + `subsetter`** for new content and fonts; optional **qpdf** for repair/linearization of unsigned docs.
- **OCR:** Apple Vision (macOS) / Windows.Media.Ocr (Windows), Tesseract or RapidOCR-via-`ort` fallback; OCRmyPDF-style sandwich layer via lopdf.
- **Signatures:** RustCrypto `cms` + `x509-cert` + OS keystores + `rustls-platform-verifier`; PAdES B-B/B-T first, LT later; pyHanko as reference.
- **Office conversion:** cloud-side paid feature (or Solid Framework license); optional local best-effort LibreOffice subprocess.
- **Licensing posture:** everything MIT/Apache/BSD/MPL — fully open-core compatible. **Nothing AGPL in-process.**

### Credible alternatives
- **B — All-native:** Slint (royalty-free tier) or egui + same PDFium/lopdf core. No webview quirks, single language; but you build every text field and preferences pane yourself.
- **C — Pay Artifex:** MuPDF commercial + mupdf-rs. Best rendering + built-in redaction/editing primitives. Rational if funded.
- **D — Flutter + flutter_rust_bridge:** Rust core, Dart UI, pdfrx-proven. Choose if mobile matters; accept Dart UI + ~30 MB binaries.

### Does Rust make sense vs C++ (Qt) or Electron?
**Yes, with eyes open.** The hard, expensive parts are *PDF-domain* problems (text-run reconstruction, incremental writers, PAdES, appearance streams), not language problems — you'd hand-build them in C++ too (Qt only gives a viewer-grade QPdfDocument; Poppler is GPL). Rust's ecosystem now covers every layer permissively (C++ can't say that without Qt LGPL care or commercial SDKs); memory safety matters in a parser-heavy attack surface; Tauri erases Electron's only advantage at 5–10% of the footprint — and Electron would still need a native PDFium addon anyway. Honest costs: `pdfium-render` bus factor, no turnkey PAdES/PDF→Office (build or buy), Tier-3 text reflow is multi-quarter regardless of language. None of these favors C++ or Electron; two shipping Tauri+PDFium editors already demonstrate the stack works.
