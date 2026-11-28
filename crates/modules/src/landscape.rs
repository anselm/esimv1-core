//! The parts of a wildland fire that are not the fire, each a module of its own so that others can
//! take part (Anselm, 2026-11-26: "we are doing 'models of whole systems' - that means that we must
//! separate modules out where possible"). `ground.terrain` holds the relief; `weather.surface` lays
//! each hour's wind, temperature and humidity on the ground's cells; `fuel.moisture` turns that air
//! into the dryness of dead fuel; `fauna.flee` is animals that leave ground the fire reaches. They
//! meet only in shared fields on one grid: the terrain owns the grid, and every other module takes
//! it from the terrain's field at its first tick, so none of them is told the window twice.
//! `fire.cells` with `inputs = "world"` reads these fields and loads no terrain or weather itself.

use esim_kernel::manifest::{param_duration, param_f64, param_str};
use esim_kernel::describe::Describe;
use esim_kernel::{Ctx, Duration, Event, LatLonGrid, Module, Time};
use esim_registry::wildfire::{hrrr_hour, wind_hourly, HrrrHour, WindSeries};
use esim_registry::{dem_window, Catalog, Fetcher};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;

fn root() -> PathBuf { std::env::var("ESIM_ROOT").ok().map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")) }

// ---------------------------------------------------------------- ground.terrain

#[derive(Serialize, Deserialize)]
pub struct Terrain {
    name: String,
    lat_s: f64, lat_n: f64, lon_w: f64, lon_e: f64, res_arcsec: f64,
    #[serde(skip)] grid: Option<LatLonGrid>,
    #[serde(skip)] elev: Vec<f32>, #[serde(skip)] slope: Vec<f32>, #[serde(skip)] aspect: Vec<f32>,
}

impl Terrain {
    pub fn new(name: &str, p: &toml::Value) -> Result<Terrain, String> {
        let need = |k: &str| p.get(k).and_then(|v| v.as_float().or(v.as_integer().map(|i| i as f64))).ok_or(format!("ground.terrain {name}: no {k}; give it a place"));
        let mut t = Terrain { name: name.into(), lat_s: need("lat_s")?, lat_n: need("lat_n")?, lon_w: need("lon_w")?, lon_e: need("lon_e")?, res_arcsec: param_f64(p, "res_arcsec", 1.0), grid: None, elev: vec![], slope: vec![], aspect: vec![] };
        t.load()?;
        Ok(t)
    }
    fn load(&mut self) -> Result<(), String> {
        let root = root();
        let cat = Catalog::load(&root.join("data/catalog.toml"))?;
        let dem = cat.asset("copernicus-dem-glo30").ok_or("catalog has no copernicus-dem-glo30")?;
        let mut fe = Fetcher::default_for(&root)?;
        let w = dem_window(&mut fe, dem, self.lat_s, self.lat_n, self.lon_w, self.lon_e, self.res_arcsec)?;
        let (slope, aspect) = w.slope_aspect(&fe);
        self.grid = Some(LatLonGrid { nx: w.nx, ny: w.ny, dlon: w.dlon, dlat: w.dlat, lon_w: w.lon_w, lat_n: w.lat_n });
        self.elev = w.elev.clone(); self.slope = slope; self.aspect = aspect;
        Ok(())
    }
    pub fn describe(name: &str, _p: &toml::Value) -> Describe {
        Describe::new("ground.terrain", name, "The relief of the window: elevation from the Copernicus 30 m model resampled to the window's cells, and the slope and aspect of each cell. It owns the grid: every other module of the scenario takes its cells from these fields. Nothing here changes during a run.")
            .dt("once, at the start")
            .param("lat_s", 0.0, "deg", "from the place").param("lat_n", 0.0, "deg", "").param("lon_w", 0.0, "deg", "").param("lon_e", 0.0, "deg", "").param("res_arcsec", 1.0, "arcsec", "")
            .own("ground.elev_m", "m").own("ground.slope_rad", "rad").own("ground.aspect_rad", "rad")
            .frame("terrain_elev_m", "m")
            .asset("copernicus-dem-glo30", "elevation; slope and aspect by Horn's method")
            .note("The elevation model is a surface model: it includes tree tops and roofs.")
    }
}

impl Module for Terrain {
    fn name(&self) -> &str { &self.name }
    fn init(&mut self, ctx: &mut Ctx) {
        let g = self.grid.clone().unwrap();
        ctx.world_write("ground.elev_m", "m", &g, self.elev.clone());
        ctx.world_write("ground.slope_rad", "rad", &g, self.slope.clone());
        ctx.world_write("ground.aspect_rad", "rad", &g, self.aspect.clone());
        ctx.field("terrain_elev_m", "m", &g, self.elev.clone());
    }
    fn handle(&mut self, _ev: &Event, _ctx: &mut Ctx) {}
    fn snapshot(&self) -> serde_json::Value { serde_json::to_value(self).unwrap() }
    fn restore(&mut self, v: serde_json::Value) { let mut t: Terrain = serde_json::from_value(v).unwrap(); t.load().expect("terrain reload"); *self = t; }
}

// ---------------------------------------------------------------- weather.surface

#[derive(Serialize, Deserialize)]
pub struct Surface {
    name: String,
    lat_s: f64, lat_n: f64, lon_w: f64, lon_e: f64,
    source: String, wind_use: String, start: String, end: String, at: String, dt: Duration, #[serde(default)] res_arcsec: f64,
    #[serde(skip)] hrrr: Vec<Option<HrrrHour>>, #[serde(skip)] hrrr_t0: i64, #[serde(skip)] shift: i64, #[serde(skip)] era5: Option<WindSeries>,
}

impl Surface {
    pub fn new(name: &str, p: &toml::Value) -> Result<Surface, String> {
        let need = |k: &str| p.get(k).and_then(|v| v.as_float().or(v.as_integer().map(|i| i as f64))).ok_or(format!("weather.surface {name}: no {k}; give it a place"));
        let s = |k: &str| p.get(k).and_then(|v| v.as_str()).map(String::from).ok_or(format!("weather.surface {name}: no {k}"));
        let mut w = Surface { name: name.into(), lat_s: need("lat_s")?, lat_n: need("lat_n")?, lon_w: need("lon_w")?, lon_e: need("lon_e")?,
            source: param_str(p, "source", "hrrr").into(), wind_use: param_str(p, "wind_use", "mean").into(), start: s("start")?, end: s("end")?, at: param_str(p, "at", "").into(),
            dt: param_duration(p, "dt", "1h")?, res_arcsec: param_f64(p, "cell_arcsec", 0.0), hrrr: vec![], hrrr_t0: 0, shift: 0, era5: None };
        w.load()?;
        Ok(w)
    }
    fn load(&mut self) -> Result<(), String> {
        let mut fe = Fetcher::default_for(&root())?;
        let t0 = Time::parse(&self.start)?.0; let t1 = Time::parse(&self.end)?.0;
        let t_at = if self.at.trim().is_empty() { t0 } else { Time::parse(&self.at)?.0 };
        self.hrrr_t0 = t_at; self.shift = t0 - t_at;
        if self.source == "hrrr" {
            let hours = ((t1 - t0) / 3600).max(1) as usize + 1;
            self.hrrr = Vec::with_capacity(hours);
            for h in 0..hours {
                let (date, hh) = crate::firecells::civil(t0 + h as i64 * 3600);
                match hrrr_hour(&mut fe, &date, hh, self.lat_s, self.lat_n, self.lon_w, self.lon_e) { Ok(x) => self.hrrr.push(Some(x)), Err(e) => { eprintln!("hrrr {date} {hh}: {e}"); self.hrrr.push(None); } }
            }
        } else {
            let (c_lat, c_lon) = ((self.lat_s + self.lat_n) / 2.0, (self.lon_w + self.lon_e) / 2.0);
            self.era5 = Some(wind_hourly(&mut fe, c_lat, c_lon, &self.start[..10], &self.end[..10])?);
        }
        Ok(())
    }
    pub fn describe(name: &str, p: &toml::Value) -> Describe {
        Describe::new("weather.surface", name, "The air at the ground, hour by hour, on the terrain's cells: wind speed and the direction it blows toward, air temperature and relative humidity. Read from a record, not modelled: NOAA's HRRR 3 km analysis sampled at each cell, or one ERA5 point for the whole window. A run of a day with no record may borrow another day's hours.")
            .priority(3).dt(param_str(p, "dt", "1h"))
            .param("source", "hrrr", "", "hrrr: a field over the window each hour; era5: one 25 km point, the same on every cell").param("wind_use", "mean", "", "mean or gust: which speed is written as the wind")
            .param("start", "", "", "the first hour of the record fetched").param("end", "", "", "").param("at", "", "", "the simulated instant `start` stands for, when the run borrows another day's weather; empty means `start` itself")
            .param("dt", "1h", "", "")
            .param("cell_arcsec", 0.0, "arcsec", "the weather's own cell size; 0 lays it on the terrain's cells. The 3 km record gains nothing from 30 m cells, and a reader lays a coarser field on its own cells").read("ground.elev_m", "m", param_f64(p, "cell_arcsec", 0.0) <= 0.0, "only for its grid: the cells the air is laid on, unless the weather has its own")
            .own("air.wind_m_s", "m/s").own("air.wind_toward_rad", "rad").own("air.t2m_c", "degC").own("air.rh_pct", "%")
            .report("air.wind_mean_m_s", "m/s").report("air.rh_mean_pct", "%")
            .asset("noaa-hrrr", "the 3 km wind, gust, temperature and humidity each hour").asset("open-meteo-era5", "the point series, when source is era5")
            .view("chart", "Mean wind over the window", "air.wind_mean_m_s", "").view("chart", "Mean relative humidity", "air.rh_mean_pct", "")
            .note("The wind is not adjusted for the terrain between the record's points, and nothing in the run changes it: a fire does not make its own wind here.")
    }
}

impl Module for Surface {
    fn name(&self) -> &str { &self.name }
    fn init(&mut self, ctx: &mut Ctx) { ctx.schedule_with_priority(self.dt, 3, "tick", json!(null)); }
    fn handle(&mut self, ev: &Event, ctx: &mut Ctx) {
        if ev.topic != "tick" { return; }
        let now = ctx.now();
        // its own spacing if one is given (readers lay it on their cells); otherwise the terrain's cells
        let g = if self.res_arcsec > 0.0 { grid_of(self.lat_s, self.lat_n, self.lon_w, self.lon_e, self.res_arcsec) }
            else { ctx.world_read("ground.elev_m").unwrap_or_else(|| panic!("{}: no ground.elev_m; a ground.terrain module must be in the manifest, or give the weather cell_arcsec", self.name)).grid.clone() };
        let n = g.nx * g.ny;
        let use_gust = self.wind_use == "gust";
        let mut out: Option<[Vec<f32>; 4]> = None;
        if self.source == "hrrr" {
            let h = ((now.0 - self.hrrr_t0) / 3600).clamp(0, self.hrrr.len() as i64 - 1) as usize;
            if let Some(hh) = &self.hrrr[h] {
                let (mut spd_v, mut dir_v, mut t_v, mut rh_v) = (vec![0f32; n], vec![0f32; n], vec![0f32; n], vec![0f32; n]);
                for r in 0..g.ny { for c in 0..g.nx {
                    let (lat, lon) = (g.lat(r), g.lon(c));
                    let (u, v) = (hh.at(&hh.u, lat, lon), hh.at(&hh.v, lat, lon));
                    let mut spd = (u * u + v * v).sqrt();
                    if use_gust { let gu = hh.at(&hh.gust, lat, lon); if gu.is_finite() { spd = gu; } }
                    let toward = u.atan2(v);
                    let k = r * g.nx + c;
                    spd_v[k] = if spd.is_finite() { spd } else { 0.0 }; dir_v[k] = if toward.is_finite() { toward } else { 0.0 };
                    t_v[k] = hh.at(&hh.t2m_c, lat, lon); rh_v[k] = hh.at(&hh.rh_pct, lat, lon);
                } }
                out = Some([spd_v, dir_v, t_v, rh_v]);
            }
        } else if let Some(w) = &self.era5 {
            if let Some(i) = w.index_at(now.0 + self.shift) {
                let spd = if use_gust { w.gust_m_s[i] } else { w.wind_m_s[i] };
                out = Some([vec![spd; n], vec![(w.dir_deg[i] + 180.0).to_radians(); n], vec![w.t2m_c[i]; n], vec![w.rh_pct[i]; n]]);
            }
        }
        // an hour the record lacks leaves the last hour's air in place
        if let Some([spd, dir, t, rh]) = out {
            let mean = |v: &[f32]| { let (s, c) = v.iter().filter(|x| x.is_finite()).fold((0f64, 0u32), |(s, c), x| (s + *x as f64, c + 1)); if c > 0 { s / c as f64 } else { f64::NAN } };
            ctx.report("air.wind_mean_m_s", "m/s", mean(&spd)); ctx.report("air.rh_mean_pct", "%", mean(&rh));
            ctx.world_write("air.wind_m_s", "m/s", &g, spd); ctx.world_write("air.wind_toward_rad", "rad", &g, dir);
            ctx.world_write("air.t2m_c", "degC", &g, t); ctx.world_write("air.rh_pct", "%", &g, rh);
        }
        ctx.schedule_with_priority(self.dt, 3, "tick", json!(null));
    }
    fn snapshot(&self) -> serde_json::Value { serde_json::to_value(self).unwrap() }
    fn restore(&mut self, v: serde_json::Value) { let mut w: Surface = serde_json::from_value(v).unwrap(); w.load().expect("weather reload"); *self = w; }
}

// ---------------------------------------------------------------- fuel.moisture

/// Dead fine fuel moisture as a fraction, from air temperature and relative humidity: the
/// equilibrium moisture content of Simard 1968 (recalled), clamped to 1 to 35 percent.
pub fn dead_moisture(t_c: f32, rh: f32) -> f32 {
    let t = t_c * 9.0 / 5.0 + 32.0; let h = rh.clamp(0.0, 100.0);
    let m = if h < 10.0 { 0.03229 + 0.281073 * h - 0.000578 * h * t } else if h < 50.0 { 2.22749 + 0.160107 * h - 0.01478 * t } else { 21.0606 + 0.005565 * h * h - 0.00035 * h * t - 0.483199 * h };
    (m / 100.0).clamp(0.01, 0.35)
}

#[derive(Serialize, Deserialize)]
pub struct Moisture { name: String, dt: Duration }

impl Moisture {
    pub fn new(name: &str, p: &toml::Value) -> Result<Moisture, String> { Ok(Moisture { name: name.into(), dt: param_duration(p, "dt", "1h")? }) }
    pub fn describe(name: &str, p: &toml::Value) -> Describe {
        Describe::new("fuel.moisture", name, "How dry the dead fine fuel is on every cell, each hour, from the air's temperature and humidity: the equilibrium moisture content, as a fraction of dry weight. Dead grass and litter follow the air within the hour; this module has no memory of rain, no live fuel and no shading by slope or canopy.")
            .priority(4).dt(param_str(p, "dt", "1h")).param("dt", "1h", "", "")
            .read("air.t2m_c", "degC", true, "").read("air.rh_pct", "%", true, "")
            .own("fuel.dead_moisture", "1")
            .report("fuel.dead_moisture_mean_pct", "%")
            .view("chart", "Mean dead fuel moisture", "fuel.dead_moisture_mean_pct", "")
            .note("The equilibrium formula is recalled from Simard 1968 and has not been checked against the paper.")
    }
}

impl Module for Moisture {
    fn name(&self) -> &str { &self.name }
    fn init(&mut self, ctx: &mut Ctx) { ctx.schedule_with_priority(self.dt, 4, "tick", json!(null)); }
    fn handle(&mut self, ev: &Event, ctx: &mut Ctx) {
        if ev.topic != "tick" { return; }
        if let (Some(t), Some(rh)) = (ctx.world_read("air.t2m_c"), ctx.world_read("air.rh_pct")) {
            let g = t.grid.clone();
            let m: Vec<f32> = t.data.iter().zip(&rh.data).map(|(&t, &h)| dead_moisture(t, h)).collect();
            let (s, c) = m.iter().filter(|x| x.is_finite()).fold((0f64, 0u32), |(s, c), x| (s + *x as f64, c + 1));
            ctx.report("fuel.dead_moisture_mean_pct", "%", if c > 0 { 100.0 * s / c as f64 } else { f64::NAN });
            ctx.world_write("fuel.dead_moisture", "1", &g, m);
        }
        ctx.schedule_with_priority(self.dt, 4, "tick", json!(null));
    }
    fn snapshot(&self) -> serde_json::Value { serde_json::to_value(self).unwrap() }
    fn restore(&mut self, v: serde_json::Value) { *self = serde_json::from_value(v).unwrap(); }
}

// ---------------------------------------------------------------- fauna.flee

const WILD: &str = "wildlife";

/// Animals that run from a fire. A placeholder population, declared as one: the same number per
/// square kilometre on every cell of tree, shrub, grass or wetland at the start. They sense the
/// front at a distance and move away from it at a stated speed, in steps of a few minutes replayed
/// from the fire's record of when each cell caught; those the front overtakes die at a stated share
/// or reach the nearest unburned ground. It knows no species. It is here so that the fire has a
/// neighbour that reacts to it, and to be replaced by a module that knows animals.
#[derive(Serialize, Deserialize)]
pub struct Flee {
    name: String,
    dt: Duration, step: Duration, frame_every: Duration, frame_res_arcsec: f64,
    density_km2: f64, mortality: f64, reach_m: f64, sense_m: f64, speed_m_h: f64, #[serde(default)] crowd: f64,
    animals: Vec<f64>, home: Vec<f64>, seen: Vec<bool>, started: bool,
    killed: f64, caught: f64, #[serde(default)] left: f64, a_last: f32, last_frame: i64,
}

fn nb8(g: &LatLonGrid, k: usize) -> impl Iterator<Item = usize> + '_ {
    let (c, r) = ((k % g.nx) as isize, (k / g.nx) as isize);
    [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)].into_iter().filter_map(move |(dc, dr)| {
        let (nc, nr) = (c + dc, r + dr);
        (nc >= 0 && nr >= 0 && nc < g.nx as isize && nr < g.ny as isize).then(|| nr as usize * g.nx + nc as usize)
    })
}

