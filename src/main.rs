use anyhow::{Context, Result, ensure};
use opencc_fmmseg::OpenCC;
use rusqlite::{Connection, OpenFlags, Row};
use serde::Serialize;
use serde_json::{Value, json};
use std::{collections::HashSet, env, io::Write, path::Path};
use tempfile::NamedTempFile;
use unicode_normalization::UnicodeNormalization;
use unicode_script::{Script, UnicodeScript};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

const BANK_SIZE: usize = 10_000;
const ALIASES_SHOWN: usize = 8;
const PUBLIC_ARCHIVE: &str = "CBDB-Yomitan.zip";

struct Person {
    id: i64,
    name: String,
    reading: String,
    dynasty: Option<(String, bool)>,
    period: Option<String>,
    place: Option<(&'static str, String, bool)>,
}

struct Alias {
    name: String,
    reading: String,
    kind: Option<String>,
    code: Option<i32>,
}

#[derive(Serialize)]
struct Term<'a>(
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    i32,
    &'a [Value],
    i64,
    &'a str,
);

fn main() -> Result<()> {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    ensure!(
        matches!(args.len(), 3 | 5),
        "Usage: cbdb-yomitan <CBDB.sqlite3> <output.zip> <source release YYYY-MM-DD> [<owner/repo> <release tag>]"
    );
    let revision = args[2].to_str().context("Release date must be ASCII")?;
    ensure!(
        revision.len() == 10
            && revision.bytes().enumerate().all(|(i, byte)| {
                if i == 4 || i == 7 {
                    byte == b'-'
                } else {
                    byte.is_ascii_digit()
                }
            }),
        "Use YYYY-MM-DD for the release date"
    );
    let output = Path::new(&args[1]);
    let mut index = json!({
            "title": "CBDB 人物",
            "revision": revision,
            "format": 3,
            "sequenced": true,
            "sourceLanguage": "zh",
            "targetLanguage": "en",
            "url": "https://github.com/cbdb-project/cbdb_sqlite",
            "description": "Historical Chinese names and aliases in both scripts, with dates, dynasty, place, and a link to the full profile.",
            "attribution": format!(
                "China Biographical Database Project (CBDB), data release {revision}. Adapted as a Yomitan name dictionary. Data terms: https://cbdb.hsites.harvard.edu/cbdb-data-licensing-terms"
            ),
    });
    if args.len() == 5 {
        let repo = args[3]
            .to_str()
            .context("GitHub repository must be ASCII")?;
        let tag = args[4].to_str().context("Release tag must be ASCII")?;
        ensure!(
            repo.split_once('/')
                .is_some_and(|(owner, repo)| github_component(owner) && github_component(repo))
                && github_component(tag),
            "Expected a GitHub owner/repo and a release tag containing only letters, numbers, dots, underscores, or hyphens"
        );
        ensure!(
            output
                .file_name()
                .is_some_and(|name| name == PUBLIC_ARCHIVE),
            "Use {PUBLIC_ARCHIVE} for a GitHub release"
        );
        let homepage = format!("https://github.com/{repo}");
        index["url"] = json!(homepage);
        index["revision"] = json!(tag);
        index["isUpdatable"] = json!(true);
        index["indexUrl"] = json!(format!("{homepage}/releases/latest/download/index.json"));
        index["downloadUrl"] = json!(format!(
            "{homepage}/releases/download/{tag}/{PUBLIC_ARCHIVE}"
        ));
    }
    build(Path::new(&args[0]), output, &index)
}

fn github_component(value: &str) -> bool {
    !value.is_empty()
        && !matches!(value, "." | "..")
        && value
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, b'.' | b'_' | b'-'))
}

