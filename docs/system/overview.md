<!-- order: 1 -->
<!-- section: Start here -->
<!-- author: Claude -->
<!-- ordered-by: Anselm -->
<!-- written: 2026-11-01 -->
<!-- revised: 2026-11-26 -->
<!-- revision: 1 -->
# What this is

A whole-system simulator: many small pieces of science on one clock, each reading real
records and writing into shared state, with accounts that must balance at every seam. A
person, or more often a conversation with a model, arranges the pieces into a manifest;
the kernel runs it; the result is a run folder anyone can replay, score, audit and look
at.

It is a bucket of tools, not a product. Everything is a file or a command: the module
kinds are code in one repository today and will be code at URLs tomorrow; a manifest is
a short file in JavaScript, JSON or TOML; a project is a folder; a run is a folder; a page like this is a Markdown
file. The viewer and the server only read and write those files, and nothing on them is
needed to use the tools. That is deliberate. The people who build engines and models in
this field are not competitors; the aim is to give them and anyone else a place where
pieces from different hands can be checked against each other and against the record.

## Whole systems from separate parts

The proposition the project tests is that a model of a whole place can be assembled from
parts that do not know each other, written by different hands and reused from one place
to the next. The fire scenarios are the worked example. A fire here is not one program.
It is about ten modules:

| part | what it holds | module |
| --- | --- | --- |
| the relief | height, slope and aspect of every 30 m cell | `ground.terrain` |
| land cover | what grows or stands on each cell | `ground.cover` |
| roads | the major roads, as breaks in the fuel | `roads.major` |
| buildings | every footprint with its height, as a table | `built.footprints` |
| weather | the hour's wind, temperature and humidity | `weather.surface` |
| fuel moisture | how dry the dead fuel is this hour | `fuel.moisture` |
| the fire | where it burns and when each cell caught | `fire.cells` or `fire.rothermel` |
| animals | a population that runs from the front | `fauna.flee` |
| firefighting | the edge the crews can hold | `fire.suppression` |
| the records | mapped perimeters, satellite detections, inspected structures | `obs.*` |

Each part reads its own source and states its own assumptions. They meet only in shared
maps and tables that one module owns and the others read, and `esim check` refuses a
scenario in which a part reads something nobody writes. Three results follow, each of
which can be seen in the runs:

- **A part can be swapped.** The Palisades fire and the Devil's Gulch scenario each run on
  two fire models. Rothermel's 1972 spread model and our own cell model read the same
  relief, land cover, weather and fuel moisture, from the same modules with the same
  settings. When the separated Rothermel fire was compared with the older all-in-one
  version on eight runs, the burned area was the same at every hour.
- **A part can be reused somewhere else.** Fort McMurray, in boreal Canada, uses the same
  parts as Los Angeles with two of them pointed at other sources: buildings from
  OpenStreetMap where there is no county survey, and weather from a global reanalysis
  where the United States' 3 km analysis stops at the border.
- **A new part can join without the others changing.** The animals were added as one
  module and one line in each scenario. They read where the fire is and when each cell
  caught, and the fire was not edited.

The limits of this, stated so they are not mistaken for results. The animals are a
placeholder with no species, there to show that a neighbour can react to the fire. Parts
see each other once an hour of simulated time; a part that wants finer time replays the
hour from what the fire recorded, which works only in one direction. Two processes that
drive each other within the hour, such as a fire and the wind it makes, would have to be
one part or take a shorter step, and that has not been tried. Some concerns should not be
split: heat passing between cells, ignition and burn-out are one computation, and they
stay inside the fire. Portland's earthquake (fuel tanks, bridges, power, routes,
hospitals, a fuel account, people) and Earth 2100 (eighteen modules from emissions to
countries) are assembled the same way; [Limits](/docs/system/limits) says what each part
gets wrong.

## The shape, in one paragraph

Records come in through a registry that pins every file it fetches by hash. Modules read
them and integrate forward, exchanging events, shared fields (maps of numbers) and shared
tables (lists of things, such as buildings), and moving conserved quantities through
ledgers. A manifest names the modules, their parameters, the place and
the window; manifests can include other manifests and projects can run a manifest under
named variants. Every run writes its log, its series, its map frames and its ledgers; an
auditor reads the finished folder and says what the numbers mean. Scores compare model
series to observed ones. The viewer shows a project's runs with their maps under one
time cursor.

## Where to start reading

- [Terminology](/docs/system/terminology), because the words have been fuzzy.
- [Honesty](/docs/system/honesty), the rules that make the rest worth doing.
- [Modules](/docs/system/modules), [Layers](/docs/system/layers), [Manifests](/docs/system/manifests), [Projects and runs](/docs/system/projects-and-runs).
- [Conversation](/docs/system/conversation), how a scenario gets built without hand-writing TOML.
- [Data](/docs/system/data), [Scoring and tearing](/docs/system/scoring-and-tearing), [Evolution](/docs/system/evolution).
- [Workspace](/docs/system/workspace), how the folders are laid out and how to behave in a shared one.
- [Time](/docs/system/time), [Vocabulary](/docs/system/vocabulary), [Adding a module](/docs/system/adding-a-module), [Limits today](/docs/system/limits).
- The future as tables: [Known future events](/docs/events/known), the calendar with a date and a source per row, and [Wild cards](/docs/events/wild-cards), the black swans with a rough chance a year and the effect a run applies.
- A guide that is not about this engine: [How to write your own digital twin](/docs/guides/writing-a-digital-twin), for people and for language models to read as context.
- A proposal: [RFC 0001](/docs/rfc/0001-manifest), the manifest as a common transport for simulations between engines, in JSON, with a schema.

The design brief and every decision since are dated entries in the repository's
`devlog/` folder. This documentation describes the system as it stands; the devlog says
why it stands that way.
