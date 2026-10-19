//! Keeps `documentation/` within the rules the statup.dev build enforces, so a
//! release never carries pages the site refuses to publish. The heading
//! addresses mirror `slugify` and `englishHeadings` of the site's
//! `src/lib/rehype-docs.mjs`; the link checks mirror `scripts/verify-docs.mjs`.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use statup::Doc;

const LANGUAGES: [&str; 2] = ["en", "fr"];
const NOTE_KINDS: [&str; 5] = ["NOTE", "TIP", "IMPORTANT", "WARNING", "CAUTION"];
/// Documented settings that no `env_or`, `parse_env`, `non_empty_env` or
/// `env::var` call names: `RUST_LOG` is read by tracing's `EnvFilter`.
const READ_ELSEWHERE: [&str; 1] = ["RUST_LOG"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("documentation")
}

fn fact_labels(lang: &str) -> [&'static str; 3] {
    match lang {
        "fr" => ["Par défaut", "Valeurs", "Depuis"],
        _ => ["Default", "Values", "Since"],
    }
}

struct Page {
    lang: String,
    id: String,
    text: String,
}

impl Page {
    fn place(&self) -> String {
        format!("documentation/{}/{}.md", self.lang, self.id)
    }

    fn at(&self, number: usize) -> String {
        format!("{}:{number}", self.place())
    }

    /// The lines after the front matter, with their line number, each marked
    /// as part of a code block (its fences included) or not. Fences are read
    /// line by line, as the site's `englishHeadings` reads them.
    fn body(&self) -> Vec<(usize, &str, bool)> {
        let start = front_matter_end(&self.text).unwrap_or(0);
        let mut fence = false;
        let mut lines = Vec::new();
        for (index, line) in self.text.lines().enumerate().skip(start) {
            lines.push((index + 1, line, fence || is_fence(line)));
            fence ^= is_fence(line);
        }
        lines
    }

    fn prose(&self) -> impl Iterator<Item = (usize, &str)> {
        self.body()
            .into_iter()
            .filter(|(_, _, fenced)| !fenced)
            .map(|(number, line, _)| (number, line))
    }
}

fn is_fence(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
}

/// Index of the first line after the closing `---` of the front matter.
fn front_matter_end(text: &str) -> Option<usize> {
    let mut lines = text.lines();
    if lines.next()? != "---" {
        return None;
    }
    lines
        .position(|line| line == "---")
        .map(|closing| closing + 2)
}

fn pages() -> Vec<Page> {
    let mut pages = Vec::new();
    for lang in LANGUAGES {
        let Ok(entries) = fs::read_dir(root().join(lang)) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if path.extension().is_some_and(|ext| ext == "md") {
                let text = fs::read_to_string(&path).unwrap_or_default();
                pages.push(Page {
                    lang: lang.into(),
                    id: id.into(),
                    text,
                });
            }
        }
    }
    pages.sort_by(|a, b| (&a.lang, &a.id).cmp(&(&b.lang, &b.id)));
    pages
}

fn english<'a>(pages: &'a [Page], id: &str) -> Option<&'a Page> {
    pages.iter().find(|page| page.lang == "en" && page.id == id)
}

fn assert_none(problems: Vec<String>) {
    assert!(problems.is_empty(), "\n  {}\n", problems.join("\n  "));
}

fn is_page_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

// Headings and their addresses.

struct Heading {
    depth: usize,
    id: String,
}

/// A heading's address: its words in lower case, punctuation dropped, each
/// run of white space turned into a hyphen (the site's `slugify`). Not the
/// `slug` crate: it would strip accents, turn `_` into `-` and trim a final
/// hyphen, so its addresses would differ from the site's.
fn slugify(words: &str) -> String {
    let lower = words.to_lowercase();
    let kept = lower
        .trim()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '_' || *c == '-');
    let mut slug = String::new();
    let mut in_space = false;
    for c in kept {
        if c.is_whitespace() {
            if !in_space {
                slug.push('-');
            }
            in_space = true;
        } else {
            slug.push(c);
            in_space = false;
        }
    }
    slug
}

/// The pieces between backticks, and the text outside them.
fn split_code(raw: &str) -> (Vec<String>, String) {
    let mut codes = Vec::new();
    let mut parts = raw.split('`');
    let mut outside = parts.next().unwrap_or_default().to_string();
    let rest: Vec<&str> = parts.collect();
    for pair in rest.chunks(2) {
        match pair {
            [code, after] => {
                codes.push((*code).to_string());
                outside.push_str(after);
            }
            [lone] => {
                outside.push('`');
                outside.push_str(lone);
            }
            _ => {}
        }
    }
    (codes, outside)
}

