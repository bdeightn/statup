# Statup documentation

The pages of the documentation published at [statup.dev/docs](https://statup.dev/docs/), in English and French. Each language has its folder (`en`, `fr`) holding the same pages under the same file names; `nav.json` gives their order, their groups and the languages published. The site copies this folder at the tag of each release, so a page describes the version it was tagged with.

English is the source. A change to a page is made in English and in French in the same change, in the same commit as the code it describes: the site does not build if a language misses a page, or if a page's headings differ from the English ones. `tests/documentation.rs` checks the same rules in `cargo test`.

## A page

```markdown
---
title: Monitoring
description: "One sentence, shown under the title, in the list of pages and in search results."
---

Text in Markdown. Headings start at `##`: the title is the page's `#`.
```

- `description` in double quotes, always.
- In the words of the app: every label as `locales/<language>.json` writes it, in bold (**Send a test**, **Envoyer un test**).
- A page added to a folder is added to every folder and to `nav.json`, or the site does not build.
- No HTML comment: the site refuses to serve one.
- No em dash, no en dash.

## Headings and their addresses

A page in French keeps the headings of the English page: the same number, at the same levels, in the same order. Each one takes the address of the English heading at the same place, so `#set-a-check` reaches the same part on `/docs/monitoring/` and on `/fr/docs/monitoring/`, and the app can link to it in any language.

An English heading's address is its words in lower case, spaces as hyphens, punctuation dropped: `## Set a check` is `#set-a-check`. A setting's heading is addressed by its name in lower case: `#public_url`. The app links to these addresses: rename an English heading only on purpose.

## Links

To another page: `[Alerts](alerts.md)`, or to one of its parts: `[PUBLIC_URL](configuration.md#public_url)`. The same in every language: the site leads to the page in the reader's language. Both work on GitHub too.

## A setting

A level-three heading that holds only the setting's name, then a list of its facts, then what it does:

```markdown
### `MONITORING`

- Default: `true`
- Values: `true`, `false`
- Since: 0.2.0

Run the checks of the services that have one.
```

The labels of the facts, in each language:

| | Default | Values | Since |
|---|---|---|---|
| en | `Default:` | `Values:` | `Since:` |
| fr | `Par défaut :` | `Valeurs :` | `Depuis :` |

The default and the values always; since when only for a setting that arrived after 0.1.0. Any level-three heading made only of code is drawn as an entry of this kind, facts or not, as the fields of the [webhook body](en/webhook.md) are.

## Notes

GitHub's alerts, one per need, kept short. The marker stays in English in every language; the site labels the note in the page's language.

```markdown
> [!NOTE]
> Something the reader would otherwise miss.

> [!WARNING]
> Something that breaks or exposes the instance if ignored.
```

`[!TIP]`, `[!IMPORTANT]` and `[!CAUTION]` work too.

## Code

Every block names its language: `bash`, `env`, `yaml`, `json`, `nginx`, `caddy`, `nix`, `text`. The site shows the name and a Copy button. A block holds what the reader types or pastes, without a prompt, and is the same in every language; only a comment line may be translated.
