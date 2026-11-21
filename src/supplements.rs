use anyhow::{Context, Result, ensure};
use flate2::read::MultiGzDecoder;
use oxrdf::{NamedOrBlankNode, Term};
use oxttl::TurtleParser;
use regex::Regex;
use rusqlite::Connection;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::{fs::File, path::Path};

const AGENT: &str = "http://data.ascdc.tw/dnb/Agent/NO";
const SERVICE: &str = "http://data.ascdc.tw/dnb/OfficialService/NO";

pub struct Link {
    pub label: &'static str,
    pub url: String,
}

pub struct Detail {
    pub label: &'static str,
    pub name: String,
    pub year: Option<i32>,
}

pub struct SinicaDetail {
    pub detail: Detail,
    pub link: Link,
}

pub struct Supplement {
    pub description: Option<String>,
    pub link: Option<Link>,
    pub title: Option<SinicaDetail>,
}

#[derive(Deserialize)]
struct WikidataRow {
    cbdb: String,
    person: String,
    description: String,
    occupation: String,
    dnb: String,
    article: String,
    history: String,
}

#[derive(Default)]
struct Candidate {
    qid: u64,
    description: Option<String>,
    occupation: Option<String>,
    dnb: Option<u32>,
    conflicting_dnb: bool,
    article: Option<String>,
    history: Option<String>,
}

#[derive(Default)]
struct Appointment {
    title: Option<String>,
    reference: bool,
    year: Option<i32>,
}

pub fn load(wikidata_path: &Path, sinica_path: &Path) -> Result<HashMap<i64, Supplement>> {
    let candidates = read_wikidata(wikidata_path)?;
    let wanted = candidates
        .values()
        .filter(|candidate| !candidate.conflicting_dnb)
        .filter_map(|candidate| candidate.dnb)
        .collect();
    let sinica = read_sinica(sinica_path, &wanted)?;
    let result = candidates.into_iter().filter_map(|(id, row)| {
        let description = match id {
            30257 => Some("Late Han warlord, statesman, and poet".into()),
            438100 => Some("Late Ming rebel leader".into()),
            _ => row.description.or(row.occupation),
        };
        let link = match id {
            30257 => Some(Link {
                label: "三國志",
                url: "https://zh.wikisource.org/wiki/三國志/卷01".into(),
            }),
            438100 => Some(Link {
                label: "明史",
                url: "https://zh.wikisource.org/wiki/明史/卷309".into(),
            }),
            _ => row.history.map(|url| Link { label: "Wikisource", url })
                .or(row.article.map(|url| Link { label: "Wikipedia", url }))
                .or_else(|| description.is_some().then(|| Link {
                    label: "Wikidata",
                    url: format!("https://www.wikidata.org/wiki/Q{}", row.qid),
                })),
        };
        let title = row.dnb.filter(|_| !row.conflicting_dnb).and_then(|number| {
            let office = sinica.get(&number)?;
            Some(SinicaDetail {
                detail: Detail {
                    label: "Title",
                    name: office.title.clone()?,
                    year: None,
                },
                link: Link {
                    label: "Academia Sinica",
                    url: format!("https://newarchive.ihp.sinica.edu.tw/sncaccgi/sncacFtp?ACTION=TQ%2CsncacFtpqf%2CSN%3D{number:06}%2C2nd%2Csearch_simple"),
                },
            })
        });
        (description.is_some() || link.is_some() || title.is_some())
            .then_some((id, Supplement { description, link, title }))
    }).collect();
    Ok(result)
}