/// `[text](url)` becomes `text`.
fn strip_links(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find("](").map(|i| open + i) else {
            break;
        };
        let Some(end) = rest[close..].find(')').map(|i| close + i) else {
            break;
        };
        out.push_str(&rest[..open]);
        out.push_str(&rest[open + 1..close]);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

fn heading_line(line: &str) -> Option<(usize, &str)> {
    let depth = line.chars().take_while(|c| *c == '#').count();
    if !(2..=4).contains(&depth) {
        return None;
    }
    let rest = &line[depth..];
    let words = rest.trim();
    (rest.starts_with(char::is_whitespace) && !words.is_empty()).then_some((depth, words))
}

/// The setting a heading names: a level three heading made only of code, as
/// "### `PUBLIC_URL`", is addressed by its first name in lower case.
fn code_only(depth: usize, raw: &str) -> Option<String> {
    let (codes, outside) = split_code(raw);
    if depth != 3 || !outside.chars().all(|c| c.is_whitespace() || c == ',') {
        return None;
    }
    codes.into_iter().next()
}

/// The backticks go with the punctuation in `slugify`.
fn heading_id(depth: usize, raw: &str) -> String {
    if let Some(name) = code_only(depth, raw) {
        return name.to_lowercase();
    }
    let plain: String = raw.replace("**", "").replace("__", "").replace('*', "");
    slugify(&strip_links(&plain))
}

fn headings(page: &Page) -> Vec<Heading> {
    let mut taken: HashMap<String, usize> = HashMap::new();
    let mut out = Vec::new();
    for (_, line) in page.prose() {
        let Some((depth, raw)) = heading_line(line) else {
            continue;
        };
        let mut id = heading_id(depth, raw);
        let seen = taken.entry(id.clone()).or_insert(0);
        if *seen > 0 {
            id = format!("{id}-{seen}");
        }
        *seen += 1;
        out.push(Heading { depth, id });
    }
    out
}

fn setting_name(line: &str) -> Option<String> {
    heading_line(line).and_then(|(depth, raw)| code_only(depth, raw))
}

/// Every file under `dir` with this extension, at any depth.
fn files_under(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut folders = vec![dir.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for path in fs::read_dir(folder)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
        {
            if path.is_dir() {
                folders.push(path);
            } else if path.extension().is_some_and(|ext| ext == extension) {
                files.push(path);
            }
        }
    }
    files
}

#[test]
fn the_folder_holds_the_published_languages_only() {
    let mut problems = Vec::new();
    let Ok(entries) = fs::read_dir(root()) else {
        panic!("documentation/ is missing");
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_dir = entry.path().is_dir();
        let expected = matches!(name.as_str(), "README.md" | "nav.json")
            || (is_dir && LANGUAGES.contains(&name.as_str()));
        if !expected {
            problems.push(format!("documentation/{name} has no place here"));
        }
        if is_dir && LANGUAGES.contains(&name.as_str()) {
            problems.extend(language_folder_problems(&name));
        }
    }
    for lang in LANGUAGES {
        if !root().join(lang).is_dir() {
            problems.push(format!("documentation/{lang}/ is missing"));
        }
    }
    assert_none(problems);
}

fn language_folder_problems(lang: &str) -> Vec<String> {
    let mut problems = Vec::new();
    for entry in fs::read_dir(root().join(lang))
        .into_iter()
        .flatten()
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        let page = name.strip_suffix(".md").filter(|id| is_page_id(id));
        if entry.path().is_dir() || page.is_none() {
            problems.push(format!(
                "documentation/{lang}/{name}: a language folder holds pages named like page-name.md, nothing else"
            ));
        }
    }
    problems
}

