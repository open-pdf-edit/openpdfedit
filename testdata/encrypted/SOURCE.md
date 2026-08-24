# Encrypted test fixtures

AES-256 (`/V 5 /R 6 /AESV3`) documents from the
[pdf.js test suite](https://github.com/mozilla/pdf.js/tree/master/test/pdfs),
used by `openpdfedit-crypt`'s tests.

| file | password |
|---|---|
| `pr6531_1.pdf` | `asdfasdf` |
| `pr6531_2.pdf` | `asdfasdf` |

They matter because this project's own encryption can't validate its own
decryption — both halves sharing one bug would agree with each other.
These were produced by other tools entirely, so decrypting them is
independent evidence.

Committed rather than fetched: they're small, and a test that silently
skips when a download fails is worse than no test.
