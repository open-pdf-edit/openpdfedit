//! The PDF Standard Security Handler, revision 6 (AES-256).
//!
//! Password-protects a document: the result opens in any conforming
//! reader with the password, and refuses without it.
//!
//! ## Why this is hand-written rather than delegated
//!
//! Two obvious routes were tried and rejected on evidence, not taste:
//!
//! - **`lopdf`'s own `encrypt`** produces documents that cannot be read
//!   back — see `openpdfedit-doc`'s module doc. Its decryption is no
//!   better: given real AES-256 files from the pdf.js corpus, it returns
//!   `Ok` from `decrypt()` on a document with **zero** objects left.
//! - **PDFium** reads AES-256 correctly but cannot write it: its save
//!   preserves whatever encryption a document already had and offers no
//!   way to add or remove any.
//! - **qpdf** is correct and mature, but it is C++ — meaning either a
//!   binary users must install or a native link, and **neither works in
//!   the WebAssembly extension build**. Encryption would exist on the
//!   desktop and nowhere else.
//!
//! So: pure Rust, using the `aes`/`cbc`/`sha2` crates already in this
//! workspace's dependency graph. Revision 6 only. That is deliberate —
//! it is what current writers produce, and the older RC4-based revisions
//! matter for *reading* legacy files, which is a different feature.
//!
//! ## What is encrypted, and what must not be
//!
//! Per ISO 32000-2, encryption applies to every string and every stream
//! *after* any other filters, with these exceptions, all of which this
//! module honours:
//!
//! - the `/Encrypt` dictionary itself, and the strings inside it;
//! - the trailer's `/ID`;
//! - cross-reference streams, which a reader must parse *before* it can
//!   decrypt anything.
//!
//! The last one is handled structurally rather than by special-casing:
//! [`encrypt_document`] writes the result with a classic cross-reference
//! *table* and no object streams, so no such stream exists to get wrong.
//! It also sidesteps the upstream bug that broke `lopdf`'s own attempt.

use aes::cipher::{BlockCipherEncrypt, BlockModeEncrypt, KeyInit, KeyIvInit};
use lopdf::{Dictionary, Document, Object, SaveOptions};
use rand::rngs::SysRng;
use rand::TryRng;
use sha2::{Digest, Sha256, Sha384, Sha512};
use thiserror::Error;

type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;
type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;

#[derive(Debug, Error)]
pub enum CryptError {
    #[error("couldn't read the document: {0}")]
    Load(String),
    #[error("couldn't write the encrypted document: {0}")]
    Save(String),
    #[error("this document is already password-protected")]
    AlreadyEncrypted,
    #[error(
        "this document couldn't be read well enough to encrypt it safely — it parsed to a \
         document with no pages, so encrypting it would produce a file nothing can open"
    )]
    NothingToEncrypt,
    #[error("a password is required")]
    EmptyPassword,
    #[error("couldn't gather randomness for the encryption key: {0}")]
    Random(String),
    #[error("encryption failed: {0}")]
    Internal(String),
}

/// What the recipient may do without the owner password.
///
/// These map to the `/P` bit flags (ISO 32000-2 table 22), which are
/// *permissions granted*: a set bit allows. Every reader enforces them
/// only by convention — they are not a security boundary, and a
/// determined recipient can ignore them. What actually protects the
/// document is the password.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Permissions {
    pub print: bool,
    pub modify: bool,
    pub copy: bool,
    pub annotate: bool,
    pub fill_forms: bool,
    pub extract_for_accessibility: bool,
    pub assemble: bool,
    pub print_high_resolution: bool,
}

impl Default for Permissions {
    /// Everything allowed — the sensible default for "I just want a
    /// password on this", which is what most people mean.
    fn default() -> Self {
        Self {
            print: true,
            modify: true,
            copy: true,
            annotate: true,
            fill_forms: true,
            extract_for_accessibility: true,
            assemble: true,
            print_high_resolution: true,
        }
    }
}

impl Permissions {
    /// The `/P` integer. Bits are numbered from 1 in the spec; bit 1 is
    /// the least significant. Bits 1-2 and 7-8 are reserved and must be
    /// 0, and all high reserved bits must be 1 — hence the mask rather
    /// than starting from zero.
    fn to_flags(self) -> i32 {
        let mut p: u32 = 0xFFFF_F0C0;
        let mut set = |bit: u32, allowed: bool| {
            if allowed {
                p |= 1 << (bit - 1);
            }
        };
        set(3, self.print);
        set(4, self.modify);
        set(5, self.copy);
        set(6, self.annotate);
        set(9, self.fill_forms);
        set(10, self.extract_for_accessibility);
        set(11, self.assemble);
        set(12, self.print_high_resolution);
        p as i32
    }
}

