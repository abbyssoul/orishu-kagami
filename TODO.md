# Unordered project ideas

This file is an inbox for ideas that have not yet been refined or prioritized.
Review each idea before turning it into a tracked task under `docs/tasks/`, and
mark whether it was rejected or promoted.


## Initial prompt
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


## Ideas

[X] Document project distribution decisions
Somewhat contrary to the Unix philosophy, where a app should do only one thing (and do it well) - orishu is distributed as a single binary that does 3 main things: forms and maintains cluster, serves simulation results and runs live simulation. A single binary design is necessitated by requirements for ease of distribute, scheduling, update and maintenance. Key observation to inform such design: to perform any of 3 activities - requires an established cluster network. Cluster formation and maintenance - naturally a networking activity; serving of simulation segments - is a distributed storage activity. And running a simulation requires whole cluster coordination for planning and halo exchanges. All of that requires a cluster membership and composition to be available, at a point where those actions are performed.

Consequently for the rest of the binaries: all components must be self sufficient and run as a stand-alone binaries, with no extra directories, config files or setup.
Motivation: when installed via `cargo install <binary>` - not configs or scripts is installed on a system.

**DONE** Decision [ADR-0003](./docs/adr/0003-single-binary-distribution.md) 

[X] Update Coding style doc to capture wider role of TEA approach in the orishu and kagami design.
In particular ELM is not UI specific, its a logical cont. of IO separation. Having clear boundaries of where messages originate is the IO boundary (user input, network etc covered uniformly).
An example - peer list tracking system. Each node in a cluster holds its own model of a cluster, who the members are and what their state (live, suspected etc). Nodes exchange messages directly or piggyback on other traffic messages - updating cluster model. In the elm terms: there is cluster model and actions to update it originating from network message exchanges and/or timers. There is no UI view though.
**Done** [Coding style](docs/Coding style.md) updated.

[] Document that on multiple levels orishu project is client-server architecture from a client perspective. And peer-to-peer is how individually orishu workers self-organize.
Fundamentally, a server (orishu) is a producer of streams of simulated states. In that such stream can be pre-recorded or live-produced. Conceptually, serving a pre-recorded stream of previously computed simulation is not different from serving a file (video stream), maybe with provisions that a file might be significant in size and thus split into chunks like BitTorrent protocol would do.

A client - is a consumer of such stream of states. It connects to a cluster, initiates streaming, can pause etc. 

[~] Smooth render: Mental model for render vs simulation: Simulation is producing snapshots of the simulated 'universe' state. Renderer consumes a stream of such 
snapshots to display them. From that respective - they are producer and consumers.
Since sim produces snapshots at its own pace, renderer should interpolate (optional) state rendered between snapshots.
(Following Q3, Doom3 design)


[X] Right, so in the context of Kagami as a scene editor, one can model it as a client with a server, representing a document/scene being authored. Kagami internal (local) server authoritative about the state of the wold/experiment. The inputs: user commands, MCP server inputs as 'user' command but also - compute server (if any) stream of state updates. In case if a simulation has been submitted to a server (orishu or local) to simulate, the internal "document authoring server" accepts authoritative state updates from the external "more authoritative" server, right?
idTech 3 and above key design idea on the client side is that a client is a producer of commands that aim to modify server state.
Considering ELM design principles (and more broadly - IO separation shell vs functional/pure core) - a client is a producer of commands that are sent to the server. Server is authoritative about the state, if a command was/is accepted/rejected etc. In design of kagami - a user produces inputs with UI actions but also an MCP server that kagami-CAD might start - will also be producing a stream of command modifying currently edited document.

Server is authoritative about the state, if a command was/is accepted/rejected etc.
Note, that there are 2 command queues: client local state (camera position angles, selected objects etc) and server states modifying commands.
Ref: https://fabiensanglard.net/doom3/


[X] Move orishu user stories and setup user stories directories.
**Done** [User stories](./docs/user-stories/README.md)

[X] Move orishu's ADR about the simulation model that is advancing time (initial conditions) model.
> I've just moved a file from original orishu project folder and I'd like to adopt it as an ADR. docs/spatiotemporal-foundation.md notes fundamental decision of the entire orishu (and kagami) design - to be a solver initial value problem. We
>  might extend that in the future to boundary value problems - but its only after releasing a product capable of solving IVP.
**Done** ADR written and accepted. See [ADR 0002](./docs/adr/0002-initial-value-problem-scope.md)


[~] Transfer field-cad's ADR about document model of the kagami.


[X] Transfer field-cad's ADR about MCP/server and UI equivalence.
Kagami should (just like established in field-cad) - provide an alternative "interface" to authoring scenes and do everything else: via MCP.
From user's perspective there should be both option: open the app directly and create/inspect scene via keyboard and mouse or to tell agent to do the same. The agent will do that via MCP (or directly server API) commanding Kagami application.
Note that so support this, an app must either start with `--mcp` flag to enable MCP, or a user must first enable MCP in the UI. 
An extra user story is required to capture that as a user I can enable/disable MCP in the running app. I also should be able to see if any clients are connected to the MCP. If I disable the MCP server with clints still connected, I understand that those agents won't be able to perform their functions etc.

MCP server: The app should expose a REST API to allow external clients to drive design and control the simulation in the same way as user with UI would. In that sense the app may provide an alternative fully functional interface to create the same scene - with UI or API or both. The API should support creating, modifying, and deleting objects in the scene, as well as starting and stopping the simulation. Additionally, it should provide endpoints for retrieving the current state of the simulation, including the positions and properties of all objects.
So as a user I want to be able to control the simulation from an external client, such as a web interface or a mobile app or an AI agent. This will allow for more flexible and remote control of the authoring and simulation environment.
Anything that a user can do in the UI should be possible to do through the API, and the API should be designed to be intuitive and entity oriented, and easy to use for developers. The API should also provide clear error messages and documentation to help developers understand how to use it effectively.

