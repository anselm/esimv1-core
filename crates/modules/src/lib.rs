//! Modules. Each kind registers two functions in `KINDS`: `build`, which constructs it
//! (and fetches its data), and `describe`, which states its surface from the parameters
//! alone, fetching nothing. The kernel checks wiring from the descriptions; the CLI
//! prints them for people and for the facade.

pub mod water;
pub mod sun;
pub mod heat;
pub mod basin;
pub mod weather;
pub mod veg;
pub mod fauna;
pub mod fire;
pub mod farms;
pub mod rules;
pub mod nitrogen;
pub mod planet;
pub mod countries;
pub mod wildfire;
pub mod record;
pub mod suppression;
pub mod hotspots;
pub mod firecells;
pub mod shock;
pub mod score;
pub mod capacity;
pub mod heatwaves;
pub mod places;
pub mod coast;
pub mod quake;
pub mod lifelines;
pub mod aftermath;
pub mod fragility;
pub mod tanks;
pub mod bridges;
pub mod power;
pub mod routes;
pub mod depot;
pub mod surfacewater;
pub mod hospitals;
pub mod agents;
pub mod losses;
pub mod landscape;

use esim_kernel::describe::Describe;
use esim_kernel::Module;

pub struct Kind {
    pub kind: &'static str,
    pub build: fn(&str, &toml::Value) -> Result<Box<dyn Module>, String>,
    pub describe: fn(&str, &toml::Value) -> Describe,
}

macro_rules! kind {
    ($k:literal, $t:ty) => {
        Kind { kind: $k, build: |n, p| Ok(Box::new(<$t>::new(n, p)?) as Box<dyn Module>), describe: <$t>::describe }
    };
}

pub static KINDS: &[Kind] = &[
    kind!("water.source", water::Source),
    kind!("water.bucket", water::Bucket),
    kind!("sun.irradiance", sun::Sun),
    kind!("heat.slab", heat::Heat),
    kind!("water.basin", basin::Basin),
    kind!("weather.era5", weather::Weather),
    kind!("veg.crop", veg::Veg),
    kind!("fauna.herbivore", fauna::Herbivore),
    kind!("fauna.predator", fauna::Predator),
    kind!("hazard.fire", fire::Fire),
    kind!("agents.farms", farms::Farms),
    kind!("rules.calendar", rules::Rules),
    kind!("nitrogen.basin", nitrogen::Nitrogen),
    kind!("carbon.emissions", planet::EmissionsIn),
    kind!("carbon.pools", planet::Pools),
    kind!("energy.twolayer", planet::TwoLayer),
    kind!("obs.planet", planet::PlanetObs),
    kind!("carbon.gases", planet::Gases),
    kind!("sea.thermal", planet::SeaThermal),
    kind!("forcing.ar6", planet::ForcingAr6),
    kind!("people.countries", countries::PeopleCountries),
    kind!("climate.countries", countries::ClimateCountries),
    kind!("people.heat", countries::PeopleHeat),
    kind!("people.wetbulb", countries::PeopleWetBulb),
    kind!("tipping.elements", countries::Tipping),
    kind!("carbon.tipped", countries::TippedCarbon),
    kind!("story.bottomline", countries::Story),
    kind!("fire.rothermel", wildfire::Rothermel),
    kind!("obs.perimeters", wildfire::Perimeters),
    kind!("obs.dins", wildfire::Structures),
    kind!("obs.record", record::Record),
    kind!("fire.suppression", suppression::Suppression),
    kind!("obs.hotspots", hotspots::Hotspots),
    kind!("fire.cells", firecells::Cells),
    kind!("events.shock", shock::Shock),
    kind!("score.regions", score::Score),
    kind!("policy.capacity", capacity::Capacity),
    kind!("weather.heatwaves", heatwaves::Heatwaves),
    kind!("obs.places", places::Places),
    kind!("coast.oneline", coast::OneLine),
    kind!("quake.ground", quake::Ground),
    kind!("quake.buildings", quake::Buildings),
    kind!("portland.lifelines", lifelines::Lifelines),
    kind!("people.aftermath", aftermath::Aftermath),
    kind!("fuel.tanks", tanks::Tanks),
    kind!("roads.bridges", bridges::Bridges),
    kind!("power.structures", power::Power),
    kind!("roads.routes", routes::Routes),
    kind!("fuel.depot", depot::Depot),
    kind!("water.surface", surfacewater::SurfaceWater),
    kind!("health.hospitals", hospitals::Hospitals),
    kind!("people.agents", agents::Agents),
    kind!("obs.losses", losses::Losses),
    kind!("ground.terrain", landscape::Terrain),
    kind!("weather.surface", landscape::Surface),
    kind!("fuel.moisture", landscape::Moisture),
    kind!("fauna.flee", landscape::Flee),
    kind!("ground.cover", landscape::Cover),
    kind!("roads.major", landscape::Roads),
    kind!("built.footprints", landscape::Footprints),
];