fn read_wikidata(path: &Path) -> Result<HashMap<i64, Candidate>> {
    let mut csv = csv::Reader::from_path(path)
        .with_context(|| format!("Opening Wikidata CSV {}", path.display()))?;
    let date = Regex::new(r"\s*[(（][^()（）]*[0-9][^()（）]*[)）]")?;
    let mut people: HashMap<i64, Candidate> = HashMap::new();
    let mut qids = HashMap::new();
    let mut ambiguous = HashSet::new();

    for row in csv.deserialize() {
        let row: WikidataRow = row?;
        let Some(qid) = row
            .person
            .strip_prefix("http://www.wikidata.org/entity/Q")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            continue;
        };
        let Ok(id) = row.cbdb.parse::<i64>() else {
            continue;
        };
        if id <= 0 || qid == 0 {
            continue;
        }
        if let Some(other) = qids.insert(qid, id)
            && other != id
        {
            ambiguous.extend([other, id]);
        }
        let person = people.entry(id).or_insert_with(|| Candidate {
            qid,
            ..Candidate::default()
        });
        if person.qid != qid {
            ambiguous.insert(id);
            continue;
        }
        if person.description.is_none() {
            person.description = short_description(&date, &row.description);
        }
        if let Some(job) = occupation(&row.occupation)
            && person
                .occupation
                .as_deref()
                .is_none_or(|current| job_rank(job) < job_rank(current))
        {
            person.occupation = Some(job.into());
        }
        if let Ok(authority_id) = row.dnb.parse() {
            if let Some(previous) = person.dnb {
                person.conflicting_dnb |= previous != authority_id;
            } else {
                person.dnb = Some(authority_id);
            }
        }
        choose_article(
            &mut person.article,
            row.article,
            "https://zh.wikipedia.org/wiki/",
        );
        choose_article(
            &mut person.history,
            row.history,
            "https://zh.wikisource.org/wiki/",
        );
    }
    for id in ambiguous {
        people.remove(&id);
    }
    ensure!(
        !people.is_empty(),
        "Wikidata CSV has no usable CBDB identifiers"
    );
    Ok(people)
}

fn short_description(date: &Regex, description: &str) -> Option<String> {
    let without_dates = date.replace_all(description, "");
    let line = without_dates
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let lower = line.to_lowercase();
    let useful = (6..=145).contains(&line.chars().count())
        && !line.bytes().any(|byte| byte.is_ascii_digit())
        && !lower.contains("cbdb")
        && !lower.contains("wikimedia")
        && !lower.ends_with(" dynasty person")
        && !matches!(
            lower.as_str(),
            "human" | "chinese person" | "chinese human" | "person"
        );
    useful.then_some(line)
}

fn occupation(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()
        && value.chars().count() <= 60
        && !value.bytes().any(|byte| byte.is_ascii_digit())
        && !matches!(value, "human" | "person" | "occupation"))
    .then_some(value)
}

fn job_rank(value: &str) -> (bool, &str) {
    (
        matches!(
            value,
            "politician" | "writer" | "artist" | "official" | "civil servant"
        ),
        value,
    )
}

fn choose_article(slot: &mut Option<String>, value: String, prefix: &str) {
    if value
        .strip_prefix(prefix)
        .is_some_and(|page| !page.is_empty())
        && !value.chars().any(char::is_whitespace)
        && slot.as_ref().is_none_or(|existing| value < *existing)
    {
        *slot = Some(value);
    }
}

fn event_key(iri: &str) -> Option<(u32, u32)> {
    let (authority, event) = iri.strip_prefix(SERVICE)?.split_once('/')?;
    Some((authority.parse().ok()?, event.parse().ok()?))
}

