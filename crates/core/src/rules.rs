//! The rules that turn processes into tenants, read from
//! `rules/windows.toml` so a wrong bill can be fixed without touching code.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

/// The rules file in the repository, built into the exe.
pub const BUILTIN: &str = include_str!("../../../rules/windows.toml");

/// What a helper image does (SPEC.md, From processes to tenants, rule 4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Helper {
    /// Follow the parent even if it is a launcher; Staff if the parent is.
    Parent,
    /// Join the tenant whose root has this image name, if it exists.
    Join(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rules {
    /// Image names that are always Staff.
    pub staff: BTreeSet<String>,
    /// Image names a root walk stops below.
    pub launchers: BTreeSet<String>,
    pub helpers: BTreeMap<String, Helper>,
    /// Display names by root image name.
    pub names: BTreeMap<String, String>,
    /// Company words dropped from the front of a FileDescription.
    pub companies: BTreeSet<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    staff: Vec<String>,
    launchers: Vec<String>,
    #[serde(default)]
    helpers: BTreeMap<String, String>,
    #[serde(default)]
    names: BTreeMap<String, String>,
    #[serde(default)]
    companies: Vec<String>,
}

impl Rules {
    /// Parses a rules file. Image names and company words are matched
    /// without regard to case.
    pub fn parse(text: &str) -> Result<Rules, String> {
        let file: File =
            toml::from_str(text).map_err(|e| format!("not a valid rules file: {e}"))?;
        let lower = |v: Vec<String>| v.into_iter().map(|s| s.to_lowercase()).collect();
        Ok(Rules {
            staff: lower(file.staff),
            launchers: lower(file.launchers),
            helpers: file
                .helpers
                .into_iter()
                .map(|(image, owner)| {
                    let owner = owner.to_lowercase();
                    let helper = if owner == "parent" {
                        Helper::Parent
                    } else {
                        Helper::Join(owner)
                    };
                    (image.to_lowercase(), helper)
                })
                .collect(),
            names: file
                .names
                .into_iter()
                .map(|(image, name)| (image.to_lowercase(), name))
                .collect(),
            companies: lower(file.companies),
        })
    }

    /// The rules file built into the exe.
    pub fn builtin() -> Rules {
        Rules::parse(BUILTIN).expect("rules/windows.toml is valid")
    }

    pub fn is_staff(&self, image: &str) -> bool {
        self.staff.contains(image)
    }

    pub fn is_launcher(&self, image: &str) -> bool {
        self.launchers.contains(image)
    }

    pub fn helper(&self, image: &str) -> Option<&Helper> {
        self.helpers.get(image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_builtin_rules_parse() {
        let rules = Rules::builtin();
        assert!(rules.is_staff("explorer.exe"));
        assert!(rules.is_launcher("steam.exe"));
        assert_eq!(rules.helper("conhost.exe"), Some(&Helper::Parent));
        assert_eq!(
            rules.helper("steamwebhelper.exe"),
            Some(&Helper::Join("steam.exe".into()))
        );
        assert_eq!(rules.names["code.exe"], "VS Code");
        assert!(rules.companies.contains("microsoft"));
        assert_eq!(rules.staff.len(), 17);
        assert_eq!(rules.launchers.len(), 13);
    }

    #[test]
    fn names_are_matched_without_case() {
        let rules = Rules::parse(
            "staff = [\"DWM.exe\"]\nlaunchers = []\n[helpers]\n\"Foo.exe\" = \"Bar.EXE\"\n",
        )
        .unwrap();
        assert!(rules.is_staff("dwm.exe"));
        assert_eq!(
            rules.helper("foo.exe"),
            Some(&Helper::Join("bar.exe".into()))
        );
    }

    #[test]
    fn a_broken_file_is_refused() {
        assert!(Rules::parse("staff = [").is_err());
        assert!(Rules::parse("staff = []\nlaunchers = []\nlanchers = []").is_err());
    }
}
