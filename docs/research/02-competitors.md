# Research Report 2 — Competitive Landscape: Desktop & Web PDF Editors (excluding Acrobat)

*Research date: 2026-08-01. Produced by a deep-research agent from ~23 web searches/fetches; citations inline.*

## 1. Per-Competitor Profiles

### 1.1 Foxit PDF Editor
- **Platforms:** Windows, macOS, Linux (reader), iOS, Android, Web. Proprietary C++ engine (also sold as Foxit PDF SDK to OEMs).
- **Pricing:** PDF Editor Standard ~$10.99/mo or ~$159.99/yr; PDF Editor+ ~$300/yr (adds cross-platform, 150GB cloud, 150 eSign envelopes/yr, AI Smart Redact). Perpetual license still exists (~$199–249 one-time, desktop-only, no AI) but is deliberately not published — hidden behind resellers. AI Assistant add-on $49.99/yr ([Scribe pricing breakdown](https://scribehow.com/page/Foxit_PDF_Editor_Pricing_2026_All_Plans_Costs_and_Hidden_Fees_Revealed__p4Ow1w31QXSZLbvZJidzvQ)).
- **Free tier:** Foxit PDF Reader (view, annotate, fill, sign); all editing paid.
- **Standout:** Closest full Acrobat replacement; claims 750M+ users.
- **Weaknesses:** Now widely seen as bloated; subscription devices randomly sign out into trial mode; UI reshuffles with updates.

### 1.2 Nitro PDF Pro
- **Platforms:** Windows, macOS (separate codebase — the Mac app is the acquired PDFpen), Web. PE-owned (Potentia, 2021).
- **Pricing:** ~$15–17.70/user/mo; "Classic" one-time $250 — **Windows-only and actually a 3-year term license, not perpetual** ([gonitro.com/pricing](https://www.gonitro.com/pricing), [ncored review](https://ncored.com/blog/nitro-pdf-review-2026.html)).
- **Free tier:** None (14-day trial).
- **Standout:** MS-Office-ribbon familiarity, batch conversion, integrated eSign; mid-size business site licenses.
- **Weaknesses:** No free tier; "one-time purchase" quietly converted to 3-year term; Mac/Windows feature disparity.

### 1.3 PDF-XChange Editor (Tracker Software)
- **Platforms:** **Windows only**.
- **Pricing:** True perpetual: Editor $56, Editor Plus ~$72, incl. 1 year of updates; deep volume discounts ([pricing](https://www.pdf-xchange.com/product/pdf-xchange-editor/pricing)).
- **Free tier:** Unusually generous — ~70% of features free including **free OCR**; paid features used free stamp a **watermark** ([PCWorld](https://www.pcworld.com/article/2171515/pdf-xchange-editor-review.html)).
- **Standout:** The Reddit/power-user darling on Windows: cheap perpetual, extremely fast, absurdly deep feature set.
- **Weaknesses:** Windows-only (exploitable gap on macOS); UI cluttered/overwhelming/dated; watermark trap surprises free users.

### 1.4 PDF Expert (Readdle)
- **Platforms:** macOS, iPadOS, iOS only. Native Apple app.
- **Pricing:** $79.99/yr subscription; lifetime $139.99 (Mac only) or $199.99 (all devices) ([billing FAQ](https://support.readdle.com/pdfexpert/en_US/billing-subscription/the-pdf-expert-billing-faq)).
- **Free tier:** Read, annotate, fill forms, draw free; text editing, page management, merge, OCR, conversion paid.
- **Standout:** Best-feeling, most Mac-native PDF UX; proof that **Mac users pay a premium for native polish**.
- **Weaknesses:** Apple-only; subscription-first pivot angered long-time users; OCR/conversion weaker than Windows rivals.

### 1.5 UPDF (Superace)
- **Platforms:** Windows, macOS, iOS, Android (one license).
- **Pricing:** Pro ~$39.99–49.99/yr or **$69.99–79.99 lifetime**; **AI excluded from both, separate subscription even for lifetime buyers** ([thebusinessdive review](https://thebusinessdive.com/updf-review)).
- **Free tier:** View/annotate with limits; watermarked output until paid.
- **Standout:** Aggressive pricing, modern UI, AI features; heavy affiliate marketing.
- **Weaknesses:** Forced re-login on perpetual licenses; AI-not-in-lifetime bait-and-switch is the top complaint; email-only support.

### 1.6 PDFgear
- **Platforms:** Windows, macOS, iOS, Web (Singapore company).
- **Pricing:** **Entirely free — no watermarks, no limits, no account.** VC-funded; plans to monetize later via AI/cloud ([cisdem review](https://www.cisdem.com/resource/pdfgear-review.html)).
- **Standout:** Full text editing, conversion, OCR, AI copilot at $0; 5 stars on Trustpilot (10k+ reviews); currently **the default Reddit answer for "free Acrobat alternative"**.
- **Weaknesses:** Closed-source, unclear business model (rug-pull fear); Mac app is a port; no redaction/compare/enterprise; users hesitate to feed it sensitive docs.

### 1.7 Wondershare PDFelement
- **Platforms:** Windows, macOS, iOS, Android, Web.
- **Pricing:** ~$79.99/yr or **$129.99 perpetual**; AI credits separate ([TrustRadius](https://www.trustradius.com/products/pdfelement/pricing)).
- **Standout:** Acrobat-like breadth at half the price.
- **Weaknesses:** Trust destruction case study: upgrade pop-ups *after* buying lifetime; forced sign-in for OCR on perpetual; AI-chatbot-only support; refund refusals ([Trustpilot](https://www.trustpilot.com/review/pdf.wondershare.com)).

### 1.8 Sejda
- **Platforms:** Web + wrapped desktop app (grew out of the open-source `sejda` Java library). Netherlands/GDPR.
- **Pricing:** Free: **3 tasks/day**, ≤50MB/200 pages, OCR ≤10 pages. Paid: $5 week pass, ~$7.50/mo web, $63/yr desktop+web.
- **Standout:** One of few web editors that edits existing text decently; desktop processes locally; honest brand.
- **Weaknesses:** Task metering; desktop is a wrapped web app; limited advanced features.

### 1.9 Smallpdf
- **Platforms:** Web-first (Switzerland). ~30M+ monthly users, ~$17.5M revenue (2024) ([growjo](https://growjo.com/company/Smallpdf)).
- **Pricing:** Free: 2 tasks/day. Pro $9–15/mo; Teams $12/user/mo.
- **Standout:** Best-polished web UI; enormous SEO moat.
- **Weaknesses:** Mandatory cloud upload (privacy dealbreaker for legal/medical/finance); stingy free tier.

### 1.10 iLovePDF
- **Platforms:** Web-first (Barcelona). ~150–226M visits/month; revenue $20–40M/yr with ~57 people; 16M+ documents daily ([growjo](https://growjo.com/company/iLovePDF)).
- **Pricing:** Premium ~$6.61/mo annual. Free: no daily cap, no watermark, 25MB cap.
- **Standout:** Proof that tiny team + freemium web PDF tools = $20M+/yr.
- **Weaknesses:** Overlay-style editing, not true text editing; ads; upload-first design.

### 1.11 Xodo (Apryse)
- **Platforms:** Web, Windows/macOS/Linux (PDF Studio, Java, from Qoppa acquisition), mobile. Backed by Apryse (PDFTron) SDK.
- **Pricing:** Free: **1 action/day** web. Web $7.99/mo; PDF Studio $9.99/mo or **$240 perpetual**; Suite $14.99/mo.
- **Standout:** 40+ tools incl. redact, OCR, compare, AI.
- **Weaknesses:** Enshittification narrative — beloved free app metered to 1 action/day after acquisition; dated Java desktop UI.

### 1.12 macOS Preview (Apple)
- Built on PDFKit, bundled free. View, annotate, fill forms, sign, merge, reorder/rotate/delete, crop, redact (since Monterey), Quartz-filter export.
- **Hard limits:** cannot edit existing text at all, no OCR, can silently damage form data, no batch ([setapp guide](https://setapp.com/how-to/cant-edit-pdf-files-in-mac-preview)).
- **Market meaning:** Preview sets the **free floor on macOS** — a paid Mac product must beat Preview's speed and nativeness, not just features ([Ask HN](https://news.ycombinator.com/item?id=37995726)).

### 1.13 SumatraPDF
- Windows-only, GPLv3, C++, ~10MB portable exe on MuPDF. Free/donations; the author's ["Lessons from 15 years of SumatraPDF"](https://news.ycombinator.com/item?id=27968900) explains why donations don't fund development.
- Fastest viewer on Windows; beloved minimalism; viewer only.

### 1.14 Okular (KDE)
- Linux/Windows/macOS, GPL, Qt/Poppler. Best open-source annotation set, form filling, digital signatures on Linux. No content editing; non-native feel off Linux.

### 1.15 Stirling PDF (self-hosted)
- Docker/self-hosted web app (Java/Spring, shells to LibreOffice/OCRmyPDF) + desktop client. **77k+ GitHub stars, 25M+ downloads** — #1 PDF repo on GitHub ([github](https://github.com/Stirling-Tools/Stirling-PDF)).
- **Model:** Open core: all features free ≤5 users; Pro/Enterprise from $99/mo or ~$12/seat/mo for SSO/SAML, SCIM, audit logs, managed hosting ([Paid Offerings](https://docs.stirlingpdf.com/Paid-Offerings/)).
- **Standout:** 50+ tools, pipelines, API. **Weak at true text editing**; requires Docker; web UI not native.

### 1.16 LibreOffice Draw
- Free, MPL. Imports PDF as editable vector objects — but line-by-line text boxes, broken formatting, lost bookmarks: "a workaround, not a PDF editor."

### 1.17 ONLYOFFICE
- Desktop editors free (core AGPL open source); Docs Enterprise from $149 one-time. PDF editor behaves like a word processor + form creation; Reddit pick for open-source users. Reflow approach mangles complex print-oriented PDFs.

### 1.18 Canva / Google Docs
- Canva: free PDF import → converts to editable design; bad fidelity. After acquiring Serif, Canva made **Affinity free forever**. Google Docs: converts to Docs, loses layout. Both prove massive demand for "just let me change this PDF."

## 2. Market-Wide Free-vs-Paid Line

Legend: **F** = typically free; **F/P** = split; **P** = almost always paid; **P+** = paid add-on above base subscription.

| # | Feature | Typical line | Notes |
|---|---------|:---:|-------|
| 1 | View/read | F | Universal loss-leader |
| 2 | Search text | F | Universal |
| 3 | Annotate/highlight/comment | F | Free everywhere |
| 4 | Fill forms (AcroForms) | F | Free in Preview, Foxit Reader, PDF Expert free, Okular |
| 5 | Basic signature (draw/image) | F | Free in most readers |
| 6 | Certificate-based digital signing | F/P | Free in Okular/PDF-XChange; paid in Nitro/Foxit |
| 7 | Merge/split | F/P | Free in web tools and PDFgear/Stirling; paid in PDF Expert |
| 8 | Reorder/rotate/delete pages | F/P | Free in Preview/Stirling/PDFgear |
| 9 | Compress | F/P | Free metered on web; unlimited = paid |
| 10 | PDF → Word/Excel/PPT | F/P | The classic meter: daily caps free, unlimited paid |
| 11 | Office → PDF create | F/P | Same pattern |
| 12 | **Edit existing text** | **P** | The single clearest paywall (exceptions: PDFgear free, LibreOffice/ONLYOFFICE clunky-free, Sejda metered) |
| 13 | Edit/replace images | P | Paired with text editing |
| 14 | Add/edit links | P | Mostly paid |
| 15 | Headers/footers/Bates numbering | P | Pro tiers |
| 16 | Watermark add/remove | P | Free tools *add their own* watermark instead |
| 17 | **OCR** | **P** | Second-clearest paywall; exceptions: PDF-XChange free OCR, PDFgear, Stirling, Sejda ≤10 pages |
| 18 | Searchable-OCR batch | P | Always pro |
| 19 | Create fillable forms | P | Pro everywhere except ONLYOFFICE |
| 20 | **Redaction (true, flattened)** | **P** | Pro-only nearly everywhere |
| 21 | Encrypt/password protect | F/P | Free on web tools & Stirling; paid in desktop suites |
| 22 | Remove password/permissions | F/P | Metered free on web |
| 23 | Flatten annotations/forms | F/P | Often buried in paid tiers |
| 24 | **Batch processing** | **P** | Universal pro feature |
| 25 | **Document compare** | **P** | Top tiers only |
| 26 | Page numbering/organize automation | F/P | Free in Stirling pipelines |
| 27 | eSign workflows (send, audit trail) | P/P+ | Envelope-metered SaaS |
| 28 | Cloud sync/storage | P | Subscription hook |
| 29 | **AI chat/summarize/translate** | **P+** | Credit-metered add-on even above paid plans; PDFgear free as a wedge |
| 30 | AI smart redact / PII detection | P+ | Newest top-tier upsell |
| 31 | Accessibility tagging (PDF/UA) | P | Rare outside Acrobat — genuinely underserved |
| 32 | API/automation/CLI | P | Sold separately |

**Pattern:** the market's paywall sits precisely at *"change what the document says"* (text edit, OCR, redact) and *"do it at scale"* (batch, API, compare), while *"read and mark up"* is free everywhere. AI is a **second paywall above the paywall** — and lifetime-license holders being excluded from AI is the #1 new complaint theme of 2025–26.

## 3. Gaps & Differentiation Opportunities

### Recurring complaints across the market
1. **Subscription fatigue / the "Adobe Tax."**
2. **Fake "lifetime" licenses** (Nitro's 3-year "one-time"; Wondershare sign-in walls; UPDF AI exclusion). **Trustworthy, verifiable offline perpetual licensing is itself a feature.**
3. **Forced cloud upload for sensitive documents.** CrabPDF's Show HN traction (privacy-first, local-only, April 2026) validated the positioning ([HN thread](https://news.ycombinator.com/item?id=47916180)) — and a native desktop app avoids even the "browser = cloud" perception a local web app fights.
4. **Bloat and clutter.** Nobody in the Acrobat-alternative space is simultaneously *fast, minimal, and full-featured*.
5. **Poor macOS citizenship.** PDF-XChange has no Mac version; only PDF Expert is truly native — and it's Apple-only, subscription-first. **A native, fast Mac editor with PDF-XChange-class features is an open lane.**
6. **Account walls and nags.** "No account required" is a repeated praise line for PDFgear.
7. **No trustworthy full open-source editor.** Recurring Ask HN threads ([2019](https://news.ycombinator.com/item?id=18762856), [2023](https://news.ycombinator.com/item?id=37995726)); Stirling's 77k stars despite weak text editing shows the demand.
8. **AI shoved in, then charged twice.**

### Where an open-source, Rust, native, offline-first editor wins
- Position against the four trust failures: (a) truly perpetual license, (b) fully offline no-account, (c) auditable open source, (d) no upsell nags in paid mode. Every major competitor violates at least two.
- **Performance as marketing**: "startup in <100ms, 10MB binary, opens a 2,000-page PDF instantly" is a shareable benchmark story.
- **The macOS + Windows dual-native gap**: PDF-XChange's depth + PDF Expert's polish on both OSes — a combination no one ships.
- **Selective free-tier generosity**: free OCR (proven adoption driver) and free true redaction (privacy-audience magnet).
- **Underserved pro niches**: document compare, PDF/UA tagging, Bates numbering, verified redaction certificates, CLI/scripting.
- **Threats**: PDFgear sets the free bar; Stirling owns self-hosted; Affinity-style free-forever moves show incumbents can nuke price floors. Defense: the trust/permanence story PDFgear can't tell and the native-desktop story Stirling can't tell.

## 4. Business Model Evidence

| App | Model | Key mechanics |
|---|---|---|
| **Obsidian** | Free proprietary app + paid services | Revenue from Sync ($4/mo) and Publish ($8/mo) — services, not features |
| **Aseprite** | Source-available, pay-for-binaries | Code public under no-redistribution EULA; ~$20 binaries sell in volume anyway |
| **Zed** | GPL/AGPL open source + paid services | Hosted collaboration + server-side AI compute |
| **Stirling PDF** | Open core | Free ≤5 users; paid line is *organizational* features (SSO, SCIM, audit), not user features |
| **RustDesk** | FOSS client + paid Pro server | Rust precedent |
| **PDF-XChange** | Freemium + cheap perpetual | ~70% free (watermarked pro output) → $56 perpetual; sustained 20+ years |
| **Affinity (pre-2025)** | One-time $70/app | Canva acquired and made it free forever (Oct 2025) — one-time pricing is vulnerable to acquirer price-zeroing; open source is the hedge that makes "free forever" credible *and* irreversible ([critique](https://wilkinson.graphics/blog/2025-11-01-affinity-is-free/)) |
| **iLovePDF** | Freemium web | ~57 people → $20–40M/yr |

### What people demonstrably pay for
1. **Editing existing text with fidelity** (every $50–250 license fundamentally sells this)
2. **OCR**
3. **Unlimited conversion**
4. **Redaction + compliance** (legal/healthcare; pattern search, certificates, audit logs)
5. **Batch/automation/API** and **team/org features**
6. **eSign envelopes**
7. **AI document chat** (monetizes, but generates the most resentment)

### Suggested synthesis
Open-source (auditable) core with free view/annotate/forms/merge/organize + free OCR as the adoption wedge; a genuinely perpetual, offline-verifiable personal license (~$50–80, PDF-XChange's proven sweet spot) covering text/image editing, redaction, compare, batch; and org-level recurring revenue (multi-seat, SSO/deployment, automation/API, optional AI with local-model option) as the subscription layer — Stirling's org-line + Aseprite's pay-for-binaries + PDF-XChange's price point, differentiated by dual-native speed and a trust story every incumbent has burned.
