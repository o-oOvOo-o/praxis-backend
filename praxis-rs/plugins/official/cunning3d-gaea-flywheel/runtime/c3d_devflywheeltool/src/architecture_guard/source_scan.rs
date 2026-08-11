use super::*;

pub(super) fn node_source_candidates(
    repo_dir: &Path,
    node: &str,
    node_snake: &str,
) -> Vec<PathBuf> {
    let lower = node.to_ascii_lowercase();
    vec![
        repo_dir
            .join("src")
            .join("nodes")
            .join("heightfield")
            .join(format!("{node_snake}.rs")),
        repo_dir
            .join("src")
            .join("nodes")
            .join("heightfield")
            .join(format!("{lower}.rs")),
    ]
}

pub(super) fn substrate_source_candidates(
    repo_dir: &Path,
    node: &str,
    node_snake: &str,
) -> Vec<PathBuf> {
    let lower = node.to_ascii_lowercase();
    vec![
        repo_dir
            .join("src")
            .join("cunning_core")
            .join("core")
            .join("geometry")
            .join("heightfield")
            .join(format!("{node_snake}.rs")),
        repo_dir
            .join("src")
            .join("cunning_core")
            .join("core")
            .join("geometry")
            .join("heightfield")
            .join(format!("{lower}.rs")),
    ]
}

pub(super) fn find_node_definition_source(
    repo_dir: &Path,
    node: &str,
    node_type_symbols: &[String],
) -> Option<NodeSource> {
    let directory = repo_dir
        .join("crates")
        .join("cunning_core")
        .join("src")
        .join("node_definitions");
    let node_key = normalize_key(node);
    let mut paths = fs::read_dir(directory)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("rs"))
        .filter(|path| path.file_stem().and_then(|stem| stem.to_str()) != Some("mod"))
        .collect::<Vec<_>>();
    paths.sort();
    paths.into_iter().find_map(|path| {
        let source = read_source(path)?;
        let stem_matches = source
            .path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(normalize_key)
            .is_some_and(|stem| stem == node_key);
        let symbol_matches = node_type_symbols.iter().any(|symbol| {
            source
                .lines
                .iter()
                .any(|line| line_contains_symbol(line, symbol))
        });
        let identity_matches = source.lines.iter().any(|line| {
            (line.contains("LEGACY_LOAD_NAME") || line.contains("EDITOR_NAME"))
                && quoted_value(line).is_some_and(|value| normalize_key(value) == node_key)
        });
        (stem_matches || symbol_matches || identity_matches).then_some(source)
    })
}

pub(super) fn quoted_value(line: &str) -> Option<&str> {
    let start = line.find('"')? + 1;
    let end = line[start..].find('"')? + start;
    Some(&line[start..end])
}

pub(super) fn read_first_existing(paths: &[PathBuf]) -> Option<NodeSource> {
    paths.iter().find_map(|path| read_source(path.clone()))
}

pub(super) fn read_source(path: PathBuf) -> Option<NodeSource> {
    let text = fs::read_to_string(&path).ok()?;
    let lines = text.lines().map(str::to_string).collect::<Vec<_>>();
    Some(NodeSource { path, text, lines })
}

pub(super) fn read_json_value(path: PathBuf) -> Option<Value> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

pub(super) fn formal_hosted_product_publication_hits(
    repo_dir: &Path,
    node_definition: &NodeSource,
    limit: usize,
) -> Vec<SourceSpan> {
    let Some(type_id) = node_definition.lines.iter().find_map(|line| {
        (line.contains("TYPE_ID") && line.contains('='))
            .then(|| quoted_value(line))
            .flatten()
    }) else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    for relative in [
        "crates/cunning_compute_products/src",
        "crates/cunning_engine_hosted_cce/src",
    ] {
        collect_sources_with_extensions(&repo_dir.join(relative), &["rs"], &mut paths);
    }
    collect_sources_with_extensions(&repo_dir.join("src"), &["wgsl"], &mut paths);
    paths.sort();

    let mut spans = Vec::new();
    for path in paths {
        let Some(source) = read_source(path) else {
            continue;
        };
        let is_wgsl = source.path.extension().and_then(|value| value.to_str()) == Some("wgsl");
        let formal_rust = source.text.contains(type_id)
            && (source.text.contains("EngineHostedNodeProgramRegistration")
                || source
                    .text
                    .contains("EngineHostedHeightfieldProductRegistration"));
        for (index, line) in source.lines.iter().enumerate() {
            let formal_wgsl = is_wgsl && line.contains("@cce-node|") && line.contains(type_id);
            let formal_rust_line = formal_rust
                && (line.contains(type_id)
                    || line.contains("EngineHostedNodeProgramRegistration")
                    || line.contains("EngineHostedHeightfieldProductRegistration"));
            if formal_wgsl || formal_rust_line {
                spans.push(SourceSpan {
                    path: relative_path(repo_dir, &source.path),
                    line_number: index + 1,
                    line: line.trim().to_string(),
                });
                if spans.len() >= limit {
                    return spans;
                }
            }
        }
    }
    spans
}

