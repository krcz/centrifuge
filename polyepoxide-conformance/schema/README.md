# Schema

These fixtures exercise schema import/export directly.

- `reaction-bench.schema.yamlld`
  Canonical YAML-LD export of the `ReactionBench` schema.

  Expected root bond CID:
  `bafyr4ihste65lvydigb5qwuv4ljcpn32crdlpbny52vqawc6jjq5lrmgye`

  Expected schema CID:
  `bagaybqabdyqfbqtx2tf7642wediyugqzsu6nelflcebvpzkkktx7c4bgie3nm2i`

- `procedure-wheel.schema.yamlld`
  Canonical YAML-LD export of the `ProcedureWheel` schema.

  Expected root bond CID:
  `bafyr4iexlq4cqpmipq4xktahosewtrhucfzvrnh3lq5mjh2tku4wkxdruq`

  Expected schema CID:
  `bagaybqabdyqfbqtx2tf7642wediyugqzsu6nelflcebvpzkkktx7c4bgie3nm2i`

- `dyn-bond.schema.yamlld`
  Canonical YAML-LD export of the `DynBond` schema.

  Expected root bond CID:
  `bafyr4ictzhoxnj6goz3snxvrjv5oqjfxhuuxdnbitjwbw3fuxhwzqzsm4e`

  Expected schema CID:
  `bagaybqabdyqfbqtx2tf7642wediyugqzsu6nelflcebvpzkkktx7c4bgie3nm2i`

- `structure-schema.yamlld`
  Canonical YAML-LD export of `Structure::schema()` itself. The corresponding
  test verifies the exported text first and then both canonical identifiers:
  the root bond CID and the `Structure` schema CID.

  In this specific self-describing case, the root bond CID is the same as
  `Structure::schema()` CID, so both checks use the same identifier while still
  validating two distinct roles.

  Expected root bond CID:
  `bagaybqabdyqfbqtx2tf7642wediyugqzsu6nelflcebvpzkkktx7c4bgie3nm2i`

  Expected `Structure` schema CID:
  `bagaybqabdyqfbqtx2tf7642wediyugqzsu6nelflcebvpzkkktx7c4bgie3nm2i`