fn find(kind: &str) -> Result<&'static Kind, String> {
    KINDS.iter().find(|k| k.kind == kind).ok_or_else(|| format!("unknown module kind {kind}"))
}

pub fn factory(name: &str, params: &toml::Value) -> Result<Box<dyn Module>, String> {
    let kind = params.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    build(kind, name, params)
}

/// Build by kind. The kernel passes params only, so the CLI folds `kind` into them.
pub fn build(kind: &str, name: &str, params: &toml::Value) -> Result<Box<dyn Module>, String> {
    (find(kind)?.build)(name, &with_place(params))
}

/// A convention of this library's modules, not of the engine: a module that works on a window of
/// the Earth takes it as one parameter, `place`, a table of `lat_s`, `lat_n`, `lon_w`, `lon_e` and
/// `res_arcsec` (and whatever else belongs to the place). Its keys are read as the module's own,
/// and a key the module sets itself wins. The manifest reader knows nothing of this: it hands each
/// module the parameters the manifest gave it.
fn with_place(params: &toml::Value) -> std::borrow::Cow<'_, toml::Value> {
    let Some(toml::Value::Table(place)) = params.get("place") else { return std::borrow::Cow::Borrowed(params) };
    let mut p = params.clone();
    if let toml::Value::Table(t) = &mut p { for (k, v) in place { t.entry(k.clone()).or_insert_with(|| v.clone()); } }
    std::borrow::Cow::Owned(p)
}

/// The warehouse product a module reads for `what` (roads, names, buildings, shore): the key of that
/// name in the manifest's `data` table, which a script states once and hands to each module as it
/// hands `place`. Empty when none is named. Products are addresses in an esimv1-data store.
pub(crate) fn data_of(params: &toml::Value, what: &str) -> String {
    params.get("data").and_then(|d| d.get(what)).and_then(|v| v.as_str()).unwrap_or("").to_string()
}
/// The roads and names of the manifest's `data` as a map table: read from the warehouse (and kept
/// reduced in the cache), or None with a line on stderr.
pub(crate) fn map_context(roads: &str, names: &str) -> Option<serde_json::Value> {
    if roads.is_empty() && names.is_empty() { return None; }
    let mut fe = esim_registry::Fetcher::default_for(&esim_registry::paths::root()).ok()?;
    match esim_registry::warehouse::context(&mut fe, roads, names) { Ok(v) => Some(v), Err(e) => { eprintln!("context: {e} (the map will have no roads or names)"); None } }
}
/// The same table if a run has already made it; never fetches (for a describe).
pub(crate) fn cached_map_context(params: &toml::Value) -> Option<serde_json::Value> {
    let fe = esim_registry::Fetcher::default_for(&esim_registry::paths::root()).ok()?;
    esim_registry::warehouse::cached_context(&fe, &data_of(params, "roads"), &data_of(params, "names"))
}

/// Describe a kind under a name and parameters without building it. Nothing is fetched.
pub fn describe(kind: &str, name: &str, params: &toml::Value) -> Result<Describe, String> {
    let mut d = (find(kind)?.describe)(name, &with_place(params));
    if d.layer.is_empty() { d.layer = layer_of(kind).to_string(); }
    if d.title.is_empty() { d.title = title_of(kind).to_string(); }
    Ok(d)
}

/// The layers of the world as a person would list them, in reading order: what is not alive
/// (natural things, then physical artifacts, then symbolic ones), then what is alive, people
/// last because they read everything and write back to all of it. `across` is a disturbance
/// that cuts through the layers; `record` is a module that scores a run against one. The order
/// is who reads whom, not slow to fast (Anselm, 2026-11-13: built before life so the abiotic
/// realm is clumped; law and money are artifacts). A tag for pages; the kernel is flat.
/// Each layer's number orders it and leaves room: a layer between water (3000) and air (4000)
/// would be 3500. `across` and `record` are 0, because they are not places in the order.
pub const LAYERS: [(u32, &str, &str); 12] = [
    (1000, "sun", "Sun"), (2000, "ground", "Ground"), (3000, "water", "Water"), (4000, "air", "Air and weather"),
    (5000, "built", "Built structures"), (6000, "flows", "Flows through the built"), (7000, "rules", "Law and money"),
    (8000, "plants", "Plants"), (9000, "animals", "Animals"), (10000, "people", "People"),
    (0, "across", "Disturbances"), (0, "record", "The record"),
];

