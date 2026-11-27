//! Shared world state: named fields on grids, written only by their owning module and
//! only through the context, so every write is an ordered, logged event. Readers see
//! the latest write at or before their own time, which with in-order delivery is the
//! conservative rule of the brief in its single-threaded form.

use crate::grid::LatLonGrid;
use crate::time::Time;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorldField {
    pub unit: String,
    pub grid: LatLonGrid,
    pub data: Vec<f32>,
    pub owner: String,
    pub written_at: Time,
    pub version: u64,
}

/// A shared table of things: one row per thing (a building, a tank, a bridge, an animal), numeric
/// columns of equal length, and optionally a shape per row as a ring of (lon, lat). For what is not
/// a map. One owner writes it whole; anyone reads it. A module that keeps its own state about the
/// same things writes a table of its own with the same number of rows, in the same order.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WorldTable {
    pub columns: BTreeMap<String, Vec<f64>>,
    #[serde(default)]
    pub shapes: Vec<Vec<(f64, f64)>>,
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub written_at: Time,
    #[serde(default)]
    pub version: u64,
}

impl WorldTable {
    pub fn rows(&self) -> usize { self.columns.values().next().map(|c| c.len()).unwrap_or(self.shapes.len()) }
    pub fn column(&self, name: &str) -> Option<&[f64]> { self.columns.get(name).map(|v| v.as_slice()) }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct World {
    pub fields: BTreeMap<String, WorldField>,
    #[serde(default)]
    pub tables: BTreeMap<String, WorldTable>,
}

impl World {
    pub fn table(&self, name: &str) -> Option<&WorldTable> { self.tables.get(name) }
    /// Returns the new version and a digest, for the log.
    pub(crate) fn write_table(&mut self, owner: &str, name: &str, columns: BTreeMap<String, Vec<f64>>, shapes: Vec<Vec<(f64, f64)>>, now: Time) -> Result<(u64, u64, usize), String> {
        let rows = columns.values().next().map(|c| c.len()).unwrap_or(shapes.len());
        if columns.values().any(|c| c.len() != rows) || (!shapes.is_empty() && shapes.len() != rows) { return Err(format!("world table {name}: columns and shapes differ in length")); }
        let mut bytes: Vec<u8> = Vec::new();
        for (k, c) in &columns { bytes.extend(k.as_bytes()); for v in c { bytes.extend(v.to_le_bytes()); } }
        for s in &shapes { for (x, y) in s { bytes.extend(x.to_le_bytes()); bytes.extend(y.to_le_bytes()); } bytes.push(0); }
        let digest = crate::hash::fnv1a(&bytes);
        let version = match self.tables.get(name) {
            Some(t) if t.owner != owner => return Err(format!("world table {name} is owned by {}, not {owner}", t.owner)),
            Some(t) => t.version + 1,
            None => 1,
        };
        self.tables.insert(name.to_string(), WorldTable { columns, shapes, owner: owner.to_string(), written_at: now, version });
        Ok((version, digest, rows))
    }
    pub fn get(&self, name: &str) -> Option<&WorldField> { self.fields.get(name) }
    /// Returns the new version and a digest of the data, for the log.
    pub(crate) fn write(&mut self, owner: &str, name: &str, unit: &str, grid: &LatLonGrid, data: Vec<f32>, now: Time) -> Result<(u64, u64), String> {
        if data.len() != grid.nx * grid.ny { return Err(format!("world field {name}: {} values for a {}x{} grid", data.len(), grid.nx, grid.ny)); }
        let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_le_bytes()).collect();
        let digest = crate::hash::fnv1a(&bytes);
        match self.fields.get_mut(name) {
            Some(f) => {
                if f.owner != owner { return Err(format!("world field {name} is owned by {}, not {owner}", f.owner)); }
                if &f.grid != grid { return Err(format!("world field {name}: grid changed")); }
                f.data = data; f.written_at = now; f.version += 1;
                Ok((f.version, digest))
            }
            None => {
                self.fields.insert(name.to_string(), WorldField { unit: unit.to_string(), grid: grid.clone(), data, owner: owner.to_string(), written_at: now, version: 1 });
                Ok((1, digest))
            }
        }
    }
}