fn build(database: &Path, output: &Path, index: &Value) -> Result<()> {
    let db = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("Opening {}", database.display()))?;
    let mut people_query = db.prepare(include_str!("people.sql"))?;
    let mut aliases_query = db.prepare(include_str!("aliases.sql"))?;
    let mut people = people_query.query([])?;
    let mut aliases = aliases_query.query([])?;
    let mut next_alias = aliases.next()?;
    let converter = OpenCC::new();

    let destination = output.parent().filter(|p| !p.as_os_str().is_empty());
    let mut archive = NamedTempFile::new_in(destination.unwrap_or(Path::new(".")))?;
    let mut zip = ZipWriter::new(archive.as_file_mut());
    let zip_options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(6));
    zip.start_file("index.json", zip_options)?;
    serde_json::to_writer(&mut zip, index)?;
    zip.start_file("styles.css", zip_options)?;
    zip.write_all(include_bytes!("styles.css"))?;

    let mut bank = Vec::with_capacity(8 * 1024 * 1024);
    let mut bank_number = 0;
    let mut row_count = 0;
    let mut person_count = 0;
    let mut skipped_count = 0;
    while let Some(row) = people.next()? {
        let person = read_person(row)?;
        let mut person_aliases = Vec::new();
        while let Some(alias) = next_alias {
            let id: i64 = alias.get(0)?;
            if id > person.id {
                break;
            }
            if id == person.id
                && let Some(alias) = read_alias(alias)?
            {
                person_aliases.push(alias);
            }
            next_alias = aliases.next()?;
        }
        if !is_name(&person.name) {
            skipped_count += 1;
            continue;
        }
        person_count += 1;

        let mut terms = Vec::new();
        let mut seen = HashSet::new();
        let names = std::iter::once((person.name.as_str(), person.reading.as_str(), true)).chain(
            person_aliases.iter().filter_map(|alias| {
                // A lone birth ordinal or surname matches ordinary running text.
                let bare_designation =
                    matches!(alias.code, Some(7 | 12 | 18)) && alias.name.chars().count() == 1;
                (!bare_designation).then_some((alias.name.as_str(), alias.reading.as_str(), false))
            }),
        );
        for (name, reading, primary) in names {
            let short = name
                .split_once(['(', '（'])
                .map(|(base, _)| base.trim_end())
                .filter(|base| name.ends_with([')', '）']) && is_name(base));
            for spelling in std::iter::once(name).chain(short) {
                for term in [
                    spelling.to_owned(),
                    converter.convert(spelling, "t2s", false),
                    converter.convert(spelling, "s2t", false),
                ] {
                    if seen.insert(term.clone()) {
                        terms.push((term, reading, primary));
                    }
                }
            }
        }

        let canonical_card = [profile(&person, &person_aliases, false)];
        let alternate_card = (terms.len() > 1).then(|| [profile(&person, &person_aliases, true)]);
        for (term, reading, primary) in terms {
            if row_count % BANK_SIZE == 0 {
                if !bank.is_empty() {
                    bank.push(b']');
                    zip.write_all(&bank)?;
                    bank.clear();
                }
                bank_number += 1;
                zip.start_file(format!("term_bank_{bank_number}.json"), zip_options)?;
                bank.push(b'[');
            } else {
                bank.push(b',');
            }
            let definition = if term == person.name {
                &canonical_card
            } else {
                alternate_card.as_ref().unwrap_or(&canonical_card)
            };
            let score = if primary {
                10
            } else if term.chars().count() == 1 {
                -30
            } else {
                0
            };
            serde_json::to_writer(
                &mut bank,
                &Term(&term, reading, "", "", score, definition, person.id, ""),
            )?;
            row_count += 1;
        }
    }
    ensure!(row_count > 0, "The database has no searchable names");
    bank.push(b']');
    zip.write_all(&bank)?;
    zip.finish()?;
    archive.as_file_mut().sync_all()?;
    archive
        .persist_noclobber(output)
        .with_context(|| format!("Writing {}", output.display()))?;
    println!("{person_count} people, {row_count} lookup entries, {bank_number} banks");
    println!("Skipped {skipped_count} rows without a searchable Chinese name (excluding ID 0)");
    Ok(())
}

fn read_person(row: &Row<'_>) -> rusqlite::Result<Person> {
    let birth_range = row.get(7)?;
    let death_range = row.get(8)?;
    let birth = year(row.get(5)?).map(|year| qualify_year(year, birth_range));
    let death = year(row.get(6)?).map(|year| qualify_year(year, death_range));
    let active_from = year(row.get(9)?);
    let active_until = year(row.get(10)?);
    let index_year = year(row.get(11)?);
    let period = match (birth, death, active_from, active_until, index_year) {
        (Some(birth), Some(death), ..) => Some(format!("{birth}–{death}")),
        (Some(birth), None, ..) => Some(format!("Born {birth}")),
        (None, Some(death), ..) => Some(format!("Died {death}")),
        (None, None, Some(from), Some(until), _) if from != until => {
            Some(format!("Active {from}–{until}"))
        }
        (None, None, Some(active), _, _) | (None, None, _, Some(active), _) => {
            Some(format!("Active {active}"))
        }
        (_, _, _, _, Some(index)) => Some(format!("Index year {index}")),
        _ => None,
    };
    let native = bilingual(row, 12, 13)?.map(|(name, han)| ("Native place", name, han));
    let index = bilingual(row, 14, 15)?.map(|(name, han)| ("Index place", name, han));
    Ok(Person {
        id: row.get(0)?,
        name: text_field(row, 1)?.unwrap_or_default(),
        reading: text_field(row, 2)?.unwrap_or_default(),
        dynasty: bilingual(row, 3, 4)?,
        period,
        place: native.or(index),
    })
}

