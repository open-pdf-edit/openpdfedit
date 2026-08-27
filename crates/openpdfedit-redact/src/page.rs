//! Redacting a page, including the things a page only points at.
//!
//! A content stream is not where a page's content necessarily lives. It
//! routinely says `/Fm1 Do` — draw that form over there — or `/Im3 Do`,
//! and the words being redacted are inside the thing pointed at, not in
//! the stream doing the pointing. Rewriting only the stream leaves them
//! where they were.
//!
//! This module is the half of redaction that needs the document rather
//! than just the bytes: it resolves what each `Do` refers to, follows
//! it, and does the removal there — rewriting a form's own content
//! stream, or clearing an image's pixels.
//!
//! ## Copying before cutting
//!
//! Resources are shared. One image object commonly backs every page of
//! a scan; one form is reused across a document's pages. Editing the
//! object in place would put this page's redaction on all of them, in
//! whatever place the rect happened to fall here. So every object on
//! the path from page to redacted content is copied first and the copy
//! is what gets edited — including the resource dictionaries in
//! between, which are shared just as often.
//!
//! ## Both directions of wrong
//!
//! Before this existed, a `Do` was handled by mapping the unit square
//! through the CTM and dropping the operator if the result overlapped.
//! For an image that is the right placement and the wrong response; for
//! a form it is not even the right placement.
//!
//! - **Images were over-removed.** A scanned page is one image covering
//!   the whole page. Redacting one line of it overlapped the image, so
//!   the image was dropped, so the page went blank. The entire visible
//!   document, to hide one address.
//! - **Forms were under-removed.** A form's real placement is its
//!   `/BBox` through its `/Matrix` through the CTM, which is nowhere
//!   near the unit square once either of those scales. So the overlap
//!   test said no, nothing was dropped, and the text stayed on the page
//!   — fully extractable, under a black rectangle. That is exactly the
//!   failure this crate was written to prevent, arriving by a side door.
//!
//! The second one is why the box drawn on top is belt and braces and
//! not the mechanism. It is also why [`redact_page`] reports how many
//! operators it removed: zero, on a page that plainly had content in
//! the rect, means something was not understood.

use std::collections::BTreeMap;

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Object, ObjectId, Stream};
use openpdfedit_doc::Document;

use crate::pixels::{self, Fill};
use crate::{redact_content_resolving, Matrix, Rect, RedactError, RemovalScope, XObjectKind};

/// How far a form may nest before this gives up following it.
///
/// Forms containing forms is ordinary; forms containing forms eight
/// deep is not, and a `/Resources` cycle would otherwise recurse
/// forever.
const MAX_FORM_DEPTH: u32 = 8;

/// Redacts `rect` on `page_index`.
///
/// Removes the content that overlaps the rect — from the page's own
/// stream, from inside any form it draws, and from the pixels of any
/// image it draws — then paints a solid `color` box over the region.
///
/// The box is not the redaction. It is there so that a gap in this
/// interpreter's understanding of a page still shows nothing, and so
/// that the reader can see where something was taken out. Everything
/// under it is meant to be gone before it is drawn.
///
/// Does not save — call [`Document::save_incremental`] afterwards.
/// Returns the number of operators removed, counting those removed
/// inside forms.
pub fn redact_page(
    doc: &mut Document,
    page_index: u32,
    rect: Rect,
    color: [f32; 3],
) -> Result<usize, RedactError> {
    let original = doc.page_content_bytes(page_index)?;
    let resources = doc.page_resources(page_index)?;
    let kinds = xobject_kinds(doc, &resources);

    let redacted = redact_content_resolving(&original, rect, RemovalScope::Everything, &|name| {
        kinds
            .get(name)
            .map(|entry| entry.kind)
            .unwrap_or(XObjectKind::Unknown)
    })?;

    let mut removed = redacted.removed_operations;
    let mut bytes = redacted.bytes;
    // Names whose object could not be edited — the draw call has to go
    // instead, since something overlapping the rect must not survive.
    let mut undecodable: Vec<Vec<u8>> = Vec::new();

    for (name, regions) in group_by_name(&redacted.partial_xobjects) {
        let Some(&Entry { id, kind }) = kinds.get(&name) else {
            undecodable.push(name);
            continue;
        };
        match redact_xobject(doc, id, &regions, kind, 0)? {
            Some(Outcome::Replaced { id, removed_inside }) => {
                removed += removed_inside;
                let resource_name = String::from_utf8_lossy(&name).into_owned();
                doc.set_page_resource(page_index, "XObject", &resource_name, id)?;
            }
            Some(Outcome::Unchanged) => {}
            None => undecodable.push(name),
        }
    }

    if !undecodable.is_empty() {
        let (stripped, dropped) = drop_named_draws(&bytes, &undecodable)?;
        bytes = stripped;
        removed += dropped;
    }

    bytes.extend(overlay_box_bytes(rect, color)?);
    doc.set_page_contents(page_index, bytes)?;
    Ok(removed)
}

