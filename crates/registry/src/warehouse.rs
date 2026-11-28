//! Reading from an esimv1-data store: the warehouse of small data products kept beside the engine
//! (https://github.com/anselm/esimv1-data). A product is named by its address in the store,
//! `<plane>/<source>/<place>/<date>`, for example `buildings/massing/nicasio/2026-11-27`. Products
//! are never changed once made, so the address in a manifest fixes the bytes a run reads; the
//! product's own description states a checksum for its file, and the bytes are checked against it.
//!
//! Stores are tried in order. `ESIM_DATA_STORES` lists them, separated by commas: folders or web
//! addresses. Left unset, the list is the workspace's own `dataservice/store` when it exists (the
//! builder's copy, which also holds products whose source does not allow passing the bytes on)
//! and then the public store. The engine asks OpenStreetMap, or a county's feature service, for
//! nothing itself: making those products is the data tool's job.

use crate::fetch::{sha256_file, Fetcher};
use crate::wildfire::Building;
use serde_json::{json, Value};
use std::io::Read;
use std::path::PathBuf;

pub const PUBLIC_STORE: &str = "https://data.earthlight.cloud";

pub fn stores() -> Vec<String> {
    if let Ok(s) = std::env::var("ESIM_DATA_STORES") {
        let v: Vec<String> = s.split(',').map(|x| x.trim().trim_end_matches('/').to_string()).filter(|x| !x.is_empty()).collect();
        if !v.is_empty() { return v; }
    }
    let mut v = Vec::new();
    let near = std::env::var("ESIM_WORKSPACE").ok().map(PathBuf::from).unwrap_or_else(|| crate::paths::root().join(".."));
    if near.join("dataservice/store/catalog.json").exists() { v.push(near.join("dataservice/store").to_string_lossy().to_string()); }
    v.push(PUBLIC_STORE.to_string());
    v
}

fn is_web(store: &str) -> bool { store.starts_with("http://") || store.starts_with("https://") }

/// A local path for one file of a store, or None when that store does not hold it.
fn file_in(fetcher: &mut Fetcher, store: &str, rel: &str) -> Result<Option<PathBuf>, String> {
    if is_web(store) {
        match fetcher.fetch(&format!("esimv1-data/{rel}"), &format!("{store}/{rel}")) {
            Ok(p) => Ok(Some(p)),
            Err(e) if e.contains("404") => Ok(None),
            Err(e) => Err(e),
        }
    } else {
        let p = PathBuf::from(store).join(rel);
        Ok(if p.is_file() { Some(p) } else { None })
    }
}

/// The local path of a product's data file, fetched if need be and checked against the checksum
/// its item states. `product` is `<plane>/<source>/<place>/<date>` (or `<plane>/<source>/<date>`).
pub fn data_file(fetcher: &mut Fetcher, product: &str) -> Result<PathBuf, String> {
    let product = product.trim_matches('/');
    let parts: Vec<&str> = product.split('/').collect();
    if parts.len() < 3 || product.contains("..") { return Err(format!("{product}: a product is named <plane>/<source>/<place>/<date>")); }
    let item_rel = format!("{product}/{}.json", parts[2..].join("-"));
    let mut tried = Vec::new();
    for store in stores() {
        let Some(item_path) = file_in(fetcher, &store, &item_rel)? else { tried.push(format!("{store}: not listed")); continue };
        let item: Value = serde_json::from_slice(&std::fs::read(&item_path).map_err(|e| e.to_string())?).map_err(|e| format!("{item_rel}: {e}"))?;
        let a = &item["assets"]["data"];
        let (Some(href), Some(sum)) = (a["href"].as_str(), a["file:checksum"].as_str()) else { return Err(format!("{item_rel}: no data file with a checksum")); };
        let rel = format!("{product}/{}", href.trim_start_matches("./"));
        let Some(path) = file_in(fetcher, &store, &rel)? else { tried.push(format!("{store}: listed, and its bytes are not kept there")); continue };
        let (sha, _) = sha256_file(&path)?;
        if sum.trim_start_matches("1220") != sha { return Err(format!("{rel} from {store}: the bytes do not match the checksum its item states")); }
        return Ok(path);
    }
    Err(format!("no store holds {product} ({}). A product listed without its bytes can be rebuilt with esim-data from the request on its item; ESIM_DATA_STORES names the stores to read.", tried.join("; ")))
}