pub(super) fn node_specific_runtime_authority_hits(
    repo_dir: &Path,
    node: &str,
    limit: usize,
) -> Vec<SourceSpan> {
    let node_snake = snake_case(node);
    if node_snake.is_empty() {
        return Vec::new();
    }
    let exact_needles = [
        format!("{node_snake}_parameter_packer"),
        format!("pack_{node_snake}_parameters"),
        format!("lower_{node_snake}_to_compute_ir"),
        format!("lower_{node_snake}_compute"),
        format!("{node_snake}_compute_ir_builder"),
        format!("{node_snake}_shader_ir_builder"),
        format!("{node_snake}_recipe_builder"),
        format!("{node_snake}_runtime_executor"),
        format!("{node_snake}_binding_table"),
        format!("{node_snake}_backend"),
    ]
    .map(|needle| normalize_key(needle.as_str()));
    let roots = [
        "crates/cunning_cda_runtime/src",
        "crates/cunning_cce_plan/src",
        "crates/cunning_compute_products/src",
        "crates/cunning_compute_core/src",
        "crates/cunning_compute_ir/src",
        "crates/cunning_shader_ir/src",
        "crates/cunning_engine_hosted_cce/src",
        "crates/cunning_engine_hosted_runtime/src",
    ];
    let mut paths = Vec::new();
    for relative in roots {
        collect_rust_sources(&repo_dir.join(relative), &mut paths);
    }
    paths.sort();

    let mut spans = Vec::new();
    for path in paths {
        let Some(source) = read_source(path) else {
            continue;
        };
        for (index, line) in source.lines.iter().enumerate() {
            let normalized_line = normalize_key(line);
            if exact_needles
                .iter()
                .any(|needle| normalized_line.contains(needle.as_str()))
            {
                spans.push(SourceSpan {
                    path: source.path.display().to_string(),
                    line_number: index + 1,
                    line: line.trim().to_string(),
                });
                if spans.len() >= limit {
                    return spans;
                }
            }
        }
    }
    spans
}

pub(super) fn runtime_parameter_packer_framework_hits(
    repo_dir: &Path,
    limit: usize,
) -> Vec<SourceSpan> {
    let root = repo_dir.join("crates/cunning_cda_runtime/src");
    let mut paths = Vec::new();
    collect_rust_sources(&root, &mut paths);
    paths.sort();

    let mut spans = Vec::new();
    for path in paths {
        let Some(source) = read_source(path) else {
            continue;
        };
        for (index, line) in source.lines.iter().enumerate() {
            let normalized = line.to_ascii_lowercase();
            let callback_type = line.contains("ComputeProgramParameterPacker");
            let node_named_factory = normalized.contains("_parameter_packer");
            let node_named_pack = normalized.contains("pack_")
                && normalized.contains("_parameters")
                && !normalized.contains("pack_projected_")
                && !normalized.contains("pack_automatic_");
            if callback_type || node_named_factory || node_named_pack {
                spans.push(SourceSpan {
                    path: source.path.display().to_string(),
                    line_number: index + 1,
                    line: line.trim().to_string(),
                });
                if spans.len() >= limit {
                    return spans;
                }
            }
        }
    }
    spans
}

pub(super) fn manual_node_product_authority_hits(repo_dir: &Path, limit: usize) -> Vec<SourceSpan> {
    let root = repo_dir.join("crates/cunning_core/src/node_definitions");
    let mut paths = Vec::new();
    collect_rust_sources(&root, &mut paths);
    paths.sort();
    let forbidden = [
        "NodeProductDescriptor::new",
        "NodeProductStageDescriptor::new",
        "NodeComputeProgramRef::new",
        "ComputeProgramEncoder::new",
        "ComputeProgramDescriptor {",
        "ComputeIrProgram",
        "ShaderIrModule",
        "create_compute_pipeline(",
        "begin_compute_pass(",
        "dispatch_workgroups(",
        "queue.submit(",
    ];
    let mut spans = Vec::new();
    for path in paths {
        let Some(source) = read_source(path) else {
            continue;
        };
        spans.extend(line_hits(
            &source,
            &forbidden,
            limit.saturating_sub(spans.len()),
        ));
        if spans.len() >= limit {
            break;
        }
    }
    spans
}

pub(super) fn untracked_explicit_parameter_projection_hits(
    repo_dir: &Path,
    limit: usize,
) -> Vec<SourceSpan> {
    if limit == 0 {
        return Vec::new();
    }
    let mut paths = Vec::new();
    collect_rust_sources(
        &repo_dir.join("crates/cunning_core/src/node_definitions"),
        &mut paths,
    );
    paths.sort();
    let mut spans = Vec::new();
    for path in paths {
        let Some(source) = read_source(path) else {
            continue;
        };
        if !source.text.contains("NodeComputeParameterWordExpr")
            && !source.text.contains("NodeComputeParameterBlockProjection")
        {
            continue;
        }
        spans.extend(line_hits(
            &source,
            &[
                "NodeComputeParameterWordExpr",
                "NodeComputeParameterBlockProjection",
            ],
            limit.saturating_sub(spans.len()),
        ));
        if spans.len() >= limit {
            break;
        }
    }
    spans
}

