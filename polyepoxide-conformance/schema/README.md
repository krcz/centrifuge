# Schema

These fixtures exercise schema import/export directly.

- `reaction-bench.schema.yamlld`
  Canonical YAML-LD export of the `ReactionBench` schema.

  Expected root bond CID:
  `bafyr4ihste65lvydigb5qwuv4ljcpn32crdlpbny52vqawc6jjq5lrmgye`

  Expected schema CID:
  `bagaybqabdyqaeegsrwbr3zyxq2rv7iztnos5xawfxaf6gheugc3gaegai7tdykq`

- `procedure-wheel.schema.yamlld`
  Canonical YAML-LD export of the `ProcedureWheel` schema.

  Expected root bond CID:
  `bafyr4iexlq4cqpmipq4xktahosewtrhucfzvrnh3lq5mjh2tku4wkxdruq`

  Expected schema CID:
  `bagaybqabdyqaeegsrwbr3zyxq2rv7iztnos5xawfxaf6gheugc3gaegai7tdykq`

- `dyn-bond.schema.yamlld`
  Canonical YAML-LD export of the `DynBond` schema.

  Expected root bond CID:
  `bafyr4igxca66cm65tqyuhhictd43o2l3omrfg4zepnf33s7kg7p6spbqge`

  Expected schema CID:
  `bagaybqabdyqaeegsrwbr3zyxq2rv7iztnos5xawfxaf6gheugc3gaegai7tdykq`

- `structure-schema.yamlld`
  Canonical YAML-LD export of `Structure::schema()` itself. The corresponding
  test verifies the exported text first and then both canonical identifiers:
  the root bond CID and the `Structure` schema CID.

  In this specific self-describing case, the root bond CID is the same as
  `Structure::schema()` CID, so both checks use the same identifier while still
  validating two distinct roles.

  Expected root bond CID:
  `bagaybqabdyqaeegsrwbr3zyxq2rv7iztnos5xawfxaf6gheugc3gaegai7tdykq`

  Expected `Structure` schema CID:
  `bagaybqabdyqaeegsrwbr3zyxq2rv7iztnos5xawfxaf6gheugc3gaegai7tdykq`