fn read_sinica(path: &Path, wanted: &HashSet<u32>) -> Result<HashMap<u32, Appointment>> {
    let archive =
        File::open(path).with_context(|| format!("Opening DNB export {}", path.display()))?;
    // The export has unescaped pipes in unrelated VIAF IRIs. The DNB
    // identifiers we use below are parsed separately as numeric components.
    let triples = TurtleParser::new()
        .lenient()
        .with_base_iri("http://data.ascdc.tw/")?
        .for_reader(MultiGzDecoder::new(archive));
    let mut appointments: HashMap<(u32, u32), Appointment> = HashMap::new();
    let mut linked = HashSet::new();
    for triple in triples {
        let triple = triple.context("Parsing the Academia Sinica Turtle export")?;
        let NamedOrBlankNode::NamedNode(subject) = triple.subject else {
            continue;
        };
        let predicate = triple.predicate.as_str();
        if predicate == "http://data.ascdc.tw/property/officialEvent"
            && let Some(id) = subject
                .as_str()
                .strip_prefix(AGENT)
                .and_then(|s| s.parse::<u32>().ok())
            && wanted.contains(&id)
            && let Term::NamedNode(value) = &triple.object
            && let Some(key) = event_key(value.as_str())
            && key.0 == id
        {
            linked.insert(key);
        }
        if let Some(key) = event_key(subject.as_str())
            && wanted.contains(&key.0)
            && let Term::Literal(value) = triple.object
        {
            match predicate {
                "http://www.w3.org/2000/01/rdf-schema#label" if value.language() == Some("zh") => {
                    let title = value.value().trim();
                    if (2..=45).contains(&title.chars().count())
                        && !matches!(title, "未詳" | "未知" | "不詳")
                    {
                        appointments.entry(key).or_default().title = Some(title.into());
                    }
                }
                "http://purl.org/dc/elements/1.1/source" if !value.value().trim().is_empty() => {
                    appointments.entry(key).or_default().reference = true;
                }
                "http://www.w3.org/2006/time#inXSDgYear" => {
                    appointments.entry(key).or_default().year = value.value().parse().ok();
                }
                _ => (),
            }
        }
    }
    let mut selected: HashMap<u32, ((Option<i32>, u32), Appointment)> = HashMap::new();
    for (key, record) in appointments {
        if record.reference && record.title.is_some() && linked.contains(&key) {
            let rank = (record.year, key.1);
            if selected
                .get(&key.0)
                .is_none_or(|(current, _)| *current < rank)
            {
                selected.insert(key.0, (rank, record));
            }
        }
    }
    ensure!(
        !selected.is_empty(),
        "DNB export yielded no linked, cited appointments"
    );
    Ok(selected
        .into_iter()
        .map(|(id, (_, record))| (id, record))
        .collect())
}

pub fn recorded_careers(db: &Connection) -> Result<HashMap<i64, Detail>> {
    let tables: i64 = db.query_row(
        "SELECT count(*) FROM sqlite_schema WHERE type = 'table' AND upper(name) IN (
            'POSTED_TO_OFFICE_DATA', 'OFFICE_CODES', 'ENTRY_DATA', 'ENTRY_CODES', 'BIOG_TEXT_DATA', 'TEXT_CODES'
        )", [], |row| row.get(0))?;
    if tables != 6 {
        return Ok(HashMap::new());
    }
    let mut query = db.prepare(include_str!("career.sql"))?;
    let rows = query.query_map([], |row| {
        let label = match row.get_ref(1)?.as_str()? {
            "Office" => "Office",
            "Appointment" => "Appointment",
            "Exam" => "Exam",
            "Work" => "Work",
            name => unreachable!("Unexpected career detail label {name}"),
        };
        Ok((
            row.get(0)?,
            Detail {
                label,
                name: row.get(2)?,
                year: row.get(3)?,
            },
        ))
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn ignores_conflicting_identifiers_and_keeps_same_name_records_separate() {
        let mut data = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            data,
            "cbdb,person,description,occupation,dnb,article,history"
        )
        .unwrap();
        for (id, qid, description) in [
            (30257, 204077, "Chinese warlord and statesman (155–220)"),
            (438100, 3324401, "Ming dynasty rebel leader (died 1642)"),
            (7, 8, "Song dynasty person CBDB = 7"),
            (1, 2, "Chinese poet"),
            (1, 3, "Chinese calligrapher"),
            (4, 5, "Chinese writer"),
            (6, 5, "Chinese writer"),
        ] {
            writeln!(
                data,
                "{id},http://www.wikidata.org/entity/Q{qid},{description},,,,"
            )
            .unwrap();
        }
        writeln!(
            data,
            "unknown,http://www.wikidata.org/entity/Q11,unknown,,,,"
        )
        .unwrap();
        let people = read_wikidata(data.path()).unwrap();
        assert_eq!(people.len(), 3);
        assert_eq!(
            people[&30257].description.as_deref(),
            Some("Chinese warlord and statesman")
        );
        assert_eq!(
            people[&438100].description.as_deref(),
            Some("Ming dynasty rebel leader")
        );
        assert!(people[&7].description.is_none());
    }
}