fn read_alias(row: &Row<'_>) -> rusqlite::Result<Option<Alias>> {
    let Some(name) = text_field(row, 1)?.filter(|name| is_name(name)) else {
        return Ok(None);
    };
    let code = row.get(3)?;
    let kind = match code {
        Some(3) => Some("別名".into()),
        Some(5) => Some("號".into()),
        _ => text_field(row, 4)?,
    };
    Ok(Some(Alias {
        name,
        reading: text_field(row, 2)?.unwrap_or_default(),
        kind,
        code,
    }))
}

fn text_field(row: &Row<'_>, column: usize) -> rusqlite::Result<Option<String>> {
    let text: Option<String> = row.get(column)?;
    Ok(text
        .map(|text| text.trim().nfc().collect::<String>())
        .filter(|text| {
            !matches!(
                text.as_str(),
                "" | "?" | "？" | "未詳" | "未详" | "未知" | "不詳" | "不详" | "[Missing Data]"
            ) && !text.eq_ignore_ascii_case("unknown")
        }))
}

fn bilingual(
    row: &Row<'_>,
    chinese: usize,
    romanized: usize,
) -> rusqlite::Result<Option<(String, bool)>> {
    Ok(text_field(row, chinese)?
        .map(|text| (text, true))
        .or(text_field(row, romanized)?.map(|text| (text, false))))
}

fn is_name(text: &str) -> bool {
    text.chars()
        .any(|character| character.script() == Script::Han)
}

fn year(value: Option<i32>) -> Option<String> {
    value.filter(|&year| year != 0).map(|year| {
        if year < 0 {
            format!("{} BCE", year.unsigned_abs())
        } else {
            year.to_string()
        }
    })
}

fn qualify_year(year: String, range: Option<i32>) -> String {
    match range {
        Some(-1) => format!("before {year}"),
        Some(1) => format!("after {year}"),
        Some(2) => format!("c. {year}"),
        _ => year,
    }
}

fn node(tag: &str, role: &str, content: impl Serialize) -> Value {
    json!({"tag": tag, "data": {"cbdb": role}, "content": content})
}

fn span(role: &str, text: &str, chinese: bool) -> Value {
    let mut span = node("span", role, text);
    if chinese {
        span["lang"] = json!("zh-Hant");
    }
    span
}

fn detail(label: &str, content: impl Serialize) -> Value {
    node(
        "div",
        "detail",
        [span("label", label, false), node("span", "value", content)],
    )
}

fn profile(person: &Person, aliases: &[Alias], show_name: bool) -> Value {
    let mut blocks = Vec::new();
    if show_name {
        let mut identity = vec![span("name", &person.name, true)];
        if !person.reading.is_empty() {
            identity.push(span("romanization", &person.reading, false));
        }
        blocks.push(node("div", "identity", identity));
    }

    let mut facts = Vec::new();
    if let Some((dynasty, han)) = &person.dynasty {
        facts.push(span("dynasty", dynasty, *han));
    }
    if let Some(period) = &person.period {
        facts.push(span("period", period, false));
    }
    if !facts.is_empty() {
        blocks.push(node("div", "facts", facts));
    }
    if let Some((label, place, han)) = &person.place {
        blocks.push(detail(label, span("place", place, *han)));
    }

    let mut seen_aliases = HashSet::new();
    let mut other_names = Vec::new();
    for alias in aliases {
        if alias.name == person.name || !seen_aliases.insert(&alias.name) {
            continue;
        }
        if seen_aliases.len() <= ALIASES_SHOWN {
            let mut label = Vec::new();
            if let Some(kind) = &alias.kind {
                label.push(span("kind", kind, true));
            }
            label.push(span("alias-name", &alias.name, true));
            other_names.push(node("span", "alias", label));
        }
    }
    if seen_aliases.len() > ALIASES_SHOWN {
        other_names.push(span(
            "more",
            &format!("+{} more", seen_aliases.len() - ALIASES_SHOWN),
            false,
        ));
    }
    if !other_names.is_empty() {
        blocks.push(detail("Other names", other_names));
    }

    blocks.push(node(
        "div",
        "source",
        json!({
            "tag": "a",
            "href": format!("https://cbdb.fas.harvard.edu/cbdbapi/person.php?id={}", person.id),
            "content": format!("CBDB #{} ↗", person.id),
        }),
    ));
    json!({"type": "structured-content", "content": node("div", "profile", blocks)})
}
