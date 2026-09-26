//! Safety check run on each PDF page before `pdf-extract` reads it.
//!
//! `pdf-extract` 0.12 follows two kinds of links by recursion, without any limit:
//! - a page's `/Parent` chain, to inherit `/Resources` and `/MediaBox`;
//! - Form XObjects drawn with the `Do` operator (a form's content can draw more forms).
//!
//! In a damaged or hostile file these links can form a loop, or simply nest very deeply.
//! pdf-extract then recurses until the thread's stack overflows — and a stack overflow is
//! *not* a panic: it aborts the whole process, so `catch_panic` cannot help. We therefore
//! walk the same links first, with loop detection and depth limits, and skip pages that
//! fail the check.
//!
//! The walk mirrors how pdf-extract looks things up (`output_doc_inner`, `get_inherited` and
//! the `Do` case of `Processor::process_stream` in its source); re-check them when upgrading
//! pdf-extract. Other stack overflows or out-of-memory aborts inside third-party parsers can
//! only be fully contained by running extraction in a separate process.

use std::collections::{HashMap, HashSet};

use pdf_extract::content::Content;
use pdf_extract::{Dictionary, Document, Object, ObjectId, Stream};

/// Longest `/Parent` chain we accept (lopdf's own page-tree walk also stops at depth 256).
const MAX_PARENT_CHAIN: usize = 256;

/// Deepest nesting of forms inside forms that we let pdf-extract follow. Real documents
/// rarely nest more than a few levels.
const MAX_FORM_NESTING: usize = 32;

/// A form as pdf-extract draws it: the form's object id, and the id of the object whose
/// `/Resources` its content uses — the form itself, or, when the form has no resources of
/// its own, whoever drew it (pdf-extract then reuses the drawer's resources).
type FormKey = (ObjectId, ObjectId);

/// Checks the pages of one document. Forms used on many pages (logos, page headers) are
/// only walked once.
#[derive(Default)]
pub(crate) struct PageChecker {
    /// How deeply forms nest in each form already checked (1 = it draws no further forms).
    nesting: HashMap<FormKey, usize>,
}

impl PageChecker {
    /// `Err(reason)` when pdf-extract would recurse without limit on this page.
    pub(crate) fn check(&mut self, doc: &Document, page_id: ObjectId) -> Result<(), String> {
        check_parent_chain(doc, page_id)?;
        // Without a page dictionary, resources or content, pdf-extract fails (or panics,
        // which is caught) before it could draw any form.
        let Ok(page) = doc.get_dictionary(page_id) else {
            return Ok(());
        };
        let Some(resources) = inherited_resources(doc, page) else {
            return Ok(());
        };
        if dictionary_at(doc, resources, b"XObject").is_none() {
            return Ok(()); // draws no forms, so no need to read the content twice
        }
        let Ok(content) = doc.get_page_content(page_id) else {
            return Ok(());
        };
        let mut path = Vec::new();
        self.nesting_in(doc, &content, resources, page_id, &mut path)?;
        Ok(())
    }

    /// How deeply forms nest in `content` (0 when it draws no forms). `resources` is what
    /// the content's names are looked up in, `owner` the object those resources belong to,
    /// and `path` the forms currently being drawn, outermost first.
    fn nesting_in(
        &mut self,
        doc: &Document,
        content: &[u8],
        resources: &Dictionary,
        owner: ObjectId,
        path: &mut Vec<FormKey>,
    ) -> Result<usize, String> {
        let mut deepest = 0;
        for name in drawn_names(content) {
            let Some((form_id, form)) = xobject(doc, resources, &name) else {
                continue;
            };
            let (form_resources, form_owner) = match own_resources(doc, form) {
                Some(own) => (own, form_id),
                None => (resources, owner),
            };
            let key = (form_id, form_owner);
            let nesting = match self.nesting.get(&key) {
                Some(known) => *known,
                None => {
                    if path.contains(&key) {
                        return Err("a form XObject draws itself (a loop)".to_string());
                    }
                    if path.len() >= MAX_FORM_NESTING {
                        return Err(too_deep());
                    }
                    path.push(key);
                    let content = stream_content(form);
                    let below = self.nesting_in(doc, &content, form_resources, form_owner, path)?;
                    path.pop();
                    self.nesting.insert(key, below + 1);
                    below + 1
                }
            };
            deepest = deepest.max(nesting);
        }
        // A form checked earlier (so not walked again) can still be too deep from here.
        if path.len() + deepest > MAX_FORM_NESTING {
            return Err(too_deep());
        }
        Ok(deepest)
    }
}

