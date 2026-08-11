fn polyreduce_precision_targets(
    targets: &[HoudiniReverseTarget],
    precision: &str,
) -> Result<Vec<HoudiniReverseTarget>, String> {
    let template_code = match precision {
        "float" => "M",
        "double" => "N",
        value => {
            return Err(format!(
                "Unsupported PolyReduce --precision '{value}'; use float or double."
            ));
        }
    };
    Ok(targets
        .iter()
        .map(|target| {
            let symbol_fragment = target.symbol_fragment.replace(
                "@M@GU_PolyReduce2@@",
                &format!("@{template_code}@GU_PolyReduce2@@"),
            );
            HoudiniReverseTarget {
                label: target.label,
                // The flywheel CLI is a short-lived process. Leaking this tiny,
                // bounded target table lets the existing static target/receipt
                // substrate represent the selected native specialization.
                symbol_fragment: Box::leak(symbol_fragment.into_boxed_str()),
                tier: target.tier,
            }
        })
        .collect())
}

fn houdini_reverse_subject(value: &str) -> Result<HoudiniReverseSubject, String> {
    match value.to_ascii_lowercase().as_str() {
        "polyreduce" => Ok(HoudiniReverseSubject {
            artifact_slug: "polyreduce",
            host_version: "22.0.368",
            default_binary: r"F:\Houdini22\bin\libGU.dll",
            binary_env: "HOUDINI_LIBGU",
            targets: POLYREDUCE_TARGETS,
        }),
        "geo" | "geo-poly-interface" => Ok(HoudiniReverseSubject {
            artifact_slug: "geo-poly-interface",
            host_version: "22.0.368",
            default_binary: r"F:\Houdini22\bin\libGEO.dll",
            binary_env: "HOUDINI_LIBGEO",
            targets: GEO_POLY_INTERFACE_TARGETS,
        }),
        "measure" | "measure-curvature" | "gu-measure-curvature" => Ok(HoudiniReverseSubject {
            artifact_slug: "measure-curvature",
            host_version: "22.0.368",
            default_binary: r"F:\Houdini22\bin\libGU.dll",
            binary_env: "HOUDINI_LIBGU",
            targets: MEASURE_CURVATURE_TARGETS,
        }),
        "group" | "group-sop" | "group-family" => Ok(HoudiniReverseSubject {
            artifact_slug: "group-sop-family",
            host_version: "22.0.368",
            default_binary: r"F:\Houdini22\bin\libSOP.dll",
            binary_env: "HOUDINI_LIBSOP",
            targets: GROUP_SOP_TARGETS,
        }),
        "group-degenerate" | "group-degenerate-bridges" => Ok(HoudiniReverseSubject {
            artifact_slug: "group-degenerate-bridges",
            host_version: "22.0.368",
            default_binary: r"F:\Houdini22\bin\libGU.dll",
            binary_env: "HOUDINI_LIBGU",
            targets: GROUP_DEGENERATE_TARGETS,
        }),
        "group-path" | "group-path-gu" => Ok(HoudiniReverseSubject {
            artifact_slug: "group-path-gu",
            host_version: "22.0.368",
            default_binary: r"F:\Houdini22\bin\libGU.dll",
            binary_env: "HOUDINI_LIBGU",
            targets: GROUP_PATH_GU_TARGETS,
        }),
        "apex" | "apex-core" => Ok(HoudiniReverseSubject {
            artifact_slug: "apex-core",
            host_version: "22.0.368",
            default_binary: r"F:\houdini22\bin\libAPEX.dll",
            binary_env: "HOUDINI_LIBAPEX",
            targets: APEX_CORE_TARGETS,
        }),
        "apexa" | "apex-animation" => Ok(HoudiniReverseSubject {
            artifact_slug: "apex-animation",
            host_version: "22.0.368",
            default_binary: r"F:\houdini22\bin\libAPEXA.dll",
            binary_env: "HOUDINI_LIBAPEXA",
            targets: APEX_ANIMATION_TARGETS,
        }),
        _ => Err(format!(
            "Unsupported Houdini reverse subject '{value}'; use polyreduce, geo-poly-interface, measure-curvature, group-sop, group-path-gu, group-degenerate, apex-core, or apex-animation."
        )),
    }
}

fn houdini_install_rank(path: &Path) -> u32 {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| {
            name.chars()
                .skip_while(|character| !character.is_ascii_digit())
                .take_while(|character| character.is_ascii_digit())
                .collect::<String>()
        })
        .and_then(|digits| digits.parse().ok())
        .unwrap_or(0)
}

fn resolve_houdini_binary(cli: &Cli, subject: HoudiniReverseSubject) -> Result<PathBuf, String> {
    if let Some(binary) = cli.flag("binary") {
        return Ok(PathBuf::from(binary));
    }
    let mut candidates = Vec::new();
    if let Some(binary) = env::var_os(subject.binary_env) {
        candidates.push(PathBuf::from(binary));
    }
    if let Some(hfs) = env::var_os("HFS") {
        candidates.push(
            PathBuf::from(hfs).join("bin").join(
                Path::new(subject.default_binary)
                    .file_name()
                    .unwrap_or_default(),
            ),
        );
    }
    candidates.push(PathBuf::from(subject.default_binary));
    let binary_name = Path::new(subject.default_binary)
        .file_name()
        .unwrap_or_default()
        .to_owned();
    let mut installs = fs::read_dir(r"F:\")
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.to_ascii_lowercase().starts_with("houdini"))
        })
        .collect::<Vec<_>>();
    installs.sort_by_key(|path| std::cmp::Reverse(houdini_install_rank(path)));
    candidates.extend(
        installs
            .into_iter()
            .map(|install| install.join("bin").join(&binary_name)),
    );
    for candidate in &candidates {
        if candidate.is_file() {
            return Ok(candidate.clone());
        }
    }
    Err(format!(
        "No Houdini {} was found; set --binary, {}, or HFS. Tried: {}",
        binary_name.to_string_lossy(),
        subject.binary_env,
        candidates
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}