pub(super) fn collect_rust_sources(root: &Path, paths: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rust_sources(&path, paths);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            paths.push(path);
        }
    }
}

pub(super) fn collect_sources_with_extensions(
    root: &Path,
    extensions: &[&str],
    paths: &mut Vec<PathBuf>,
) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_sources_with_extensions(&path, extensions, paths);
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extensions.contains(&extension))
        {
            paths.push(path);
        }
    }
}

pub(super) fn find_decompiled_source(
    ctx: &Context,
    node: &str,
    node_key: &str,
) -> Option<NodeSource> {
    let roots = vec![
        ctx.gaea_decompiled_root.join("Gaea.Nodes"),
        ctx.gaea_decompiled_root.join("Gaea"),
    ];
    let mut stack = roots
        .iter()
        .cloned()
        .filter(|root| root.exists())
        .collect::<Vec<_>>();
    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("cs") {
                continue;
            }
            let file_key = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(normalize_key)
                .unwrap_or_default();
            if file_key == node_key || file_key.contains(node_key) {
                if let Some(source) = read_source(path) {
                    return Some(source);
                }
            }
        }
    }
    let fallback_key = normalize_key(node);
    roots
        .iter()
        .filter(|root| root.exists())
        .find_map(|root| find_source_by_text(root, &fallback_key))
}

pub(super) fn find_source_by_text(root: &Path, node_key: &str) -> Option<NodeSource> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).ok()?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("cs") {
                continue;
            }
            let source = read_source(path)?;
            if normalize_key(&source.text).contains(node_key) {
                return Some(source);
            }
        }
    }
    None
}

pub(super) fn extract_node_type_symbols(source: &NodeSource) -> Vec<String> {
    let mut symbols = Vec::new();
    for line in &source.lines {
        let mut offset = 0usize;
        while let Some(index) = line[offset..].find("NODE_HEIGHTFIELD") {
            let start = offset + index;
            let tail = &line[start..];
            let end = tail
                .find(|ch: char| !(ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_'))
                .unwrap_or(tail.len());
            let symbol = tail[..end].to_string();
            if !symbols.contains(&symbol) {
                symbols.push(symbol);
            }
            offset = start + end;
        }
    }
    symbols
}

pub(super) fn fallback_node_type_symbols(node: &str, node_snake: &str) -> Vec<String> {
    let upper = node_snake.to_ascii_uppercase();
    vec![
        format!("NODE_HEIGHTFIELD_{upper}"),
        format!("NODE_HEIGHTFIELD_{}", node.to_ascii_uppercase()),
    ]
}

pub(super) fn line_hits(source: &NodeSource, needles: &[&str], limit: usize) -> Vec<SourceSpan> {
    let mut spans = Vec::new();
    for (index, line) in source.lines.iter().enumerate() {
        if needles.iter().any(|needle| line.contains(needle)) {
            spans.push(SourceSpan {
                path: source.path.display().to_string(),
                line_number: index + 1,
                line: line.trim().to_string(),
            });
            if spans.len() >= limit {
                break;
            }
        }
    }
    spans
}

pub(super) fn symbol_hits(
    source: &NodeSource,
    symbols: &[String],
    limit: usize,
) -> Vec<SourceSpan> {
    let mut spans = Vec::new();
    for (index, line) in source.lines.iter().enumerate() {
        if symbols
            .iter()
            .any(|symbol| line_contains_symbol(line, symbol))
        {
            spans.push(SourceSpan {
                path: source.path.display().to_string(),
                line_number: index + 1,
                line: line.trim().to_string(),
            });
            if spans.len() >= limit {
                break;
            }
        }
    }
    spans
}

pub(super) fn line_contains_symbol(line: &str, symbol: &str) -> bool {
    let mut search_from = 0usize;
    while let Some(index) = line[search_from..].find(symbol) {
        let start = search_from + index;
        let end = start + symbol.len();
        let before_ok = line[..start]
            .chars()
            .next_back()
            .map(|ch| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .unwrap_or(true);
        let after_ok = line[end..]
            .chars()
            .next()
            .map(|ch| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .unwrap_or(true);
        if before_ok && after_ok {
            return true;
        }
        search_from = end;
    }
    false
}

pub(super) fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

pub(super) fn snake_case(value: &str) -> String {
    let mut out = String::new();
    let mut previous_is_lower_or_digit = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            if ch.is_ascii_uppercase() && previous_is_lower_or_digit && !out.ends_with('_') {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
            previous_is_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        } else {
            if !out.ends_with('_') && !out.is_empty() {
                out.push('_');
            }
            previous_is_lower_or_digit = false;
        }
    }
    out.trim_matches('_').to_string()
}

pub(super) fn normalize_key(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}