/// What a kind is in a few plain words, for a box on a diagram, when its describe does not say.
/// Every kind has a row; `tests/describe.rs` fails on a kind without one.
pub fn title_of(kind: &str) -> &'static str {
    match kind {
        "water.source" => "A tap",
        "water.bucket" => "A leaky bucket",
        "sun.irradiance" => "Sunlight on the ground",
        "heat.slab" => "Heat in the ground surface",
        "water.basin" => "Rain, soil and river",
        "weather.era5" => "Weather from the record",
        "veg.crop" => "Crops and grass",
        "fauna.herbivore" => "Grazing animals",
        "fauna.predator" => "Predators",
        "hazard.fire" => "Cropland fire",
        "agents.farms" => "Farmers' choices",
        "rules.calendar" => "A land-use rule",
        "nitrogen.basin" => "Nitrogen in the soil",
        "carbon.emissions" => "CO2 emitted each year",
        "carbon.pools" => "CO2 in the air",
        "energy.twolayer" => "Heat in the ocean",
        "obs.planet" => "Measured CO2 and warming",
        "carbon.gases" => "Methane and nitrous oxide",
        "sea.thermal" => "Sea level from warming",
        "forcing.ar6" => "Other climate forcings",
        "people.countries" => "Where people live",
        "climate.countries" => "Each country's climate",
        "people.heat" => "People outside the niche",
        "people.wetbulb" => "Dangerous humid heat",
        "tipping.elements" => "Tipping points",
        "carbon.tipped" => "Carbon from tipping",
        "story.bottomline" => "The story so far",
        "fire.rothermel" => "Fire spread (Rothermel)",
        "obs.perimeters" => "Mapped fire edge",
        "obs.dins" => "Inspected structures",
        "obs.record" => "The incident record",
        "fire.suppression" => "Firefighting",
        "obs.hotspots" => "Fire seen from orbit",
        "fire.cells" => "Fire spread, cell by cell",
        "events.shock" => "Wild cards",
        "score.regions" => "Countries scored",
        "policy.capacity" => "Capacity to cut emissions",
        "weather.heatwaves" => "Heatwaves",
        "obs.places" => "Named places",
        "coast.oneline" => "The shoreline",
        "quake.ground" => "Ground shaking",
        "quake.buildings" => "Building damage",
        "portland.lifelines" => "Services after the quake",
        "people.aftermath" => "Deaths in the aftermath",
        "fuel.tanks" => "Fuel tanks",
        "roads.bridges" => "Bridges",
        "power.structures" => "Power lines and towers",
        "roads.routes" => "Emergency routes",
        "fuel.depot" => "Fuel account",
        "water.surface" => "Where water lies",
        "health.hospitals" => "Hospitals",
        "people.agents" => "People, one by one",
        "obs.losses" => "Loss estimates on record",
        "ground.terrain" => "The relief",
        "weather.surface" => "Wind and air at the ground",
        "fuel.moisture" => "How dry the fuel is",
        "fauna.flee" => "Animals fleeing",
        "ground.cover" => "What covers the ground",
        "roads.major" => "Major roads",
        "built.footprints" => "Buildings",
        _ => "",
    }
}

/// The layer a kind belongs to when its describe does not say: by the kind's family, with the
/// exceptions named first.
pub fn layer_of(kind: &str) -> &'static str {
    match kind {
        "quake.ground" | "hazard.fire" | "carbon.tipped" => "across",
        "quake.buildings" => "built",
        "fuel.moisture" => "plants",
        "ground.terrain" => "ground",
        "ground.cover" => "plants",
        "built.footprints" => "built",
        "fauna.flee" => "Animals fleeing",
        "portland.lifelines" | "fuel.depot" => "flows",
        "health.hospitals" => "built",
        "agents.farms" => "people",
        "nitrogen.basin" => "water",
        _ => match kind.split('.').next().unwrap_or("") {
            "sun" => "sun",
            "water" | "sea" | "coast" => "water",
            "weather" | "carbon" | "energy" | "forcing" | "heat" | "climate" => "air",
            "fuel" | "roads" => "built",
            "power" => "flows",
            "rules" | "policy" => "rules",
            "veg" => "plants",
            "fauna" => "animals",
            "people" => "people",
            "fire" | "events" | "tipping" => "across",
            "obs" | "score" | "story" => "record",
            _ => "",
        },
    }
}

pub fn kinds() -> Vec<&'static str> { KINDS.iter().map(|k| k.kind).collect() }
