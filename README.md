# CBDB 人物 for Yomitan

A Chinese historical name dictionary built from the China Biographical Database. Look up traditional and simplified forms of personal names, courtesy names, style names, and other recorded aliases. Each compact entry shows the available dates, dynasty, and place, with a link to the full CBDB profile.

Where the sources provide more detail, the card also shows a short identity line, a selected appointment, office, examination, or work, and a link to further reading.

## Install

1. Download [CBDB-Yomitan.zip](https://github.com/caocaochan/cbdb-yomitan/releases/latest/download/CBDB-Yomitan.zip).
2. Import the ZIP in Yomitan's dictionary settings with a Chinese profile selected.
3. For later releases, select **Check for Updates** in the dictionary manager, then apply the update on the **CBDB 人物** row.

Try **蘇軾**, **苏轼**, **東坡居士**, or **东坡居士**. They point to the same CBDB profile. Separate people who share a name retain separate CBDB IDs.

The popup displays up to eight aliases. Single-character birth-order designations and surnames remain visible in the profile when present and are left out of the lookup index. Script-converted forms are generated with OpenCC dictionaries and can be ambiguous for rare names.

Mandarin readings show tone marks, such as **Cáo Cāo** for **曹操**. The converter adds tones using the recorded CBDB spelling and a character pronunciation dictionary. It leaves readings blank when the spelling cannot be matched to the characters. Tones may still be ambiguous when several readings share the same spelling.

## Data and attribution

The public dictionary adapts **CBDB_bi_20250520, build 20250604**, from the China Biographical Database Project (CBDB). It uses [the standalone data published through Harvard Dataverse](https://dataverse.harvard.edu/file.xhtml?fileId=14008421). The public dictionary data are distributed under [CC BY-NC-SA 4.0](https://creativecommons.org/licenses/by-nc-sa/4.0/) under [CBDB's data terms](https://cbdb.hsites.harvard.edu/cbdb-data-licensing-terms).

The SHA-256 of the source archive is `dd0536357a09e77cc8af2c7400ac14e63bdadfb6ec933d4c6746d282fc4e1fe4`.

Short descriptions, occupation labels, and external links come from [Wikidata](https://www.wikidata.org/wiki/Property:P497), whose structured data are [CC0](https://www.wikidata.org/wiki/Wikidata:Copyright). The converter uses the CBDB identifier, skips ambiguous cross-references, and excludes years from the imported descriptions. Dates in the card come from CBDB.

Some titles come from the [Academia Sinica Center for Digital Cultures' Database of Names and Biographies](https://old.datahub.io/@ascdc/database-of-the-names-and-biographies-dnb/) (2018 linked-data export, CC BY), matched through the Wikidata authority identifier. The profile links to Academia Sinica when it shows one of these titles.

## Build from SQLite

With stable Rust and Visual Studio Build Tools, use a local CBDB SQLite export and supply its source release date.

```powershell
cargo run --release --locked -- cbdb.sqlite3 CBDB-Yomitan-local.zip 2026-11-14
```

To reproduce the public data input, extract the standalone archive and use its `DATA*.mdb` files with the included Access-to-SQLite adapter. Node 24 provides SQLite and the Access reader handles the MDB format.

```powershell
npm.cmd ci --prefix tools
$mdbFiles = @(Get-ChildItem -LiteralPath .cache/CBDB_bi_20250520_Build20250604 -Filter 'CBDB_20250520_DATA*.mdb' -File | ForEach-Object FullName)
node --max-old-space-size=4096 tools/access-to-sqlite.mjs .cache/cbdb-enriched-source.sqlite3 @mdbFiles
```

Fetch the supplementary data with `./tools/get-wikidata.ps1 -Output .cache/wikidata-enrichment.csv` and download the [Academia Sinica Turtle export](https://api.old.datahub.io/dataset/f2d96429-4157-4445-b396-b9e7dedd43c8/resource/e9125037-1f7f-4257-8b55-1d6048476fb7/download/dump_dnb_20181212_000001.ttl.gz) to `.cache/academia-sinica-dnb.ttl.gz`. The Wikidata query reads a live index, so a later fetch can produce different descriptions and links.

## Publish an update

Use a redistributable CBDB source and a release tag in `YYYY.MM.DD.N` format greater than the previous tag. From the repository root, build and attach these assets to a regular GitHub release. The example uses the date of the data release independently of the dictionary revision.

```powershell
$tag = '2026.11.21.4'
$sourceDate = '2025-05-20'
$releaseDir = ".cache/release-$tag"
New-Item -ItemType Directory -Force $releaseDir | Out-Null
cargo run --release --locked -- .cache/cbdb-enriched-source.sqlite3 "$releaseDir/CBDB-Yomitan.zip" $sourceDate caocaochan/cbdb-yomitan $tag --enrich .cache/wikidata-enrichment.csv .cache/academia-sinica-dnb.ttl.gz
if ($LASTEXITCODE) { throw 'Dictionary build failed' }
tar -xf "$releaseDir/CBDB-Yomitan.zip" -C $releaseDir index.json
if ($LASTEXITCODE) { throw 'Could not extract the update index' }
gh release create $tag "$releaseDir/CBDB-Yomitan.zip" "$releaseDir/index.json" --latest --target main --generate-notes
```

Yomitan fetches the [latest release index](https://github.com/caocaochan/cbdb-yomitan/releases/latest/download/index.json). Each index advertises its own versioned ZIP, so an update retrieves the archive for the revision it saw.
