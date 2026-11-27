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
    source: String, wind_use: String, start: String, end: String, at: String, dt: Duration,
    #[serde(skip)] hrrr: Vec<Option<HrrrHour>>, #[serde(skip)] hrrr_t0: i64, #[serde(skip)] shift: i64, #[serde(skip)] era5: Option<WindSeries>,
}

impl Surface {
    pub fn new(name: &str, p: &toml::Value) -> Result<Surface, String> {
        let need = |k: &str| p.get(k).and_then(|v| v.as_float().or(v.as_integer().map(|i| i as f64))).ok_or(format!("weather.surface {name}: no {k}; give it a place"));
        let s = |k: &str| p.get(k).and_then(|v| v.as_str()).map(String::from).ok_or(format!("weather.surface {name}: no {k}"));
        let mut w = Surface { name: name.into(), lat_s: need("lat_s")?, lat_n: need("lat_n")?, lon_w: need("lon_w")?, lon_e: need("lon_e")?,
            source: param_str(p, "source", "hrrr").into(), wind_use: param_str(p, "wind_use", "mean").into(), start: s("start")?, end: s("end")?, at: param_str(p, "at", "").into(),
            dt: param_duration(p, "dt", "1h")?, hrrr: vec![], hrrr_t0: 0, shift: 0, era5: None };
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
            .read("ground.elev_m", "m", true, "only for its grid: the cells the air is laid on")
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
        let g = ctx.world_read("ground.elev_m").unwrap_or_else(|| panic!("{}: no ground.elev_m; a ground.terrain module must be in the manifest", self.name)).grid.clone();
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

/// Animals that leave ground the fire reaches. A placeholder population, declared as one: the same
/// number per square kilometre on every cell of wildland fuel at the start. Each hour the module
/// reads the fire's burned cells; the animals on ground burned since the last hour either die (a
/// stated share) or move to the nearest unburned cell, and those with no unburned ground within
/// reach die. It says nothing about any species; it is here so that the fire has a neighbour that
/// reacts to it, and to be replaced by a module that knows animals.
#[derive(Serialize, Deserialize)]
pub struct Flee {
    name: String,
    dt: Duration, frame_every: Duration, frame_res_arcsec: f64,
    density_km2: f64, mortality: f64, reach_m: f64,
    animals: Vec<f64>, seen: Vec<bool>, started: bool,
    killed: f64, moved: f64, last_frame: i64,
}

impl Flee {
    pub fn new(name: &str, p: &toml::Value) -> Result<Flee, String> {
        Ok(Flee { name: name.into(), dt: param_duration(p, "dt", "1h")?, frame_every: param_duration(p, "frame_every", "6h")?, frame_res_arcsec: param_f64(p, "frame_res_arcsec", 3.0),
            density_km2: param_f64(p, "density_km2", 20.0), mortality: param_f64(p, "mortality", 0.1), reach_m: param_f64(p, "reach_m", 3000.0),
            animals: vec![], seen: vec![], started: false, killed: 0.0, moved: 0.0, last_frame: i64::MIN / 4 })
    }
    pub fn describe(name: &str, p: &toml::Value) -> Describe {
        Describe::new("fauna.flee", name, "Animals that leave ground the fire reaches. A placeholder population: the same number per square kilometre on every cell of grass, shrub or tree at the start. Each hour the animals on newly burned ground either die, at a stated share, or move to the nearest unburned cell; those with no unburned ground within reach die. Every animal is accounted for: alive, or killed by the fire.")
            .priority(7).dt(param_str(p, "dt", "1h"))
            .param("dt", "1h", "", "").param("frame_every", "6h", "", "").param("frame_res_arcsec", 3.0, "arcsec", "")
            .param("density_km2", 20.0, "1/km2", "a placeholder, not a survey: animals per square kilometre of wildland fuel at the start").param("mortality", 0.1, "", "guess: the share of animals on newly burned ground that die there")
            .param("reach_m", 3000.0, "m", "guess: how far an animal can get in one step to unburned ground; farther than this and it dies")
            .read("fire.burned_frac", "1", true, "the fire's burned cells, read each hour after the fire has moved").read("ground.fuel_kind", "0 none 1 grass 2 shrub 3 tree 4 built", true, "where animals start: grass, shrub and tree")
            .ledger(WILD, "individuals", 1e-3)
            .frame("animals_density_km2", "1/km2")
            .report("fauna.alive", "individuals").report("fauna.killed_by_fire", "individuals").report("fauna.displaced", "individuals").report("fauna.fleeing_hour", "individuals/h")
            .view("map", "Animals per square kilometre", "animals_density_km2", "they pile up along the fire's edge as they flee").view("chart", "Animals displaced so far", "fauna.displaced", "each move counted, so an animal driven twice counts twice").view("chart", "Animals killed by the fire", "fauna.killed_by_fire", "")
            .note("No species, no behaviour, no senses: an animal moves only when its own cell has burned, and to the nearest unburned cell whatever that cell is, a road or a town included. The starting number, the share that dies and the reach are placeholders.")
    }
    fn frame(&self, g: &LatLonGrid) -> (LatLonGrid, Vec<f32>) {
        let k = (self.frame_res_arcsec / (g.dlon * 3600.0)).round().max(1.0) as usize;
        let fg = LatLonGrid { nx: g.nx.div_ceil(k), ny: g.ny.div_ceil(k), dlon: g.dlon * k as f64, dlat: g.dlat * k as f64, lon_w: g.lon_w, lat_n: g.lat_n };
        let mut sum = vec![0f64; fg.nx * fg.ny]; let mut area = vec![0f64; fg.nx * fg.ny];
        for r in 0..g.ny { let a = g.row_area(r); for c in 0..g.nx { let j = (r / k) * fg.nx + c / k; sum[j] += self.animals[r * g.nx + c]; area[j] += a; } }
        (fg, sum.iter().zip(&area).map(|(s, a)| if *a > 0.0 { (s / (a / 1e6)) as f32 } else { f32::NAN }).collect())
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
        let kind: Vec<f32> = ctx.world_read_on("ground.fuel_kind", &g).unwrap_or_else(|| panic!("{}: no ground.fuel_kind", self.name)).data.clone();
        if !self.started {
            self.animals = (0..n).map(|k| if (1.0..=3.0).contains(&kind[k]) && !burnt[k] { self.density_km2 * g.row_area(k / g.nx) / 1e6 } else { 0.0 }).collect();
            self.seen = vec![false; n]; self.started = true;
            let total: f64 = self.animals.iter().sum();
            ctx.declare_stock(WILD, "alive", total);
        }
        // the cells burned since the last look, and the nearest unburned ground for each: one search
        // outward from the unburned cells that touch them, through the new burn only
        let new: Vec<usize> = (0..n).filter(|&k| burnt[k] && !self.seen[k]).collect();
        let (mut fleeing, mut killed, mut moved) = (0f64, 0f64, 0f64);
        if !new.is_empty() {
            let mut origin = vec![u32::MAX; n]; let mut steps = vec![0u32; n];
            let mut queue = std::collections::VecDeque::new();
            let nb = |k: usize| { let (c, r) = (k % g.nx, k / g.nx); [(c > 0).then(|| k - 1), (c + 1 < g.nx).then(|| k + 1), (r > 0).then(|| k - g.nx), (r + 1 < g.ny).then(|| k + g.nx)] };
            for &k in &new { for j in nb(k).into_iter().flatten() { if !burnt[j] && kind[j] > 0.0 && origin[j] == u32::MAX { origin[j] = j as u32; queue.push_back(j); } } }
            let (mx, _) = g.metres_per_degree(g.ny / 2);
            let max_steps = (self.reach_m / (mx * g.dlon)).ceil() as u32;
            while let Some(k) = queue.pop_front() {
                if steps[k] >= max_steps { continue; }
                for j in nb(k).into_iter().flatten() { if burnt[j] && !self.seen[j] && origin[j] == u32::MAX { origin[j] = origin[k]; steps[j] = steps[k] + 1; queue.push_back(j); } }
            }
            for &k in &new {
                let a = self.animals[k]; self.seen[k] = true;
                if a <= 0.0 { continue; }
                fleeing += a; self.animals[k] = 0.0;
                if origin[k] == u32::MAX { killed += a; continue; }
                let dead = a * self.mortality; killed += dead; moved += a - dead;
                self.animals[origin[k] as usize] += a - dead;
            }
        }
        if killed > 0.0 { ctx.transfer(WILD, "alive", "killed_by_fire", killed); }
        self.killed += killed; self.moved += moved;
        let alive: f64 = self.animals.iter().sum();
        ctx.declare_stock(WILD, "alive", alive);
        ctx.report("fauna.alive", "individuals", alive); ctx.report("fauna.killed_by_fire", "individuals", self.killed);
        ctx.report("fauna.displaced", "individuals", self.moved); ctx.report("fauna.fleeing_hour", "individuals/h", fleeing);
        if now.0 - self.last_frame >= self.frame_every.0 { let (fg, f) = self.frame(&g); ctx.field("animals_density_km2", "1/km2", &fg, f); self.last_frame = now.0; }
        ctx.schedule_with_priority(self.dt, 7, "tick", json!(null));
    }
    fn snapshot(&self) -> serde_json::Value { serde_json::to_value(self).unwrap() }
    fn restore(&mut self, v: serde_json::Value) { *self = serde_json::from_value(v).unwrap(); }
}
