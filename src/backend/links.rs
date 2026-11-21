use uuid::Uuid;

use crate::backend::page::{Frontmatter, PageData};

/// A wiki link after it has been turned into a markdown link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedLink {
    pub title: String,
    pub url: String,
    pub page: Option<Uuid>,
}

/// Replace `[[Title]]` with a markdown link.
///
/// Resolved titles point at `mot://open/{id}`. Missing titles point at
/// `mot://create/{title}` and are labeled as missing.
pub(crate) fn render_wiki_links(
    text: &str,
    mut resolve: impl FnMut(&str) -> Option<Uuid>,
) -> (String, Vec<RenderedLink>) {
    let mut links = Vec::new();
    let rendered = rewrite_links(text, |title| {
        let page = resolve(title);
        let url = match page {
            Some(id) => format!("mot://open/{id}"),
            None => format!("mot://create/{}", percent_encode(title)),
        };
        let label = match page {
            Some(_) => title.to_owned(),
            None => format!("{title} (missing)"),
        };
        links.push(RenderedLink {
            title: title.to_owned(),
            url: url.clone(),
            page,
        });
        format!("[{label}]({url})")
    });
    (rendered, links)
}

/// Replace wiki links whose title matches `old` with `new`.
pub(crate) fn rewrite_wiki_links(text: &str, old: &str, new: &str) -> String {
    rewrite_links(text, |title| {
        if title.eq_ignore_ascii_case(old) {
            format!("[[{new}]]")
        } else {
            format!("[[{title}]]")
        }
    })
}

/// Update structured title references. The page title itself is left alone.
pub(crate) fn rewrite_title_fields(frontmatter: &mut Frontmatter, old: &str, new: &str) {
    match &mut frontmatter.data {
        PageData::Note => {}
        PageData::Nation(meta) => {
            rewrite_list(&mut meta.leaders, old, new);
            for relation in &mut meta.relations {
                rewrite_exact(&mut relation.target, old, new);
            }
        }
        PageData::Player(meta) => {
            if let Some(nation) = &mut meta.nation {
                rewrite_exact(nation, old, new);
            }
        }
        PageData::Event(meta) => {
            rewrite_list(&mut meta.nations, old, new);
            rewrite_list(&mut meta.participants, old, new);
        }
    }
}

/// Resolve a wiki target by title, then by file stem.
///
/// Each item is `(title, file stem, id)`.
pub(crate) fn resolve_link<'a>(
    pages: impl IntoIterator<Item = (&'a str, Option<&'a str>, Uuid)>,
    link: &str,
) -> Option<Uuid> {
    let needle = link.trim();
    if needle.is_empty() {
        return None;
    }
    let pages: Vec<(&str, Option<&str>, Uuid)> = pages.into_iter().collect();
    if let Some((_, _, id)) = pages
        .iter()
        .find(|(title, _, _)| title.eq_ignore_ascii_case(needle))
    {
        return Some(*id);
    }
    pages.iter().find_map(|(_, stem, id)| {
        stem.is_some_and(|stem| stem.eq_ignore_ascii_case(needle))
            .then_some(*id)
    })
}

fn rewrite_list(values: &mut [String], old: &str, new: &str) {
    for value in values {
        rewrite_exact(value, old, new);
    }
}

fn rewrite_exact(value: &mut String, old: &str, new: &str) {
    if value.eq_ignore_ascii_case(old) {
        *value = new.to_owned();
    }
}

fn rewrite_links(text: &str, mut replace: impl FnMut(&str) -> String) -> String {
    let mut out = String::new();
    let mut index = 0;
    while index < text.len() {
        if let Some(title_start) = text[index..].find("[[") {
            let absolute = index + title_start;
            out.push_str(&text[index..absolute]);
            let content_start = absolute + 2;
            if let Some(end) = find_close(text, content_start) {
                let title = text[content_start..end].trim();
                if title.is_empty() {
                    out.push_str("[[");
                    index = content_start;
                    continue;
                }
                out.push_str(&replace(title));
                index = end + 2;
                continue;
            }
            out.push_str("[[");
            index = content_start;
            continue;
        }
        out.push_str(&text[index..]);
        break;
    }
    out
}

fn find_close(text: &str, start: usize) -> Option<usize> {
    let rest = text.get(start..)?;
    let relative = rest.find("]]")?;
    let title = rest.get(..relative)?;
    if title.contains('\n') || title.contains('\r') {
        return None;
    }
    Some(start + relative)
}

fn percent_encode(text: &str) -> String {
    let mut encoded = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' || byte == b'.' {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::{render_wiki_links, resolve_link, rewrite_wiki_links};
    use crate::backend::page::Frontmatter;

    #[test]
    fn resolves_title_stem_and_miss() {
        let rome = Frontmatter::note("Rome".to_owned());
        let republic = Frontmatter::note("The Republic".to_owned());
        let pages = [
            (rome.title.as_str(), Some("rome"), rome.id),
            (republic.title.as_str(), Some("republic"), republic.id),
        ];
        assert_eq!(resolve_link(pages, "rome"), Some(rome.id));
        assert_eq!(resolve_link(pages, "republic"), Some(republic.id));
        assert_eq!(resolve_link(pages, "Gaul"), None);

        let (markdown, links) = render_wiki_links("See [[Rome]] and [[Gaul]].", |title| {
            resolve_link(pages, title)
        });
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].page, Some(rome.id));
        assert_eq!(links[1].page, None);
        assert!(markdown.contains(&format!("mot://open/{}", rome.id)));
        assert!(markdown.contains("Gaul (missing)"));
        assert!(links[1].url.starts_with("mot://create/"));
    }

    #[test]
    fn rewrite_replaces_matching_titles_only() {
        let text = "[[Rome]] met [[Roman Empire]].";
        assert_eq!(
            rewrite_wiki_links(text, "Rome", "Italia"),
            "[[Italia]] met [[Roman Empire]]."
        );
    }
}
