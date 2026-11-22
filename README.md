# esimv1-core

The engine of [esimv1](https://github.com/anselm/esimv1): a simulator for whole places.
Fields, networks, populations, agents and rules run on one clock, with ledgers that must
close at every seam and runs that replay to the same digest.

This repository is the engine alone. It reads a manifest, fetches the data its modules
ask for, runs, and writes a run folder. Drawing a run is the job of
[esimv1-viewer](https://github.com/anselm/esimv1-viewer); the site where people keep and
share scenarios is [esimv1](https://github.com/anselm/esimv1).

## What is here

```
crates/kernel     the clock, the bus, ledgers, world fields, random streams, snapshots, the manifest reader
crates/registry   the catalog of sources, the fetcher, the readers; where things are on disk
crates/modules    the kinds: water, fire, coast, earthquake, planet, people, ...
crates/audit      reads a run folder after the fact
crates/cli        esim: run, prepare, project, verify, ensemble, sweep, fit, regress, kinds, ...
data/             catalog.toml (every source, its licence, the day it was checked), catalog.lock.toml,
                  events/ (shared tables and the records of the scenarios kept here); cache/ is not in git
manifests/        the ladder, from two buckets to the planet
regress/          the hindcast ladder and its accepted scores
docs/             the engine's documentation: system/, rfc/, events/, protocol-v0.md
```

## Run it

```
cargo build --release
./target/release/esim kinds
./target/release/esim run manifests/p1-planet-co2-1850-2025.toml     # 175 years of planet, about a second
./target/release/esim regress                                        # the hindcast ladder
cargo test --release
```

A scenario kept in its own folder or repository runs by path:

```
./target/release/esim project run /path/to/scenario/folder --variant name
```

`ESIM_ROOT` names this folder when `esim` is run from somewhere else. A project names the
oldest engine it runs on with `engine = "0.3.0"` in its `project.toml`. `docs/system/projects-and-runs.md`
has the rest, and describes the run folder, whose layout carries a `format` number.

## Status

Version 0.3.0. A research project. It has been used by one team, on fourteen scenarios.
New kinds of module are compiled in; a scenario kept elsewhere cannot yet bring its own.
`docs/system/limits.md` lists what each model is known to get wrong.

Free and open source, MIT.
