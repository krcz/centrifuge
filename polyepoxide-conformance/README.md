# Polyepoxide Conformance Fixtures

This directory contains external conformance fixtures used by
`polyepoxide-rs/polyepoxide-core/tests`.

The fixtures follow a small organic chemistry laboratory story:

- reagent bottles represent stockroom materials
- reaction benches represent configured work areas
- a reflux loop fixture exercises ligation-based procedure cycles
- the schema fixture captures the exported schema of `Structure` itself

The fixtures are grouped by the area they test:

- `import-export/`
- `solvent/`
- `sync/`
- `schema/`

Schema fixtures in `schema/` are used by the value fixtures in the other
directories. The conformance tests import those schema files first and then use
their root CIDs to interpret the value fixtures.

Most fixtures use plain YAML in `direct` mode. Fixtures that use ligation use
`canonical` mode. JSON-LD and YAML-LD fixtures are included to exercise the
import/export surface rather than to serve as the primary authoring format.
