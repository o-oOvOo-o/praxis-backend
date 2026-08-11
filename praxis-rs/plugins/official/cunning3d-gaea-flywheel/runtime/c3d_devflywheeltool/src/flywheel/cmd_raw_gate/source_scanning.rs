fn collect_cs_files_checked(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    if !root.exists() {
        return Ok(files);
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)
            .map_err(|error| format!("Failed to scan '{}': {error}", dir.display()))?
        {
            let entry =
                entry.map_err(|error| format!("Failed to read '{}': {error}", dir.display()))?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(OsStr::to_str) == Some("cs") {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn is_shared_blackbox_source(class: &str) -> bool {
    if matches!(
        class,
        "AttributeHelper"
            | "Base3264Encoding"
            | "HmacClientHelper"
            | "NodeHelper"
            | "FileHelper"
            | "PathHelper"
    ) {
        return false;
    }
    if class.ends_with("Attribute") || class.ends_with("Serialization") || class.ends_with("Args") {
        return false;
    }
    !matches!(
        class,
        "Node"
            | "Port"
            | "Parameter"
            | "Parameters"
            | "Group"
            | "Name"
            | "Family"
            | "Toolbox"
            | "Classification"
            | "Icon"
            | "RequiresBaking"
    )
}

fn primary_source_type_name(text: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with('[') {
            continue;
        }
        for keyword in [" class ", " struct "] {
            if let Some((_, after)) = trimmed.split_once(keyword) {
                let name = after
                    .split(|ch: char| {
                        ch.is_whitespace() || ch == ':' || ch == '<' || ch == '{' || ch == '('
                    })
                    .next()
                    .unwrap_or_default()
                    .trim();
                if is_identifier(name) {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

fn is_decompiler_generated_class(class: &str) -> bool {
    let lower = class.to_ascii_lowercase();
    lower.starts_with("__c")
        || lower.starts_with("__")
        || lower.starts_with('_')
        || lower.contains("displayclass")
        || lower.contains("anonymous")
        || lower.contains("<")
        || lower.contains(">")
}

fn coded_segments(line: &str) -> Vec<String> {
    line.split('`')
        .skip(1)
        .step_by(2)
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(str::to_string)
        .collect()
}

fn extract_static_method_names(text: &str) -> Vec<String> {
    let mut methods = Vec::new();
    for line in text.lines() {
        if !line.contains("static") || !line.contains('(') || line.contains(" class ") {
            continue;
        }
        let Some(before_paren) = line.split('(').next() else {
            continue;
        };
        let Some(name) = before_paren
            .split(|ch: char| ch.is_whitespace() || ch == '<' || ch == '>')
            .filter(|token| !token.is_empty())
            .last()
        else {
            continue;
        };
        if is_identifier(name) && !matches!(name, "operator" | "get" | "set") {
            push_unique_string(&mut methods, name);
        }
    }
    methods
}

fn dedup_operator_methods(methods: &mut Vec<CatalogOperatorMethod>) {
    let mut seen = BTreeSet::new();
    methods.retain(|method| {
        !is_decompiler_generated_class(&method.class) && is_identifier(&method.method)
    });
    methods.retain(|method| {
        seen.insert(format!(
            "{}.{}",
            method.class.to_ascii_lowercase(),
            method.method.to_ascii_lowercase()
        ))
    });
}

fn blackbox_class_set(methods: &[CatalogOperatorMethod]) -> BTreeSet<String> {
    let mut classes = methods
        .iter()
        .map(|method| method.class.clone())
        .collect::<BTreeSet<_>>();
    for class in [
        "AspectMaps",
        "Combiner",
        "MapHelper",
        "Masking",
        "RockCore",
        "Lighting2",
        "ClassicCombiner",
        "Morphology",
        "Morphology2",
        "MorphologyRT",
        "HybridBlender",
        "VectorMask",
        "WarpField",
        "RawNoise",
        "FilterCore",
        "DebrisCore",
        "FacetedRock",
    ] {
        classes.insert(class.to_string());
    }
    classes
}

fn extract_method_body(text: &str, method: &str) -> Option<String> {
    let needle = format!("{method}(");
    let mut search_start = 0usize;
    while let Some(relative) = text[search_start..].find(&needle) {
        let method_index = search_start + relative;
        let signature_start = text[..method_index]
            .rfind('\n')
            .map(|index| index + 1)
            .unwrap_or(0);
        let signature = text[signature_start..method_index].trim();
        if !signature.contains("static") {
            search_start = method_index + needle.len();
            continue;
        }
        let after_signature = &text[signature_start..];
        let brace_relative = after_signature.find('{')?;
        let body_start = signature_start + brace_relative;
        let mut depth = 0usize;
        for (relative, ch) in text[body_start..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some(text[body_start..body_start + relative + 1].to_string());
                    }
                }
                _ => {}
            }
        }
        search_start = method_index + needle.len();
    }
    None
}

fn extract_blackbox_calls(text: &str, classes: &BTreeSet<String>) -> Vec<(String, String)> {
    let mut calls = Vec::new();
    for class in classes {
        if is_decompiler_generated_class(class) {
            continue;
        }
        let needle = format!("{class}.");
        let mut search_start = 0usize;
        while let Some(relative) = text[search_start..].find(&needle) {
            let method_start = search_start + relative + needle.len();
            let Some((method, method_end)) = read_identifier_at(text, method_start) else {
                search_start = method_start;
                continue;
            };
            let after = text[method_end..].trim_start();
            if after.starts_with('(') || after.starts_with('<') {
                calls.push((class.clone(), method));
            }
            search_start = method_end;
        }
        let ctor_needle = format!("new {class}(");
        if text.contains(&ctor_needle) {
            calls.push((class.clone(), "ctor".to_string()));
        }
    }
    calls.sort();
    calls.dedup();
    calls
}

fn read_identifier_at(text: &str, start: usize) -> Option<(String, usize)> {
    let mut end = start;
    for (relative, ch) in text[start..].char_indices() {
        if relative == 0 && !(ch == '_' || ch.is_ascii_alphabetic()) {
            return None;
        }
        if ch == '_' || ch.is_ascii_alphanumeric() {
            end = start + relative + ch.len_utf8();
        } else {
            break;
        }
    }
    if end == start {
        None
    } else {
        Some((text[start..end].to_string(), end))
    }
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}
