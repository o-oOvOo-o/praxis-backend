#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct HoudiniInternalTarget {
    label: String,
    rva: String,
}

fn builtin_houdini_internal_targets(
    artifact_slug: &str,
    sha256: &str,
) -> Vec<HoudiniInternalTarget> {
    let targets: &[(&str, &str)] = match (artifact_slug, sha256.to_ascii_lowercase().as_str()) {
        (
            "group-sop-family",
            "4df1bdb205b211b30a46fd9d1a5b8bc9276727cf6bd0f9bc30349704b290b9b2",
        ) => &[
            ("promote_verb_cook", "0x4b9b080"),
            ("promote_rule_apply", "0x4ba01b0"),
            ("range_verb_cook", "0x4ba8710"),
            ("expand_verb_cook", "0x4b87100"),
            ("find_path_verb_cook", "0x4b8f200"),
            ("find_path_pair_builder", "0x4b91a70"),
            ("find_path_edge_apply", "0x4b95d40"),
            ("find_path_point_apply", "0x4b96920"),
            ("find_path_primitive_apply", "0x4b97140"),
            ("find_path_vertex_apply", "0x4b979c0"),
        ],
        (
            "group-degenerate-bridges",
            "075f799a2a0b03e42ff791a3879fb9210111263445a7ef3bc31db63efc28a62a",
        ) => &[
            ("primitive_degenerate_predicate", "0x5feb290"),
            ("point_degenerate_membership_apply", "0x535ec40"),
        ],
        ("group-path-gu", "075f799a2a0b03e42ff791a3879fb9210111263445a7ef3bc31db63efc28a62a") => &[
            ("path_successor_dispatch", "0x5abaab0"),
            ("boundary_successor", "0x5ab9090"),
            ("opposite_successor", "0x5abace0"),
            ("quad_left_successor", "0x5abb970"),
            ("quad_right_successor", "0x5abc090"),
            ("path_uv_edge_compatible", "0x5aac220"),
            ("path_signed_lnext", "0x5ac3080"),
            ("path_signed_lprev", "0x5ac33a0"),
            ("edge_loop_cost_callback", "0x5aada30"),
            ("edge_ring_cost_callback", "0x5aada40"),
            ("edge_loop_heap_sift", "0x5aae910"),
            ("edge_ring_heap_sift", "0x5aaeaa0"),
        ],
        _ => &[],
    };
    targets
        .iter()
        .map(|&(label, rva)| HoudiniInternalTarget {
            label: label.into(),
            rva: rva.into(),
        })
        .collect()
}

fn parse_houdini_internal_targets(values: &[String]) -> Result<Vec<HoudiniInternalTarget>, String> {
    let mut targets = Vec::with_capacity(values.len());
    let mut labels = BTreeSet::new();
    for value in values {
        let Some((label, rva)) = value.split_once("@rva:") else {
            return Err(format!(
                "Invalid --internal-target '{value}'; expected label@rva:0xHEX."
            ));
        };
        if label.is_empty()
            || !label
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
        {
            return Err(format!(
                "Invalid internal target label '{label}'; use ASCII letters, digits, '_' or '-'."
            ));
        }
        let Some(hex) = rva.strip_prefix("0x").or_else(|| rva.strip_prefix("0X")) else {
            return Err(format!(
                "Invalid internal target RVA '{rva}'; expected a 0x-prefixed hexadecimal RVA."
            ));
        };
        if hex.is_empty() || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return Err(format!(
                "Invalid internal target RVA '{rva}'; expected a 0x-prefixed hexadecimal RVA."
            ));
        }
        let parsed = u64::from_str_radix(hex, 16)
            .map_err(|error| format!("Invalid internal target RVA '{rva}': {error}"))?;
        if parsed == 0 {
            return Err("Internal target RVA must be non-zero.".to_string());
        }
        if !labels.insert(label.to_string()) {
            return Err(format!("Duplicate internal target label '{label}'."));
        }
        targets.push(HoudiniInternalTarget {
            label: label.to_string(),
            rva: format!("0x{parsed:x}"),
        });
    }
    Ok(targets)
}

#[cfg(test)]
mod houdini_internal_target_tests {
    use super::*;

    #[test]
    fn accepts_repeated_validated_rvas_and_rejects_ambiguous_input() {
        let targets = parse_houdini_internal_targets(&[
            "contract_ring_false@rva:0x3C61B0".to_string(),
            "contract-ring-true@rva:0x3c5660".to_string(),
        ])
        .unwrap();
        assert_eq!(
            targets,
            vec![
                HoudiniInternalTarget {
                    label: "contract_ring_false".to_string(),
                    rva: "0x3c61b0".to_string(),
                },
                HoudiniInternalTarget {
                    label: "contract-ring-true".to_string(),
                    rva: "0x3c5660".to_string(),
                },
            ]
        );
        assert!(parse_houdini_internal_targets(&["missing-rva".to_string()]).is_err());
        assert!(parse_houdini_internal_targets(&["bad label@rva:0x10".to_string()]).is_err());
        assert!(parse_houdini_internal_targets(&[
            "same@rva:0x10".to_string(),
            "same@rva:0x20".to_string(),
        ])
        .is_err());
    }

    #[test]
    fn pins_group_internal_targets_to_the_exact_houdini_binary_hash() {
        let targets = builtin_houdini_internal_targets(
            "group-sop-family",
            "4DF1BDB205B211B30A46FD9D1A5B8BC9276727CF6BD0F9BC30349704B290B9B2",
        );
        assert_eq!(targets.len(), 10);
        assert_eq!(targets[0].label, "promote_verb_cook");
        assert!(targets
            .iter()
            .any(|target| target.label == "range_verb_cook"));
        assert!(targets
            .iter()
            .any(|target| target.label == "expand_verb_cook"));
        assert!(targets
            .iter()
            .any(|target| target.label == "find_path_verb_cook"));
        for label in [
            "find_path_edge_apply",
            "find_path_pair_builder",
            "find_path_point_apply",
            "find_path_primitive_apply",
            "find_path_vertex_apply",
        ] {
            assert!(targets.iter().any(|target| target.label == label));
        }
        assert!(builtin_houdini_internal_targets("group-sop-family", "different").is_empty());
    }

    #[test]
    fn pins_group_path_cost_closure_to_the_exact_libgu_hash() {
        let targets = builtin_houdini_internal_targets(
            "group-path-gu",
            "075F799A2A0B03E42FF791A3879FB9210111263445A7EF3BC31DB63EFC28A62A",
        );
        assert_eq!(targets.len(), 12);
        assert!(targets
            .iter()
            .any(|target| target.label == "path_successor_dispatch"));
        for label in [
            "boundary_successor",
            "opposite_successor",
            "quad_left_successor",
            "quad_right_successor",
            "path_uv_edge_compatible",
            "path_signed_lnext",
            "path_signed_lprev",
        ] {
            assert!(targets.iter().any(|target| target.label == label));
        }
        assert!(targets
            .iter()
            .any(|target| target.label == "edge_loop_cost_callback"));
        assert!(targets
            .iter()
            .any(|target| target.label == "edge_ring_cost_callback"));
        assert!(targets
            .iter()
            .any(|target| target.label == "edge_loop_heap_sift"));
        assert!(targets
            .iter()
            .any(|target| target.label == "edge_ring_heap_sift"));
    }
}
