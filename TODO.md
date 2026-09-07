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
**Done** [Coding style](docs/Coding%20style.md) updated.

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


[X] Transfer field-cad's ADR about document model of the kagami.

**Done** Field CAD's document mechanics (atomic validated commit, checkpoint
undo, interactive-edit bracketing, the versioned document and its durable write
protocol) are adopted, and the decisions of its that do *not* survive this
repository's ADRs are recorded alongside them, in
[ADR 0019](./docs/adr/0019-kagami-experiment-document-model.md). Implementation
is the [Kagami capability programme](./docs/tasks/kagami/README.md).


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

Perfect. Its time to create a proper implementation roadmap. Maybe place it in the ./docs/roadmap/ Outline milestones, focus areas and exit criteria etc. Pay attention to which work is shared between orishu and kagami and which can be done in parallel. Ideally multiple agents will be working in parallel to implement tasks so its important to annotate interdependencies.


[X] I'd like you to start implementation with implementing swarm model, that will be held and update by each orishu-worker.
The implementation should be in the style of [SANS-IO library design](../swe-llm-wiki/wiki/networking/sans-io-protocol-architecture.md) and in accordance with [TEA](./docs/Coding%20style.md) concept of IO separation from the functional core. That is - a crate should provide type to represent a set of node's peers, and `update(msg) -> tribe_model` that set in response to network originated messages as per [p2p protocol](./docs/protocol-p2p.md)

**Promoted** to [Implement the sans-IO cluster membership core](./docs/tasks/implement-membership-model.md), governed by [ADR 0013](./docs/adr/0013-cluster-formation-and-node-identity.md). Named `orishu-membership` rather than "swarm"/"tribe" — see task file for rationale. ADR 0015 governs committed-artifact transfer and is intentionally outside this membership task.


/goal Implement the sans-IO cluster membership crate for orishu-worker. The design as a task file at @docs/tasks/implement-membership-model.md
Don't forget to add bench similar to @crates/orishu-variables/benches to profile library performance in isolation.

Do adhere to the TEA design principles and SANS-IO library design and coding guidelines as outlined in @docs/Coding style.md
According to the @docs/roadmap/readme.md this task depend only on S-IDENTITY so you'll need to implement missing parts yourself.

[X] Looking at the roadmap file - I'd like to be able to see the roadmap as a mermaid timeline diagram with stages/tasks
not dates. and task/steps completion registry. Can you crate a new file in docs/roadmap as to not pollute existing roadmap
readme. Will it an report artifact?


[]
As an intermediate step I want to create another crate, salvaging from field-cad prototype, to implement kagami's particle/template catalog. The field-cad's ADR is /home/soultaker/workspace/field-cad/docs/adr/0019-generic-particle-catalog-is-data.md and the task is 
/home/soultaker/workspace/field-cad/docs/tasks/server-authoritative-catalog.md

I'd like you to create a new task in docs/tasks that an agent can take and work independently to bring this new crate with all of this projects ADRs in mind, Noting that field-cad version was a PoC and is different form what we plan in this project. Main difference being - instantiation of a template in field-cad creates a link and document embedding. If a template later changed, the instance may update. In this project, instantiating a template, just creates a materialized version
without linkage, because catalog is own by authoring client and two different clients might have different catalogs. 
Even more, orishu cluster will have no catalog at all - only workload bundle. Thus, anything from client catalog must be in the bundle. One extra consideration when designing this - is the role of variables. Template value can and will use variables.
They also define and export variable into user's namespace. The use case: as a user I want to define 'planets.yml' - my catalog of planets and later I want to create an object with mass half of the Sun: `new_object.mass = planets.sun.mass / 2`.
In a way - doing so will make a link from the document object to the catalog. In this new design this link is transitive via capture of exported variables. I guess we need an ADR to describe that first, user-story in kagami second, and only then a task.

Does it make sense to you? What did I miss in use cases? Ask me questions if you need clarification. If not - lets capture all this first.

> It seems like the agent slightly mis-understood the original field-cad design. 

As an intermediate step I want to create another crate, salvaging from field-cad prototype, to implement kagami's particle/template catalog. The field-cad's ADR is /home/soultaker/workspace/field-cad/docs/adr/0019-generic-particle-catalog-is-data.md and the task is 
/home/soultaker/workspace/field-cad/docs/tasks/server-authoritative-catalog.md