#[test]
fn nav_lists_every_page_once_in_every_language() {
    let text = fs::read_to_string(root().join("nav.json")).unwrap_or_default();
    let nav: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    let mut problems = Vec::new();
    let languages: Vec<&str> = nav["languages"]
        .as_array()
        .map(|list| list.iter().filter_map(|lang| lang.as_str()).collect())
        .unwrap_or_default();
    if languages != LANGUAGES {
        problems.push(format!(
            "nav.json lists the languages {languages:?}, not {LANGUAGES:?}"
        ));
    }
    let mut listed = BTreeSet::new();
    for group in nav["groups"].as_array().into_iter().flatten() {
        for lang in LANGUAGES {
            if group["name"][lang].as_str().is_none_or(str::is_empty) {
                problems.push(format!("nav.json: a group has no {lang} name"));
            }
        }
        for id in group["pages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|id| id.as_str())
        {
            if !listed.insert(id.to_string()) {
                problems.push(format!("nav.json names the page {id} twice"));
            }
        }
    }
    let pages = pages();
    for lang in LANGUAGES {
        let present: BTreeSet<String> = pages
            .iter()
            .filter(|page| page.lang == lang)
            .map(|page| page.id.clone())
            .collect();
        for id in listed.difference(&present) {
            problems.push(format!(
                "nav.json names {id}, missing from documentation/{lang}/"
            ));
        }
        for id in present.difference(&listed) {
            problems.push(format!(
                "documentation/{lang}/{id}.md is in no group of nav.json"
            ));
        }
    }
    assert_none(problems);
}

#[test]
fn every_page_has_a_title_and_a_quoted_description() {
    let mut problems = Vec::new();
    for page in pages() {
        let Some(end) = front_matter_end(&page.text) else {
            problems.push(format!(
                "{} does not open with a front matter",
                page.place()
            ));
            continue;
        };
        let mut names = BTreeSet::new();
        for field in page.text.lines().take(end - 1).skip(1) {
            match field.split_once(':') {
                Some((name, value)) => {
                    names.insert(name);
                    problems.extend(field_problem(&page, name, value.trim()));
                }
                None => problems.push(format!(
                    "{}: front matter line `{field}` is not a field",
                    page.place()
                )),
            }
        }
        if names != BTreeSet::from(["description", "title"]) {
            problems.push(format!("{} needs a title and a description", page.place()));
        }
    }
    assert_none(problems);
}

fn field_problem(page: &Page, name: &str, value: &str) -> Option<String> {
    let quoted = value.len() > 2 && value.starts_with('"') && value.ends_with('"');
    match name {
        "title" if value.trim_matches('"').trim().is_empty() => {
            Some(format!("{} has an empty title", page.place()))
        }
        "description" if !quoted => Some(format!(
            "{}: the description is written between double quotes, and is not empty",
            page.place()
        )),
        "title" | "description" => None,
        other => Some(format!(
            "{}: front matter field {other} is unknown",
            page.place()
        )),
    }
}

#[test]
fn headings_are_plain_and_follow_the_english_page() {
    let pages = pages();
    let mut problems = Vec::new();
    for page in &pages {
        problems.extend(heading_shape_problems(page));
        if page.lang == "en" {
            continue;
        }
        let Some(source) = english(&pages, &page.id) else {
            problems.push(format!("{} has no English page", page.place()));
            continue;
        };
        let depths = |p: &Page| headings(p).iter().map(|h| h.depth).collect::<Vec<_>>();
        let (mine, theirs) = (depths(page), depths(source));
        if mine != theirs {
            problems.push(format!(
                "{} does not keep the headings of the English page: levels {mine:?} where English has {theirs:?}",
                page.place()
            ));
        }
    }
    assert_none(problems);
}

fn heading_shape_problems(page: &Page) -> Vec<String> {
    let mut problems = Vec::new();
    let mut previous = "";
    for (number, line) in page.prose() {
        let at = page.at(number);
        let depth = line.chars().take_while(|c| *c == '#').count();
        let spaced = line[depth..].starts_with(char::is_whitespace);
        if depth > 0 && spaced && heading_line(line).is_none() {
            problems.push(format!(
                "{at}: headings run from ## to ####, the page title is the front matter's"
            ));
        }
        let inner = line
            .trim_start()
            .trim_start_matches(['-', '*', '+', '>'])
            .trim_start();
        if inner.len() < line.len() && heading_line(inner).is_some() {
            problems.push(format!(
                "{at}: a heading starts its line, outside any list or quote"
            ));
        }
        let underline = !line.is_empty() && line.chars().all(|c| c == '=' || c == '-');
        if underline && !previous.trim().is_empty() && line.len() >= 2 {
            problems.push(format!(
                "{at}: underlined headings are not used, write ## instead"
            ));
        }
        previous = line;
    }
    problems
}

#[test]
fn links_lead_to_pages_and_parts_that_exist() {
    let pages = pages();
    let anchors = english_anchors(&pages);
    let mut problems = Vec::new();
    for page in &pages {
        for (number, line) in page.prose() {
            let at = page.at(number);
            for target in link_targets(line) {
                if let Some(problem) = link_problem(&target, &page.id, &anchors) {
                    problems.push(format!("{at}: {target} {problem}"));
                }
            }
            if line.contains("![") {
                problems.push(format!("{at}: no image without the site's agreement"));
            }
            if is_reference_link(line) {
                problems.push(format!(
                    "{at}: links are written inline, [text](page.md#part)"
                ));
            }
        }
    }
    assert_none(problems);
}

/// Each English page with the addresses of its headings.
fn english_anchors(pages: &[Page]) -> HashMap<String, BTreeSet<String>> {
    pages
        .iter()
        .filter(|page| page.lang == "en")
        .map(|page| {
            let ids = headings(page).into_iter().map(|h| h.id).collect();
            (page.id.clone(), ids)
        })
        .collect()
}

/// A reference-style link (`[text][ref]`, or its `[ref]: target` line), which
/// the link check could not follow.
fn is_reference_link(line: &str) -> bool {
    let (_, outside) = split_code(line);
    let definition = outside
        .trim_start()
        .strip_prefix('[')
        .and_then(|rest| rest.split_once("]:"))
        .is_some_and(|(label, _)| !label.contains(']'));
    definition || outside.contains("][")
}

#[test]
fn the_app_links_to_pages_and_parts_that_exist() {
    let anchors = english_anchors(&pages());
    let mut problems = Vec::new();
    for doc in Doc::ALL {
        let target = match doc.page_part() {
            // The documentation's home always exists.
            ("", _) => continue,
            (page, "") => format!("{page}.md"),
            (page, part) => format!("{page}.md#{part}"),
        };
        if let Some(problem) = link_problem(&target, "", &anchors) {
            problems.push(format!("Doc::{doc:?}, {target}, {problem}"));
        }
    }
    assert_none(problems);
}

/// The targets of the `[text](target)` links of a line, code spans left out.
fn link_targets(line: &str) -> Vec<String> {
    let (_, outside) = split_code(line);
    let mut targets = Vec::new();
    let mut rest = outside.as_str();
    while let Some(start) = rest.find("](") {
        let after = &rest[start + 2..];
        let Some(end) = after.find(')') else {
            break;
        };
        let target = after[..end].split_whitespace().next().unwrap_or_default();
        targets.push(target.to_string());
        rest = &after[end..];
    }
    targets
}

fn link_problem(
    target: &str,
    current: &str,
    anchors: &HashMap<String, BTreeSet<String>>,
) -> Option<&'static str> {
    if target.starts_with("https://") || target.starts_with("mailto:") {
        return None;
    }
    let (path, anchor) = match target.split_once('#') {
        Some((path, anchor)) => (path, Some(anchor)),
        None => (target, None),
    };
    let page = if path.is_empty() {
        current
    } else {
        let Some(id) = path
            .trim_start_matches("./")
            .strip_suffix(".md")
            .filter(|id| is_page_id(id))
        else {
            return Some("is neither another page (page.md#part) nor an https:// address");
        };
        id
    };
    let Some(parts) = anchors.get(page) else {
        return Some("leads to a page that does not exist");
    };
    match anchor {
        Some(anchor) if !parts.contains(anchor) => {
            Some("leads to a part the English page does not have")
        }
        _ => None,
    }
}

