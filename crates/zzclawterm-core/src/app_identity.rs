use semver::Version;

/// Application identity is fixed by the binary version, not by user settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppFlavor {
    Stable,
    Preview,
}

impl AppFlavor {
    pub fn for_version(version: &Version) -> Self {
        if version.pre.is_empty() {
            Self::Stable
        } else {
            Self::Preview
        }
    }

    pub fn current() -> Self {
        Self::for_version(&Version::parse(env!("CARGO_PKG_VERSION")).expect("Cargo SemVer"))
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Stable => "ZzClawTerm",
            Self::Preview => "ZzClawTerm Preview",
        }
    }

    pub fn config_directory_name(self) -> &'static str {
        match self {
            Self::Stable => ".zzclawterm",
            Self::Preview => ".zzclawterm-preview",
        }
    }

    pub fn desktop_id(self) -> &'static str {
        match self {
            Self::Stable => "zzclawterm",
            Self::Preview => "zzclawterm-preview",
        }
    }

    pub fn macos_bundle_name(self) -> &'static str {
        match self {
            Self::Stable => "ZzClawTerm.app",
            Self::Preview => "ZzClawTerm Preview.app",
        }
    }

    pub fn application_identifier(self) -> &'static str {
        match self {
            Self::Stable => "com.kang.zzclawterm",
            Self::Preview => "com.kang.zzclawterm.preview",
        }
    }

    pub fn accepts_update(self, version: &Version) -> bool {
        self == Self::for_version(version)
    }
}

#[cfg(test)]
mod tests {
    use crate::app_identity::AppFlavor;
    use crate::updater::UpdateChannel;
    use semver::Version;

    #[test]
    fn semantic_versions_resolve_consistent_application_and_update_flavors() {
        for (text, flavor, channel) in [
            ("2.0.0", AppFlavor::Stable, UpdateChannel::Stable),
            ("2.0.1", AppFlavor::Stable, UpdateChannel::Stable),
            ("2.1.0+build.1", AppFlavor::Stable, UpdateChannel::Stable),
            (
                "2.0.0-preview.1",
                AppFlavor::Preview,
                UpdateChannel::Preview,
            ),
            (
                "2.0.0-preview.2",
                AppFlavor::Preview,
                UpdateChannel::Preview,
            ),
            ("2.0.0-beta.1", AppFlavor::Preview, UpdateChannel::Preview),
            ("2.0.0-rc.1", AppFlavor::Preview, UpdateChannel::Preview),
        ] {
            let version = Version::parse(text).unwrap();
            assert_eq!(AppFlavor::for_version(&version), flavor);
            assert_eq!(UpdateChannel::for_version(&version), channel);
            assert_eq!(UpdateChannel::from(flavor), channel);
            assert!(flavor.accepts_update(&version));
        }
        assert!(!AppFlavor::Preview.accepts_update(&Version::parse("3.0.0").unwrap()));
        assert!(!AppFlavor::Stable.accepts_update(&Version::parse("3.0.0-preview.1").unwrap()));
    }
}
