# kagami-catalog

Kagami's editable object-template catalog: the format, the bounded loader, the
variable projection, guarded file writes, and the one catalog authority that UI
and MCP adapters submit commands to.

Decisions:
[ADR 0008](../../docs/adr/0008-catalog-templates-instantiate-self-contained-objects.md)
and
[ADR 0018](../../docs/adr/0018-catalog-values-are-captured-by-reference-not-linked.md).
Task: [Implement the Kagami object catalog](../../docs/tasks/implement-kagami-object-catalog.md).

A catalog entry is reusable authored data — a composition of
plugin-contributed components and their property values. It is never
executable, and no template name, catalog name, or file name selects solver
behaviour.

## The format

One file holds one or more `---`-separated documents. One catalog identity does
not imply one file, and one file may publish into several catalogs.

```yaml
apiVersion: kagami.catalog/v1
kind: ObjectTemplate
metadata:
  catalog: planets
  name: sun
  description: Sol
  annotations:
    source: NASA/JPL Planetary Fact Sheet
spec:
  parameters:
    scale:
      default: "1"
      description: Mass multiplier applied at instantiation
  helpers:
    solar_mass: {expression: "1.989e30", unit: kg}
  components:
  - type: {plugin: kagami.mass_sources, name: inertial_mass}
    properties:
      mass: {quantity: "planets.sun.solar_mass * planets.sun.scale"}
  - type: {plugin: kagami.geometry, name: sphere}
    properties:
      radius: {quantity: {expression: "6.9634e5", unit: km}}
```

- **`parameters`** are instantiation inputs. Each declares a default, so a
  template can always be previewed without inventing inputs for it.
- **`helpers`** are template-local intermediates, private unless declared
  `visibility: public`.
- **`components`** name plugin-qualified component types. A component may
  appear once per template, because its name is the namespace its properties
  are published under.
- **`properties`** are `{quantity: ...}`, `{boolean: ...}`, or `{text: ...}`.
  A quantity may be written as the shorthand `{quantity: "1.5"}` when it needs
  no unit or visibility annotation.

Instance identity, display name, and placement are deliberately absent: they
belong to the instantiation command, not to reusable content.

Every name — catalog, template, component, property, parameter, helper — must
be a valid variable-name segment (letters, digits, `_`; not starting with a
digit). `anti-proton` is refused at the document boundary, because a name like
that could never be referenced from an expression.

## Values are variables

Every expression-capable property, helper, and parameter is published into the
shared `orishu-variables` environment. There is no export list.

| Binding | Canonical name |
| --- | --- |
| property | `catalog.template.component.property` |
| helper | `catalog.template.helper` |
| parameter | `catalog.template.parameter` |

A property also resolves by the concise `catalog.template.property` when
exactly one binding claims it; an ambiguous spelling is reported, never broken
arbitrarily. Properties are public by default, helpers private by default.

Values are published in canonical SI. A `unit:` scales the authored magnitude,
so `{expression: "6.9634e5", unit: km}` publishes `6.9634e8`.

## Three states, and what each one means

| State | Meaning | How it is fixed |
| --- | --- | --- |
| `Available` | usable now | — |
| `Unavailable` | true of *this installation*: a plugin is missing, or a catalog it depends on is not loaded | install the plugin, or copy the other catalog |
| `Invalid` | a defect in the file, identical on every machine | edit the file |

One bad entry never hides its siblings, and nothing is ever repaired by
substituting a fabricated scientific value.

## Instantiation copies; it does not link

Materialising a template resolves the complete transitive closure of every
definition its expressions need, copies those definitions into object-local
identities, and rewrites every reference onto the copies. The resulting object
resolves with no catalog in scope at all.

It records the source catalog, template, document position, and content
fingerprint as *provenance* — historical evidence, not a pointer. There is no
tracking link, no propagation index, and no compare/apply path. Editing,
renaming, or deleting a template never changes an object already made from it.

## Writing is safe by construction

Writes are atomic (temporary file plus rename), conflict-checked against the
file digest the caller read, and sibling-preserving: editing one document in a
multi-document file splices only that document's text, so its neighbours keep
their comments.

Two rules make that safe against an adapter that asks for something hostile:

- **A name has one owner.** A create or a rename onto an identity another
  document already claims is refused *before* any file effect, so nothing on
  disk changes and neither the revision nor the command history advances. An
  invalid or unavailable entry still owns the identity its metadata names — the
  alternative would let repairing one file steal a name from another. A
  successful rename publishes both identities, so a cached view removes the row
  it retired and inserts the new one without a full refresh.
- **Every effect stays inside the root.** Target paths must be relative and
  carry a catalog extension, and containment is checked *physically*, never on
  a path assembled from the root and untrusted components. The configured root
  is canonicalised, then each requested component is proven in turn: it must be
  a real directory — a symbolic link is refused, not followed — whose canonical
  form still lies beneath the root, and the walk continues from that proven
  directory. A missing directory is created only below a component already
  proven, and removed again if a later step fails, so a rejected command
  creates nothing outside. A catalog file must be a regular file; a symlinked
  directory entry is reported rather than loaded, because the bytes behind it
  are not this catalog's to own. Temporary files are created exclusively under
  an unpredictable name, so one planted at a guessable path can be neither
  followed nor overwritten.

The race boundary is deliberate: these are path-based `std` operations, which
contain a *pre-existing* hostile link — what an untrusted MCP client can plant —
but not a process concurrently swapping a directory for a link between one
check and the next. Closing that would need directory-handle-relative,
no-follow operations, which this crate does not claim.

## Examples

[`etc/catalogs`](../../etc/catalogs) ships three catalogs, and the integration
tests load those exact files:

- `planets.yaml` — Solar System bodies (NASA/JPL).
- `particles.yaml` — standard-model particles (NIST CODATA 2022).
- `scenarios.yaml` — worked examples of parameters, private and public helpers,
  and a cross-catalog reference.

## Development

```sh
cargo test -p kagami-catalog --all-targets
cargo bench -p kagami-catalog --bench catalog
cargo run --release -p kagami-catalog --example profile_catalog --features dhat
```

The benchmark covers parsing, resolution, binding evaluation, and
instantiation over a dependency chain; the `dhat` example reports the
allocation cost of the same phases.