#[test]
fn settings_list_their_facts_in_the_page_language() {
    let mut problems = Vec::new();
    for page in pages() {
        let lines: Vec<(usize, &str)> = page.prose().collect();
        for (index, (number, line)) in lines.iter().enumerate() {
            if setting_name(line).is_none() {
                continue;
            }
            let items: Vec<&str> = lines[index + 1..]
                .iter()
                .map(|(_, line)| *line)
                .skip_while(|line| line.trim().is_empty())
                .take_while(|line| line.starts_with("- "))
                .map(|line| &line[2..])
                .collect();
            problems.extend(fact_problems(
                &page.at(*number),
                &page.lang,
                &items,
                page.id == "configuration",
            ));
        }
    }
    assert_none(problems);
}

fn fact_label<'a>(item: &str, labels: &[&'a str]) -> Option<&'a str> {
    labels.iter().copied().find(|label| {
        item.strip_prefix(label)
            .is_some_and(|rest| rest.trim_start().starts_with(':'))
    })
}

fn fact_problems(at: &str, lang: &str, items: &[&str], is_setting: bool) -> Vec<String> {
    let labels = fact_labels(lang);
    let found: Vec<&str> = items
        .iter()
        .filter_map(|item| fact_label(item, &labels))
        .collect();
    let foreign = LANGUAGES
        .iter()
        .filter(|other| **other != lang)
        .any(|other| {
            items
                .iter()
                .any(|item| fact_label(item, &fact_labels(other)).is_some())
        });
    let mut problems = Vec::new();
    if foreign {
        problems.push(format!(
            "{at}: a fact is labelled in another language than the page's"
        ));
    }
    if !found.is_empty() && found.len() != items.len() {
        problems.push(format!(
            "{at}: every fact of a setting is labelled {labels:?}"
        ));
    }
    let needs = is_setting || !found.is_empty();
    if needs && !(found.contains(&labels[0]) && found.contains(&labels[1])) {
        problems.push(format!(
            "{at}: a setting states its {} and its {}",
            labels[0], labels[1]
        ));
    }
    problems
}