fn too_deep() -> String {
    format!("form XObjects are nested more than {MAX_FORM_NESTING} levels deep")
}

/// The page's `/Parent` chain must end, and not be absurdly long.
fn check_parent_chain(doc: &Document, page_id: ObjectId) -> Result<(), String> {
    let mut seen = HashSet::from([page_id]);
    let mut node = doc.get_dictionary(page_id).ok();
    while let Some(dict) = node {
        let Some(parent_id) = parent_id(dict) else {
            return Ok(());
        };
        if !seen.insert(parent_id) {
            return Err("the page tree loops back on itself".to_string());
        }
        if seen.len() > MAX_PARENT_CHAIN {
            return Err(format!(
                "the page tree is more than {MAX_PARENT_CHAIN} levels deep"
            ));
        }
        node = doc.get_dictionary(parent_id).ok();
    }
    Ok(())
}

fn parent_id(node: &Dictionary) -> Option<ObjectId> {
    node.get(b"Parent").ok()?.as_reference().ok()
}

/// The page's `/Resources`, inherited from its ancestors when missing (like pdf-extract's
/// `get_inherited`).
fn inherited_resources<'a>(doc: &'a Document, page: &'a Dictionary) -> Option<&'a Dictionary> {
    let mut node = page;
    for _ in 0..=MAX_PARENT_CHAIN {
        if let Some(resources) = dictionary_at(doc, node, b"Resources") {
            return Some(resources);
        }
        node = doc.get_dictionary(parent_id(node)?).ok()?;
    }
    None
}

/// `dict[key]` as a dictionary, following a reference.
fn dictionary_at<'a>(
    doc: &'a Document,
    dict: &'a Dictionary,
    key: &[u8],
) -> Option<&'a Dictionary> {
    match dict.get(key).ok()? {
        Object::Reference(id) => doc.get_dictionary(*id).ok(),
        other => other.as_dict().ok(),
    }
}

/// A form's own `/Resources`, if it has them.
fn own_resources<'a>(doc: &'a Document, form: &'a Stream) -> Option<&'a Dictionary> {
    dictionary_at(doc, &form.dict, b"Resources")
}

/// The XObject called `name` in `resources` (`/XObject` → `name`) and its object id.
/// XObjects are streams, and streams always live in their own numbered object.
fn xobject<'a>(
    doc: &'a Document,
    resources: &'a Dictionary,
    name: &[u8],
) -> Option<(ObjectId, &'a Stream)> {
    let xobjects = dictionary_at(doc, resources, b"XObject")?;
    let (Some(id), object) = doc.dereference(xobjects.get(name).ok()?).ok()? else {
        return None;
    };
    Some((id, object.as_stream().ok()?))
}

/// The names drawn with `Do` in a content stream (`/Fm1 Do` → `Fm1`). Content that does not
/// parse draws nothing (pdf-extract then fails on that page by itself).
fn drawn_names(content: &[u8]) -> Vec<Vec<u8>> {
    let Ok(content) = Content::decode(content) else {
        return Vec::new();
    };
    content
        .operations
        .iter()
        .filter(|operation| operation.operator == "Do")
        .filter_map(|operation| operation.operands.first()?.as_name().ok())
        .map(<[u8]>::to_vec)
        .collect()
}

/// A stream's content the way pdf-extract reads it (`get_contents`): decompressed when
/// possible, raw otherwise.
fn stream_content(stream: &Stream) -> Vec<u8> {
    if stream.filters().is_ok() {
        stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone())
    } else {
        stream.content.clone()
    }
}
