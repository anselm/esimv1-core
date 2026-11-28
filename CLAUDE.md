# esimv1-core

Project-specific orientation only. House policy (devlog, git, UX, writing, deploy) comes from the
harness and is not repeated here.

## What this is

The engine of esimv1: a whole-system simulator in Rust. A kernel (a clock, a bus, ledgers that must close, world fields, per-module random streams, snapshots), a registry that knows where data is and fetches it, the module kinds, an auditor, and the `esim` command line. It writes run folders; it draws nothing.

## Where things are

- The devlog before 22 November 2026 is in `anselm/esimv1` (`devlog/`), as is the long list of project facts in its
  `CLAUDE.md`; paths there that begin `crates/`, `data/`, `manifests/`, `regress/` or `core/` are this repository.
- On canada1 the working clone is `~/projects/2026/earth-simulator-1/core` (inside the hub's folder, ignored by the
  hub's git). `cargo` is at `~/.cargo/bin`. The fetched cache is `data/cache/` (several GB, not in git).
- Run `esim regress` before committing anything that touches a planet or basin module. When a docs page changes, set
  its `revised` stamp. The kernel stays small: new behaviour goes in a module, the registry or the command line.
- Records of scenarios that moved out (Portland, the Palisades) are in their repositories, not in `data/events/`.
- Since 2026-11-28 (version 0.4.0) the engine asks OpenStreetMap or a county service for nothing. Roads, buildings,
  place names and shorelines are products of the data warehouse (`anselm/esimv1-data`, public store
  https://data.earthlight.cloud), read by `crates/registry/src/warehouse.rs`. A manifest names them in a `data` table
  handed to modules beside `place` (`crate::data_of` in the modules). Fifteen small global files are read from the
  store through `data/catalog.toml`, as the same bytes as before. `ESIM_DATA_STORES` lists stores; unset, the
  hub's `dataservice/store` is read first when it exists, then the public store. `data/osm/` is gone. A new place
  needs its products built by the data tool first (`docs/system/data.md`, The data warehouse).
