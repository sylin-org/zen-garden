
            /// Returns the `rustc` SemVer version and additional metadata
            /// like the git short hash and build date.
            pub fn version_meta() -> VersionMeta {
                VersionMeta {
                    semver: Version {
                        major: 1,
                        minor: 95,
                        patch: 0,
                        pre: Prerelease::new("").unwrap(),
                        build: BuildMetadata::new("").unwrap(),
                    },
                    host: "x86_64-pc-windows-msvc".to_owned(),
                    short_version_string: "rustc 1.95.0 (59807616e 2026-04-14)".to_owned(),
                    commit_hash: Some("59807616e1fa2540724bfbac14d7976d7e4a3860".to_owned()),
                    commit_date: Some("2026-04-14".to_owned()),
                    build_date: None,
                    channel: Channel::Stable,
                    llvm_version: Some(LlvmVersion{ major: 22, minor: 1 }),
                }
            }
            