fn features(fetcher: &mut Fetcher, product: &str) -> Result<Vec<Value>, String> {
    let path = data_file(fetcher, product)?;
    let raw = std::fs::read(&path).map_err(|e| e.to_string())?;
    let mut text = Vec::new();
    if raw.starts_with(&[0x1f, 0x8b]) { flate2::read::GzDecoder::new(&raw[..]).read_to_end(&mut text).map_err(|e| format!("{product}: {e}"))?; } else { text = raw; }
    let mut v: Value = serde_json::from_slice(&text).map_err(|e| format!("{product}: {e}"))?;
    match v["features"].take() { Value::Array(a) => Ok(a), _ => Err(format!("{product}: not a feature collection")) }
}
fn line(g: &Value) -> Vec<(f64, f64)> { g.as_array().into_iter().flatten().filter_map(|p| Some((p.get(0)?.as_f64()?, p.get(1)?.as_f64()?))).collect() }
fn r5(x: f64) -> f64 { (x * 1e5).round() / 1e5 }
fn thin(pts: Vec<(f64, f64)>, max: usize) -> Vec<(f64, f64)> {
    if pts.len() <= max { return pts; }
    let step = pts.len() as f64 / max as f64;
    let mut out: Vec<(f64, f64)> = (0..max).map(|i| pts[(i as f64 * step) as usize]).collect();
    if let Some(&last) = pts.last() { out.push(last); }
    out
}

/// The buildings of a product (schema buildings-1). The engine's footprints carry their height in
/// feet, as the county's did, so metres are converted; a building with no height has NaN. When
/// the source kept an inspected damage (`DINS_MaximumDamage`), it comes along.
pub fn buildings(fetcher: &mut Fetcher, product: &str) -> Result<Vec<Building>, String> {
    let mut out = Vec::new();
    for f in features(fetcher, product)? {
        let mut ring = line(&f["geometry"]["coordinates"][0]);
        if ring.len() > 3 && ring.first() == ring.last() { ring.pop(); }
        if ring.len() < 3 { continue; }
        let p = &f["properties"];
        out.push(Building { ring, height: p["height_m"].as_f64().map(|m| m / 0.3048).unwrap_or(f64::NAN), damage: p["DINS_MaximumDamage"].as_str().unwrap_or("").to_string() });
    }
    Ok(out)
}

const MAJOR: [&str; 5] = ["motorway", "trunk", "primary", "secondary", "tertiary"];

/// What a map of a window needs, from a roads product and a names product (either may be empty):
/// the major roads by class, the named places as points and the city limits as lines. The reduced
/// table is kept in the cache, since a city's roads are a large file to read for five classes.
pub fn context(fetcher: &mut Fetcher, roads: &str, names: &str) -> Result<Value, String> {
    if roads.is_empty() && names.is_empty() { return Err("no roads or names product named".into()); }
    let kept = fetcher.kept_path(&format!("derived_context_{}.json", format!("{roads}+{names}").replace(['/', ' '], "_")));
    if let Ok(t) = std::fs::read_to_string(&kept) { if let Ok(v) = serde_json::from_str(&t) { return Ok(v); } }
    let (mut rs, mut places, mut bounds) = (Vec::new(), Vec::new(), Vec::new());
    let mut complete = true;
    if !roads.is_empty() {
        for f in features(fetcher, roads)? {
            let class = f["properties"]["class"].as_str().unwrap_or("");
            if !MAJOR.contains(&class) { continue; }
            let pts = thin(line(&f["geometry"]["coordinates"]), 120);
            if pts.len() > 1 { rs.push(json!({ "class": class, "name": f["properties"]["name"].as_str().unwrap_or(""), "pts": pts })); }
        }
    }
    if !names.is_empty() {
        // names only label the map: a store that cannot give them costs the labels, not the run
        let named = match features(fetcher, names) { Ok(f) => f, Err(e) => { eprintln!("names: {e} (the map will have no place names)"); Vec::new() } };
        complete = !named.is_empty();
        for f in named {
            let (p, g) = (&f["properties"], &f["geometry"]);
            let name = p["name"].as_str().unwrap_or("");
            if g["type"] == "Point" {
                if let (Some(lon), Some(lat)) = (g["coordinates"][0].as_f64(), g["coordinates"][1].as_f64()) { places.push(json!({ "name": name, "kind": p["kind"].as_str().unwrap_or(""), "lon": r5(lon), "lat": r5(lat) })); }
            } else {
                let pts = thin(line(&g["coordinates"]), 200);
                if pts.len() > 1 { bounds.push(json!({ "name": name, "pts": pts })); }
            }
        }
    }
    let v = json!({ "roads": rs, "places": places, "boundaries": bounds, "source": "OpenStreetMap contributors, ODbL, through esimv1-data", "products": [roads, names] });
    if !names.is_empty() && !complete { return Ok(v); }   // not kept, so the names are asked for again next time
    if let Some(dir) = kept.parent() { std::fs::create_dir_all(dir).ok(); }
    let tmp = kept.with_extension(format!("{}.part", std::process::id()));
    if std::fs::write(&tmp, serde_json::to_vec(&v).map_err(|e| e.to_string())?).is_ok() { std::fs::rename(&tmp, &kept).ok(); }
    Ok(v)
}

