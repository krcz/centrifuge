use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::bond::Bond;
use crate::dyn_bond::DynBond;
use crate::oxide::{BondVisitor, Oxide};
use crate::schema::Structure;
use crate::solvent::Solvent;

/// A basic named collection of dynamic bonds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalogue {
    pub items: HashMap<String, DynBond>,
}

impl Catalogue {
    pub fn new() -> Self {
        Self {
            items: HashMap::new(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            items: HashMap::with_capacity(capacity),
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.items.contains_key(key)
    }

    pub fn get(&self, key: &str) -> Option<&DynBond> {
        self.items.get(key)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut DynBond> {
        self.items.get_mut(key)
    }

    pub fn insert(&mut self, key: String, value: DynBond) -> Option<DynBond> {
        self.items.insert(key, value)
    }

    pub fn remove(&mut self, key: &str) -> Option<DynBond> {
        self.items.remove(key)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &DynBond)> {
        self.items.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&String, &mut DynBond)> {
        self.items.iter_mut()
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.items.keys()
    }

    pub fn values(&self) -> impl Iterator<Item = &DynBond> {
        self.items.values()
    }
}

impl Default for Catalogue {
    fn default() -> Self {
        Self::new()
    }
}

impl std::iter::FromIterator<(String, DynBond)> for Catalogue {
    fn from_iter<T: IntoIterator<Item = (String, DynBond)>>(iter: T) -> Self {
        Self {
            items: iter.into_iter().collect(),
        }
    }
}

impl Extend<(String, DynBond)> for Catalogue {
    fn extend<T: IntoIterator<Item = (String, DynBond)>>(&mut self, iter: T) {
        self.items.extend(iter);
    }
}

impl IntoIterator for Catalogue {
    type Item = (String, DynBond);
    type IntoIter = std::collections::hash_map::IntoIter<String, DynBond>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl<'a> IntoIterator for &'a Catalogue {
    type Item = (&'a String, &'a DynBond);
    type IntoIter = std::collections::hash_map::Iter<'a, String, DynBond>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

impl<'a> IntoIterator for &'a mut Catalogue {
    type Item = (&'a String, &'a mut DynBond);
    type IntoIter = std::collections::hash_map::IterMut<'a, String, DynBond>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter_mut()
    }
}

impl Oxide for Catalogue {
    fn schema() -> Bond<Structure> {
        Structure::record([(
            "items",
            Structure::map(Bond::new(Structure::Unicode), DynBond::schema()),
        )])
    }

    fn visit_bonds(&self, visitor: &mut dyn BondVisitor) {
        for item in self.items.values() {
            item.visit_bonds(visitor);
        }
    }

    fn dissolve_in(&self, solvent: &Solvent) -> Self {
        Self {
            items: self
                .items
                .iter()
                .map(|(k, v)| (k.clone(), v.dissolve_in(solvent)))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::MemoryStore;

    #[test]
    fn catalogue_roundtrip_and_persist() {
        let solvent = Solvent::new();
        let store = MemoryStore::new();

        let mut items = HashMap::new();
        items.insert(
            "greeting".to_string(),
            DynBond::from_typed(Bond::new("hello".to_string())),
        );

        let catalogue = Catalogue { items };
        let bytes = catalogue.to_bytes();
        let recovered: Catalogue = Oxide::from_bytes(&bytes).unwrap();
        assert!(recovered.items.contains_key("greeting"));

        let cell = solvent.add(catalogue);
        let (_value_cid, _schema_cid) = solvent.persist_cell(&cell, &store).unwrap();
    }

    #[test]
    fn catalogue_map_like_api() {
        let mut catalogue = Catalogue::new();
        assert!(catalogue.is_empty());

        let value = DynBond::from_typed(Bond::new("hello".to_string()));
        assert!(catalogue.insert("greeting".to_string(), value).is_none());
        assert_eq!(catalogue.len(), 1);
        assert!(catalogue.contains_key("greeting"));
        assert!(catalogue.get("greeting").is_some());
        assert_eq!(catalogue.keys().count(), 1);
        assert_eq!(catalogue.values().count(), 1);
        assert_eq!(catalogue.iter().count(), 1);
        assert_eq!(catalogue.iter_mut().count(), 1);

        let removed = catalogue.remove("greeting");
        assert!(removed.is_some());
        assert!(catalogue.is_empty());
    }

    #[test]
    fn catalogue_std_map_traits() {
        let one = (
            "one".to_string(),
            DynBond::from_typed(Bond::new("1".to_string())),
        );
        let two = (
            "two".to_string(),
            DynBond::from_typed(Bond::new("2".to_string())),
        );
        let three = (
            "three".to_string(),
            DynBond::from_typed(Bond::new("3".to_string())),
        );

        let mut catalogue: Catalogue = vec![one, two].into_iter().collect();
        assert_eq!(catalogue.len(), 2);

        catalogue.extend(vec![three]);
        assert_eq!(catalogue.len(), 3);

        assert_eq!((&catalogue).into_iter().count(), 3);
        assert_eq!((&mut catalogue).into_iter().count(), 3);
        assert_eq!(catalogue.into_iter().count(), 3);
    }
}
