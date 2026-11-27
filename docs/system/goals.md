<!-- order: 1 -->
<!-- section: Where this is going -->
<!-- author: Claude -->
<!-- ordered-by: Anselm -->
<!-- revised-by: Anselm -->
<!-- written: 2026-11-14 -->
<!-- revised: 2026-11-26 -->
<!-- revision: 1 -->
# Goals and milestones

## The goal

The goal of this simulation engine is to build a software tool that ordinary people can use to
model issues in their own communities and bioregions.

Some uses will be didactic: a simulation on a city government's web page that shows what a
policy would do. Some will be participatory: residents changing the assumptions and seeing what
follows, as a way for a community to reason toward agreement. The expectation is that
specialised firms will help cities and communities set up and run simulation engines such as
this one, much as small development houses built the first generation of web sites, and that the
city will often be the one paying.

That sets an end-to-end requirement. The software has to be packaged, deployable by someone who
did not write it, and usable by someone who is not a modeller. A simulation engine that sits in
a repository and is forgotten does not meet the goal. The aim is solutions that reach millions
of people.

## The phases

The goal is too large to deliver at once, so it is broken into phases. Each phase is something
the project can actually finish, and each one feeds the next.

**1. Discovery.** Model a variety of real situations, choosing the most useful issues the
project can find: a fire, a flood forecast, a harbor that silts up, a city after an earthquake.
This is exercise for the engine. Along the way the modules are refined, split and joined, with
the aim that they settle into reusable units of science that serve more than one scenario. *The
project is in this phase now.*

**2. Refinement.** Bring the scenarios closer to how a person imagines the situation. At
present a scenario generated with a language model is more abstract than a layperson's picture
of the same thing: its parts are grouped by method where a person would group them by subject.
The [layers](/docs/system/layers) are the first step of this. Two separations are examples: the
Portland scenario into tanks, bridges, roads, power, hospitals and a fuel account, and the fire
scenarios into relief, land cover, roads, buildings, weather, fuel moisture, the fire, animals
and firefighting, with two fire models reading the same parts
([What this is](/docs/system/overview) has the account).

**3. Experience.** Polish what a visitor sees and does. This phase needs a way for an ordinary
user to talk to a scenario through a language model and try variations of it, and documentation
written for a layperson. Whether it needs user accounts is open. An earlier plan had visitors
sign in and copy scenarios on the web site; that was dropped in November 2026, because the
engine is not yet something a visitor can change safely from a browser.

**4. Deployment.** Package the engine so that a city, a community or a firm working for them can
run their own instance for their own place.

## How things work today

- **Scenarios arrive through GitHub.** A scenario is a repository, or a folder in the hub's
  repository for a small one. The web site presents them and nobody signs in to it; a scenario
  is made and run on a developer's machine.
- **Scenario pages are written by hand, with help.** Each project has a long page that tells its
  story. These pages are expected to become highly customised, probably as custom HTML, because
  different audiences need different presentations.
- **Language models do much of the writing.** They generate both the scenarios and the pages
  that present them. That will probably continue for some time before there is a conversational
  interface.

## Where the project stands

The scenarios so far are listed on the front page by workspace. What each model gets wrong is on
[Limits today](/docs/system/limits). The module kinds, by layer, are on the
[kinds page](/kinds). None of the current scenarios should be used to make a decision: they are
the discovery phase's exercises.