#[test]
fn notes_and_code_blocks_use_the_known_forms() {
    let mut problems = Vec::new();
    for page in pages() {
        let mut in_block = false;
        for (number, line, _) in page.body() {
            let at = page.at(number);
            if is_fence(line) {
                let info = line.trim_start().trim_start_matches(['`', '~']).trim();
                if !in_block && info.is_empty() {
                    problems.push(format!("{at}: a code block names its language"));
                }
                in_block = !in_block;
                continue;
            }
            let quoted = line.trim_start().strip_prefix('>').map(str::trim_start);
            if let Some(marker) = quoted.and_then(|rest| rest.strip_prefix("[!")) {
                let kind = marker.split(']').next().unwrap_or_default();
                if !in_block && !NOTE_KINDS.contains(&kind) {
                    problems.push(format!("{at}: notes are one of {NOTE_KINDS:?}, in English"));
                }
            }
        }
    }
    assert_none(problems);
}

#[test]
fn no_dash_and_no_html_comment_anywhere_in_the_folder() {
    let readme = fs::read_to_string(root().join("README.md")).unwrap_or_default();
    let files = pages().into_iter().map(|page| (page.place(), page.text));
    let mut problems = Vec::new();
    for (place, text) in files.chain([("documentation/README.md".to_string(), readme)]) {
        for (index, line) in text.lines().enumerate() {
            let at = format!("{place}:{}", index + 1);
            if line.contains(['\u{2013}', '\u{2014}']) {
                problems.push(format!("{at}: em dash or en dash"));
            }
            if line.contains("<!--") {
                problems.push(format!("{at}: HTML comment"));
            }
        }
    }
    assert_none(problems);
}

#[test]
fn configuration_documents_every_setting_the_code_reads() {
    let read = settings_read_by_the_code();
    let mut problems = Vec::new();
    for page in pages().iter().filter(|page| page.id == "configuration") {
        let documented: BTreeSet<String> = page
            .prose()
            .filter_map(|(_, line)| setting_name(line))
            .collect();
        for name in read.difference(&documented) {
            problems.push(format!(
                "{} has no ### `{name}`, read by the code",
                page.place()
            ));
        }
        for name in documented.difference(&read) {
            if !READ_ELSEWHERE.contains(&name.as_str()) {
                problems.push(format!(
                    "{} documents {name}, which the code never reads",
                    page.place()
                ));
            }
        }
    }
    assert!(!read.is_empty(), "no setting found in src/");
    assert_none(problems);
}

fn settings_read_by_the_code() -> BTreeSet<String> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut names = BTreeSet::new();
    for path in files_under(&src, "rs") {
        let text = fs::read_to_string(&path).unwrap_or_default();
        // The test module, not a lone `#[cfg(test)]` item in the middle of a file.
        let code = text
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or_default();
        names.extend(setting_names(code));
    }
    names
}

fn setting_names(code: &str) -> Vec<String> {
    let mut names = Vec::new();
    for reader in [
        "env_or(\"",
        "parse_env(\"",
        "non_empty_env(\"",
        "env::var(\"",
    ] {
        for (start, _) in code.match_indices(reader) {
            let rest = &code[start + reader.len()..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_')
                .collect();
            if rest[name.len()..].starts_with('"') && !name.is_empty() {
                names.push(name);
            }
        }
    }
    names
}

#[test]
fn heading_addresses_match_the_site() {
    assert_eq!(slugify("Set a check"), "set-a-check");
    assert_eq!(slugify("Who can see the page?"), "who-can-see-the-page");
    assert_eq!(slugify("Foo ?"), "foo-");
    assert_eq!(slugify("Pare-feu et Wi-Fi"), "pare-feu-et-wi-fi");
    assert_eq!(heading_id(3, "`PUBLIC_URL`"), "public_url");
    assert_eq!(
        heading_id(3, "`SMTP_USERNAME`, `SMTP_PASSWORD`"),
        "smtp_username"
    );
    assert_eq!(
        heading_id(2, "The **body** sent to [any tool](x.md)"),
        "the-body-sent-to-any-tool"
    );
    assert_eq!(heading_id(2, "Run `reset-password`"), "run-reset-password");
}