/// The reduced map table of two products if a run has made it, never fetching (for a describe).
pub fn cached_context(fetcher: &Fetcher, roads: &str, names: &str) -> Option<Value> {
    if roads.is_empty() && names.is_empty() { return None; }
    serde_json::from_str(&std::fs::read_to_string(fetcher.kept_path(&format!("derived_context_{}.json", format!("{roads}+{names}").replace(['/', ' '], "_")))).ok()?).ok()
}
/// The bridge lines of a roads product if a run has made them, never fetching (for a describe).
pub fn cached_bridges(fetcher: &Fetcher, roads: &str) -> Option<Value> {
    if roads.is_empty() { return None; }
    serde_json::from_str(&std::fs::read_to_string(fetcher.kept_path(&format!("derived_bridges_{}.json", roads.replace(['/', ' '], "_")))).ok()?).ok()
}

const BRIDGE_ROADS: [&str; 9] = ["motorway", "trunk", "primary", "secondary", "tertiary", "busway", "motorway_link", "trunk_link", "primary_link"];

/// The bridges of a roads product that carry a road of some weight: `{ ways: [{ name, carries, pts }] }`.
/// Railway bridges are not in a roads product and so are not here.
pub fn bridges(fetcher: &mut Fetcher, roads: &str) -> Result<Value, String> {
    if roads.is_empty() { return Err("no roads product named".into()); }
    let kept = fetcher.kept_path(&format!("derived_bridges_{}.json", roads.replace(['/', ' '], "_")));
    if let Ok(t) = std::fs::read_to_string(&kept) { if let Ok(v) = serde_json::from_str(&t) { return Ok(v); } }
    let mut ways = Vec::new();
    for f in features(fetcher, roads)? {
        let p = &f["properties"];
        let class = p["class"].as_str().unwrap_or("");
        if p["bridge"].as_bool() != Some(true) || !BRIDGE_ROADS.contains(&class) { continue; }
        let pts = line(&f["geometry"]["coordinates"]);
        if pts.len() > 1 { ways.push(json!({ "name": p["name"].as_str().unwrap_or(""), "carries": class, "pts": pts })); }
    }
    let v = json!({ "ways": ways, "source": "OpenStreetMap contributors, ODbL, through esimv1-data", "products": [roads] });
    let tmp = kept.with_extension(format!("{}.part", std::process::id()));
    if std::fs::write(&tmp, serde_json::to_vec(&v).map_err(|e| e.to_string())?).is_ok() { std::fs::rename(&tmp, &kept).ok(); }
    Ok(v)
}

/// The coastline and breakwaters of a shore product, each as (kind, name, way id, points as (lat, lon)).
pub fn shore(fetcher: &mut Fetcher, product: &str) -> Result<Vec<(String, String, i64, Vec<(f64, f64)>)>, String> {
    let mut out = Vec::new();
    for f in features(fetcher, product)? {
        let kind = f["properties"]["kind"].as_str().unwrap_or("");
        if kind != "coastline" && kind != "breakwater" { continue; }
        let id = f["id"].as_str().and_then(|s| s.rsplit('/').next()).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
        let pts: Vec<(f64, f64)> = line(&f["geometry"]["coordinates"]).into_iter().map(|(lon, lat)| (lat, lon)).collect();
        if pts.len() > 1 { out.push((kind.to_string(), f["properties"]["name"].as_str().unwrap_or("").to_string(), id, pts)); }
    }
    Ok(out)
}