impl Flee {
    pub fn new(name: &str, p: &toml::Value) -> Result<Flee, String> {
        Ok(Flee { name: name.into(), dt: param_duration(p, "dt", "1h")?, step: param_duration(p, "step", "5m")?, frame_every: param_duration(p, "frame_every", "6h")?, frame_res_arcsec: param_f64(p, "frame_res_arcsec", 3.0),
            density_km2: param_f64(p, "density_km2", 20.0), mortality: param_f64(p, "mortality", 0.3), reach_m: param_f64(p, "reach_m", 3000.0),
            sense_m: param_f64(p, "sense_m", 600.0), speed_m_h: param_f64(p, "speed_m_h", 3000.0), crowd: param_f64(p, "crowd", 4.0),
            animals: vec![], home: vec![], seen: vec![], started: false, killed: 0.0, caught: 0.0, left: 0.0, a_last: f32::NAN, last_frame: i64::MIN / 4 })
    }
    pub fn describe(name: &str, p: &toml::Value) -> Describe {
        Describe::new("fauna.flee", name, "Animals that run from a fire. A placeholder population: the same number per square kilometre on every cell of tree, shrub, grass or wetland at the start. They notice the front at a stated distance and move away from it at a stated speed, a few minutes at a time, following the fire's own record of when each cell caught. Those the front overtakes die at a stated share or reach the nearest unburned ground; those driven to the edge of the modelled window leave it. Every animal is accounted for: alive, killed by the fire, or gone from the window.")
            .priority(7).dt(param_str(p, "dt", "1h"))
            .param("dt", "1h", "", "how often it looks at the fire").param("step", "5m", "", "the steps the hour since the last look is replayed in, when the fire says when each cell caught").param("frame_every", "6h", "", "").param("frame_res_arcsec", 3.0, "arcsec", "")
            .param("density_km2", 20.0, "1/km2", "a placeholder, not a survey: animals per square kilometre of habitat at the start")
            .param("sense_m", 600.0, "m", "guess: how far from newly burning ground an animal notices the fire").param("speed_m_h", 3000.0, "m/h", "guess: how fast it moves away while it notices")
            .param("crowd", 4.0, "", "guess: a cell holding more than this many times the starting number sheds the excess to the cells around it, a few cells each look")
            .param("mortality", 0.3, "", "guess: the share that die of those the front overtakes").param("reach_m", 3000.0, "m", "guess: how far an overtaken animal can get across burned ground to unburned; farther and it dies")
            .read("fire.burned_frac", "1", true, "the fire's burned cells, read after the fire has moved").read("fire.arrival_s", "s", false, "when each cell caught, to replay the hour in steps; without it the hour is one step")
            .read("ground.cover_code", "worldcover code", true, "habitat and water, from the land cover module")
            .ledger(WILD, "individuals", 1e-3)
            .frame("animals_density_km2", "1/km2")
            .report("fauna.alive", "individuals").report("fauna.killed_by_fire", "individuals").report("fauna.caught", "individuals").report("fauna.displaced", "individuals").report("fauna.in_flight", "individuals").report("fauna.left_window", "individuals")
            .view("map", "Animals per square kilometre", "animals_density_km2", "emptied ground behind the front, crowded ground ahead of it")
            .view("chart", "Animals displaced", "fauna.displaced", "those now on ground beyond what it held at the start")
            .view("chart", "Animals overtaken by the front", "fauna.caught", "running total").view("chart", "Animals killed by the fire", "fauna.killed_by_fire", "")
            .note("No species, no habitat preference, no memory: an animal moves only while newly burning ground is within its sensing distance, straight away from it, across anything but water, towns and roads included. Animals outside the window never enter it. The starting number, the distance, the speed and the share that dies are placeholders. The fire does not react to the animals.")
    }
    fn frame(&self, g: &LatLonGrid, v: &[f64]) -> (LatLonGrid, Vec<f32>) {
        let k = (self.frame_res_arcsec / (g.dlon * 3600.0)).round().max(1.0) as usize;
        let fg = LatLonGrid { nx: g.nx.div_ceil(k), ny: g.ny.div_ceil(k), dlon: g.dlon * k as f64, dlat: g.dlat * k as f64, lon_w: g.lon_w, lat_n: g.lat_n };
        let mut sum = vec![0f64; fg.nx * fg.ny]; let mut area = vec![0f64; fg.nx * fg.ny];
        for r in 0..g.ny { let a = g.row_area(r); for c in 0..g.nx { let j = (r / k) * fg.nx + c / k; sum[j] += v[r * g.nx + c]; area[j] += a; } }
        (fg, sum.iter().zip(&area).map(|(s, a)| if *a > 0.0 { (s / (a / 1e6)) as f32 } else { f32::NAN }).collect())
    }
    /// Crowding: no cell keeps more than `crowd` times the starting number per cell; the excess
    /// spreads to the open unburned cells around it, a few cells' distance per look.
    fn disperse(&mut self, g: &LatLonGrid, open: &[bool], cap: f64) {
        let mut work: Vec<usize> = (0..self.animals.len()).filter(|&k| self.animals[k] > cap).collect();
        for _ in 0..12 {
            if work.is_empty() { break; }
            let mut next = Vec::new();
            for &k in &work {
                let excess = self.animals[k] - cap; if excess <= 0.0 { continue; }
                let to: Vec<usize> = nb8(g, k).filter(|&j| !self.seen[j] && open[j] && self.animals[j] < self.animals[k]).collect();
                if to.is_empty() { continue; }
                self.animals[k] = cap;
                for &j in &to { self.animals[j] += excess / to.len() as f64; if self.animals[j] > cap { next.push(j); } }
            }
            next.sort_unstable(); next.dedup(); work = next;
        }
    }
    /// One step: the cells in `batch` have just caught. The animals on them are overtaken; then the
    /// animals within sensing distance of them move away, and those that reach the window's edge
    /// leave it. Returns (killed, overtaken, moved away, left the window).
    fn step(&mut self, g: &LatLonGrid, open: &[bool], batch: &[usize], cell_m: f64, step_h: f64) -> (f64, f64, f64, f64) {
        let n = g.nx * g.ny;
        for &k in batch { self.seen[k] = true; }
        // overtaken: the nearest unburned open cell, searched outward through burned ground
        let (mut killed, mut caught) = (0f64, 0f64);
        if batch.iter().any(|&k| self.animals[k] > 0.0) {
            let mut origin = vec![u32::MAX; n]; let mut steps = vec![0u32; n];
            let mut queue = std::collections::VecDeque::new();
            for &k in batch { for j in nb8(g, k) { if !self.seen[j] && open[j] && origin[j] == u32::MAX { origin[j] = j as u32; queue.push_back(j); } } }
            let max_steps = (self.reach_m / cell_m).ceil() as u32;
            while let Some(k) = queue.pop_front() {
                if steps[k] >= max_steps { continue; }
                for j in nb8(g, k) { if self.seen[j] && self.animals[j] > 0.0 && origin[j] == u32::MAX { origin[j] = origin[k]; steps[j] = steps[k] + 1; queue.push_back(j); } }
            }
            for &k in batch {
                let a = self.animals[k]; if a <= 0.0 { continue; }
                caught += a; self.animals[k] = 0.0;
                if origin[k] == u32::MAX { killed += a; continue; }
                let dead = a * self.mortality; killed += dead;
                self.animals[origin[k] as usize] += a - dead;
            }
        }
        // sensing: distance in cells from the newly burning ground, outward over open unburned cells
        let sense = (self.sense_m / cell_m).ceil() as u32;
        let moves = ((self.speed_m_h * step_h) / cell_m).round().max(1.0) as u32;
        let depth = sense + moves;
        let mut dist: std::collections::HashMap<usize, u32> = std::collections::HashMap::new();
        let mut queue = std::collections::VecDeque::new();
        for &k in batch { for j in nb8(g, k) { if !self.seen[j] && open[j] && !dist.contains_key(&j) { dist.insert(j, 1); queue.push_back(j); } } }
        let mut near: Vec<usize> = Vec::new();
        while let Some(k) = queue.pop_front() {
            let d = dist[&k];
            if d <= sense && self.animals[k] > 0.0 { near.push(k); }
            if d >= depth { continue; }
            for j in nb8(g, k) { if !self.seen[j] && open[j] && !dist.contains_key(&j) { dist.insert(j, d + 1); queue.push_back(j); } }
        }
        let far = |k: usize| dist.get(&k).copied().unwrap_or(u32::MAX);
        let (mut moved, mut left) = (0f64, 0f64);
        let mut arrivals: Vec<(usize, f64)> = Vec::with_capacity(near.len());
        for &k in &near {
            let mut at = k;
            for _ in 0..moves {
                let here = far(at);
                // the farthest neighbour from the fire; among equals, one chosen by the cell, so a crowd fans out
                let best = nb8(g, at).filter(|&j| !self.seen[j] && open[j] && far(j) > here).map(far).max();
                let Some(best) = best else { break };
                let ties: Vec<usize> = nb8(g, at).filter(|&j| !self.seen[j] && open[j] && far(j) == best).collect();
                at = ties[(k.wrapping_mul(2654435761) >> 7).wrapping_add(at) % ties.len()];
                if far(at) == u32::MAX { break; }
            }
            // the window is not a wall: an animal driven to its edge goes on out of the model
            let (c, r) = (at % g.nx, at / g.nx);
            if c == 0 || r == 0 || c + 1 == g.nx || r + 1 == g.ny { left += self.animals[k]; moved += self.animals[k]; self.animals[k] = 0.0; }
            else if at != k { arrivals.push((at, self.animals[k])); moved += self.animals[k]; self.animals[k] = 0.0; }
        }
        for (k, a) in arrivals { self.animals[k] += a; }
        (killed, caught, moved, left)
    }
}

