# Research Report 4 — Open-Source Reuse & Reference Map

*Research date: 2026-08-01. Produced by a deep-research agent; stars/activity pulled from GitHub/crates.io APIs on that date. Licensing verdicts assume an MIT/Apache-2.0 app core with proprietary premium add-ons (GPL/AGPL untouchable for reuse; MPL usable file-by-file; LGPL only via dynamic linking or separate process).*

**Headline findings:**
1. **PDF4QT relicensed from LGPL-3 to MIT on 2025-04-27** — a full-featured C++ PDF *editor* (rendering, annotations, forms, signatures, optimization, encryption, document diff, redaction) is now permissively licensed. The single biggest reuse opportunity nobody expects ([repo](https://github.com/JakubMelka/PDF4QT)).
2. **The Rust-native stack is now real**: hayro (pure-Rust renderer, Apache/MIT) + krilla/pdf-writer + lopdf + cosmic-text + vello/resvg covers render→write permissively.
3. **pyHanko is MIT** — a complete, spec-serious PAdES/PDF signing implementation to port.
4. **Stirling-PDF is open-core MIT** (proprietary code isolated in `app/proprietary/`); MIT parts safe to study/reuse; largest PDF-tool community (88.5k stars).

## 1. Viewers / Editors

| Project | License (SPDX) | Language | Signals (2026-08-01) | Reuse verdict |
|---|---|---|---|---|
| [PDF4QT](https://github.com/JakubMelka/PDF4QT) | **MIT** (since 2025-04-27) | C++20/Qt6 | 1,433 stars, pushed 2026-07-12, solo-lead | **Port/copy code. The crown jewel** |
| [pdf.js](https://github.com/mozilla/pdf.js) | **Apache-2.0** | JavaScript | 53,653 stars, pushed 2026-07-31 | **Port/copy code** |
| [SumatraPDF](https://github.com/sumatrapdfreader/sumatrapdf) | GPL-3.0-only | C/C++ | 17,224 stars | Study only |
| [MuPDF](https://mupdf.com/) | **AGPL-3.0** (or Artifex commercial) | C | very active | **Avoid code entirely**; concepts only |
| [Okular](https://invent.kde.org/graphics/okular) | GPL-2.0-or-later | C++/Qt | KDE | Study only |
| Evince / [Papers](https://gitlab.gnome.org/GNOME/papers) | GPL-2.0-or-later | C / Rust+GTK4 | Papers is GNOME 49 default | Study only (Rust GPL viewer patterns) |
| [Zathura](https://github.com/pwmt/zathura) | **Zlib** (permissive!) | C | 3,226 stars | **Code reusable**; plugin-based document-backend architecture worth copying (its poppler/mupdf plugins carry those licenses) |
| [sioyek](https://github.com/ahrm/sioyek) | GPL-3.0-only | C++ | 9,738 stars | Study only — research-reading UX |
| [Xournal++](https://github.com/xournalpp/xournalpp) | GPL-2.0-or-later | C++/GTK | 15,078 stars | Study only — pen/annotation UX reference |
| [PDFium](https://pdfium.googlesource.com/pdfium/) | **BSD-3-Clause** | C++ | daily commits (Chrome) | **Link it.** Copy from `examples/` and Chromium's `pdf/` component (BSD-3) for form-fill/find/accessibility patterns |
| [Skia PDF backend](https://skia.org/docs/user/sample/pdf/) | BSD-3-Clause | C++ | Google | Reference for display-list→PDF output |

**Details worth acting on:**
- **PDF4QT**: core `Pdf4QtLib` + apps (viewer, editor, CLI PdfTool, page-master) + editor plugin system (object editing, dimensions, sign, **redaction**). Implements PDF 2.0 directly (no Poppler), Blend2D/Qt raster, OpenSSL crypto. MIT means you can translate whole algorithms (transparency groups, blend modes, AcroForm widgets, signature validation flow) into Rust line-by-line, legally. Weakness: bus factor 1, Windows/Linux focus (macOS port is your work).
- **pdf.js**: ships an **AnnotationEditorLayer** (FreeText, Ink, Highlight, Stamp, signature editor) that serializes real PDF annotation objects, plus full **AcroForm fill + JS scripting sandbox** and XFA rendering. Apache-2.0: port its algorithms (font sanitizer, CMap handling, annotation appearance-stream generation) directly ([editor layer guide](https://www.nutrient.io/blog/pdfjs-annotation-editor-layer/)).
- **hayro's NOTICE.md proves the porting pattern is normal and legal**: hayro ports code from PDFBox (font encodings, type-0 functions) and pdf.js (color conversion, decompression, AES/MD5/SHA/RC4) under Apache-2.0 ([NOTICE](https://github.com/LaurenzV/hayro/blob/main/NOTICE.md)).

## 2. Manipulation / Tools

| Project | License | Language | Signals | Reuse verdict |
|---|---|---|---|---|
| [qpdf](https://github.com/qpdf/qpdf) | **Apache-2.0** | C++ | 5,277 stars, exemplary maintenance | **Link (C API) or port.** Gold standard for lossless transforms: linearization, object streams, encryption, JSON round-trip |
| [Stirling-PDF](https://github.com/Stirling-Tools/Stirling-PDF) | **MIT core** + proprietary `app/proprietary/` ([LICENSE](https://github.com/Stirling-Tools/Stirling-PDF/blob/main/LICENSE)) | Java | 88,510 stars, very high velocity | MIT parts reusable; most valuable as **feature taxonomy and open-core precedent** |
| [OCRmyPDF](https://github.com/ocrmypdf/OCRmyPDF) | **MPL-2.0** | Python | 34,333 stars | **The** OCR-pipeline architecture reference. Clean-room Rust reimplementation is unencumbered; direct file ports only obligate sharing those files |
| [pikepdf](https://github.com/pikepdf/pikepdf) | MPL-2.0 | Python/qpdf | 2,770 stars | Reference for a pleasant high-level API over qpdf semantics |
| [Apache PDFBox](https://github.com/apache/pdfbox) | **Apache-2.0** | Java | 3,100 stars, ASF, 20+ yrs | **Port freely.** Best spec-compliant reference for AcroForm logic, appearance streams, signature creation/validation, font handling |
| iText 7 | AGPL-3.0 (dual) | Java | — | **Avoid entirely** (not even close translation) |
| [pdfcpu](https://github.com/pdfcpu/pdfcpu) | **Apache-2.0** | Go | 8,742 stars | **Port logic.** Best open reference for validation/lint (strict/relaxed), optimization, watermark/stamp, booklet/n-up; Go→Rust ports are cheap |
| Ghostscript | AGPL-3.0 | C | Artifex | **Avoid**; use qpdf + image recoders instead |
| [Tesseract](https://github.com/tesseract-ocr/tesseract) | Apache-2.0 | C++ | 75,669 stars | **Link/bundle** as OCR engine |

## 3. Rust Ecosystem (actual building blocks)

All dual **MIT OR Apache-2.0** unless noted — zero friction for the model.

| Crate/Project | License | Signals | What it gives you |
|---|---|---|---|
| [hayro](https://github.com/LaurenzV/hayro) | Apache-2.0 OR MIT | 728 stars, v0.7.1 (2026-06), fast-moving | Most feature-complete pure-Rust PDF renderer; long-term escape from PDFium |
| [krilla](https://github.com/LaurenzV/krilla) | MIT OR Apache-2.0 | 425 stars, v0.8.2 | High-level PDF creation: font subsetting, PDF/A, PDF/UA tagging (typst's export path) |
| [pdf-writer](https://github.com/typst/pdf-writer) | MIT OR Apache-2.0 | 733 stars, v0.15.0 | Low-level typed PDF serializer — writer substrate for incremental saves/appearance streams |
| [typst](https://github.com/typst/typst) | Apache-2.0 | 55,226 stars | Study layout/font pipeline (rustybuzz, subsetting) for "add text box"; whole PDF export chain reusable |
| [lopdf](https://github.com/J-F-Liu/lopdf) | MIT | 2,209 stars | Object-level read/edit/write; page ops, incremental updates, merging |
| [pdf-rs/pdf](https://github.com/pdf-rs/pdf) | MIT | 1,686 stars | Alternative typed parser; lazy object resolution ideas |
| [pdfium-render](https://github.com/ajrcarey/pdfium-render) | MIT OR Apache-2.0 | 689 stars, v0.9.x | **Day-1 engine**: rendering, text extraction, form fill, annotation create/inspect, page-object editing, signature inspection, WASM. `examples/` is a viewer-building tutorial |
| [vello](https://github.com/linebender/vello) | Apache-2.0 OR MIT | 4,228 stars, v0.9.0 | GPU 2D renderer — hayro's natural raster target |
| [resvg](https://github.com/linebender/resvg) | Apache-2.0 OR MIT | 3,974 stars | SVG import for stamps/vector assets; tiny-skia CPU fallback |
| [cosmic-text](https://github.com/pop-os/cosmic-text) | MIT OR Apache-2.0 | 2,119 stars, v0.19.0 | Shaping + bidi + multi-line **editing** buffer — core of FreeText/text-edit widget |
| [printpdf](https://github.com/fschutt/printpdf) | MIT | v0.9.1 | Read+write+render ideas, WASM demo |
| [ocrs](https://github.com/robertknight/ocrs) | MIT OR Apache-2.0 | early preview | Pure-Rust neural OCR; accuracy behind Tesseract |
| [tdf](https://github.com/itsjunetime/tdf) | AGPL-3.0 (wraps MuPDF) | — | Avoid; illustrates MuPDF wrapper contamination |
| [oxidize-pdf](https://github.com/bzsanti/oxidizePdf) | **Conflicting: GPL-3.0 on crates.io v1.0.0 vs MIT on GitHub** | — | **Treat as GPL until clarified**; license hygiene red flag |
| [pdf_oxide](https://github.com/yfedoseev/pdf_oxide) | MIT/Apache claimed | new | Watch; verify before depending |

## 4. E-Sign / Signatures

| Project | License | Language | Verdict |
|---|---|---|---|
| [pyHanko](https://github.com/MatthiasValvekens/pyHanko) | **MIT** | Python | **Port it.** Complete PAdES B-B/B-T/B-LT/B-LTA, CMS construction, timestamps, validation, incremental-update-safe signing. The best permissive signing codebase in existence (748 stars, meticulous) |
| [EU DSS](https://github.com/esig/dss) | LGPL-2.1 | Java | Don't port; use as **conformance oracle** for eIDAS credibility |
| [PDFBox signature examples](https://github.com/apache/pdfbox/tree/trunk/examples/src/main/java/org/apache/pdfbox/examples/signature) | Apache-2.0 | Java | Port-friendly incl. visible signatures, CMS, LTV |
| [pdf_signing](https://github.com/ralpha/pdf_signing) / trust_pdf | check per-crate | Rust | Starting points only |
| RustCrypto [`cms`](https://crates.io/crates/cms)/`x509-cert`/`rsa` | Apache/MIT | Rust | The actual CMS/PKCS#7 building blocks |

## The Borrow Map

**COPY** = permissive, port/link directly (keep Apache NOTICE attributions). **STUDY** = copyleft — architecture/behavior only; never closely translate GPL/AGPL source.

| Subsystem | Best source to COPY/port | Best reference to STUDY | Verdict |
|---|---|---|---|
| **Rendering** | PDFium via pdfium-render now; migrate to hayro as it matures; raster via vello | pdf.js internals (also copyable); MuPDF concepts only | Compatible. Never link MuPDF |
| **Page ops** | qpdf (link/port) + lopdf | pdfcpu (also copyable); Stirling for taxonomy | Compatible |
| **Annotations** | pdf.js AnnotationEditorLayer (appearance-stream generation, editor state model); PDF4QT annotation code | Xournal++ (GPL) for pen UX feel only | Compatible |
| **Forms (AcroForm)** | PDFBox `PDAcroForm` + pdf.js forms/scripting; runtime fill via pdfium-render | PDF4QT form widgets (also copyable) | Compatible |
| **Text editing in place** (hardest) | **PDF4QT page-content editor plugin (MIT — the only permissive shipped implementation)**; rebuild runs with cosmic-text + krilla/pdf-writer; PDFium page-object APIs | Skia PDF backend; commercial editors for UX | Compatible — PDF4QT's MIT relicense is decisive |
| **OCR pipeline** | Tesseract (subprocess/leptess) or ocrs; port hOCR→PDF overlay technique | OCRmyPDF (MPL) pipeline: deskew → OCR → invisible text layer → optimize | Compatible |
| **Signatures / PAdES** | pyHanko (MIT) onto RustCrypto cms + pdf-writer incremental updates; PDFBox examples | EU DSS as external conformance oracle | Compatible |
| **Redaction** | PDF4QT redaction plugin (MIT) + qpdf-style object surgery via lopdf | Stirling redaction (MIT parts); pdfcpu content-stream filtering | Compatible |
| **Compress/optimize** | qpdf (object streams, linearization) + pdfcpu logic + oxipng/mozjpeg/image crates | OCRmyPDF optimizer stage; **never Ghostscript** | Compatible sans Ghostscript |
| **PDF writing/export** | pdf-writer + krilla (PDF/A, tagging, subsetting) | Skia SkPDF (also copyable); typst export chain | Compatible |
| **Validation/lint** | pdfcpu validation modes (port to Rust) | veraPDF (use MPL branch or external checker) | Compatible |
| **Viewer UI patterns** | pdf.js viewer (thumbnails, find bar, virtualized scrolling); PDF4QT Qt chrome; Zathura plugin architecture | SumatraPDF (startup speed, caching, tabs), sioyek, Papers | Copy column compatible; study column no-code |

### Practical license rules
- **Green (copy/link freely):** MIT, Apache-2.0 (preserve NOTICE), BSD-3, Zlib. Apache's patent grant is a plus for PDF.
- **Yellow:** MPL-2.0 (copied *files* stay MPL and must be published; reimplementation of the *pipeline* carries no obligation). LGPL-2.1 — dynamic-link/separate-process only.
- **Red (no code, no close translation, ever — including into the MIT core, since premium builds on it):** GPL-2/3, AGPL. Architecture/UX study fine; side-by-side porting is not.
- **Verify before use:** oxidize-pdf (license contradiction); small Rust signing crates.

**Suggested spine:** pdfium-render (render/forms day-1) + lopdf/qpdf (object surgery) + pdf-writer/krilla (writing, PDF/A) + cosmic-text (text UI) + ported pyHanko (signing) + ported PDF4QT algorithms (editing/redaction) + Tesseract-or-platform-OCR behind an OCRmyPDF-shaped pipeline — with hayro+vello as the planned pure-Rust rendering replacement.
