//! The GET allow-list: the only Canvas API URLs this crate can build. Anything else — in
//! particular any non-GET request — is impossible by construction (see `transport`).

use chrono::NaiveDate;
use url::Url;

use crate::json::CanvasId;

/// One allowed Canvas API request (all under `{base}/api/v1`, all GET).
#[derive(Clone, Debug)]
pub(crate) enum Endpoint<'a> {
    UsersSelf,
    Courses,
    Tabs {
        course: &'a CanvasId,
    },
    Modules {
        course: &'a CanvasId,
    },
    ModuleItems {
        course: &'a CanvasId,
        module: &'a CanvasId,
    },
    Files {
        course: &'a CanvasId,
    },
    File {
        course: &'a CanvasId,
        file: &'a CanvasId,
    },
    Pages {
        course: &'a CanvasId,
    },
    /// `url_or_id` is a page slug from Canvas; it is percent-encoded as one path segment.
    Page {
        course: &'a CanvasId,
        url_or_id: &'a str,
    },
    Assignments {
        course: &'a CanvasId,
    },
    Announcements {
        course: &'a CanvasId,
        start: NaiveDate,
        end: NaiveDate,
    },
    PlannerItems {
        start: NaiveDate,
        end: NaiveDate,
    },
}

impl Endpoint<'_> {
    /// The full URL under `base` (e.g. `https://lms.example.edu`).
    pub(crate) fn url(&self, base: &Url) -> Url {
        let mut url = base.clone();
        url.set_query(None);
        url.set_fragment(None);
        let mut query: Vec<(&str, String)> = Vec::new();
        {
            let mut path = url
                .path_segments_mut()
                .expect("a normalised http(s) base URL can have path segments");
            path.clear().extend(["api", "v1"]);
            match self {
                Endpoint::UsersSelf => {
                    path.extend(["users", "self"]);
                }
                Endpoint::Courses => {
                    path.push("courses");
                    query.push(("enrollment_state", "active".into()));
                    query.push(("include[]", "term".into()));
                    query.push(("include[]", "syllabus_body".into()));
                }
                Endpoint::Tabs { course } => {
                    path.extend(["courses", &course.0, "tabs"]);
                }
                Endpoint::Modules { course } => {
                    path.extend(["courses", &course.0, "modules"]);
                    query.push(("include[]", "items".into()));
                    query.push(("include[]", "content_details".into()));
                }
                Endpoint::ModuleItems { course, module } => {
                    path.extend(["courses", &course.0, "modules", &module.0, "items"]);
                    query.push(("include[]", "content_details".into()));
                }
                Endpoint::Files { course } => {
                    path.extend(["courses", &course.0, "files"]);
                }
                Endpoint::File { course, file } => {
                    path.extend(["courses", &course.0, "files", &file.0]);
                }
                Endpoint::Pages { course } => {
                    path.extend(["courses", &course.0, "pages"]);
                }
                Endpoint::Page { course, url_or_id } => {
                    path.extend(["courses", &course.0, "pages", url_or_id]);
                }
                Endpoint::Assignments { course } => {
                    path.extend(["courses", &course.0, "assignments"]);
                }
                Endpoint::Announcements { course, start, end } => {
                    path.push("announcements");
                    query.push(("context_codes[]", format!("course_{}", course.0)));
                    query.push(("start_date", start.to_string()));
                    query.push(("end_date", end.to_string()));
                }
                Endpoint::PlannerItems { start, end } => {
                    path.extend(["planner", "items"]);
                    query.push(("start_date", start.to_string()));
                    query.push(("end_date", end.to_string()));
                }
            }
        }
        if self.is_list() {
            query.push(("per_page", "100".into()));
        }
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query);
        }
        url
    }

    /// Whether the endpoint returns a paginated JSON array.
    pub(crate) fn is_list(&self) -> bool {
        !matches!(
            self,
            Endpoint::UsersSelf | Endpoint::File { .. } | Endpoint::Page { .. }
        )
    }
}

/// Same scheme, host and port.
pub(crate) fn same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str().map(str::to_ascii_lowercase) == b.host_str().map(str::to_ascii_lowercase)
        && a.port_or_known_default() == b.port_or_known_default()
}

/// What a response says about the next page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Next {
    /// No `rel="next"` link: the listing is complete.
    End,
    Link(Url),
    /// A `rel="next"` link exists but can't be used (unparseable, non-ASCII header…): the
    /// listing must be treated as INCOMPLETE, never as finished.
    Unusable,
}

/// A pagination `next` link is followed only if it stays on the Canvas origin, keeps the
/// path of the first request (Canvas only changes the query) and carries no user info.
pub(crate) fn acceptable_next_link(first: &Url, next: &Url) -> bool {
    same_origin(first, next)
        && next.path() == first.path()
        && next.username().is_empty()
        && next.password().is_none()
}

/// `<target>; params` entries of a Link header (commas inside `<…>` are part of the URL).
static LINK_ENTRY: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"<([^>]*)>([^<]*)").expect("valid regex"));
static REL_PARAM: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r#"(?i)\brel\s*=\s*"?([^";,]*)"?"#).expect("valid regex")
});