impl Module for Flee {
    fn name(&self) -> &str { &self.name }
    fn init(&mut self, ctx: &mut Ctx) {
        ctx.ledger(WILD, "individuals", 1e-3);
        ctx.schedule_with_priority(self.dt, 7, "tick", json!(null));
    }
    fn handle(&mut self, ev: &Event, ctx: &mut Ctx) {
        if ev.topic != "tick" { return; }
        let now = ctx.now();
        let burned = ctx.world_read("fire.burned_frac").unwrap_or_else(|| panic!("{}: no fire.burned_frac; a fire module must be in the manifest", self.name));
        let g = burned.grid.clone(); let n = g.nx * g.ny;
        let burnt: Vec<bool> = burned.data.iter().map(|&b| b > 0.0).collect();
        let cover = field_on(ctx, "ground.cover_code", &g).unwrap_or_else(|| panic!("{}: no ground.cover_code; a ground.cover module must be in the manifest", self.name));
        let arrival = field_on(ctx, "fire.arrival_s", &g);
        // open ground: anything an animal can cross (not water, not outside the data)
        let open: Vec<bool> = cover.iter().map(|&c| c.is_finite() && c > 0.0 && c != 80.0).collect();
        if !self.started {
            let habitat = |c: f32| matches!(c as i32, 10 | 20 | 30 | 90 | 95 | 100);
            self.animals = (0..n).map(|k| if habitat(cover[k]) && !burnt[k] { self.density_km2 * g.row_area(k / g.nx) / 1e6 } else { 0.0 }).collect();
            self.home = self.animals.clone();
            self.seen = vec![false; n]; self.started = true;
            let total: f64 = self.animals.iter().sum();
            ctx.declare_stock(WILD, "alive", total);
        }
        let mut new: Vec<usize> = (0..n).filter(|&k| burnt[k] && !self.seen[k]).collect();
        let (mx, my) = g.metres_per_degree(g.ny / 2);
        let cell_m = ((mx * g.dlon) * (my * g.dlat)).sqrt();
        let (mut killed, mut caught, mut in_flight, mut left) = (0f64, 0f64, 0f64, 0f64);
        if !new.is_empty() {
            match &arrival {
                Some(a) => {
                    // replay the time since the last look: the cells in the order they caught
                    new.sort_by(|&x, &y| a[x].partial_cmp(&a[y]).unwrap_or(std::cmp::Ordering::Equal));
                    let hi = a[*new.last().unwrap()]; let lo = if self.a_last.is_finite() { self.a_last.min(a[new[0]]) } else { a[new[0]] };
                    let step_s = self.step.as_secs_f64().max(1.0);
                    let m = (((hi - lo) as f64 / step_s).ceil() as usize).clamp(1, (self.dt.as_secs_f64() / step_s).ceil().max(1.0) as usize);
                    let mut from = 0usize;
                    for j in 1..=m {
                        let until = lo + (hi - lo) * j as f32 / m as f32;
                        let to = if j == m { new.len() } else { from + new[from..].partition_point(|&k| a[k] <= until) };
                        if to > from {
                            let (k, c, f, l) = self.step(&g, &open, &new[from..to], cell_m, (hi - lo) as f64 / m as f64 / 3600.0);
                            killed += k; caught += c; in_flight = f; left += l;
                        } else { in_flight = 0.0; }
                        from = to;
                    }
                    self.a_last = hi;
                }
                None => { let (k, c, f, l) = self.step(&g, &open, &new, cell_m, self.dt.as_secs_f64() / 3600.0); killed += k; caught += c; in_flight = f; left += l; }
            }
        }
        if !new.is_empty() { let cap = self.crowd * self.density_km2 * g.row_area(g.ny / 2) / 1e6; self.disperse(&g, &open, cap); }
        if killed > 0.0 { ctx.transfer(WILD, "alive", "killed_by_fire", killed); }
        if left > 0.0 { ctx.transfer(WILD, "alive", "left_the_window", left); }
        self.killed += killed; self.caught += caught; self.left += left;
        let alive: f64 = self.animals.iter().sum();
        let displaced: f64 = self.animals.iter().zip(&self.home).map(|(a, h)| (a - h).max(0.0)).sum();
        ctx.declare_stock(WILD, "alive", alive);
        ctx.report("fauna.alive", "individuals", alive); ctx.report("fauna.killed_by_fire", "individuals", self.killed);
        ctx.report("fauna.caught", "individuals", self.caught); ctx.report("fauna.displaced", "individuals", displaced); ctx.report("fauna.in_flight", "individuals", in_flight); ctx.report("fauna.left_window", "individuals", self.left);
        if now.0 - self.last_frame >= self.frame_every.0 {
            let (fg, f) = self.frame(&g, &self.animals); ctx.field("animals_density_km2", "1/km2", &fg, f);
            self.last_frame = now.0;
        }
        ctx.schedule_with_priority(self.dt, 7, "tick", json!(null));
    }
    fn snapshot(&self) -> serde_json::Value { serde_json::to_value(self).unwrap() }
    fn restore(&mut self, v: serde_json::Value) { *self = serde_json::from_value(v).unwrap(); }
}

