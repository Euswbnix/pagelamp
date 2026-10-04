//! What a link in a course's HTML leads to.
//!
//! An address written in HTML is never requested. It is only read: is it on this Canvas, in
//! the course being synced, and a page or a file? Then the sync asks for that page or file
//! through `Endpoint`, which builds the request from the course id and the slug or file id
//! alone. Everything else is noted as not read, or counted when it is outside Canvas.

use pagelamp_core::coverage::{CoverageArea, CoverageReason};
use pagelamp_extract::links::Link;
use url::Url;

use crate::endpoint::same_origin;
use crate::json::CanvasId;

/// The longest page slug that is followed, in bytes.
const MAX_SLUG_BYTES: usize = 200;

/// Where a link leads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    /// A page of this course, by its slug.
    Page { slug: String },
    /// A file of this course, by its Canvas id.
    File { id: String },
    /// Something on this Canvas that PageLamp doesn't follow. `url` has no query or fragment.
    NotFollowed {
        area: CoverageArea,
        reason: CoverageReason,
        url: String,
    },
    /// An address outside this Canvas.
    OffSite,
    /// Nothing to note: no usable address, or a part of the course the sync reads anyway
    /// (its home, modules, announcements, the syllabus).
    Nothing,
}

/// Where `link` leads, for a sync of `course` on the Canvas at `base`. A link's `href` decides
/// unless only its `data-api-endpoint` names a page or a file of this course.
pub(crate) fn classify(base: &Url, course: &CanvasId, link: &Link) -> Target {
    let from_href = link.href.as_deref().map(|href| address(base, course, href));
    if let Some(target @ (Target::Page { .. } | Target::File { .. })) = &from_href {
        return target.clone();
    }
    let from_api = link
        .api_endpoint
        .as_deref()
        .map(|endpoint| address(base, course, endpoint));
    if let Some(target @ (Target::Page { .. } | Target::File { .. })) = &from_api {
        return target.clone();
    }
    from_href.or(from_api).unwrap_or(Target::Nothing)
}

fn address(base: &Url, course: &CanvasId, written: &str) -> Target {
    // A fragment alone points into the same text.
    if written.starts_with('#') {
        return Target::Nothing;
    }
    let Ok(url) = base.join(written) else {
        return Target::Nothing;
    };
    if !matches!(url.scheme(), "http" | "https") {
        // mailto:, tel:, javascript:, data:…
        return Target::Nothing;
    }
    if !same_origin(base, &url) {
        return Target::OffSite;
    }
    let mut segments: Vec<&str> = url
        .path_segments()
        .map(|segments| segments.collect())
        .unwrap_or_default();
    // A trailing slash leaves an empty last segment.
    while segments.last() == Some(&"") {
        segments.pop();
    }
    // `data-api-endpoint` addresses start with /api/v1.
    if segments.starts_with(&["api", "v1"]) {
        segments.drain(..2);
    }
    let shown = || {
        let mut shown = url.clone();
        shown.set_query(None);
        shown.set_fragment(None);
        shown.to_string()
    };
    let not_followed = |area, reason| Target::NotFollowed {
        area,
        reason,
        url: shown(),
    };
    match segments.as_slice() {
        [] => Target::Nothing,
        ["courses", id, rest @ ..] => {
            if !is_canvas_id(id) {
                return Target::Nothing;
            }
            if *id != course.0 {
                return not_followed(CoverageArea::Other, CoverageReason::OtherCourse);
            }
            match rest {
                ["pages" | "wiki", slug] => match page_slug(slug) {
                    Some(slug) => Target::Page { slug },
                    None => Target::Nothing,
                },
                ["files", file] | ["files", file, "download" | "preview"] if is_canvas_id(file) => {
                    Target::File {
                        id: (*file).to_string(),
                    }
                }
                // The syllabus lives under /assignments/syllabus; the sync reads it.
                ["assignments", "syllabus"] => Target::Nothing,
                ["assignments", _, ..] => {
                    not_followed(CoverageArea::Assignments, CoverageReason::ByRule)
                }
                ["quizzes", _, ..] => not_followed(CoverageArea::Quizzes, CoverageReason::ByRule),
                ["discussion_topics", _, ..] => {
                    not_followed(CoverageArea::Discussions, CoverageReason::NotRead)
                }
                ["external_tools", ..] => {
                    not_followed(CoverageArea::ExternalTool, CoverageReason::OutsideCanvas)
                }
                ["grades", ..] => not_followed(CoverageArea::Grades, CoverageReason::NotRead),
                ["users", ..] => not_followed(CoverageArea::People, CoverageReason::NotRead),
                // The course itself and the lists a sync reads (or leaves alone when hidden).
                []
                | [
                    "pages" | "wiki" | "files" | "modules" | "announcements" | "assignments"
                    | "quizzes" | "discussion_topics",
                    ..,
                ] => Target::Nothing,
                _ => not_followed(CoverageArea::Other, CoverageReason::NotRead),
            }
        }
        // A file named without its course: which course it belongs to can't be told from
        // the address, and asking Canvas would be a request outside the allow-list.
        ["files", file, ..] if is_canvas_id(file) => {
            not_followed(CoverageArea::Files, CoverageReason::Other)
        }
        _ => not_followed(CoverageArea::Other, CoverageReason::NotRead),
    }
}

