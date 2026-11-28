//! The coast: the hourly wave climate at a point from the ERA5-Ocean reanalysis through
//! Open-Meteo's marine API, and the shoreline and breakwaters of a window from OpenStreetMap.

use crate::fetch::Fetcher;
use crate::obs::{fetch_json_text, parse_iso_hour};

/// Hourly waves at one point: significant height, the direction they come from, mean and peak period.
#[derive(Clone, Debug, Default)]
pub struct WaveSeries {
    pub times: Vec<i64>,
    pub hs_m: Vec<f32>,
    pub dir_deg: Vec<f32>,
    pub tm_s: Vec<f32>,
    pub tp_s: Vec<f32>,
    pub cell_lat: f64,
    pub cell_lon: f64,
}

impl WaveSeries {
    pub fn index_at(&self, t: i64) -> Option<usize> {
        if self.times.is_empty() || t < self.times[0] { return None; }
        Some(match self.times.binary_search(&t) { Ok(i) => i, Err(i) => i.saturating_sub(1) })
    }
}

/// The ERA5-Ocean hours of whole years, fetched one year per request and cached.
pub fn waves_hourly(fetcher: &mut Fetcher, lat: f64, lon: f64, year_from: i32, year_to: i32) -> Result<WaveSeries, String> {
    let lat = (lat * 1e3).round() / 1e3;
    let lon = (lon * 1e3).round() / 1e3;
    let mut s = WaveSeries::default();
    for y in year_from..=year_to {
        let url = format!("https://marine-api.open-meteo.com/v1/marine?latitude={lat}&longitude={lon}&start_date={y}-01-01&end_date={y}-12-31&hourly=wave_height,wave_direction,wave_period,wave_peak_period&models=era5_ocean&timezone=UTC");
        let key = format!("open-meteo-era5-ocean-waves/{lat}_{lon}_{y}.json");
        let text = fetch_json_text(fetcher, &key, &url)?;
        let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("open-meteo marine json: {e}"))?;
        if let Some(r) = v.get("reason") { return Err(format!("open-meteo marine {y}: {r}")); }
        s.cell_lat = v["latitude"].as_f64().unwrap_or(lat); s.cell_lon = v["longitude"].as_f64().unwrap_or(lon);
        let h = &v["hourly"];
        let times: Vec<i64> = h["time"].as_array().ok_or_else(|| format!("open-meteo marine {y}: no hourly.time"))?.iter().map(|t| parse_iso_hour(t.as_str().unwrap_or(""))).collect::<Result<_, _>>()?;
        let take = |name: &str| -> Vec<f32> { h[name].as_array().cloned().unwrap_or_default().iter().map(|x| x.as_f64().map(|f| f as f32).unwrap_or(f32::NAN)).collect() };
        let (hs, dir, tm, tp) = (take("wave_height"), take("wave_direction"), take("wave_period"), take("wave_peak_period"));
        if hs.len() != times.len() || dir.len() != times.len() { return Err(format!("open-meteo marine {y}: ragged arrays")); }
        s.times.extend(times); s.hs_m.extend(hs); s.dir_deg.extend(dir); s.tm_s.extend(tm); s.tp_s.extend(tp);
    }
    Ok(s)
}

/// A mapped line: OpenStreetMap coastline ways and breakwater outlines inside a window, as
/// (lat, lon) vertex lists with the feature's kind and id.
#[derive(Clone, Debug)]
pub struct ShoreWay { pub id: i64, pub kind: String, pub name: String, pub pts: Vec<(f64, f64)> }


/// The coastline and breakwaters of a shore product from the warehouse (`shore/osm/<place>/<date>`).
pub fn osm_shore(fetcher: &mut Fetcher, product: &str) -> Result<Vec<ShoreWay>, String> {
    Ok(crate::warehouse::shore(fetcher, product)?.into_iter().map(|(kind, name, id, pts)| ShoreWay { id, kind, name, pts }).collect())
}