In my example the catalog is a file resource, as defined by field-cad. 
For example: /home/soultaker/workspace/field-cad/etc/catalogs/planets.yaml defines a set of templates with properties (mass, charge etc) when instantiated they become 'objects' that orishu-kagami simulate. 
Note that objects in this sense are ECS entities - a named collection of properties. This is what kagami models - a set of entities with properties contributed by plugins (systems in ESC terminology) Property value (charge) can be defined using an expression and constitutes in essence a variable definition: catalogs["planets"].templates['sun'].spec.components['fieldcad.mass-source'].mass
(or something similar)

The guiding principle for orishu-kagami is that a catalog is a collection of templates - similar in function to idTech 4's entity definition.

Pleas ensure that ARD is consistent.
To address your identified gaps from the original field-cad design: 
1. There is no explicit export of properties. Catalog file is loaded into catalog system. Each value property on a template is exported into a variable system with namespace of the catalog.template_name
I intend to use namespace for visibility boundary with explicit private/public modifiers.
2. Catalogs are expected to be sharable between user - by copying files. If a value expression in a catalog can not be resolved - it means it depends on a value defined outside of the catalog. We either need to prohibit it - but we can't really as users can modify value expressions in the exported files. Thus we need to diagnose those and warn user that some entries contain errors and can not be instantiated. This is the common issues with the catalog - when a user denies new template, they specify what component an template will have. But component are continued by plugins. Thus catalog authored on one kagami instance may have a different set of 'known' properties from another instance its loaded in. The ADR in field-cad had options: don't allow instantiation of such templates with unknown properties if a user don't have corresponding plugins; or allow, insatiate and link properties to the plugin later on - when a plugin is loaded.
3. Original field-cad adr called to maintain a link from instance to the catalog entry. In orishu-kagami we want to just use variables, not the instance-template link model. With variables closure captured when exporting bundled workload.

Please address the adr gaps and update the user story accordingly.

[X] /goal Pick up a task docs/tasks/implement-kagami-object-catalog.md
Base your work on prior PoC implementation in /home/soultaker/workspace/field-cad/crates/fieldcad-catalog but me mindful that new design have significant differences in the way instance is linked back to the template.
Adhere to the docs/Coding style.md and don't forget to add bench similar to @crates/orishu-variables/benches to profile library performance in isolation.
For data example use /home/soultaker/workspace/field-cad/etc/catalogs/planets.yaml and /home/soultaker/workspace/field-cad/etc/catalogs/particles.yaml

[X] Extract crates/orishu/src/model/manifest.rs into a shared crate
A number of resources in orishu as well as in kagami are representable as human readable/editable text files. As such we often follow k8s resource style definition format with version, kind, meta and spec.
Examples include: kagami-catalog - a list of such resources defining templates for simulated objects. On dist they are represented following k8s-style resources. 
Similarly, when using `orishuctl` a user is expected to interact with cluster definition and individual cluster resources as k8s style resources (at least when printed out or saved into files)
crates/orishu/src/model/manifest.rs already defines basic structs to support such resource definitions. The goal of this task is to refactor existing resource usage and use a common crate (perhaps: orishu-resource) that will be shared library for both kagami and orishu to define read and manipulate resources as k8s definitions.
Related decision has been documented in original orishu repository in '/home/soultaker/workspace/orishu/backlog/decisions/decision-005 - Cluster-manifest-is-a-synthetic-resource-rather-than-a-user-authored-durable-object.md' which I believe is informative in our case.

**Promoted** to [Extract shared resource crate](docs/tasks/extract-shared-resource-envelope.md), governed by [ADR 0013](./docs/adr/0013-cluster-formation-and-node-identity.md).


[X] These are important finds and I'd like you to act on it.
1. Do create docs/install.md with the proposed format documenting currently available install options as well as planned ones event though we currently don't publish required artifacts.
2. Do document in the relevant milestone to support publishing required artifacts as documented and create a proper task in ./docs/task with details.
3. Most importantly, do address rust toolchain version drift. We do targe 1.97 (current) to dockerfile, release workflow and dev guide all should be updated.
4. do fix etc/systemd/orishu-worker.service:3 link
5. Debian metadata fix and README conflict should be documented to be re-visited as part of the release milestone. No need for decision which version is correct now.
6. please update  etc/orishu-worker.conf reference

And I will fix README's license badge ref.


