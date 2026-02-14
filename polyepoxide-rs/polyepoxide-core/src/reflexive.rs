use cid::{Cid, multihash::Multihash};
use multihash_codetable::{Code, MultihashDigest};
use serde::{Deserialize, Serialize};

use crate::bond::{Bond, ErasedBond};
use crate::oxide::{BondVisitor, DAG_CBOR_CODEC, Oxide};
use crate::{IntType, Solvent, Store, Structure};

/// Internal multicodec used for Polyepoxide reflexive references.
pub const POLYEPOXIDE_REFLEXIVE_CODEC: u64 = 0x300001;
/// Multihash code for identity hashes.
pub const MULTIHASH_IDENTITY: u64 = 0x00;

/// Connectivity node for reflexive references.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ligation {
    /// Establishes scope and entry-point (index 0).
    Ligase(Vec<ErasedBond>),
    /// Resolves to scope[index] at traversal time.
    Slot(u16),
}

impl Oxide for Ligation {
    fn schema() -> crate::Bond<Structure> {
        Structure::tagged([
            ("Ligase", Structure::sequence(Structure::Cid)),
            ("Slot", Bond::new(Structure::Int(IntType::U16))),
        ])
    }

    fn visit_bonds(&self, visitor: &mut dyn BondVisitor) {
        if let Ligation::Ligase(args) = self {
            for arg in args {
                arg.visit_bonds(visitor);
            }
        }
    }

    fn dissolve_in(&self, solvent: &Solvent) -> Self {
        match self {
            Ligation::Ligase(args) => Ligation::Ligase(
                args.iter()
                    .map(|arg| solvent.add_erased_bond(arg))
                    .collect(),
            ),
            Ligation::Slot(index) => Ligation::Slot(*index),
        }
    }
}

/// Returns true if the CID uses identity multihash.
pub fn is_identity_cid(cid: &Cid) -> bool {
    cid.hash().code() == MULTIHASH_IDENTITY
}

/// Returns true if the CID uses the reflexive multicodec.
pub fn is_reflexive_cid(cid: &Cid) -> bool {
    cid.codec() == POLYEPOXIDE_REFLEXIVE_CODEC
}

/// Re-encodes a CID with a different multicodec while preserving multihash.
pub fn with_codec(cid: &Cid, codec: u64) -> Cid {
    Cid::new_v1(codec, *cid.hash())
}

/// Converts reflexive CID to DAG-CBOR CID.
pub fn reflexive_to_data_cid(cid: &Cid) -> Cid {
    with_codec(cid, DAG_CBOR_CODEC)
}

/// Converts DAG-CBOR CID to reflexive CID.
pub fn data_to_reflexive_cid(cid: &Cid) -> Cid {
    with_codec(cid, POLYEPOXIDE_REFLEXIVE_CODEC)
}

/// Builds a CID using identity multihash for the provided raw bytes.
///
/// Identity CIDs embed the payload directly in the multihash digest.
pub fn make_identity_cid(codec: u64, bytes: &[u8]) -> Result<Cid, cid::multihash::Error> {
    let hash = Multihash::<64>::wrap(MULTIHASH_IDENTITY, bytes)?;
    Ok(Cid::new_v1(codec, hash))
}

/// Creates a reflexive slot CID with identity multihash payload.
pub fn slot_cid(slot: u16) -> Cid {
    let bytes = Ligation::Slot(slot).to_bytes();
    make_identity_cid(POLYEPOXIDE_REFLEXIVE_CODEC, &bytes)
        .expect("slot ligation should fit identity multihash")
}

/// Creates a non-identity reflexive CID for a ligase argument list.
pub fn ligase_cid(args: Vec<ErasedBond>) -> Cid {
    let bytes = Ligation::Ligase(args).to_bytes();
    let hash = Code::Blake3_256.digest(&bytes);
    Cid::new_v1(POLYEPOXIDE_REFLEXIVE_CODEC, hash)
}

/// Computes a reflexive CID for a ligation value.
pub fn ligation_cid(ligation: &Ligation) -> Cid {
    match ligation {
        Ligation::Slot(slot) => slot_cid(*slot),
        Ligation::Ligase(args) => ligase_cid(args.clone()),
    }
}

/// Parses a `Ligation` from DAG-CBOR bytes.
pub fn parse_ligation_bytes(bytes: &[u8]) -> Option<Ligation> {
    Ligation::from_bytes(bytes).ok()
}

/// Resolves a possibly reflexive CID to a concrete target CID and resulting scope.
///
/// For non-reflexive CIDs, the original CID and scope are returned.
/// For reflexive CIDs, ligation data is loaded from identity digest bytes or store.
/// Invalid ligation payloads return `Ok(None)`.
pub fn resolve_reflexive_with_store<S: Store>(
    store: &S,
    cid: Cid,
    scope: &[Cid],
) -> Result<Option<(Cid, Vec<Cid>)>, S::Error> {
    if !is_reflexive_cid(&cid) {
        return Ok(Some((cid, scope.to_vec())));
    }

    let ligation = if is_identity_cid(&cid) {
        parse_ligation_bytes(cid.hash().digest())
    } else {
        let data_cid = reflexive_to_data_cid(&cid);
        let Some(bytes) = store.get(&data_cid)? else {
            return Ok(None);
        };
        parse_ligation_bytes(&bytes)
    };

    Ok(resolve_ligation(ligation, scope))
}

/// Resolves ligation semantics using erased bonds.
pub fn resolve_ligation_bond(
    ligation: Option<Ligation>,
    scope: &[ErasedBond],
) -> Option<(ErasedBond, Vec<ErasedBond>)> {
    let ligation = ligation?;
    match ligation {
        Ligation::Ligase(args) => {
            let entry = args.first()?.clone();
            Some((entry, args))
        }
        Ligation::Slot(index) => scope
            .get(index as usize)
            .cloned()
            .map(|target| (target, scope.to_vec())),
    }
}

/// Resolves ligation semantics to concrete CIDs.
///
/// This helper is kept for schema/IPLD traversal code that tracks CID-only scope.
pub fn resolve_ligation(ligation: Option<Ligation>, scope: &[Cid]) -> Option<(Cid, Vec<Cid>)> {
    let scope_bonds: Vec<_> = scope.iter().copied().map(ErasedBond::from_cid).collect();
    let (target, next_scope) = resolve_ligation_bond(ligation, &scope_bonds)?;
    Some((
        target.cid(),
        next_scope.into_iter().map(|bond| bond.cid()).collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_identity_roundtrip() {
        let cid = slot_cid(7);
        assert!(is_reflexive_cid(&cid));
        assert!(is_identity_cid(&cid));

        let parsed = parse_ligation_bytes(cid.hash().digest()).unwrap();
        assert_eq!(parsed, Ligation::Slot(7));
    }

    #[test]
    fn ligase_resolution() {
        let a = crate::compute_cid(b"a");
        let b = crate::compute_cid(b"b");

        let resolved = resolve_ligation(
            Some(Ligation::Ligase(vec![
                ErasedBond::from_cid(a),
                ErasedBond::from_cid(b),
            ])),
            &[],
        )
        .unwrap();
        assert_eq!(resolved.0, a);
        assert_eq!(resolved.1, vec![a, b]);
    }

    #[test]
    fn out_of_range_slot_is_none() {
        let a = crate::compute_cid(b"a");
        assert!(resolve_ligation(Some(Ligation::Slot(1)), &[a]).is_none());
    }
}
