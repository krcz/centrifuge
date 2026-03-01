# Import / Export

These fixtures check typed import, typed export, and CID computation.

- `mpv-reduction-bench.yaml`
  Direct YAML fixture for a Meerwein-Ponndorf-Verley reduction bench setup.
  Use `schema/reaction-bench.schema.yamlld`.
- `mpv-reduction-bench.full.jsonld`
  JSON-LD fixture for the same value, using explicit `$value` wrappers.
  Use `schema/reaction-bench.schema.yamlld`.
- `mpv-reduction-bench.dyn-bond.yamlld`
  YAML-LD direct fixture for a self-describing `DynBond` pointing at the same
  MPV bench root CID. Use `schema/dyn-bond.schema.yamlld`.
- `reflux-loop.canonical.yaml`
  Canonical YAML fixture for a cyclic reflux procedure, using ligation.
  Use `schema/procedure-wheel.schema.yamlld`.