/// What became of one XObject the rect cut through.
enum Outcome {
    /// A redacted copy was made; point the resource at it.
    Replaced { id: ObjectId, removed_inside: usize },
    /// The rect turned out to overlap nothing inside it after all.
    Unchanged,
}

/// One entry of a page's (or form's) `/XObject` resources.
#[derive(Clone, Copy)]
struct Entry {
    id: ObjectId,
    kind: XObjectKind,
}

/// Reads an `/XObject` resource dictionary into the kinds the content
/// walker asks about.
fn xobject_kinds(doc: &Document, resources: &Dictionary) -> BTreeMap<Vec<u8>, Entry> {
    let mut out = BTreeMap::new();
    let entries = match resources.get(b"XObject").map(|o| doc.resolve(o)) {
        Ok(Object::Dictionary(d)) => d.clone(),
        _ => return out,
    };

    for (name, value) in entries.iter() {
        let Ok(id) = value.as_reference() else {
            continue;
        };
        let Ok(dict) = doc.dictionary_or_stream_dict(id) else {
            continue;
        };
        let subtype = dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok());
        let kind = match subtype {
            Some(b"Form") => match form_placement(doc, dict) {
                Some((bbox, matrix)) => XObjectKind::Form { bbox, matrix },
                // A form with no usable `/BBox` paints nothing that can
                // be located, and `/BBox` is required — so this is a
                // malformed object, not a shape to guess at.
                None => XObjectKind::Unknown,
            },
            Some(b"Image") => XObjectKind::Image,
            _ => XObjectKind::Unknown,
        };
        out.insert(name.to_vec(), Entry { id, kind });
    }
    out
}

/// A form's `/BBox` and `/Matrix`, the two things that say where it
/// lands. `/Matrix` is optional and defaults to the identity.
fn form_placement(doc: &Document, dict: &Dictionary) -> Option<(Rect, Matrix)> {
    let bbox = match doc.resolve(dict.get(b"BBox").ok()?) {
        Object::Array(items) if items.len() == 4 => {
            let v: Vec<f64> = items
                .iter()
                .map(|o| crate::number(doc.resolve(o)))
                .collect();
            // `/BBox` corners come in either order; normalise.
            Rect {
                x0: v[0].min(v[2]),
                y0: v[1].min(v[3]),
                x1: v[0].max(v[2]),
                y1: v[1].max(v[3]),
            }
        }
        _ => return None,
    };
    let matrix = match dict.get(b"Matrix").map(|o| doc.resolve(o)) {
        Ok(Object::Array(items)) if items.len() == 6 => {
            std::array::from_fn(|i| crate::number(doc.resolve(&items[i])))
        }
        _ => crate::IDENTITY,
    };
    Some((bbox, matrix))
}

