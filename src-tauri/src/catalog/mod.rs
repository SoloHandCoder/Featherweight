//! Declarative tweak catalog: TOML definitions plus validation.
#![allow(dead_code)]

mod validate;

use include_dir::{Dir, include_dir};
use serde::{Deserialize, Serialize};

use crate::model::{Hive, Level, Preset, RegData, Section, StartType, ValueKey};
#[allow(unused_imports)]
pub use validate::validate;

static TWEAKS_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../tweaks");

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum AllUsers {
    Always,
    Optional,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
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
    /// `type = "powershell"` (the spec's name, not serde's `power_shell`).
    #[serde(rename = "powershell")]
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
#[serde(deny_unknown_fields)]
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
    /// Another tweak that changes the same state and can't be on at the same
    /// time (e.g. two power plans). Must be mutual; the UI turns the other
    /// one off when this one is switched on.
    #[serde(default)]
    pub exclusive_with: Option<String>,
    /// Overrides the per-user detection, for tweaks whose effect on the
    /// signed-in account can't be seen from their actions (PowerShell).
    #[serde(default)]
    pub per_user: Option<bool>,
    pub actions: Vec<ActionDef>,
}

impl TweakDef {
    pub fn is_per_user(&self) -> bool {
        if let Some(p) = self.per_user {
            return p;
        }
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
    #[error("tweak `{0}` has an exclusive_with that is not a mutual pair with another tweak")]
    BadExclusive(String),
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct Catalog {
    pub tweaks: Vec<TweakDef>,
}

impl Catalog {
    fn from_files<'a>(
        items: impl IntoIterator<Item = (String, &'a [u8])>,
    ) -> Result<Catalog, CatalogError> {
        let mut tweaks = Vec::new();
        for (file, bytes) in items {
            let src = match std::str::from_utf8(bytes) {
                Ok(s) => s,
                Err(e) => {
                    return Err(CatalogError::Parse { file, msg: format!("not valid UTF-8: {e}") });
                }
            };
            let t: TweakDef = toml::from_str(src).map_err(|e| CatalogError::Parse {
                file,
                msg: e.to_string(),
            })?;
            tweaks.push(t);
        }
        Ok(Catalog { tweaks })
    }

    pub fn from_strs(srcs: &[&str]) -> Result<Catalog, CatalogError> {
        Self::from_files(
            srcs.iter()
                .enumerate()
                .map(|(i, s)| (format!("input {i}"), s.as_bytes())),
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
        Self::from_files(files.iter().map(|f| (f.path().display().to_string(), f.contents())))
            .expect("bundled tweak catalog must parse")
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

    #[test]
    fn non_utf8_file_is_parse_error() {
        let bytes: &[u8] = b"id = \"a\xff\"\n";
        let r = Catalog::from_files([("bad.toml".to_string(), bytes)]);
        match r {
            Err(CatalogError::Parse { file, msg }) => {
                assert_eq!(file, "bad.toml");
                assert!(msg.contains("UTF-8"), "{msg}");
            }
            other => panic!("expected parse error, got {other:?}"),
        }
    }

    #[test]
    fn powershell_action_type_is_named_powershell() {
        let src = tweak(
            "a",
            "",
            "[[actions]]\ntype = \"powershell\"\ncheck = \"c\"\napply = \"a\"\nundo = \"u\"\n",
        );
        let c = Catalog::from_strs(&[&src]).unwrap();
        assert!(matches!(&c.tweaks[0].actions[0], ActionDef::PowerShell { .. }));
    }

    #[test]
    fn unknown_tweak_field_is_rejected() {
        let src = tweak("a", "irreversable = true", HKCU_X);
        assert!(matches!(Catalog::from_strs(&[&src]), Err(CatalogError::Parse { .. })));
    }

    #[test]
    fn unknown_action_field_is_rejected() {
        let src = tweak(
            "a",
            "",
            "[[actions]]\ntype = \"service\"\nname = \"S\"\nstart = \"manual\"\nstrat = \"disabled\"\n",
        );
        assert!(matches!(Catalog::from_strs(&[&src]), Err(CatalogError::Parse { .. })));
    }

    #[test]
    fn example_registry_value_is_dword_zero() {
        let c = Catalog::from_strs(&[EXAMPLE]).unwrap();
        let t = c.get("example-tweak").unwrap();
        assert!(matches!(
            &t.actions[0],
            ActionDef::Registry { value: Some(RegData::Dword(0)), .. }
        ));
    }

    const POWER: &str = "[[actions]]\ntype = \"power_plan\"\nscheme_guid = \"8c5e7fda-e8bf-4a96-9a85-cf2dd2b47a5c\"\n";

    #[test]
    fn validate_allows_overlap_between_mutually_exclusive_tweaks() {
        let a = tweak("a", "exclusive_with = \"b\"", POWER);
        let b = tweak("b", "exclusive_with = \"a\"", POWER);
        let c = Catalog::from_strs(&[&a, &b]).unwrap();
        assert!(validate(&c).is_empty(), "{:?}", validate(&c));
    }

    #[test]
    fn validate_rejects_one_sided_exclusivity() {
        let a = tweak("a", "exclusive_with = \"b\"", POWER);
        let b = tweak("b", "", POWER);
        let c = Catalog::from_strs(&[&a, &b]).unwrap();
        let errs = validate(&c);
        assert!(errs.contains(&CatalogError::BadExclusive("a".into())), "{errs:?}");
        // The one-sided pair also still counts as an overlap.
        assert!(errs.iter().any(|e| matches!(e, CatalogError::OverlappingValue { .. })));
    }

    #[test]
    fn validate_rejects_exclusive_with_unknown_or_self() {
        let a = tweak("a", "exclusive_with = \"nope\"", HKCU_X);
        let b = tweak("b", "exclusive_with = \"b\"", "[[actions]]\ntype = \"service\"\nname = \"S\"\nstart = \"manual\"\n");
        let c = Catalog::from_strs(&[&a, &b]).unwrap();
        assert_eq!(
            validate(&c),
            vec![CatalogError::BadExclusive("a".into()), CatalogError::BadExclusive("b".into())]
        );
    }

    #[test]
    fn validate_rejects_overlap_with_third_tweak_even_if_exclusive_pair() {
        let a = tweak("a", "exclusive_with = \"b\"", POWER);
        let b = tweak("b", "exclusive_with = \"a\"", POWER);
        let x = tweak("x", "", POWER);
        let c = Catalog::from_strs(&[&a, &b, &x]).unwrap();
        let errs = validate(&c);
        assert!(
            errs.iter().any(|e| matches!(e, CatalogError::OverlappingValue { b, .. } if b == "x")),
            "{errs:?}"
        );
    }

    // ----------------------------------------------------- bundled v1 catalog

    const V1_IDS: &[&str] = &[
        "remove-junk-apps",
        "block-suggested-app-installs",
        "disable-copilot",
        "disable-recall",
        "disable-widgets",
        "no-bing-in-start",
        "no-start-ads",
        "no-lockscreen-ads",
        "no-explorer-ads",
        "remove-onedrive",
        "remove-xbox-apps",
        "disable-telemetry-services",
        "disable-telemetry-tasks",
        "disable-unused-services",
        "disable-print-spooler",
        "disable-bluetooth-support",
        "power-high-performance",
        "power-ultimate-performance",
        "game-mode-on",
        "game-bar-recording-off",
        "mouse-accel-off",
        "network-throttling-off",
        "system-responsiveness",
    ];

    #[test]
    fn bundled_catalog_validates() {
        let errs = validate(&Catalog::bundled());
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn bundled_catalog_has_v1_ids() {
        let c = Catalog::bundled();
        let missing: Vec<_> = V1_IDS.iter().filter(|id| c.get(id).is_none()).collect();
        assert!(missing.is_empty(), "missing: {missing:?}");
        let extra: Vec<_> = c.tweaks.iter().filter(|t| !V1_IDS.contains(&t.id.as_str())).map(|t| &t.id).collect();
        assert!(extra.is_empty(), "unexpected: {extra:?}");
    }

    #[test]
    fn bundled_sections_levels_and_presets_match_brief() {
        use Level::*;
        let c = Catalog::bundled();
        let both: &[Preset] = &[Preset::Everyday, Preset::Gaming];
        let expect: &[(&str, Section, Level, Option<Level>, &[Preset])] = &[
            ("remove-junk-apps", Section::Remove, Safe, None, both),
            ("block-suggested-app-installs", Section::Remove, Safe, None, both),
            ("disable-copilot", Section::Remove, Safe, None, both),
            ("disable-recall", Section::Remove, Safe, None, both),
            ("disable-widgets", Section::Remove, Safe, None, both),
            ("no-bing-in-start", Section::Remove, Safe, None, both),
            ("no-start-ads", Section::Remove, Safe, None, both),
            ("no-lockscreen-ads", Section::Remove, Safe, None, both),
            ("no-explorer-ads", Section::Remove, Safe, None, both),
            ("remove-onedrive", Section::Remove, Advanced, None, &[]),
            ("remove-xbox-apps", Section::Remove, Advanced, None, &[]),
            ("disable-telemetry-services", Section::Background, Safe, None, both),
            ("disable-telemetry-tasks", Section::Background, Safe, None, both),
            ("disable-unused-services", Section::Background, Safe, None, both),
            ("disable-print-spooler", Section::Background, Advanced, None, &[]),
            ("disable-bluetooth-support", Section::Background, Advanced, None, &[]),
            ("power-high-performance", Section::Gaming, Safe, Some(Advanced), &[Preset::Gaming]),
            ("power-ultimate-performance", Section::Gaming, Advanced, None, &[]),
            ("game-mode-on", Section::Gaming, Safe, None, &[Preset::Gaming]),
            ("game-bar-recording-off", Section::Gaming, Safe, None, &[Preset::Gaming]),
            ("mouse-accel-off", Section::Gaming, Safe, None, &[Preset::Gaming]),
            ("network-throttling-off", Section::Gaming, Advanced, None, &[]),
            ("system-responsiveness", Section::Gaming, Advanced, None, &[]),
        ];
        for (id, section, level, laptop, presets) in expect {
            let t = c.get(id).unwrap_or_else(|| panic!("missing {id}"));
            assert_eq!(t.section, *section, "{id} section");
            assert_eq!(t.level, *level, "{id} level");
            assert_eq!(t.laptop_level, *laptop, "{id} laptop_level");
            assert_eq!(t.presets, presets.to_vec(), "{id} presets");
        }
        assert_eq!(c.get("disable-recall").unwrap().min_build, Some(26100));
    }

    #[test]
    fn xbox_never_in_gaming_preset() {
        let c = Catalog::bundled();
        assert!(c.get("remove-xbox-apps").unwrap().presets.is_empty());
        for t in c.tweaks.iter().filter(|t| t.presets.contains(&Preset::Gaming)) {
            for a in &t.actions {
                if let ActionDef::Appx { package, .. } = a {
                    assert!(
                        !package.to_lowercase().contains("xbox")
                            && !package.to_lowercase().contains("gamingapp"),
                        "{} removes {package} but is in the Gaming preset",
                        t.id
                    );
                }
            }
        }
    }

    #[test]
    fn irreversible_flags() {
        let c = Catalog::bundled();
        let irr = ["remove-junk-apps", "remove-onedrive", "remove-xbox-apps"];
        for t in &c.tweaks {
            assert_eq!(t.irreversible, irr.contains(&t.id.as_str()), "{}", t.id);
        }
    }

    #[test]
    fn exclusive_with_is_symmetric() {
        let c = Catalog::bundled();
        let hp = c.get("power-high-performance").unwrap();
        let up = c.get("power-ultimate-performance").unwrap();
        assert_eq!(hp.exclusive_with.as_deref(), Some("power-ultimate-performance"));
        assert_eq!(up.exclusive_with.as_deref(), Some("power-high-performance"));
        for t in &c.tweaks {
            if let Some(o) = &t.exclusive_with {
                let other = c.get(o).unwrap_or_else(|| panic!("{} names unknown {o}", t.id));
                assert_eq!(other.exclusive_with.as_deref(), Some(t.id.as_str()));
            }
        }
    }

    #[test]
    fn everyday_implies_gaming_and_gaming_section_not_everyday() {
        for t in &Catalog::bundled().tweaks {
            if t.presets.contains(&Preset::Everyday) {
                assert!(t.presets.contains(&Preset::Gaming), "{} is everyday but not gaming", t.id);
            }
            if t.section == Section::Gaming {
                assert!(!t.presets.contains(&Preset::Everyday), "{} is a Gaming tweak in everyday", t.id);
            }
        }
    }

    #[test]
    fn presets_only_contain_safe_reversible_or_confirmed_tweaks() {
        for t in &Catalog::bundled().tweaks {
            if !t.presets.is_empty() {
                assert_eq!(t.level, Level::Safe, "{} is in a preset but not safe", t.id);
            }
        }
    }

    #[test]
    fn bundled_descriptions_are_short_and_plain() {
        for t in &Catalog::bundled().tweaks {
            let d = t.description.trim();
            let sentences = d
                .char_indices()
                .filter(|(i, ch)| {
                    matches!(ch, '.' | '!' | '?')
                        && d[i + 1..].chars().next().is_none_or(char::is_whitespace)
                })
                .count();
            assert!((1..=2).contains(&sentences), "{}: {sentences} sentences", t.id);
            assert!(d.len() <= 240, "{}: description too long", t.id);
            assert!(!t.title.trim().is_empty());
        }
    }

    #[test]
    fn bundled_action_values_are_well_formed() {
        let pkg = regex::Regex::new(r"^[A-Za-z0-9._-]+$").unwrap();
        let guid = regex::Regex::new(
            r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$",
        )
        .unwrap();
        for t in &Catalog::bundled().tweaks {
            for a in &t.actions {
                match a {
                    ActionDef::Appx { package, reinstall, .. } => {
                        assert!(pkg.is_match(package), "{}: bad package {package}", t.id);
                        if let Some(r) = reinstall {
                            assert!(
                                r.starts_with("ms-windows-store://pdp/?ProductId="),
                                "{}: bad reinstall {r}",
                                t.id
                            );
                        }
                    }
                    ActionDef::PowerPlan { scheme_guid, duplicate_from } => {
                        assert!(guid.is_match(scheme_guid), "{}: {scheme_guid}", t.id);
                        if let Some(d) = duplicate_from {
                            assert!(guid.is_match(d), "{}: {d}", t.id);
                        }
                    }
                    ActionDef::ScheduledTask { path, .. } => {
                        assert!(path.starts_with('\\'), "{}: {path}", t.id);
                    }
                    ActionDef::Registry { path, .. } => {
                        assert!(!path.starts_with('\\') && !path.ends_with('\\'), "{}: {path}", t.id);
                        assert!(!path.to_uppercase().starts_with("HK"), "{}: {path}", t.id);
                    }
                    ActionDef::PowerShell { check, apply, undo } => {
                        assert!(!check.trim().is_empty() && !apply.trim().is_empty() && !undo.trim().is_empty());
                    }
                    ActionDef::Service { .. } => {}
                }
            }
        }
    }

    #[test]
    fn bundled_never_touches_defender_or_windows_update() {
        // Whole service names / registry, task and package path segments.
        let banned = [
            "windefend", "windows defender", "microsoft defender", "wdnissvc", "sense",
            "securityhealthservice", "wscsvc", "wuauserv", "windowsupdate", "usosvc",
            "waasmedicsvc", "bits", "dosvc", "updateorchestrator",
        ];
        let segments = |s: &str| -> Vec<String> {
            s.split('\\').filter(|p| !p.is_empty()).map(str::to_lowercase).collect()
        };
        for t in &Catalog::bundled().tweaks {
            for a in &t.actions {
                let names: Vec<String> = match a {
                    ActionDef::Registry { path, name, .. } => {
                        let mut v = segments(path);
                        v.push(name.to_lowercase());
                        v
                    }
                    ActionDef::Service { name, .. } => vec![name.to_lowercase()],
                    ActionDef::ScheduledTask { path, .. } => segments(path),
                    ActionDef::Appx { package, .. } => package.to_lowercase().split('.').map(String::from).collect(),
                    ActionDef::PowerPlan { .. } | ActionDef::PowerShell { .. } => vec![],
                };
                for n in &names {
                    assert!(!banned.contains(&n.as_str()), "{} touches {n}", t.id);
                }
            }
        }
    }

    #[test]
    fn per_user_override() {
        let svc = "[[actions]]\ntype = \"service\"\nname = \"S\"\nstart = \"manual\"\n";
        let forced = Catalog::from_strs(&[&tweak("a", "per_user = true", svc)]).unwrap();
        assert!(forced.tweaks[0].is_per_user());
        let off = Catalog::from_strs(&[&tweak("b", "per_user = false", HKCU_X)]).unwrap();
        assert!(!off.tweaks[0].is_per_user());
        let c = Catalog::bundled();
        let od = c.get("remove-onedrive").unwrap();
        assert_eq!(od.per_user, Some(true));
        assert!(od.is_per_user());
    }

    #[test]
    fn junk_list_excludes_packages_that_break_things() {
        let c = Catalog::bundled();
        let pkgs: Vec<&str> = c
            .get("remove-junk-apps")
            .unwrap()
            .actions
            .iter()
            .filter_map(|a| match a {
                ActionDef::Appx { package, .. } => Some(package.as_str()),
                _ => None,
            })
            .collect();
        for p in ["Microsoft.GetHelp", "Microsoft.MixedReality.Portal"] {
            assert!(!pkgs.contains(&p), "{p} must not be in the SAFE junk list");
        }
        assert!(pkgs.contains(&"king.com.CandyCrushSaga"));
    }
}