Product motivation and data-parallel philosophy, Scaling targets etc do deserve its own full-body documents. But README is the first point of entry for potential users - they need to know that these concepts are covered and front and center for orishu. Worth having a brief mention with a link to the full docs.
Same goes for hostile-input and architecture diagram. It is simple and not exact but gives readers a high level mental model of what the moving parts are:
there are no viewer and studio, but there are clients (orishuctl, orishu-monitor and Kagami) that are used in various roles. Especially kagami - to craft the simulation/experiment (that's what old docs called 'studio') and to watch results - viewers. a simple mermaid diagram of thous roles goes a long way and worth including in the README


Let me know if I haven't cover anything else and lets update docs/capture this info/fix issues.

[] Objective: transfer relevant functionality from existing PoC: field-cad to build a new version of the Field-CAD called kagami in this monorepo.
Note the task is not to simply copy-paste code. This monorepo has a number of ARDs that you must check first that might be different from PoC decisions made while prototyping field-cad.

In particular, this repo already transferred variables package (which was initially prototyped but not used in field-cad). Field-cad is using fieldcad-expressions crate which is a PoC solving the same problem: a CAD user expects to be able to use math expression when defining properties of the objects they work with. For example to enter: 'position.x = 13/ 4 + 1'. Expressions crate solved that well but the implementation needed cleanup. 
Do use new implementation in this repo - crates/orishu-variables - as a generic variables system with namespace etc. It allows plugins, catalog templates etc to define variables which users can use through out UI.
For example, a gravity system plugin will export a global const gravity const (G) which user will be able to use through out the app.
Note also, that design objective for kagami to be a user facing UI to craft/view/modify an experiment. Maybe even preview results using local runtime. But for details experiments, to submit the simulation job the orishu cluster.

Similarly, we already transferred the catalog but changed the ARD. Catalog instantiated object are not directly linked to the instance, but only indirectly via variables system. 

Note also that not all of the implemented field-cad user stories have been captured in this repo ./docs/user-stories/kagami So some you'll need to infer from the implementation of field-cad.
Start with defining a stand-alone crate to be used by kagami as a document model (in the TEA model+actions).
Then define a document server - as a sole mechanism to create / modify scene document - either from user UI actions or via MCP. (keep in mind MCP - UI equivalence; which is also implemented in field-cad) 

Create a plan with tasks breakdown first. I anticipate the work will take a few sessions thus keeping plan persistent as a collection of task an agent can take on is important.
place it under ./docs/tasks/kagami and link to existing tasks, adrs, [roadmap](./docs/roadmap/README.md) where it makes sense.

Field-CAD source code: ../field-cad
You are free to explore it.

Do adhere to the TEA design principles and SANS-IO library design and coding guidelines as outlined in @docs/Coding style.md
Note that another agent is actively working in this repo on the orishu-worker PoC. Your work is unlikely to intersect but its something to be mindful of if using workspace scoped cargo commands.

**Promoted** to the [Kagami capability programme](./docs/tasks/kagami/README.md):
thirteen ordered, independently assignable tasks (K1–K13) covering the sans-IO
experiment model crate, the document server, persistence, catalog
instantiation, app adoption, and the missing user stories. The decisions —
including where Field CAD's prototype is deliberately superseded — are recorded
in [ADR 0019](./docs/adr/0019-kagami-experiment-document-model.md).


[x] User stories for Kagami. Promoted to the
[Kagami stories](./docs/user-stories/kagami/README.md), ADRs
[0020](./docs/adr/0020-compose-object-behaviour-through-plugin-components.md)–[0022](./docs/adr/0022-persist-default-view-outside-experiment-intent.md),
and K7–K13 in the [Kagami capability programme](./docs/tasks/kagami/README.md).

Original request retained for provenance:
Is it clear from the body of work to recreate field-cad capabilities in this repo as Kagami that:
- a user can creates simulated objects/entries by composition. Component are contributed by kagami plugins.
Dynamics contributes position, velocity, acceleration (impulse/forces), while gravity plugin contributes gravity field, gravity coupling charge - gravitational mass etc.
- A view dialog allows user to select switch Orthographic/Perspective projection mode, selectable from the view control.
- field-cad essentially has two main classes of entities: modeled objects (particles and fields) and probs/sensors. particles moves, fields evolve, while probs/sensors don't change the simulation but record or visualize value (such as fields using flow lines or vectors).
- Probs can be attached to particles and camera can follow objects as well.
- sensors values can be queries via MCP to make it possible for agent to 'sense' the scene simulated.
- we have added a type of simulated entity - Particle emitter. Its entity like any other, if dynamics attached to it - it will move dynamically. If its coupled to a field it will obey field flows. It can also spawn other object from the catalog (multiple types with a preset frequency and capacity).
- Will we include ability to see particle's trails - recorded history of positions that makes for an interactive visualization? 

etc.

Do we have corresponding user stories in docs/user-stories/kagami ?

Does it change the tasks or the plan of moving field-cad capabilities over?


--- 
Composition of objects from plugin contributed components (although in practice it will be instantiation of templates) - is the core of the design that touches how orishu simulates objects behavior as well as how kagami 'edits' experiments. We need to capture that in key docs.

Perspective/Orthographic projection toggle is client side only, but it is preserved in the saved files as to not to confuse the user. We do need a user story to write it as a feature that something user can/able to do in the app.

Simulated objects / probs - again, core part of the design. Needs a clear user story.
Same: attachment of prob to object and camera following an object - is a user story.

Do clarify that reading sensor values via MCP is a user story. It is a feature that an agent can use to 'sense' the scene simulated.

Particle emitters is where design shines: catalog and object creation by compositions combined. It is a valuable feature that proved its worth in field-cad and we must not lose it. It is a user story.

Particle trails is a visualization feature that is nice to have but not essential. It was fiddly to add to the field-cad and we should not lose it. It is a user story.

Field vectors and flow lines - essential for visualizing fields. It is a user story.


I guess if authoring and viewport behaviors are currently left in the follow-up, we at least can close some of the gaps as to not loose this information.

undo and redo refusal while a simulation runs - this requires a bit more consideration. Lets say for now that when a simulation is submitted to run - it's a new mode - observation (replay) where no experiment modification is accepted. If a user wants to modify initial experiment - it is allowed, but will in effect switch a user from 'watching results unfold' to the initial condition editing. This should be very clear from UI - which mode the user is in. So from that perspective - 'refusal to undo' is a UI feature "no undo button while simulation is playing; stop first - which will take you to the initial scene, then undo".
Important to clarify this nuance in our design docs.

I overall agree with your recommended domain clarification with the above comments.

I totally agree that its time to clarify dynamics and gravity. This decision already been made in field-cad PoC and I guess its worth repeating here.
Just as you have proposed: position and velocity are indeed intrinsic components of an particle (entity ESC modeled by kagami). A user need to add 'dynamics' component to have inertial mass, impulse and forces accumulating; Impulse drives object's velocity and consequently position;
Adding field integration component (electrostatic, not just gravity) - provides updates to (drives) entity forces (and thus whole dynamics). So thus the system composes!

You are spot on with 'Particle emitters are a major addition'. That's why I mentioned it. Do update the design as you proposed to include it. This means we do need to embed emmitable definitions into workload and the whole instantiation machinery is to be shared with orishu runtime.

Please update the stories: add new/amend existing to capture the above. And update the tasks accordingly.

And do Update the plan accordingly.
---
[X]
answers:
1. We will return to it later.
2. Agreed with your recommendation. remove persisted motion authority. An object without Dynamics is kinematic/static during a run; an object with Dynamics is integrated.
3. Agreed with recommended resolution.
4. That's a major point of entire app and indeed must be clearly documented.
Fields are - plugin owned state for the domain. An experiment author defines a domain (like 1x2x1 box) and choses fields that will be modeled in this 'box': electro-magnetic, gravitational or something else contributed by custom plugin. An extended example is hydrodynamic model - where 'field' is a flow of real medium. Key property of fields - they are defined for each point in the domain. Thus sensors - prob the values. 
Note that plugins contribute a computational model, how fields are computed. Key example is Coulomb (electrostatic) and Maxwell/Yee (electrodynamic) - do model the same 'electromagnetic' field, coupled using the same 'property' on objects - charge. But they compute that fields differently and can't be used together. The same for gravity: we aim to ship two models of gravity: classical and GEM (Gravitoelectromagnetism).
Thus, plugin bundles more then just a field definition, it also bundles update method expressed as code/kernel.

5. Indeed, the proposed mode is how field-cad implements it.
Related inconsistency is noted. Think about this way. orishu produces a new snapshot of the whole experimental setup: positions of all objects, forces on them etc. It also produces field values. In case of maxwell solver or GEM - its a value in each point of the domain. A simple client will receive entire field snapshot. 
It is an optimization to subscribe to only receive update in a region where sensors are. 
Yes - a user cap specify preference etc. 

Totally agree that real invariant should be that observers cannot affect scientific state or block simulation commit. Need to write it down.

6. Suggestion accepted.
7. Good point - M3 is the right place for field visualization feature.
8. This uncovers an interesting gap indeed. Lets consider two scenarios form user's perspective:
 - As a user I open a previously authored file - my new experiment. I move camera, change perspective etc. If I close the app and re-open it - I expect my view to show exactly where I left: perspective and view and all. SO that tells us that view revisions do mark doc dirty.
 - A user opening computed result - they see initial experiment and start 'playback'. Things fly around, user can move camera etc. There is no changes to the original stream - if another user opens it - they will start from the same initial position where it was. That's because in playback mode - a client is not editing anything (that includes the view). Its viewing a server owned data stream from different angles. 
9. Agreed.

Please update planning inconsistencies that can be corrected directly and then lets try to settle above decisions with answers provided, except for #1. We should revisit #1 after your update.

---

[X] You've been working bringing field-cad, the PoC part of the experiment authoring UI into this repo as kagami. This is not a simple copy-paste, because orushi made a few decisions differently (or closely) learning from PoCs.
In the previous session you've create a list of task here docs/tasks/kagami, which had been refined since it was originally written. Please pick from K1/K3 follow-up.

---
- [X] `orishu-monitor`: implement the API-independent TUI shell

Done. The shell is implemented in @apps/orishu-monitor: Overview/Members with a
Help overlay, an owned terminal session guard, non-TTY refusal, bounded event
polling, and headless render tests. It has no `orishu` dependency and connects
to no worker. Live integration remains backlog under
@docs/tasks/integrate-orishu-monitor-operator-api.md.

The original assignment follows, as written.

---

Implement the complete bounded shell slice specified in
@docs/tasks/implement-orishu-monitor-admin-tui.md. The current placeholder is
@apps/orishu-monitor, whose binary only prints `Hello, world!`.

## Product context

`orishuctl` is the scriptable, one-shot administration client;
`orishu-monitor` is its interactive admin-TUI counterpart. The eventual product
goal is story-level operator parity between them, except that the TUI must not
reveal or print raw formation-admission secrets. **That is the destination, not
the scope of this slice.** This task delivers only the terminal shell described
by the linked task.

Read @README.md, @CONTEXT.md, @docs/architecture.md,
@docs/Coding style.md, and the linked task before editing. Treat source and
tests as authoritative for current behavior. @docs/protocol-client.md explains
the future boundary, but do not implement or alter that protocol in this slice.
The follow-up is tracked separately in
@docs/tasks/integrate-orishu-monitor-operator-api.md; it is backlog context, not
additional scope for this assignment.

The sibling project `../avahi-tui` (Kinjo) may be inspected as a reference for
proven terminal lifecycle, layout, input, and headless-test techniques. Adapt
ideas deliberately to this repository's terminology and guidelines; do not
bulk-copy its architecture or bring its configurable-keybinding feature into
scope. k9s is product inspiration, not an API or code template.

## Required scope

- Replace the placeholder with the full-screen Overview/Members shell and Help
  overlay defined by the task.
- Keep terminal IO in a thin shell and navigation/state transitions in a
  deterministic model/message/update core that tests can exercise without a
  terminal.
- Implement safe terminal acquisition/restoration, non-TTY refusal, bounded
  idle event polling, resize/minimum-size behavior, documented keys, honest
  integration-unavailable states, theme tokens, and headless render tests.
- Use maintained TUI dependencies appropriate to the workspace; the expected
  choice is `ratatui` plus `crossterm`. Keep dependency changes minimal and
  reproducible. Remove the direct `orishu` dependency if it is unused.
- Update @apps/orishu-monitor/README.md and package metadata so they describe
  exactly what this slice implements.

## Hard boundaries

- Do **not** connect to a worker or use @crates/orishu/src/client merely because
  its legacy methods exist. Do not add host, credential, refresh, polling,
  streaming, or mutation behavior. Those require later tasks after the owning
  worker routes and public projections stabilize.
- Do not add `--config`, `--log-level`, `-H`/`--host`, user-configurable
  shortcuts, or a speculative shared client-options abstraction. Preserve
  `--help` and `--version`; shared client CLI/config design belongs to the
  live-integration pass.
- Do not modify @apps/orishu-worker, @apps/orishu-ctl, the client protocol, or
  shared Orishu domain models for this task.
- Another agent is actively changing formation and worker code. Preserve all
  existing worktree changes and prefer package-scoped commands while iterating.

If an implementation choice is costly to reverse and the task or repository
guidance does not settle it, ask with concrete options and implications.
Otherwise make the smallest in-scope choice and continue.

## Verification and handoff

Format only the package while iterating, then run at minimum:

```sh
cargo fmt -p orishu-monitor -- --check
cargo clippy --locked -p orishu-monitor --all-targets -- -D warnings
cargo test --locked -p orishu-monitor --all-targets
make docs-check
```

Also run the task's workspace formatting check before handoff if concurrent
changes permit it; report unrelated failures rather than rewriting another
agent's files. Manually smoke-test the real TUI in a terminal, including Help,
both sections, resize/minimum-size behavior, `q`, `Ctrl-C`, and terminal
restoration. If the environment cannot provide a TTY, say so explicitly and
rely on the required headless tests. At handoff, list files changed, dependency
choices, checks run, manual verification, and any failure that predates this
task.
