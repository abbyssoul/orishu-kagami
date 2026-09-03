# Unordered project ideas

This file is an inbox for ideas that have not yet been refined or prioritized.
Review each idea before turning it into a tracked task under `docs/tasks/`, and
mark whether it was rejected or promoted.

## Ideas

Initial prompt:
> Objective: setup a new monorepo project moving/merging existing repos into a single monorepo.
The new monorepo will be called `orishu-kagami` and will combine the following projects:
- `orishu` - the main simulation engine and cluster management tool
- `field-cad` - the visualization and analysis tool for simulation results
- `kagami` - the new prototype for native UI for simulation control and visualization.

What to do:
1. Ensure the all the community/repo public docs for the new monorepo are properly setup, clear and concise, explaining the purpose of the monorepo and how to get started with each of the included projects.
2. Set up project structure for the monorepo: it means ./docs contains an overall project documentation.
Everything in the public docs applies to the whole project.

3. Set up proper CI/CD pipeline on day 1: 
`make build`, `make test` etc. 
Use best of ../orishu and ../avahi-tui  CI/CD pipeline as a baseline

[] Document project distribution decisions
Somewhat contrary to the Unix philosophy, where a app should do only one thing (and do it well) - orishu is distributed as a single binary that does 3 main things: forms and maintains cluster, serves simulation results and runs live simulation. A single binary design is necessitated by requirements for ease of distribute, scheduling, update and maintenance. Key observation to inform such design: to perform any of 3 activities - requires an established cluster network. Cluster formation and maintenance - naturally a networking activity; serving of simulation segments - is a distributed storage activity. And running a simulation requires whole cluster coordination for planning and halo exchanges. All of that requires a cluster membership and composition to be available, at a point where those actions are performed.

[] Update Coding style doc to capture wider role of TEA approach in the orishu and kagami design.
In particular ELM is not UI specific, its a logical cont. of IO separation. Having clear boundaries of where messages originate is the IO boundary (user input, network etc covered uniformly).
An example - peer list tracking system. Each node in a cluster holds its own model of a cluster, who the members are and what their state (live, suspected etc). Nodes exchange messages directly or piggyback on other traffic messages - updating cluster model. In the elm terms: there is cluster model and actions to update it originating from network message exchanges and/or timers. There is no UI view though.

[] Move orishu user stories and setup user stories directories.
[] Move orishu's ADR about the simulation model that is advancing time (initial conditions) model.