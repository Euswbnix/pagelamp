//! The links in an HTML body: each `<a>`'s `href` and `data-api-endpoint`, in document order.
//!
//! One pass of the HTML tokenizer, without building a tree: nothing is rendered and nothing
//! is fetched. Only addresses come out, never a link's words, so the result can be kept where
//! body text must not be. Elements that load something by themselves (`<img>`, `<iframe>`,
//! `<embed>`, `<video>`…) are not links, and what is written inside a `<script>`, a `<style>`
//! or a `<textarea>` is text, not markup.
//!
//! The caller decides what an address means (which course, a page or a file); this module
//! doesn't resolve, clean or judge one.

use std::cell::RefCell;

use html5ever::tendril::StrTendril;
use html5ever::tokenizer::states::RawKind;
use html5ever::tokenizer::{
    BufferQueue, Tag, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
};

/// The most links taken from one body.
pub const MAX_LINKS: usize = 300;
/// How much of one body is looked at, in bytes.
pub const MAX_BYTES: usize = 2 * 1024 * 1024;

/// One `<a>` element's addresses, as written (entities decoded, surrounding space removed).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Link {
    pub href: Option<String>,
    /// Canvas adds `data-api-endpoint` to links to its own content: the API address of what
    /// the link opens.
    pub api_endpoint: Option<String>,
}

/// The links of one body.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Links {
    pub links: Vec<Link>,
    /// The body was longer than `MAX_BYTES` or held more than `MAX_LINKS` links: the rest
    /// wasn't taken.
    pub cut: bool,
}

#[derive(Default)]
struct Sink(RefCell<Links>);

impl TokenSink for Sink {
    type Handle = ();

    fn process_token(&self, token: Token, _line: u64) -> TokenSinkResult<()> {
        let Token::TagToken(Tag {
            kind: TagKind::StartTag,
            name,
            attrs,
            ..
        }) = token
        else {
            return TokenSinkResult::Continue;
        };
        match &*name {
            "a" => {
                let value = |wanted: &str| {
                    attrs
                        .iter()
                        .find(|attr| &*attr.name.local == wanted)
                        .map(|attr| attr.value.trim().to_string())
                        .filter(|value| !value.is_empty())
                };
                let link = Link {
                    href: value("href"),
                    api_endpoint: value("data-api-endpoint"),
                };
                if link != Link::default() {
                    let mut found = self.0.borrow_mut();
                    if found.links.len() < MAX_LINKS {
                        found.links.push(link);
                    } else {
                        found.cut = true;
                    }
                }
                TokenSinkResult::Continue
            }
            // Without a tree builder the tokenizer has to be told where markup stops: what
            // follows these start tags is text up to their end tag.
            "script" => TokenSinkResult::RawData(RawKind::ScriptData),
            "style" | "iframe" | "noembed" | "noframes" | "xmp" => {
                TokenSinkResult::RawData(RawKind::Rawtext)
            }
            "textarea" | "title" => TokenSinkResult::RawData(RawKind::Rcdata),
            "plaintext" => TokenSinkResult::Plaintext,
            _ => TokenSinkResult::Continue,
        }
    }
}

