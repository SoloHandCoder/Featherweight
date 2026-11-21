use std::collections::{HashMap, HashSet};

use super::{Catalog, CatalogError, TweakDef};
use crate::model::ValueKey;

/// Whether `a` and `b` name each other in `exclusive_with`.
fn mutually_exclusive(a: &TweakDef, b: &TweakDef) -> bool {
    a.exclusive_with.as_deref() == Some(b.id.as_str())
        && b.exclusive_with.as_deref() == Some(a.id.as_str())
}

/// Pure structural validation of a catalog.
pub fn validate(c: &Catalog) -> Vec<CatalogError> {
    let mut errs = Vec::new();
    let mut ids: HashSet<&str> = HashSet::new();
    // Every tweak that touches a key, in catalog order.
    let mut owners: HashMap<ValueKey, Vec<&TweakDef>> = HashMap::new();

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
        if let Some(other) = &t.exclusive_with {
            let mutual = other != &t.id
                && c.tweaks.iter().any(|o| o.id == *other && mutually_exclusive(t, o));
            if !mutual {
                errs.push(CatalogError::BadExclusive(t.id.clone()));
            }
        }
        for (i, a) in t.actions.iter().enumerate() {
            let key = a.key(&t.id, i);
            let prev = owners.entry(key.clone()).or_default();
            // Report one overlap per (key, tweak): against the first earlier
            // owner this tweak isn't allowed to share the key with.
            if let Some(p) = prev
                .iter()
                .find(|p| p.id != t.id && !mutually_exclusive(p, t))
            {
                errs.push(CatalogError::OverlappingValue {
                    key,
                    a: p.id.clone(),
                    b: t.id.clone(),
                });
            }
            if !prev.iter().any(|p| p.id == t.id) {
                prev.push(t);
            }
        }
    }
    errs
}