/// Passwords are truncated to 127 bytes of UTF-8 (ISO 32000-2 7.6.4.3).
const MAX_PASSWORD_BYTES: usize = 127;

fn prepare_password(password: &str) -> Vec<u8> {
    // The spec calls for SASLprep normalization. This does the part that
    // matters in practice — UTF-8 encoding and the length cap — and
    // deliberately does not attempt the full profile: the failure mode
    // of getting SASLprep subtly wrong is a document whose password
    // doesn't work in another reader, so anything beyond plain UTF-8 is
    // worse than not trying. ASCII passwords, which is nearly all of
    // them, are unaffected either way.
    let mut bytes = password.as_bytes().to_vec();
    bytes.truncate(MAX_PASSWORD_BYTES);
    bytes
}

fn random_bytes<const N: usize>() -> Result<[u8; N], CryptError> {
    let mut out = [0u8; N];
    SysRng
        .try_fill_bytes(&mut out)
        .map_err(|e| CryptError::Random(e.to_string()))?;
    Ok(out)
}

/// Algorithm 2.B — the revision 6 password hash.
///
/// An iterated construction that alternates SHA-256/384/512 with
/// AES-128-CBC, deliberately expensive to make guessing costly. It runs
/// at least 64 rounds and then keeps going until the last byte of the
/// AES output is small enough, so the round count itself depends on the
/// data.
fn hash_r6(password: &[u8], salt: &[u8], user_key: &[u8]) -> Vec<u8> {
    let mut k = {
        let mut h = Sha256::new();
        h.update(password);
        h.update(salt);
        h.update(user_key);
        h.finalize().to_vec()
    };

    let mut round = 0usize;
    loop {
        // K1 = 64 repetitions of (password || K || user_key).
        let mut k1 = Vec::with_capacity(64 * (password.len() + k.len() + user_key.len()));
        for _ in 0..64 {
            k1.extend_from_slice(password);
            k1.extend_from_slice(&k);
            k1.extend_from_slice(user_key);
        }

        // E = AES-128-CBC(key = K[0..16], iv = K[16..32]) over K1, no
        // padding — K1's length is always a multiple of 16 because K is.
        let key: [u8; 16] = k[0..16].try_into().expect("K is 32+ bytes");
        let iv: [u8; 16] = k[16..32].try_into().expect("K is 32+ bytes");
        let mut e = k1;
        let e_len = e.len();
        Aes128CbcEnc::new(&key.into(), &iv.into())
            .encrypt_padded::<aes::cipher::block_padding::NoPadding>(&mut e, e_len)
            .expect("K1 is block-aligned, so no padding is needed");

        // The next digest is chosen by the first 16 bytes of E, summed
        // and taken modulo 3.
        let sum: u32 = e[..16].iter().map(|b| u32::from(*b)).sum();
        k = match sum % 3 {
            0 => Sha256::digest(&e).to_vec(),
            1 => Sha384::digest(&e).to_vec(),
            _ => Sha512::digest(&e).to_vec(),
        };

        round += 1;
        // At least 64 rounds, then stop once E's last byte is small.
        if round >= 64 && usize::from(*e.last().expect("E is non-empty")) <= round - 32 {
            break;
        }
    }

    k.truncate(32);
    k
}

/// AES-256-CBC with a zero IV and no padding — how the file encryption
/// key is wrapped into `/UE` and `/OE`.
fn wrap_key(intermediate: &[u8], file_key: &[u8; 32]) -> Result<Vec<u8>, CryptError> {
    let key: [u8; 32] = intermediate
        .try_into()
        .map_err(|_| CryptError::Internal("intermediate key must be 32 bytes".into()))?;
    let mut buf = file_key.to_vec();
    Aes256CbcEnc::new(&key.into(), &[0u8; 16].into())
        .encrypt_padded::<aes::cipher::block_padding::NoPadding>(&mut buf, 32)
        .map_err(|_| CryptError::Internal("key wrapping failed".into()))?;
    Ok(buf)
}

