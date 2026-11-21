//! Declarative tweak catalog: TOML definitions plus validation.

mod validate;

use include_dir::{Dir, include_dir};
use serde::{Deserialize, Serialize};

use crate::model::{Hive, Level, Preset, RegData, Section, StartType, ValueKey};
pub use validate::validate;

static TWEAKS_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../tweaks");

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum AllUsers {
    Always,
    Optional,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActionDef {
    Registry {
        hive: Hive,
        path: String,
        name: String,
        /// `None` means delete the value.
        #[serde(default)]
        value: Option<RegData>,
    },
    Service {
        name: String,
        start: StartType,
    },
    ScheduledTask {
        path: String,
        enabled: bool,
    },
    Appx {
        package: String,
        all_users: AllUsers,
        #[serde(default)]
        reinstall: Option<String>,
    },
    PowerPlan {
        scheme_guid: String,
        #[serde(default)]
        duplicate_from: Option<String>,
    },
    PowerShell {
        /// Prints `1` when the tweak is applied.
        check: String,
        apply: String,
        undo: String,
    },
}

impl ActionDef {
    /// The state this action touches. Registry, service and task names are
    /// case-insensitive on Windows, so they are lowercased here.
    pub fn key(&self, tweak_id: &str, index: usize) -> ValueKey {
        match self {
            ActionDef::Registry { hive, path, name, .. } => ValueKey::Registry {
                hive: *hive,
                path: path.to_lowercase(),
                name: name.to_lowercase(),
            },
            ActionDef::Service { name, .. } => ValueKey::Service { name: name.to_lowercase() },
            ActionDef::ScheduledTask { path, .. } => ValueKey::Task { path: path.to_lowercase() },
            ActionDef::Appx { package, .. } => ValueKey::Appx { package: package.clone() },
            ActionDef::PowerPlan { .. } => ValueKey::PowerScheme,
            ActionDef::PowerShell { .. } => ValueKey::Script {
                tweak_id: tweak_id.to_string(),
                index,
            },
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct TweakDef {
    pub id: String,
    pub title: String,
    pub description: String,
    pub section: Section,
    pub level: Level,
    #[serde(default)]
    pub laptop_level: Option<Level>,
    #[serde(default)]
    pub presets: Vec<Preset>,
    #[serde(default)]
    pub min_build: Option<u32>,
    #[serde(default)]
    pub max_build: Option<u32>,
    #[serde(default)]
    pub irreversible: bool,
    pub actions: Vec<ActionDef>,
}

impl TweakDef {
    pub fn is_per_user(&self) -> bool {
        self.actions.iter().any(|a| {
            matches!(
                a,
                ActionDef::Registry { hive: Hive::Hkcu, .. } | ActionDef::Appx { .. }
            )
        })
    }
}

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum CatalogError {
    #[error("failed to parse {file}: {msg}")]
    Parse { file: String, msg: String },
    #[error("duplicate tweak id `{0}`")]
    DuplicateId(String),
    #[error("tweaks `{a}` and `{b}` both modify {key:?}")]
    OverlappingValue { key: ValueKey, a: String, b: String },
    #[error("tweak `{0}` has min_build greater than max_build")]
    BadBuildRange(String),
    #[error("tweak `{0}` has no actions")]
    EmptyActions(String),
    #[error("tweak `{0}` has an empty title or description")]
    EmptyText(String),
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct Catalog {
    pub tweaks: Vec<TweakDef>,
}

impl Catalog {
    fn from_named<'a>(
        items: impl IntoIterator<Item = (String, &'a str)>,
    ) -> Result<Catalog, CatalogError> {
        let mut tweaks = Vec::new();
        for (file, src) in items {
            let t: TweakDef = toml::from_str(src).map_err(|e| CatalogError::Parse {
                file,
                msg: e.to_string(),
            })?;
            tweaks.push(t);
        }
        Ok(Catalog { tweaks })
    }

    #[cfg(test)]
    pub fn from_strs(srcs: &[&str]) -> Result<Catalog, CatalogError> {
        Self::from_named(
            srcs.iter()
                .enumerate()
                .map(|(i, s)| (format!("input {i}"), *s)),
        )
    }

    /// Catalog embedded at build time from `tweaks/*.toml`, skipping `_*` files.
    pub fn bundled() -> Catalog {
        let mut files: Vec<_> = TWEAKS_DIR
            .files()
            .filter(|f| {
                let name = f.path().file_name().and_then(|n| n.to_str()).unwrap_or("");
                name.ends_with(".toml") && !name.starts_with('_')
            })
            .collect();
        files.sort_by_key(|f| f.path().to_path_buf());
        let c = Self::from_named(files.iter().map(|f| {
            (
                f.path().display().to_string(),
                f.contents_utf8().unwrap_or(""),
            )
        }))
        .expect("bundled tweak catalog must parse");
        let errs = validate(&c);
        assert!(errs.is_empty(), "bundled tweak catalog is invalid: {errs:?}");
        c
    }

    pub fn get(&self, id: &str) -> Option<&TweakDef> {
        self.tweaks.iter().find(|t| t.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = include_str!("../../../tweaks/_example.toml");

    fn tweak(id: &str, extra: &str, actions: &str) -> String {
        format!(
            "id = \"{id}\"\ntitle = \"T\"\ndescription = \"D\"\nsection = \"remove\"\nlevel = \"safe\"\n{extra}\n{actions}"
        )
    }

    const HKCU_X: &str = "[[actions]]\ntype = \"registry\"\nhive = \"hkcu\"\npath = 'Software\\X'\nname = \"Y\"\nvalue = { dword = 1 }\n";

    #[test]
    fn parses_example_tweak() {
        let c = Catalog::from_strs(&[EXAMPLE]).unwrap();
        let t = c.get("example-tweak").unwrap();
        assert_eq!(t.level, Level::Safe);
        assert_eq!(t.actions.len(), 2);
        assert!(validate(&c).is_empty());
    }

    #[test]
    fn registry_value_omitted_means_delete() {
        let src = tweak(
            "a",
            "",
            "[[actions]]\ntype = \"registry\"\nhive = \"hklm\"\npath = 'S\\X'\nname = \"Y\"\n",
        );
        let c = Catalog::from_strs(&[&src]).unwrap();
        assert!(matches!(
            &c.tweaks[0].actions[0],
            ActionDef::Registry { value: None, .. }
        ));
    }

    #[test]
    fn bundled_works_and_skips_example() {
        let c = Catalog::bundled();
        assert!(c.get("example-tweak").is_none());
        assert!(validate(&c).is_empty());
    }

    #[test]
    fn validate_rejects_duplicate_ids() {
        let a = tweak("a", "", HKCU_X);
        let b = tweak(
            "a",
            "",
            "[[actions]]\ntype = \"service\"\nname = \"S\"\nstart = \"manual\"\n",
        );
        let c = Catalog::from_strs(&[&a, &b]).unwrap();
        assert_eq!(validate(&c), vec![CatalogError::DuplicateId("a".into())]);
    }

    #[test]
    fn validate_rejects_overlapping_registry_value() {
        let a = tweak("a", "", HKCU_X);
        let b = tweak("b", "", &HKCU_X.replace("Software\\X", "SOFTWARE\\x"));
        let c = Catalog::from_strs(&[&a, &b]).unwrap();
        let errs = validate(&c);
        assert_eq!(errs.len(), 1);
        assert!(matches!(
            &errs[0],
            CatalogError::OverlappingValue { a, b, .. } if a == "a" && b == "b"
        ));
    }

    #[test]
    fn validate_rejects_min_greater_than_max() {
        let a = tweak("a", "min_build = 22000\nmax_build = 19000", HKCU_X);
        let c = Catalog::from_strs(&[&a]).unwrap();
        assert_eq!(validate(&c), vec![CatalogError::BadBuildRange("a".into())]);
    }

    #[test]
    fn validate_rejects_empty_description_or_actions() {
        let no_desc = tweak("a", "", HKCU_X).replace("description = \"D\"", "description = \" \"");
        let no_actions = tweak("b", "actions = []", "");
        let c = Catalog::from_strs(&[&no_desc, &no_actions]).unwrap();
        assert_eq!(
            validate(&c),
            vec![
                CatalogError::EmptyText("a".into()),
                CatalogError::EmptyActions("b".into())
            ]
        );
    }

    #[test]
    fn per_user_detection() {
        let user = Catalog::from_strs(&[&tweak("a", "", HKCU_X)]).unwrap();
        assert!(user.tweaks[0].is_per_user());
        let sys = Catalog::from_strs(&[&tweak(
            "b",
            "",
            "[[actions]]\ntype = \"service\"\nname = \"S\"\nstart = \"manual\"\n",
        )])
        .unwrap();
        assert!(!sys.tweaks[0].is_per_user());
    }

    #[test]
    fn parse_error_reports_file() {
        assert!(matches!(
            Catalog::from_strs(&["not = valid"]),
            Err(CatalogError::Parse { .. })
        ));
    }
}