/// Groups the walker's per-occurrence reports by resource name.
///
/// An XObject drawn twice on one page yields two regions, each in the
/// same local space but placed by a different CTM. Both holes go into
/// the one copy: an image drawn twice where the rect clips only one
/// occurrence ends up cleared in both, which removes more than was
/// asked and never less — the direction this crate errs in everywhere
/// else too.
fn group_by_name(partials: &[crate::PartialXObject]) -> Vec<(Vec<u8>, Vec<Rect>)> {
    let mut grouped: BTreeMap<Vec<u8>, Vec<Rect>> = BTreeMap::new();
    for partial in partials {
        grouped
            .entry(partial.name.clone())
            .or_default()
            .push(partial.rect);
    }
    grouped.into_iter().collect()
}

/// Makes a redacted copy of one XObject and returns its id.
fn redact_xobject(
    doc: &mut Document,
    id: ObjectId,
    regions: &[Rect],
    kind: XObjectKind,
    depth: u32,
) -> Result<Option<Outcome>, RedactError> {
    match kind {
        XObjectKind::Form { .. } => redact_form(doc, id, regions, depth),
        XObjectKind::Image => Ok(redact_image(doc, id, regions).map(|id| Outcome::Replaced {
            id,
            removed_inside: 0,
        })),
        XObjectKind::Unknown => Ok(None),
    }
}

/// Rewrites a form's content stream with each region removed, and
/// returns the copy.
fn redact_form(
    doc: &mut Document,
    id: ObjectId,
    regions: &[Rect],
    depth: u32,
) -> Result<Option<Outcome>, RedactError> {
    if depth >= MAX_FORM_DEPTH {
        return Ok(None);
    }
    let Ok(dict) = doc.dictionary_or_stream_dict(id) else {
        return Ok(None);
    };
    let mut dict = dict.clone();
    let mut bytes = doc.decoded_stream(id)?;

    let resources = match dict.get(b"Resources").map(|o| doc.resolve(o)) {
        Ok(Object::Dictionary(d)) => d.clone(),
        _ => Dictionary::new(),
    };
    let kinds = xobject_kinds(doc, &resources);

    let mut removed_inside = 0usize;
    // Collected across every region before any of them is acted on. Two
    // regions landing on the same nested XObject have to become two
    // holes in one copy: redacting the original once per region and
    // keeping the last copy would silently drop all but the final hole.
    let mut nested: BTreeMap<Vec<u8>, Vec<Rect>> = BTreeMap::new();

    for region in regions {
        let pass = redact_content_resolving(&bytes, *region, RemovalScope::Everything, &|name| {
            kinds
                .get(name)
                .map(|entry| entry.kind)
                .unwrap_or(XObjectKind::Unknown)
        })?;
        removed_inside += pass.removed_operations;
        bytes = pass.bytes;
        for (name, found) in group_by_name(&pass.partial_xobjects) {
            nested.entry(name).or_default().extend(found);
        }
    }

    let mut replacements: Vec<(Vec<u8>, ObjectId)> = Vec::new();
    for (name, regions) in nested {
        let Some(&Entry { id, kind }) = kinds.get(&name) else {
            continue;
        };
        match redact_xobject(doc, id, &regions, kind, depth + 1)? {
            Some(Outcome::Replaced {
                id,
                removed_inside: n,
            }) => {
                removed_inside += n;
                replacements.push((name, id));
            }
            // Cannot edit what it points at, so stop pointing at it.
            None => {
                let (stripped, dropped) = drop_named_draws(&bytes, &[name])?;
                bytes = stripped;
                removed_inside += dropped;
            }
            Some(Outcome::Unchanged) => {}
        }
    }

    if removed_inside == 0 && replacements.is_empty() {
        return Ok(Some(Outcome::Unchanged));
    }

    if !replacements.is_empty() {
        // The copy carries its own resources inline, so repointing a
        // name here cannot reach the original form or anything else
        // sharing that dictionary.
        let mut resources = resources;
        let mut entries = match resources.get(b"XObject").map(|o| doc.resolve(o)) {
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => Dictionary::new(),
        };
        for (name, id) in replacements {
            entries.set(
                String::from_utf8_lossy(&name).into_owned(),
                Object::Reference(id),
            );
        }
        resources.set("XObject", Object::Dictionary(entries));
        dict.set("Resources", Object::Dictionary(resources));
    }

    dict.remove(b"Filter");
    dict.remove(b"DecodeParms");
    let mut stream = Stream::new(dict, bytes);
    let _ = stream.compress();
    let new_id = doc.add_object(Object::Stream(stream));
    Ok(Some(Outcome::Replaced {
        id: new_id,
        removed_inside,
    }))
}