/// AES-256-CBC with a random IV prepended and PKCS#7 padding — how every
/// string and stream in the document is encrypted.
fn encrypt_data(file_key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, CryptError> {
    let iv: [u8; 16] = random_bytes()?;
    // Room for the IV, the data, and up to a full block of padding.
    let padded_len = plaintext.len() + 16 - (plaintext.len() % 16);
    let mut out = Vec::with_capacity(16 + padded_len);
    out.extend_from_slice(&iv);
    out.extend_from_slice(plaintext);
    out.resize(16 + padded_len, 0);

    Aes256CbcEnc::new(&(*file_key).into(), &iv.into())
        .encrypt_padded::<aes::cipher::block_padding::Pkcs7>(&mut out[16..], plaintext.len())
        .map_err(|_| CryptError::Internal("data encryption failed".into()))?;
    Ok(out)
}

/// Algorithm 10 — the `/Perms` block, which lets a reader detect that
/// `/P` has been tampered with after the fact.
fn build_perms(file_key: &[u8; 32], p: i32, encrypt_metadata: bool) -> Result<Vec<u8>, CryptError> {
    let mut block = [0u8; 16];
    block[..4].copy_from_slice(&(p as u32).to_le_bytes());
    block[4..8].copy_from_slice(&[0xFF; 4]);
    block[8] = if encrypt_metadata { b'T' } else { b'F' };
    block[9..12].copy_from_slice(b"adb");
    block[12..16].copy_from_slice(&random_bytes::<4>()?);

    // A single block, so ECB and raw block encryption are the same
    // thing — no mode crate needed.
    let cipher = aes::Aes256::new(&(*file_key).into());
    let mut b = block.into();
    cipher.encrypt_block(&mut b);
    Ok(b.to_vec())
}

/// Password-protects `pdf`, returning the encrypted document.
///
/// `user_password` is what a reader is prompted for. `owner_password`
/// unlocks it with full permissions; passing the same string for both is
/// normal and is what "just put a password on it" means.
pub fn encrypt_document(
    pdf: &[u8],
    user_password: &str,
    owner_password: &str,
    permissions: Permissions,
) -> Result<Vec<u8>, CryptError> {
    if user_password.is_empty() {
        return Err(CryptError::EmptyPassword);
    }
    let mut doc = Document::load_mem(pdf).map_err(|e| CryptError::Load(e.to_string()))?;
    if doc.trailer.get(b"Encrypt").is_ok() {
        // Re-encrypting would need the current password first, and
        // silently double-encrypting produces a file nothing can open.
        return Err(CryptError::AlreadyEncrypted);
    }
    // A damaged document can parse into an object graph with no pages
    // at all — PDFium recovers such files leniently, `lopdf` does not.
    // Encrypting *that* writes out a password-protected file which is
    // empty, and the password would be blamed for the emptiness. Found
    // via a deliberately-fuzzed file in the test corpus, where the plain
    // round-trip was already destroying the document before encryption
    // was involved.
    if doc.get_pages().is_empty() {
        return Err(CryptError::NothingToEncrypt);
    }

    let user_pw = prepare_password(user_password);
    let owner_pw = prepare_password(if owner_password.is_empty() {
        user_password
    } else {
        owner_password
    });

    let file_key: [u8; 32] = random_bytes()?;

    // Algorithm 8 — /U and /UE.
    let user_validation_salt: [u8; 8] = random_bytes()?;
    let user_key_salt: [u8; 8] = random_bytes()?;
    let mut u = hash_r6(&user_pw, &user_validation_salt, &[]);
    u.extend_from_slice(&user_validation_salt);
    u.extend_from_slice(&user_key_salt);
    let ue = wrap_key(&hash_r6(&user_pw, &user_key_salt, &[]), &file_key)?;

    // Algorithm 9 — /O and /OE. These hash the *whole* 48-byte /U as
    // well, which is what binds the owner password to this specific
    // document rather than to the password alone.
    let owner_validation_salt: [u8; 8] = random_bytes()?;
    let owner_key_salt: [u8; 8] = random_bytes()?;
    let mut o = hash_r6(&owner_pw, &owner_validation_salt, &u);
    o.extend_from_slice(&owner_validation_salt);
    o.extend_from_slice(&owner_key_salt);
    let oe = wrap_key(&hash_r6(&owner_pw, &owner_key_salt, &u), &file_key)?;

    let p = permissions.to_flags();
    let perms = build_perms(&file_key, p, true)?;

    // Encrypt the body *before* the /Encrypt dictionary is attached, so
    // its own strings can't be caught by the sweep.
    encrypt_body(&mut doc, &file_key)?;

    let encrypt_dict = Dictionary::from_iter([
        ("Filter", Object::Name(b"Standard".to_vec())),
        ("V", Object::Integer(5)),
        ("R", Object::Integer(6)),
        ("Length", Object::Integer(256)),
        (
            "CF",
            Object::Dictionary(Dictionary::from_iter([(
                "StdCF",
                Object::Dictionary(Dictionary::from_iter([
                    ("CFM", Object::Name(b"AESV3".to_vec())),
                    ("AuthEvent", Object::Name(b"DocOpen".to_vec())),
                    ("Length", Object::Integer(32)),
                ])),
            )])),
        ),
        ("StmF", Object::Name(b"StdCF".to_vec())),
        ("StrF", Object::Name(b"StdCF".to_vec())),
        ("U", Object::String(u, lopdf::StringFormat::Hexadecimal)),
        ("UE", Object::String(ue, lopdf::StringFormat::Hexadecimal)),
        ("O", Object::String(o, lopdf::StringFormat::Hexadecimal)),
        ("OE", Object::String(oe, lopdf::StringFormat::Hexadecimal)),
        ("P", Object::Integer(i64::from(p))),
        (
            "Perms",
            Object::String(perms, lopdf::StringFormat::Hexadecimal),
        ),
        ("EncryptMetadata", Object::Boolean(true)),
    ]);
    let encrypt_id = doc.add_object(Object::Dictionary(encrypt_dict));
    doc.trailer.set("Encrypt", Object::Reference(encrypt_id));

    // /ID is required once a document is encrypted, and must not itself
    // be encrypted. A document that lacks one gets a fresh pair.
    if doc.trailer.get(b"ID").is_err() {
        let id: [u8; 16] = random_bytes()?;
        let entry = Object::String(id.to_vec(), lopdf::StringFormat::Hexadecimal);
        doc.trailer
            .set("ID", Object::Array(vec![entry.clone(), entry]));
    }

    // A classic cross-reference table and no object streams: a reader
    // must parse the cross-reference *before* it can decrypt anything,
    // so an encrypted xref stream is unreadable by construction. Not
    // writing one at all is simpler and safer than special-casing it —
    // and it avoids the upstream lopdf bug that forces xref streams the
    // moment a document is encrypted.
    let mut out = Vec::with_capacity(pdf.len() + 4096);
    doc.save_with_options(
        &mut out,
        SaveOptions::builder()
            .use_object_streams(false)
            .use_xref_streams(false)
            .build(),
    )
    .map_err(|e| CryptError::Save(e.to_string()))?;
    Ok(out)
}

/// Encrypts every string and stream in the document.
fn encrypt_body(doc: &mut Document, file_key: &[u8; 32]) -> Result<(), CryptError> {
    let ids: Vec<_> = doc.objects.keys().copied().collect();
    for id in ids {
        let Some(object) = doc.objects.remove(&id) else {
            continue;
        };
        let encrypted = encrypt_object(object, file_key)?;
        doc.objects.insert(id, encrypted);
    }
    Ok(())
}

fn encrypt_object(object: Object, file_key: &[u8; 32]) -> Result<Object, CryptError> {
    Ok(match object {
        Object::String(bytes, format) => Object::String(encrypt_data(file_key, &bytes)?, format),
        Object::Array(items) => Object::Array(
            items
                .into_iter()
                .map(|item| encrypt_object(item, file_key))
                .collect::<Result<_, _>>()?,
        ),
        Object::Dictionary(dict) => Object::Dictionary(encrypt_dictionary(dict, file_key)?),
        Object::Stream(mut stream) => {
            stream.dict = encrypt_dictionary(stream.dict, file_key)?;
            let content = encrypt_data(file_key, &stream.content)?;
            stream.set_content(content);
            // The stored bytes are now ciphertext and final. Letting the
            // writer compress them afterwards would both corrupt them
            // and contradict the /Filter already on the dictionary.
            stream.allows_compression = false;
            Object::Stream(stream)
        }
        other => other,
    })
}

fn encrypt_dictionary(dict: Dictionary, file_key: &[u8; 32]) -> Result<Dictionary, CryptError> {
    let mut out = Dictionary::new();
    for (key, value) in dict.into_iter() {
        out.set(key, encrypt_object(value, file_key)?);
    }
    Ok(out)
}

/// True if `pdf` carries a `/Encrypt` entry — i.e. a password is needed
/// to open it.
pub fn is_encrypted(pdf: &[u8]) -> bool {
    // Checked on the raw bytes rather than by parsing: `lopdf` cannot
    // load an encrypted document's object graph at all (its objects live
    // in encrypted object streams it won't expand), so a parse-based
    // answer would be unreliable on exactly the files this question is
    // asked about.
    find_subslice(pdf, b"/Encrypt").is_some()
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}