**Done** Field CAD had recorded this decision in its architecture overview and
story inventory rather than as an ADR; it is now captured here as
[ADR 0006](./docs/adr/0006-mcp-ui-equivalence.md), with user stories in
[docs/user-stories/kagami/mcp.md](./docs/user-stories/kagami/mcp.md). The
original "REST API" wording is resolved as MCP over HTTP, as Field CAD did.

[X] Variables system
Another key concept that Kagami CAD and idTech has in common is a variable system. When authoring scenes user can input value (object positions, masses etc) as expressions "1/3 + 0.1".
But we also want to support defining variables and using them in expressions.
For example a user may define math constants or "mass_of_sun = 1e32 kg" and use it in expressions. That already has been prototyped in field-cad as  ../field-cad/crates/fieldcad-expressions but later refined into a ../field-cad/crates/fieldcad-variables - which hasn't been adopted yet.
Lets capture that we need such subsystem.

**Promoted** to [Migrate and integrate the shared variables subsystem](./docs/tasks/migrate-and-integrate-variables-subsystem.md), governed by [ADR 0005](./docs/adr/0005-author-numeric-values-as-unit-aware-expressions.md) and [ADR 0007](./docs/adr/0007-share-expression-semantics-with-workload-resources.md).
> Followup: Update user stories to capture that a user can define variables and use them in expressions. Don't forget the UI / MCP equivalence: a user/agent can define variables and use them in expressions via MCP as well.


[X] Another idTech similarity: kagami should allow user to create objects from an object catalog.
In essence - object catalog is a collection of files, defining templates with properties (supporting variables and expressions). When such template is instantiated, it becomes an object in the experiment (scene).
Note that catalog can be edited and is a property of kagami client, while instantiations of the catalog - are part of the persisted scene(experiment) file.
Also note: catalog can be edited/listed via MCP as well.
This is very much mirrors the concept of entity definition in `idDeclManager` in idTech 4

**Promoted** to [Implement the Kagami object catalog](./docs/tasks/implement-kagami-object-catalog.md),
governed by [ADR 0008](./docs/adr/0008-catalog-templates-instantiate-self-contained-objects.md).

[X] Treat submitted simulation code as a portable sandboxed workload program,
following the id Tech VM boundary with WebAssembly Component capabilities.

**Adopted** in the [workload contract](./docs/protocol-workload.md) and governed
by [ADR 0009](./docs/adr/0009-execute-workloads-as-sandboxed-portable-programs.md).


[X] Add a new persona: custom compute engineer producing custom solvers for physical phenomenons.
Its a good analysis found a real gap - a missing persona.
The initial idea is that in orishu-kagami world there are two major roles: 
 - cluster owners - people(agents) that provision compute resources, configure networking, install orishu workers and ensure workers form a cluster. For admin purposes these user rely on `orishuctl` or orishu-monitor tools.
 - researcher using kagami - they author physical experiments and use pre-configured cluster to run computations and get scientific results back. They use Kagami to author experiments that when submitted to the cluster become cluster workloads. They also use kagami to review results of the experiments.

Now, by design the whole system includes a degree of extensibility. The nature of physical experiment should be extendable and the method of experiment should be extendable also. This is similar to how advance players of a game tend to modify (to mod) a game to create new rules and situations. But all within the contracts of the game. In case of orishu - the rules that define the game - are computational kernels that compute physical properties over the space as a initial variable problem. Changing the kernel we can model electrodynamics, gravity of starts and galaxies - or hydrodynamics etc.

The choice of experiment is with kagami users, orishu provides compute platform. Kagami will be shipped with a pre-defined set of kernels for electrodynamics and gravity. But advanced user may expend it to other types of fields. That will require writing compute kernel - which is code. Coding is outside of kagami design because existing coding tools and IDEs are better suited for it. However, one such kernel is created it can be added to kagami and a scene using this kernel to compute the next state can be authored.

Does it make sense to you?
I've asked the product owner to update the user stories to capture that advanced users can author and manage custom plugins.
Lets ensure that this is clear from our readme, and other docs.

To answer your questions: 
1. we need to document a new persona like advanced user authoring kernels.
2. bundle - as in an full artifact containing scene, initial conditions and custom kernels (if any) - is produced by kagami, not by cluster admins (so not by `orishuctl`). Its literally should be an option in 'File > Export...' menu
3. installation via 'cargo install <>' is one of many options that will be available to orishu admins. Preferred option should be installation via platform native package manager: `apt install orishu-worker` or `brew install orishuctl`. But also 'docker run orishu-worker' and others. The reason we talk about Single-binary self-sufficiency at all - is because `cargo install <>` has this constraint - no config. Since we want to support cargo install as a method - we need to support running without initial config (maybe even on a system with no write access as a true portable executable).
Installation of orishu is in scope - yes.

Lets ensure the all of that is clear from our user-stories.

> the term kernel is a bit narrow as its only a part of the user story. What advance users really craft and manage - are plugins for kagami. A plugin includes manifest and defines what kind of physical phenomenon is modeled, exports variable etc. It also, when loaded, registers the code that simulates the phenomenon - kernel. So from kagami's users perspective they create, share and manage active plugins in kagami. When a scene/experiment is created - it records what phenomenon it models and bundles code to compute it when exported. This way runtime - orishu - can load the experiment bundle and run it independent of kagami instance that produced it. 
Update user stories accordingly