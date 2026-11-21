use std::collections::{HashMap, HashSet};

use super::{Catalog, CatalogError};
use crate::model::ValueKey;

/// Pure structural validation of a catalog.
pub fn validate(c: &Catalog) -> Vec<CatalogError> {
    let mut errs = Vec::new();
    let mut ids: HashSet<&str> = HashSet::new();
    let mut owners: HashMap<ValueKey, &str> = HashMap::new();

    for t in &c.tweaks {
        if !ids.insert(t.id.as_str()) {
            errs.push(CatalogError::DuplicateId(t.id.clone()));
        }
        if let (Some(min), Some(max)) = (t.min_build, t.max_build) {
            if min > max {
                errs.push(CatalogError::BadBuildRange(t.id.clone()));
            }
        }
        if t.actions.is_empty() {
            errs.push(CatalogError::EmptyActions(t.id.clone()));
        }
        if t.title.trim().is_empty() || t.description.trim().is_empty() {
            errs.push(CatalogError::EmptyText(t.id.clone()));
        }
        for (i, a) in t.actions.iter().enumerate() {
            let key = a.key(&t.id, i);
            match owners.get(&key) {
                Some(prev) if *prev != t.id => errs.push(CatalogError::OverlappingValue {
                    key: key.clone(),
                    a: (*prev).to_string(),
                    b: t.id.clone(),
                }),
                Some(_) => {}
                None => {
                    owners.insert(key, t.id.as_str());
                }
            }
        }
    }
    errs
}