/// Canvas ids are digits, with `~` in the short form of an id from another Canvas shard.
fn is_canvas_id(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_ascii_digit() || c == '~')
}

/// The slug a path segment names: percent-decoded once. Refused (`None`) when it is empty,
/// isn't UTF-8, is longer than `MAX_SLUG_BYTES`, or holds a `/`, `..` or a control character.
fn page_slug(segment: &str) -> Option<String> {
    let slug = String::from_utf8(percent_decode(segment)).ok()?;
    let fine = !slug.is_empty()
        && slug.len() <= MAX_SLUG_BYTES
        && !slug.contains('/')
        && !slug.contains("..")
        && !slug.chars().any(char::is_control);
    fine.then_some(slug)
}

/// `%XX` to its byte, once; anything else stays.
fn percent_decode(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push((high * 16 + low) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(href: &str) -> Target {
        classify(
            &Url::parse("https://lms.example.edu").unwrap(),
            &CanvasId("101".into()),
            &Link {
                href: Some(href.into()),
                api_endpoint: None,
            },
        )
    }

    fn page(slug: &str) -> Target {
        Target::Page { slug: slug.into() }
    }

    fn file(id: &str) -> Target {
        Target::File { id: id.into() }
    }

    fn noted(area: CoverageArea, reason: CoverageReason, url: &str) -> Target {
        Target::NotFollowed {
            area,
            reason,
            url: url.into(),
        }
    }

    #[test]
    fn pages_and_files_of_this_course_are_named_by_slug_and_id() {
        for (href, expected) in [
            ("/courses/101/pages/week-1", page("week-1")),
            (
                "https://lms.example.edu/courses/101/pages/week-1?module_item_id=5#top",
                page("week-1"),
            ),
            (
                "https://LMS.example.edu:443/courses/101/pages/week-1/",
                page("week-1"),
            ),
            // The old path, and an encoded slug (decoded once: %2520 stays %20).
            ("/courses/101/wiki/week-1", page("week-1")),
            ("/courses/101/pages/notes%20%E9%A1%B5", page("notes 页")),
            ("/courses/101/pages/a%2520b", page("a%20b")),
            ("/courses/101/files/7", file("7")),
            (
                "/courses/101/files/7/download?verifier=SECRET&wrap=1",
                file("7"),
            ),
            ("/courses/101/files/7/preview", file("7")),
            ("/courses/101/files/1~7/download", file("1~7")),
            ("/api/v1/courses/101/pages/week-1", page("week-1")),
            ("/api/v1/courses/101/files/7", file("7")),
        ] {
            assert_eq!(target(href), expected, "{href}");
        }
    }

    #[test]
    fn a_slug_that_could_leave_the_page_path_is_refused() {
        for href in [
            "/courses/101/pages/a%2Fb",
            "/courses/101/pages/%2E%2E",
            "/courses/101/pages/a..b",
            "/courses/101/pages/a%0Ab",
            "/courses/101/pages/%FF",
            &format!("/courses/101/pages/{}", "x".repeat(MAX_SLUG_BYTES + 1)),
            // More than a slug after /pages/.
            "/courses/101/pages/week-1/edit",
            "/courses/101/pages/week-1/revisions",
            // Not a file id.
            "/courses/101/files/folder/week-1",
            "/courses/101/files/7/edit",
        ] {
            assert_eq!(target(href), Target::Nothing, "{href}");
        }
        assert_eq!(
            target(&format!(
                "/courses/101/pages/{}",
                "x".repeat(MAX_SLUG_BYTES)
            )),
            page(&"x".repeat(MAX_SLUG_BYTES))
        );
    }

    #[test]
    fn what_is_not_followed_is_noted_without_its_query() {
        use CoverageArea as A;
        use CoverageReason as R;
        let at = |path: &str| format!("https://lms.example.edu{path}");
        for (href, expected) in [
            (
                "/courses/101/assignments/9?module_item_id=3",
                noted(A::Assignments, R::ByRule, &at("/courses/101/assignments/9")),
            ),
            (
                "/courses/101/quizzes/4/take",
                noted(A::Quizzes, R::ByRule, &at("/courses/101/quizzes/4/take")),
            ),
            (
                "/courses/101/discussion_topics/8",
                noted(
                    A::Discussions,
                    R::NotRead,
                    &at("/courses/101/discussion_topics/8"),
                ),
            ),
            (
                "/courses/101/external_tools/2",
                noted(
                    A::ExternalTool,
                    R::OutsideCanvas,
                    &at("/courses/101/external_tools/2"),
                ),
            ),
            (
                "/courses/101/grades",
                noted(A::Grades, R::NotRead, &at("/courses/101/grades")),
            ),
            (
                "/courses/101/users/5",
                noted(A::People, R::NotRead, &at("/courses/101/users/5")),
            ),
            (
                "/courses/101/collaborations",
                noted(A::Other, R::NotRead, &at("/courses/101/collaborations")),
            ),
            // Another course: not even its pages or files.
            (
                "/courses/202/pages/week-1",
                noted(A::Other, R::OtherCourse, &at("/courses/202/pages/week-1")),
            ),
            (
                "/courses/1010/files/7/download?verifier=SECRET",
                noted(
                    A::Other,
                    R::OtherCourse,
                    &at("/courses/1010/files/7/download"),
                ),
            ),
            // A file without its course.
            (
                "/files/7/download?verifier=SECRET",
                noted(A::Files, R::Other, &at("/files/7/download")),
            ),
            (
                "/users/5/files/7",
                noted(A::Other, R::NotRead, &at("/users/5/files/7")),
            ),
            ("/calendar", noted(A::Other, R::NotRead, &at("/calendar"))),
        ] {
            assert_eq!(target(href), expected, "{href}");
        }
    }

    #[test]
    fn other_sites_are_off_site_and_the_rest_is_nothing() {
        for href in [
            "https://example.org/notes.pdf",
            "http://lms.example.edu/courses/101/pages/week-1",
            "https://lms.example.edu:8443/courses/101/pages/week-1",
            "https://lms.example.edu.evil.test/courses/101/pages/week-1",
            "//cdn.example.org/a.png",
        ] {
            assert_eq!(target(href), Target::OffSite, "{href}");
        }
        for href in [
            "#top",
            "mailto:teacher@example.edu",
            "javascript:void(0)",
            "tel:+15550100",
            "data:text/plain,hello",
            "/",
            "/courses/101",
            "/courses/101/",
            "/courses/101/modules",
            "/courses/101/modules/items/55",
            "/courses/101/pages",
            "/courses/101/files",
            "/courses/101/announcements",
            "/courses/101/assignments",
            "/courses/101/assignments/syllabus",
            "/courses/abc/pages/week-1",
            "http://[broken",
        ] {
            assert_eq!(target(href), Target::Nothing, "{href}");
        }
    }

    #[test]
    fn the_api_address_counts_when_the_href_names_no_page_or_file() {
        let base = Url::parse("https://lms.example.edu").unwrap();
        let course = CanvasId("101".into());
        let link = |href: Option<&str>, api: Option<&str>| Link {
            href: href.map(str::to_string),
            api_endpoint: api.map(str::to_string),
        };
        let api_page = "https://lms.example.edu/api/v1/courses/101/pages/week-2";
        // Only the API address.
        assert_eq!(
            classify(&base, &course, &link(None, Some(api_page))),
            page("week-2")
        );
        // The href wins when it names a page or a file.
        assert_eq!(
            classify(
                &base,
                &course,
                &link(Some("/courses/101/pages/week-1"), Some(api_page))
            ),
            page("week-1")
        );
        // An href that names nothing followable gives way to the API address.
        assert_eq!(
            classify(&base, &course, &link(Some("#"), Some(api_page))),
            page("week-2")
        );
        // An API address of another Canvas is never used.
        assert_eq!(
            classify(
                &base,
                &course,
                &link(
                    Some("https://example.org/x"),
                    Some("https://other.example.edu/api/v1/courses/101/pages/week-2")
                )
            ),
            Target::OffSite
        );
        assert_eq!(classify(&base, &course, &link(None, None)), Target::Nothing);
    }
}
