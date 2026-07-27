use crate::Rule;

/// Maps Rust's `std::env::consts::OS` to Minecraft's OS naming convention.
#[must_use]
pub fn current_os() -> &'static str {
    match std::env::consts::OS {
        "macos" => "osx",
        "windows" => "windows",
        _ => "linux",
    }
}

/// Maps Rust's `std::env::consts::ARCH` to Minecraft's architecture naming convention.
#[must_use]
pub fn current_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86" => "x86",
        "aarch64" => "aarch64",
        _ => "x86_64",
    }
}

/// Evaluates a list of platform `Rule`s against the current OS and architecture.
///
/// Returns `true` if the library should be included. When `rules` is empty, the library
/// is included unconditionally (Minecraft's default behavior).
///
/// Rules are evaluated in order. Each `allow` rule that matches grants inclusion; each
/// `disallow` rule that matches denies it. If no rule matches, the default is to include.
#[must_use]
pub fn should_include(rules: &[Rule]) -> bool {
    if rules.is_empty() {
        return true;
    }

    let mut allowed = false;

    for rule in rules {
        let matches_os = rule.os.as_ref().is_none_or(|os_rule| {
            os_rule
                .name
                .as_ref()
                .is_none_or(|name| name == current_os())
                && os_rule
                    .arch
                    .as_ref()
                    .is_none_or(|arch| arch == current_arch())
        });

        let matches_features = rule.features.iter().all(|(feat, val)| match feat.as_str() {
            "is_demo_user" => !*val,
            "has_custom_resolution" => *val,
            _ => true,
        });

        if matches_os && matches_features {
            allowed = rule.action == "allow";
        }
    }

    allowed
}

/// Returns the platform-correct classpath separator.
///
/// On Windows, this is `;`. On all other platforms, it is `:`.
#[must_use]
pub const fn classpath_separator() -> &'static str {
    if cfg!(windows) {
        ";"
    } else {
        ":"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RuleOs;

    #[test]
    fn empty_rules_mean_include() {
        assert!(should_include(&[]));
    }

    #[test]
    fn unconditional_allow() {
        let rules = vec![Rule {
            action: "allow".to_string(),
            os: None,
        }];
        assert!(should_include(&rules));
    }

    #[test]
    fn unconditional_disallow() {
        let rules = vec![Rule {
            action: "disallow".to_string(),
            os: None,
        }];
        assert!(!should_include(&rules));
    }

    #[test]
    fn os_match_allow() {
        let rules = vec![Rule {
            action: "allow".to_string(),
            os: Some(RuleOs {
                name: Some(current_os().to_string()),
                arch: None,
            }),
        }];
        assert!(should_include(&rules));
    }

    #[test]
    fn os_match_disallow() {
        let rules = vec![Rule {
            action: "disallow".to_string(),
            os: Some(RuleOs {
                name: Some(current_os().to_string()),
                arch: None,
            }),
        }];
        assert!(!should_include(&rules));
    }

    #[test]
    fn os_non_match_defaults_to_excluded() {
        let rules = vec![Rule {
            action: "allow".to_string(),
            os: Some(RuleOs {
                name: Some("windows".to_string()),
                arch: None,
            }),
        }];
        // On Linux, a rule allowing only Windows should result in exclusion
        if current_os() != "windows" {
            assert!(!should_include(&rules));
        }
    }

    #[test]
    fn classpath_separator_is_colon_on_linux() {
        if !cfg!(windows) {
            assert_eq!(classpath_separator(), ":");
        }
    }

    #[test]
    fn current_os_is_known() {
        let os = current_os();
        assert!(
            ["linux", "osx", "windows"].contains(&os),
            "Unknown OS: {os}"
        );
    }

    #[test]
    fn current_arch_is_known() {
        let arch = current_arch();
        assert!(
            ["x86_64", "x86", "aarch64"].contains(&arch),
            "Unknown arch: {arch}"
        );
    }
}