/// Clears the regions from an image's pixels and returns the copy, or
/// `None` if the image is not one [`pixels`] will edit.
fn redact_image(doc: &mut Document, id: ObjectId, regions: &[Rect]) -> Option<ObjectId> {
    let dict = doc.dictionary_or_stream_dict(id).ok()?.clone();
    let data = doc.decoded_stream(id).ok()?;
    let components = dict
        .get(b"ColorSpace")
        .ok()
        .and_then(|cs| pixels::components_for(cs, &|o| doc.resolve(o).clone()));

    let mut stream = pixels::clear_regions(&dict, &data, regions, components, Fill::White)?;

    // The soft mask is a second image holding this one's opacity, and
    // it has to lose the same region — otherwise the cleared pixels
    // stay fully opaque and a white block covers whatever the image was
    // drawn over. If the mask cannot be edited, leaving it alone is
    // safe: the colour data is already gone, and an opaque white patch
    // hides more, not less.
    if let Some(mask_id) = stream
        .dict
        .get(b"SMask")
        .ok()
        .and_then(|o| o.as_reference().ok())
    {
        if let Some(new_mask) = redact_soft_mask(doc, mask_id, regions) {
            stream.dict.set("SMask", Object::Reference(new_mask));
        }
    }

    Some(doc.add_object(Object::Stream(stream)))
}

fn redact_soft_mask(doc: &mut Document, id: ObjectId, regions: &[Rect]) -> Option<ObjectId> {
    let dict = doc.dictionary_or_stream_dict(id).ok()?.clone();
    let data = doc.decoded_stream(id).ok()?;
    // A soft mask is greyscale by definition, whether or not it says so.
    let cleared = pixels::clear_regions(&dict, &data, regions, Some(1), Fill::Transparent)?;
    Some(doc.add_object(Object::Stream(cleared)))
}

/// Removes every `Do` naming one of `names` from a content stream.
///
/// The last resort for an XObject that overlaps the rect and cannot be
/// edited — a fax-encoded image, a form nested past the depth limit.
/// The page loses the whole thing, which is the behaviour every image
/// used to get; what matters is that it is now the exception rather
/// than the rule.
fn drop_named_draws(bytes: &[u8], names: &[Vec<u8>]) -> Result<(Vec<u8>, usize), RedactError> {
    let content = Content::decode(bytes).map_err(|e| RedactError::ContentDecode(e.to_string()))?;
    let mut dropped = 0usize;
    let kept: Vec<Operation> = content
        .operations
        .into_iter()
        .filter(|op| {
            if op.operator != "Do" {
                return true;
            }
            let named = op
                .operands
                .first()
                .and_then(|o| o.as_name().ok())
                .is_some_and(|n| names.iter().any(|candidate| candidate == n));
            if named {
                dropped += 1;
            }
            !named
        })
        .collect();
    let bytes = Content { operations: kept }
        .encode()
        .map_err(|e| RedactError::ContentEncode(e.to_string()))?;
    Ok((bytes, dropped))
}

fn overlay_box_bytes(rect: Rect, color: [f32; 3]) -> Result<Vec<u8>, RedactError> {
    let ops = vec![
        Operation::new("q", vec![]),
        Operation::new(
            "rg",
            vec![
                (color[0] as f64).into(),
                (color[1] as f64).into(),
                (color[2] as f64).into(),
            ],
        ),
        Operation::new(
            "re",
            vec![
                rect.x0.into(),
                rect.y0.into(),
                (rect.x1 - rect.x0).into(),
                (rect.y1 - rect.y0).into(),
            ],
        ),
        Operation::new("f", vec![]),
        Operation::new("Q", vec![]),
    ];
    Content { operations: ops }
        .encode()
        .map_err(|e| RedactError::ContentEncode(e.to_string()))
}