/// A grid's values laid on another grid: each cell takes the value of the source cell that holds
/// its centre, or of the nearest one at the edge. For fields whose writer works at its own spacing
/// (a weather record, a coarse survey); it does not average, so it suits a finer reader.
pub fn resample(from: &LatLonGrid, data: &[f32], to: &LatLonGrid) -> Vec<f32> {
    let cols: Vec<usize> = (0..to.nx).map(|c| (((to.lon(c) - from.lon_w) / from.dlon).floor().max(0.0) as usize).min(from.nx - 1)).collect();
    let mut out = Vec::with_capacity(to.nx * to.ny);
    for r in 0..to.ny {
        let fr = (((from.lat_n - to.lat(r)) / from.dlat).floor().max(0.0) as usize).min(from.ny - 1);
        for &fc in &cols { out.push(data[fr * from.nx + fc]); }
    }
    out
}
/// A shared field on a module's own cells, whatever cells its writer used.
pub fn field_on(ctx: &Ctx, name: &str, g: &LatLonGrid) -> Option<Vec<f32>> {
    let f = ctx.world_read(name)?;
    Some(if &f.grid == g { f.data.clone() } else { resample(&f.grid, &f.data, g) })
}

// ---------------------------------------------------------------- the grid, and three more parts of the ground