/// The links of `html` (at most `MAX_LINKS`, from its first `MAX_BYTES`).
pub fn links(html: &str) -> Links {
    let mut end = html.len().min(MAX_BYTES);
    while !html.is_char_boundary(end) {
        end -= 1;
    }
    let input = BufferQueue::default();
    input.push_back(StrTendril::from_slice(&html[..end]));
    let tokenizer = Tokenizer::new(Sink::default(), TokenizerOpts::default());
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    let mut found = tokenizer.sink.0.take();
    found.cut |= end < html.len();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hrefs(html: &str) -> Vec<String> {
        links(html)
            .links
            .into_iter()
            .map(|link| link.href.unwrap_or_default())
            .collect()
    }

    #[test]
    fn links_come_in_document_order_with_both_addresses() {
        let found = links(
            r#"<p>See <a class="x" href="/courses/101/pages/week-1?a=1&amp;b=2#top"
                 data-api-endpoint="https://lms.example.edu/api/v1/courses/101/pages/week-1">Week 1</a>,
               the <A HREF=' /courses/101/files/7/download '>slides</A>
               and <a data-api-endpoint="https://lms.example.edu/api/v1/courses/101/files/8">this</a>.</p>"#,
        );
        assert!(!found.cut);
        assert_eq!(
            found.links,
            [
                Link {
                    href: Some("/courses/101/pages/week-1?a=1&b=2#top".into()),
                    api_endpoint: Some(
                        "https://lms.example.edu/api/v1/courses/101/pages/week-1".into()
                    ),
                },
                Link {
                    href: Some("/courses/101/files/7/download".into()),
                    api_endpoint: None,
                },
                Link {
                    href: None,
                    api_endpoint: Some("https://lms.example.edu/api/v1/courses/101/files/8".into()),
                },
            ]
        );
    }

    #[test]
    fn only_a_elements_are_links() {
        assert_eq!(
            hrefs(
                r#"<img src="/courses/101/files/1/preview">
                   <iframe src="/courses/101/files/2/download"></iframe>
                   <embed src="/courses/101/files/3"><video src="/courses/101/files/4"></video>
                   <link href="/courses/101/files/5"><area href="/courses/101/files/6">
                   <a name="anchor">no address</a><a href="  ">empty</a>
                   <a href="/courses/101/pages/real">real</a>"#
            ),
            ["/courses/101/pages/real"]
        );
    }

    #[test]
    fn what_is_written_as_text_is_not_a_link() {
        assert_eq!(
            hrefs(
                r#"<script>var s = '<a href="/courses/101/pages/in-script">x</a>';</script>
                   <style>/* <a href="/courses/101/pages/in-style"> */</style>
                   <textarea><a href="/courses/101/pages/in-textarea">x</a></textarea>
                   <!-- <a href="/courses/101/pages/in-comment">x</a> -->
                   <iframe><a href="/courses/101/pages/in-iframe">x</a></iframe>
                   <p>&lt;a href="/courses/101/pages/escaped"&gt;</p>
                   <a href="/courses/101/pages/after">after</a>"#
            ),
            ["/courses/101/pages/after"]
        );
    }

    #[test]
    fn broken_markup_gives_what_can_be_read() {
        assert_eq!(
            hrefs(
                r#"<p><a href="/courses/101/pages/one">one<a href=/courses/101/pages/two>two</p><a href="/courses/101/pages/thr"#
            ),
            ["/courses/101/pages/one", "/courses/101/pages/two"]
        );
        assert_eq!(links(""), Links::default());
        assert_eq!(links("just words, no markup"), Links::default());
        // The first of two attributes with the same name counts, as in a browser.
        assert_eq!(
            hrefs(r#"<a href="/courses/101/pages/first" href="/courses/101/pages/second">x</a>"#),
            ["/courses/101/pages/first"]
        );
    }

    #[test]
    fn a_body_with_too_many_links_or_too_long_is_cut() {
        let many: String = (0..MAX_LINKS + 5)
            .map(|n| format!(r#"<a href="/courses/101/pages/p{n}">p</a>"#))
            .collect();
        let found = links(&many);
        assert!(found.cut);
        assert_eq!(found.links.len(), MAX_LINKS);
        assert_eq!(
            found.links[MAX_LINKS - 1].href.as_deref(),
            Some(format!("/courses/101/pages/p{}", MAX_LINKS - 1).as_str())
        );

        // A long body: the link after the limit isn't taken, and a character that straddles
        // the limit doesn't break the cut.
        let mut long = String::from(r#"<a href="/courses/101/pages/early">early</a>"#);
        while long.len() < MAX_BYTES - 1 {
            long.push('x');
        }
        long.push('页');
        long.push_str(r#"<a href="/courses/101/pages/late">late</a>"#);
        let found = links(&long);
        assert!(found.cut);
        assert_eq!(
            found
                .links
                .iter()
                .map(|link| link.href.as_deref().unwrap())
                .collect::<Vec<_>>(),
            ["/courses/101/pages/early"]
        );
    }
}