/// Find the `rel="next"` target in every `Link` header value. Relative targets are resolved
/// against the request URL.
pub(crate) fn next_link<'a>(values: impl IntoIterator<Item = &'a [u8]>, request: &Url) -> Next {
    for value in values {
        let Ok(text) = std::str::from_utf8(value) else {
            // Not ASCII/UTF-8: if it might announce a next page, don't trust "complete".
            if String::from_utf8_lossy(value)
                .to_ascii_lowercase()
                .contains("next")
            {
                return Next::Unusable;
            }
            continue;
        };
        for entry in LINK_ENTRY.captures_iter(text) {
            let params = &entry[2];
            let is_next = REL_PARAM.captures_iter(params).any(|rel| {
                rel[1]
                    .split_whitespace()
                    .any(|r| r.eq_ignore_ascii_case("next"))
            });
            if is_next {
                return match request.join(entry[1].trim()) {
                    Ok(url) => Next::Link(url),
                    Err(_) => Next::Unusable,
                };
            }
        }
        if text.to_ascii_lowercase().contains("next") && !LINK_ENTRY.is_match(text) {
            return Next::Unusable;
        }
    }
    Next::End
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://lms.example.edu").unwrap()
    }

    fn id(text: &str) -> CanvasId {
        CanvasId(text.into())
    }

    #[test]
    fn urls_match_the_allow_list() {
        let course = id("101");
        let module = id("7");
        let day = NaiveDate::from_ymd_opt(2026, 9, 1).unwrap();
        let cases = [
            (
                Endpoint::UsersSelf,
                "https://lms.example.edu/api/v1/users/self",
            ),
            (
                Endpoint::Courses,
                "https://lms.example.edu/api/v1/courses?enrollment_state=active&include%5B%5D=term&include%5B%5D=syllabus_body&per_page=100",
            ),
            (
                Endpoint::Tabs { course: &course },
                "https://lms.example.edu/api/v1/courses/101/tabs?per_page=100",
            ),
            (
                Endpoint::ModuleItems {
                    course: &course,
                    module: &module,
                },
                "https://lms.example.edu/api/v1/courses/101/modules/7/items?include%5B%5D=content_details&per_page=100",
            ),
            (
                Endpoint::Page {
                    course: &course,
                    url_or_id: "week-3/../../users",
                },
                "https://lms.example.edu/api/v1/courses/101/pages/week-3%2F..%2F..%2Fusers",
            ),
            (
                Endpoint::Announcements {
                    course: &course,
                    start: day,
                    end: day,
                },
                "https://lms.example.edu/api/v1/announcements?context_codes%5B%5D=course_101&start_date=2026-09-01&end_date=2026-09-01&per_page=100",
            ),
        ];
        for (endpoint, expected) in cases {
            assert_eq!(endpoint.url(&base()).as_str(), expected);
        }
    }

    #[test]
    fn next_links_are_parsed_and_checked() {
        let first = Url::parse("https://lms.example.edu/api/v1/courses?per_page=100").unwrap();
        let next = |value: &str| next_link([value.as_bytes()], &first);
        let header = r#"<https://lms.example.edu/api/v1/courses?page=2&per_page=100>; rel="current",<https://lms.example.edu/api/v1/courses?page=3&per_page=100>; rel="next", <https://lms.example.edu/api/v1/courses?page=9>; rel="last""#;
        let Next::Link(url) = next(header) else {
            panic!("no next")
        };
        assert_eq!(
            url.as_str(),
            "https://lms.example.edu/api/v1/courses?page=3&per_page=100"
        );
        assert!(acceptable_next_link(&first, &url));
        assert_eq!(
            next(r#"<https://lms.example.edu/api/v1/courses?page=9>; rel="last""#),
            Next::End
        );
        // Relative targets, commas inside the URL, unquoted rel, several header values.
        let Next::Link(url) = next(r#"</api/v1/courses?page=2&x=a,b>; rel=next"#) else {
            panic!()
        };
        assert_eq!(
            url.as_str(),
            "https://lms.example.edu/api/v1/courses?page=2&x=a,b"
        );
        let values: [&[u8]; 2] = [
            b"<https://lms.example.edu/api/v1/courses?page=1>; rel=\"first\"",
            b"</api/v1/courses?page=2>; rel=\"next\"",
        ];
        assert!(matches!(next_link(values, &first), Next::Link(_)));
        assert_eq!(
            next_link([&b"<\xff\xfe>; rel=\"next\""[..]], &first),
            Next::Unusable
        );
        assert_eq!(next("garbage rel=next"), Next::Unusable);

        for foreign in [
            "https://evil.example.com/api/v1/courses?page=2",
            "http://lms.example.edu/api/v1/courses?page=2",
            "https://lms.example.edu:444/api/v1/courses?page=2",
            "https://lms.example.edu/api/v1/conversations?page=2",
            "https://lms.example.edu/login?page=2",
            "https://user@lms.example.edu/api/v1/courses",
        ] {
            assert!(
                !acceptable_next_link(&first, &Url::parse(foreign).unwrap()),
                "{foreign}"
            );
        }
        assert!(acceptable_next_link(
            &first,
            &Url::parse("https://LMS.example.edu:443/api/v1/courses?page=2").unwrap()
        ));
    }
}