/// The cells of a place: the same for every module given the same place, with nothing fetched.
pub fn grid_of(lat_s: f64, lat_n: f64, lon_w: f64, lon_e: f64, res_arcsec: f64) -> LatLonGrid {
    let (nx, ny, d, lon_w, lat_n) = esim_registry::window_grid(lat_s, lat_n, lon_w, lon_e, res_arcsec);
    LatLonGrid { nx, ny, dlon: d, dlat: d, lon_w, lat_n }
}
fn place_of(kind: &str, name: &str, p: &toml::Value) -> Result<(f64, f64, f64, f64, f64), String> {
    let need = |k: &str| p.get(k).and_then(|v| v.as_float().or(v.as_integer().map(|i| i as f64))).ok_or(format!("{kind} {name}: no {k}; give it a place"));
    Ok((need("lat_s")?, need("lat_n")?, need("lon_w")?, need("lon_e")?, param_f64(p, "res_arcsec", 1.0)))
}
fn place_params(d: Describe) -> Describe {
    d.param("lat_s", 0.0, "deg", "from the place").param("lat_n", 0.0, "deg", "").param("lon_w", 0.0, "deg", "").param("lon_e", 0.0, "deg", "").param("res_arcsec", 1.0, "arcsec", "")
}

/// `ground.cover`: what covers each cell, as ESA WorldCover's class codes.
#[derive(Serialize, Deserialize)]
pub struct Cover { name: String, place: (f64, f64, f64, f64, f64), #[serde(skip)] cover: Vec<u8> }

impl Cover {
    pub fn new(name: &str, p: &toml::Value) -> Result<Cover, String> {
        let mut c = Cover { name: name.into(), place: place_of("ground.cover", name, p)?, cover: vec![] };
        c.load()?; Ok(c)
    }
    fn load(&mut self) -> Result<(), String> {
        let root = root();
        let cat = Catalog::load(&root.join("data/catalog.toml"))?;
        let lc = cat.asset("esa-worldcover-2021").ok_or("catalog has no esa-worldcover-2021")?;
        let mut fe = Fetcher::default_for(&root)?;
        let (a, b, c, d, r) = self.place; let g = grid_of(a, b, c, d, r);
        self.cover = esim_registry::wildfire::landcover_window(&mut fe, lc, g.nx, g.ny, g.dlon, g.dlat, g.lon_w, g.lat_n)?;
        Ok(())
    }
    pub fn describe(name: &str, _p: &toml::Value) -> Describe {
        place_params(Describe::new("ground.cover", name, "What covers each cell of the window: tree, shrub, grass, crop, built-up, bare, water and the rest, as the class codes of ESA WorldCover 2021, a map made from satellite images at 10 metres. Read once; nothing here changes during a run, so a burn or a clearing does not show in it."))
            .dt("once, at the start").own("ground.cover_code", "worldcover code")
            .asset("esa-worldcover-2021", "land cover")
            .note("The map is of 2021 and is not updated for what a run does to the ground.")
    }
}
impl Module for Cover {
    fn name(&self) -> &str { &self.name }
    fn init(&mut self, ctx: &mut Ctx) {
        let (a, b, c, d, r) = self.place; let g = grid_of(a, b, c, d, r);
        ctx.world_write("ground.cover_code", "worldcover code", &g, self.cover.iter().map(|&c| c as f32).collect());
    }
    fn handle(&mut self, _ev: &Event, _ctx: &mut Ctx) {}
    fn snapshot(&self) -> serde_json::Value { serde_json::to_value(self).unwrap() }
    fn restore(&mut self, v: serde_json::Value) { let mut c: Cover = serde_json::from_value(v).unwrap(); c.load().expect("cover reload"); *self = c; }
}

/// `roads.major`: the cells under a road wide enough to matter, from OpenStreetMap.
#[derive(Serialize, Deserialize)]
pub struct Roads { name: String, place: (f64, f64, f64, f64, f64), widths_m: String, #[serde(default)] roads: String, #[serde(default)] names: String, #[serde(skip)] road: Vec<f32> }

impl Roads {
    pub fn new(name: &str, p: &toml::Value) -> Result<Roads, String> {
        let mut r = Roads { name: name.into(), place: place_of("roads.major", name, p)?, widths_m: param_str(p, "widths_m", "motorway=30,trunk=20,primary=14,secondary=10,tertiary=8").into(), roads: crate::data_of(p, "roads"), names: crate::data_of(p, "names"), road: vec![] };
        r.load()?; Ok(r)
    }
    fn load(&mut self) -> Result<(), String> {
        let (lat_s, lat_n, lon_w, lon_e, res) = self.place; let grid = grid_of(lat_s, lat_n, lon_w, lon_e, res);
        if self.roads.is_empty() { eprintln!("roads: no roads product named in data; no cell is marked"); }
        // a roads product the manifest names and no store can give stops the run: the fire would otherwise cross every road
        let cv = if self.roads.is_empty() { None } else { let mut fe = Fetcher::default_for(&root())?; Some(esim_registry::warehouse::context(&mut fe, &self.roads, &self.names)?) };
        self.road = rasterise_roads(cv.as_ref(), &self.widths_m, &grid).into_iter().map(|b| if b { 1.0 } else { 0.0 }).collect();
        Ok(())
    }
    pub fn describe(name: &str, p: &toml::Value) -> Describe {
        let d = place_params(Describe::new("roads.major", name, "The major roads of the window, as the cells they cover, from the roads product the manifest names in `data` (OpenStreetMap's ways, kept in the data warehouse): each road is given a width by its class and every cell under it is marked. A fire reads the marks as ground with no fuel; anything else may read them as roads."))
            .dt("once, at the start").param("widths_m", "motorway=30,trunk=20,primary=14,secondary=10,tertiary=8", "", "the width given to each class of road, in metres; classes not named are not marked")
            .param("data", "", "", "a table naming warehouse products: `roads` (a roads product, `roads/osm/<place>/<date>`) and optionally `names` (place names and city limits for the map)").own("roads.cells", "1").asset("esimv1-data", "the roads and place names of the window")
            .note("Widths are by class, not measured. Minor roads are left out.");
        match crate::cached_map_context(p) { Some(c) => d.table("context", c), None => d }
    }
}
impl Module for Roads {
    fn name(&self) -> &str { &self.name }
    fn init(&mut self, ctx: &mut Ctx) {
        let (a, b, c, d, r) = self.place; let g = grid_of(a, b, c, d, r);
        ctx.world_write("roads.cells", "1", &g, self.road.clone());
    }
    fn handle(&mut self, _ev: &Event, _ctx: &mut Ctx) {}
    fn snapshot(&self) -> serde_json::Value { serde_json::to_value(self).unwrap() }
    fn restore(&mut self, v: serde_json::Value) { let mut c: Roads = serde_json::from_value(v).unwrap(); c.load().expect("roads reload"); *self = c; }
}

/// The cells under the roads of a context table that have a width in `widths` ("class=metres,...").
pub fn rasterise_roads(ctxv: Option<&serde_json::Value>, widths_m: &str, grid: &LatLonGrid) -> Vec<bool> {
    let n = grid.nx * grid.ny;
    let mut road = vec![false; n];
    let metres: Vec<(f64, f64)> = (0..grid.ny).map(|r| { let (mx, my) = grid.metres_per_degree(r); (mx * grid.dlon, my * grid.dlat) }).collect();
    let widths: std::collections::HashMap<String, f64> = widths_m.split(',').filter_map(|kv| kv.split_once('=')).map(|(k, v)| (k.trim().to_string(), v.trim().parse().unwrap_or(10.0))).collect();
    let Some(cv) = ctxv else { return road };
    for rd in cv["roads"].as_array().into_iter().flatten() {
        let Some(&wm) = widths.get(rd["class"].as_str().unwrap_or("")) else { continue };
        let pts: Vec<(f64, f64)> = rd["pts"].as_array().into_iter().flatten().filter_map(|p| Some((p[0].as_f64()?, p[1].as_f64()?))).collect();
        for seg in pts.windows(2) {
            let (ax, ay) = seg[0]; let (bx, by) = seg[1];
            let row_mid = (((grid.lat_n - (ay + by) / 2.0) / grid.dlat) as usize).min(grid.ny - 1);
            let (mx, my) = metres[row_mid];
            let len = (((bx - ax) * mx / grid.dlon).powi(2) + ((by - ay) * my / grid.dlat).powi(2)).sqrt();
            let steps = (len / 5.0).ceil().max(1.0) as usize;
            let half = wm / 2.0;
            for s in 0..=steps {
                let t = s as f64 / steps as f64; let (x, y) = (ax + (bx - ax) * t, ay + (by - ay) * t);
                if x < grid.lon_w || x >= grid.lon_e() || y <= grid.lat_s() || y > grid.lat_n { continue; }
                let c = ((x - grid.lon_w) / grid.dlon) as usize; let r = ((grid.lat_n - y) / grid.dlat) as usize;
                let (rc, rr) = ((half / mx).ceil() as isize, (half / my).ceil() as isize);
                for dr in -rr..=rr { for dc in -rc..=rc {
                    let (cc, rrr) = (c as isize + dc, r as isize + dr);
                    if cc < 0 || rrr < 0 || cc >= grid.nx as isize || rrr >= grid.ny as isize { continue; }
                    if ((dc as f64 * mx).powi(2) + (dr as f64 * my).powi(2)).sqrt() <= half.max(mx * 0.5) { road[rrr as usize * grid.nx + cc as usize] = true; }
                } }
            }
        }
    }
    road
}

/// The share of each cell under a footprint, and the mean stated height there (NaN where none).
pub fn rasterise_buildings(bs: &[esim_registry::wildfire::Building], grid: &LatLonGrid) -> (Vec<f32>, Vec<f32>) {
    let n = grid.nx * grid.ny;
    let mut built_frac = vec![0.0f32; n]; let mut built_height = vec![f32::NAN; n];
    let mut hsum = vec![0f32; n]; let mut hcnt = vec![0u16; n];
    for b in bs {
        // the footprint's bounding cells, then its area share by sub-sampling each cell 3x3
        let (xs, ys): (Vec<f64>, Vec<f64>) = b.ring.iter().cloned().unzip();
        let (x0, x1) = (xs.iter().cloned().fold(f64::INFINITY, f64::min), xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
        let (y0, y1) = (ys.iter().cloned().fold(f64::INFINITY, f64::min), ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
        if x1 < grid.lon_w || x0 > grid.lon_e() || y1 < grid.lat_s() || y0 > grid.lat_n { continue; }
        let c0 = (((x0 - grid.lon_w) / grid.dlon).floor().max(0.0)) as usize; let c1 = (((x1 - grid.lon_w) / grid.dlon).floor() as usize).min(grid.nx - 1);
        let r0 = (((grid.lat_n - y1) / grid.dlat).floor().max(0.0)) as usize; let r1 = (((grid.lat_n - y0) / grid.dlat).floor() as usize).min(grid.ny - 1);
        let inside = |x: f64, y: f64| -> bool { let mut c = false; let m = b.ring.len(); for i in 0..m { let (xi, yi) = b.ring[i]; let (xj, yj) = b.ring[(i + 1) % m]; if (yi > y) != (yj > y) && x < xi + (y - yi) * (xj - xi) / (yj - yi) { c = !c; } } c };
        for r in r0..=r1 { for c in c0..=c1 {
            let mut hits = 0;
            for sy in 0..3 { for sx in 0..3 {
                let x = grid.lon_w + (c as f64 + (sx as f64 + 0.5) / 3.0) * grid.dlon; let y = grid.lat_n - (r as f64 + (sy as f64 + 0.5) / 3.0) * grid.dlat;
                if inside(x, y) { hits += 1; }
            } }
            if hits > 0 { let k = r * grid.nx + c; built_frac[k] = (built_frac[k] + hits as f32 / 9.0).min(1.0); if b.height.is_finite() { hsum[k] += b.height as f32; hcnt[k] += 1; } }
        } }
    }
    for k in 0..n { if hcnt[k] > 0 { built_height[k] = hsum[k] / hcnt[k] as f32; } }
    (built_frac, built_height)
}

/// What the inspectors recorded for a building, as a number: 0 unknown, 1 no damage, 2 damaged, 3 destroyed.
pub fn fate_code(damage: &str) -> u8 { if damage.starts_with("Destroyed") { 3 } else if damage.starts_with("No Damage") { 1 } else if damage.is_empty() { 0 } else { 2 } }

/// `built.footprints`: the buildings of the window, as a shared table of things and as two maps.
#[derive(Serialize, Deserialize)]
pub struct Footprints {
    name: String, place: (f64, f64, f64, f64, f64),
    #[serde(default)] data: String,
    #[serde(skip)] bs: Vec<esim_registry::wildfire::Building>, #[serde(skip)] frac: Vec<f32>, #[serde(skip)] height: Vec<f32>,
}

impl Footprints {
    pub fn new(name: &str, p: &toml::Value) -> Result<Footprints, String> {
        let mut f = Footprints { name: name.into(), place: place_of("built.footprints", name, p)?, data: crate::data_of(p, "buildings"), bs: vec![], frac: vec![], height: vec![] };
        f.load()?; Ok(f)
    }
    fn load(&mut self) -> Result<(), String> {
        let (lat_s, lat_n, lon_w, lon_e, res) = self.place; let grid = grid_of(lat_s, lat_n, lon_w, lon_e, res);
        let mut fe = Fetcher::default_for(&root())?;
        let fetched = if self.data.is_empty() { Err("no buildings product named in data".to_string()) } else { esim_registry::warehouse::buildings(&mut fe, &self.data) };
        self.bs = match fetched { Ok(bs) => bs, Err(e) if self.data.is_empty() => { eprintln!("buildings: {e}; no footprints"); vec![] } Err(e) => return Err(format!("buildings: {e}")) };
        let (f, h) = rasterise_buildings(&self.bs, &grid);
        eprintln!("buildings: {} footprints, {} cells touched", self.bs.len(), f.iter().filter(|&&x| x > 0.0).count());
        self.frac = f; self.height = h;
        Ok(())
    }
    pub fn describe(name: &str, _p: &toml::Value) -> Describe {
        place_params(Describe::new("built.footprints", name, "The buildings of the window. Each footprint is one row of a shared table with its outline, its stated height and, where inspectors recorded it after an event, what happened to it. The same footprints are also laid on the cells as two maps: the share of each cell under a building and the mean height there. Any module may read the table: a fire to burn the buildings, people to live in them, a count to score against."))
            .dt("once, at the start")
            .param("data", "", "", "a table naming warehouse products; this module reads `buildings`, a buildings product such as `buildings/massing/<place>/<date>`. With none named there are no footprints")
            .own("built.frac", "1").own("built.height", "ft (as stated)").own("built.footprints", "table")
            .frame("building_height", "ft (as stated)")
            .asset("esimv1-data", "footprints with a height each and, where the source kept it, the inspected damage")
            .note("The table built.footprints has one row per building: columns height and fate (0 unknown, 1 no damage, 2 damaged, 3 destroyed) and the outline as a shape.").note("A height is in feet whatever the product's source. A massing product gives every outline a height and says where each came from (measured, tagged, estimated from pictures, or guessed from neighbours); this module does not carry that mark, so a guessed height reads like a measured one here. What a building is made of is not known.")
    }
}
impl Module for Footprints {
    fn name(&self) -> &str { &self.name }
    fn init(&mut self, ctx: &mut Ctx) {
        let (a, b, c, d, r) = self.place; let g = grid_of(a, b, c, d, r);
        ctx.world_write("built.frac", "1", &g, self.frac.clone());
        ctx.world_write("built.height", "ft (as stated)", &g, self.height.clone());
        ctx.field("building_height", "ft (as stated)", &g, self.height.clone());
        let mut cols = std::collections::BTreeMap::new();
        cols.insert("height".to_string(), self.bs.iter().map(|b| b.height).collect::<Vec<f64>>());
        cols.insert("fate".to_string(), self.bs.iter().map(|b| fate_code(&b.damage) as f64).collect::<Vec<f64>>());
        ctx.world_table_write("built.footprints", cols, self.bs.iter().map(|b| b.ring.clone()).collect());
    }
    fn handle(&mut self, _ev: &Event, _ctx: &mut Ctx) {}
    fn snapshot(&self) -> serde_json::Value { serde_json::to_value(self).unwrap() }
    fn restore(&mut self, v: serde_json::Value) { let mut c: Footprints = serde_json::from_value(v).unwrap(); c.load().expect("footprints reload"); *self = c; }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_coarse_field_is_laid_on_fine_cells() {
        // 2 x 2 cells of one degree, read on 4 x 4 cells of half a degree over the same window
        let coarse = LatLonGrid { nx: 2, ny: 2, dlon: 1.0, dlat: 1.0, lon_w: 0.0, lat_n: 2.0 };
        let fine = LatLonGrid { nx: 4, ny: 4, dlon: 0.5, dlat: 0.5, lon_w: 0.0, lat_n: 2.0 };
        let out = resample(&coarse, &[1.0, 2.0, 3.0, 4.0], &fine);
        assert_eq!(out, vec![1.0, 1.0, 2.0, 2.0, 1.0, 1.0, 2.0, 2.0, 3.0, 3.0, 4.0, 4.0, 3.0, 3.0, 4.0, 4.0]);
        // a reader whose window overhangs the writer's takes the nearest edge cell
        let wide = LatLonGrid { nx: 3, ny: 1, dlon: 1.0, dlat: 1.0, lon_w: -1.0, lat_n: 2.0 };
        assert_eq!(resample(&coarse, &[1.0, 2.0, 3.0, 4.0], &wide), vec![1.0, 1.0, 2.0]);
    }
}
