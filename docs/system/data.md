<!-- order: 9 -->
<!-- section: Projects and the record -->
<!-- author: Claude -->
<!-- ordered-by: Anselm -->
<!-- written: 2026-11-01 -->
<!-- revised: 2026-11-28 -->
<!-- revision: 1 -->
# Data

## The registry

Every byte a module reads comes through the registry: a catalog of sources, a fetcher
with a content cache and a lock file, and readers for each format. The catalog
(`data/catalog.toml`) holds, per asset, its URL template, format, units, source,
licence, the date and machine it was last verified from, and notes about its quirks.
The lock file (`data/catalog.lock.toml`, committed) holds the sha256 of every file ever
fetched, so a run's inputs can be checked byte for byte later. The cache
(`data/cache/`, not committed) holds the bytes.

Sources today, all anonymous: Copernicus elevation tiles; USGS gauges; GHCN-Daily
stations; ERA5 and GEFS through Open-Meteo; MODIS vegetation through the Planetary
Computer; Global Carbon Project emissions through Our World in Data; Mauna Loa, methane
and nitrous oxide from NOAA; Law Dome ice cores; HadCRUT5 and GISTEMP; the IPCC AR6
assessed forcings; Natural Earth outlines and capitals; UN population; CRU country
temperatures through the World Bank portal; NOAA's station history and daily station
summaries (GSOD) for the weather station nearest each capital. Each
catalog entry says what was verified and what licence text still has to be fetched and
stored before any public use; [Sources](/docs/system/sources) is that list as a page.

For the fire (since 2026-11-03) the catalog adds ESA WorldCover, NOAA's HRRR analyses read by
byte range, Open-Meteo's ERA5 wind with gusts, NIFC's perimeters, CAL FIRE's damage
inspections, NOAA's emergency response imagery and UL Research Institutes' timestamped
incident dataset.

## The data warehouse

Since 2026-11-28 the engine asks OpenStreetMap, or a county's map service, for nothing itself.
The buildings, roads, place names and shoreline of a place come from a separate store of
small data products, the data warehouse, at <https://data.earthlight.cloud>. It is made by a
separate tool, [esimv1-data](https://github.com/anselm/esimv1-data), which does not depend on
the engine. Several small global tables (emissions, the temperature records, country outlines,
population) are read from it too, as the same bytes the runs used before.

A manifest names the products it reads by their address in the store, once, in a `data` table
that it hands to each module beside `place`:

```js
const data = {
  roads: "roads/osm/nicasio/2026-11-27",
  names: "names/osm/nicasio/2026-11-28",
  buildings: "buildings/massing/nicasio/2026-11-27",
};
const roads = { kind: "roads.major", params: { place, data } };
```

A product is never changed once made: a newer map is a new product under a newer date. So the
address in the manifest fixes the bytes a run reads, and the product's own description states
a checksum that the engine checks the bytes against. A roads or buildings product that is
named and cannot be read stops the run. A names product only labels the map, so its absence
costs the labels and nothing else.

`ESIM_DATA_STORES` lists the stores to read, separated by commas, each a folder or a web
address. Left unset, the engine reads the workspace's own `dataservice/store` when it exists
and then the public store. One product in use is listed in the public store without its bytes:
Los Angeles County's building footprints for the Palisades fire, because the county's terms
were not read when it was made. Anyone can rebuild it with the data tool from the request its
listing carries.

What the change did to the runs is in the hub's devlog entry of 2026-11-28. In short: runs
that take only their map's labels from these products, and the shoreline model, are identical.
The three cell-fire scenarios read roads and buildings as ground, the products differ from the
engine's old copies of them, and those fires moved.

## The record of an event

A studied event gets a folder under `data/events/`, committed, holding a reading copy of the
public record it is scored against, with a README that names each source, its licence and the
day it was fetched: `data/events/palisades-2025/` holds CAL FIRE's 344 status reports as a
table, UL FSRI's 4,785 timestamped events and the text of its timeline report, and NIFC's
2,334 fire lines and 825 points. The catalog stays the registry for what modules fetch; the
folder is so the record cannot drift or vanish under the runs, and so a reader of the
repository sees the sources beside the code.

## Sizes and how they are kept in hand

A basin stage warms about half a gigabyte of cache: four elevation tiles, a year of
hourly weather at a dozen points, vegetation composites, gauge and station records. The
planetary layers add about 25 MB of small files. Four habits keep this workable:

- **Fetch once, hash, reuse.** Nothing is downloaded twice; ensembles warm the cache
  before they spawn threads.
- **Resolve to what the question needs.** The basin runs at 30 arc-seconds; the country
  layer at two degrees. Finer is four times the cost per halving and rarely more
  truthful than the record behind it.
- **Mind the other side's limits.** Open-Meteo weights large hourly requests heavily and
  rate-limited the capitals' ten-year hourly fetch after twenty; the wet-bulb layer uses
  NOAA's daily station summaries instead, 1,700 small files from a bucket with no such
  limit. When a source throttles, change the source or the resolution, not the record.
- **Thin the output, not the state.** Modules write map frames on a stated cadence
  (every five years for a 250-year run) while integrating at their own step. Series are
  kept at full resolution because they are small.

## Scoreboards

A record a module reports as a series, beside the model's series of the same quantity:
the gauge's discharge next to the simulated discharge, the satellite's vegetation index
next to the crop's, Mauna Loa next to the carbon pools. A manifest names the pairs to
score and `esim run` scores them. The viewer draws the pair on one chart.

## Provenance of the data layer itself

A reader converts units on the way in and says so in the catalog; a record's known gaps
are in the notes (the vegetation archive has no composites for part of 2025 over the
basin's tile; the weather archive lags five days); provisional records are marked and
the lock pins the bytes a run used so a later revision does not silently change a result.

## What is not here

A general gridded climate record at useful resolution: the obvious file is HDF5 and the
project avoids heavy dependencies, so the country layer uses a per-country record
instead and says the loss. A population layer finer than a country. Humidity. Ocean
heat content, whose server was down the day it was tried. Each is on the list, with the
URL that answered.

## Where data lives later

The cache and the run folders are immutable blobs with content hashes and belong in
object storage; the catalog and the lock file belong in git. Which data sources exist, at
what licence, for what questions, is the kind of enumeration that belongs on a public
wiki rather than in one project's catalog, and the catalog is written so it can be
lifted there.
