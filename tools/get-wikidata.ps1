param(
    [Parameter(Mandatory)]
    [string] $Output
)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$query = @'
PREFIX wdt: <http://www.wikidata.org/prop/direct/>
PREFIX p: <http://www.wikidata.org/prop/>
PREFIX pq: <http://www.wikidata.org/prop/qualifier/>
PREFIX schema: <http://schema.org/>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
SELECT ?cbdb ?person ?description ?occupation ?dnb ?article ?history WHERE {
  ?person wdt:P497 ?cbdb.
  OPTIONAL { ?person schema:description ?description. FILTER(LANG(?description) = "en") }
  OPTIONAL {
    ?person wdt:P106 ?job.
    ?job rdfs:label ?occupation.
    FILTER(LANG(?occupation) = "en")
  }
  OPTIONAL { ?person wdt:P6705 ?dnb. }
  OPTIONAL { ?article schema:about ?person; schema:isPartOf <https://zh.wikipedia.org/>. }
  OPTIONAL {
    ?person p:P1343/pq:P805 ?chapter.
    ?history schema:about ?chapter; schema:isPartOf <https://zh.wikisource.org/>.
  }
}
'@

curl.exe --fail --silent --show-error --get --data-urlencode "query=$query" `
    --header 'Accept: text/csv' --compressed --max-time 120 `
    --output $Output 'https://qlever.dev/api/wikidata'
