#[cfg(test)]
mod houdini_native_reverse_tests {
    use super::*;

    #[test]
    fn polyreduce_subject_is_pinned_to_current_houdini_22_oracle() {
        let subject = houdini_reverse_subject("polyreduce").unwrap();
        assert_eq!(subject.host_version, "22.0.368");
        assert_eq!(subject.default_binary, r"F:\Houdini22\bin\libGU.dll");
    }

    #[test]
    fn polyreduce_targets_select_the_runtime_double_specialization() {
        let targets = polyreduce_precision_targets(POLYREDUCE_TARGETS, "double").unwrap();
        assert!(
            targets
                .iter()
                .all(|target| !target.symbol_fragment.contains("@M@GU_PolyReduce2@@"))
        );
        assert!(
            targets
                .iter()
                .all(|target| target.symbol_fragment.contains("@N@GU_PolyReduce2@@"))
        );
        assert!(polyreduce_precision_targets(POLYREDUCE_TARGETS, "half").is_err());
    }

    #[test]
    fn houdini_22_apex_subjects_are_version_pinned() {
        let core = houdini_reverse_subject("apex-core").unwrap();
        assert_eq!(core.artifact_slug, "apex-core");
        assert_eq!(core.host_version, "22.0.368");
        assert_eq!(core.default_binary, r"F:\houdini22\bin\libAPEX.dll");
        assert!(core.targets.len() >= 30);

        let animation = houdini_reverse_subject("apex-animation").unwrap();
        assert_eq!(animation.artifact_slug, "apex-animation");
        assert_eq!(animation.host_version, "22.0.368");
        assert_eq!(animation.default_binary, r"F:\houdini22\bin\libAPEXA.dll");
        assert!(animation.targets.len() >= 30);
    }

    #[test]
    fn apex_core_target_resolution_is_fail_closed() {
        let subject = houdini_reverse_subject("apex-core").unwrap();
        let exports = subject
            .targets
            .iter()
            .filter(|target| target.tier == 0)
            .enumerate()
            .map(|(index, target)| PeExport {
                name: format!("{}suffix", target.symbol_fragment),
                rva: format!("0x{:x}", index + 1),
            })
            .collect::<Vec<_>>();
        let expected = subject
            .targets
            .iter()
            .filter(|target| target.tier == 0)
            .count();
        assert_eq!(
            resolve_houdini_targets(subject.targets, &exports, 0)
                .unwrap()
                .len(),
            expected
        );
        assert!(resolve_houdini_targets(subject.targets, &exports[..expected - 1], 0).is_err());
    }

    #[test]
    fn apex_animation_target_resolution_is_fail_closed() {
        let subject = houdini_reverse_subject("apex-animation").unwrap();
        let exports = subject
            .targets
            .iter()
            .filter(|target| target.tier == 0)
            .enumerate()
            .map(|(index, target)| PeExport {
                name: format!("{}suffix", target.symbol_fragment),
                rva: format!("0x{:x}", index + 1),
            })
            .collect::<Vec<_>>();
        let expected = subject
            .targets
            .iter()
            .filter(|target| target.tier == 0)
            .count();
        assert_eq!(
            resolve_houdini_targets(subject.targets, &exports, 0)
                .unwrap()
                .len(),
            expected
        );
        assert!(resolve_houdini_targets(subject.targets, &exports[..expected - 1], 0).is_err());
    }

    #[test]
    fn parses_llvm_readobj_export_blocks() {
        let exports = parse_pe_exports(
            "Export {\n Name: ?reduce@?$DecimatorT@M@GU_PolyReduce2@@x\n RVA: 0x123\n}\n",
        );
        assert_eq!(
            exports,
            vec![PeExport {
                name: "?reduce@?$DecimatorT@M@GU_PolyReduce2@@x".into(),
                rva: "0x123".into()
            }]
        );
    }

    #[test]
    fn core_target_resolution_is_fail_closed() {
        let exports = POLYREDUCE_TARGETS
            .iter()
            .filter(|target| target.tier == 0)
            .map(|target| PeExport {
                name: format!("{}suffix", target.symbol_fragment),
                rva: "0x1".into(),
            })
            .collect::<Vec<_>>();
        assert_eq!(resolve_polyreduce_targets(&exports, 0).unwrap().len(), 9);
        assert!(resolve_polyreduce_targets(&exports[..8], 0).is_err());
    }

    #[test]
    fn polyreduce_extended_targets_capture_refresh_lifecycle_body() {
        let refresh = POLYREDUCE_TARGETS
            .iter()
            .find(|target| target.label == "refresh_collapse_data")
            .expect("refreshCollapseData must be a first-class reverse target");
        assert_eq!(refresh.tier, 1);
        assert!(refresh.symbol_fragment.contains("?refreshCollapseData@"));
    }

    #[test]
    fn geo_poly_interface_targets_capture_primary_resolution_and_contraction() {
        let subject = houdini_reverse_subject("geo-poly-interface").unwrap();
        assert_eq!(subject.default_binary, r"F:\Houdini22\bin\libGEO.dll");
        let find_primary = subject
            .targets
            .iter()
            .find(|target| target.label == "find_primary_hedge")
            .expect("findPrimary must be a first-class reverse target");
        assert_eq!(find_primary.tier, 0);
        assert!(
            find_primary
                .symbol_fragment
                .contains("?findPrimary@GEO_PolyInterface@@")
        );
        let contract = subject
            .targets
            .iter()
            .find(|target| target.label == "contract_hedge")
            .expect("contract must be a first-class reverse target");
        assert_eq!(contract.tier, 0);
        assert!(
            contract
                .symbol_fragment
                .contains("?contract@GEO_PolyInterface@@")
        );
        let sym_link = subject
            .targets
            .iter()
            .find(|target| target.label == "sym_link")
            .expect("symLink must be a first-class reverse target");
        assert_eq!(sym_link.tier, 0);
        assert!(
            sym_link
                .symbol_fragment
                .contains("?symLink@GEO_PolyInterface@@")
        );
    }

    #[test]
    fn measure_curvature_subject_pins_gu_measure_entry_point() {
        let subject = houdini_reverse_subject("measure-curvature").unwrap();
        assert_eq!(subject.artifact_slug, "measure-curvature");
        assert_eq!(subject.host_version, "22.0.368");
        assert_eq!(subject.default_binary, r"F:\Houdini22\bin\libGU.dll");
        assert_eq!(subject.targets, MEASURE_CURVATURE_TARGETS);
        assert_eq!(subject.targets[0].label, "compute_curvature");
        assert!(
            subject.targets[0]
                .symbol_fragment
                .contains("?computeCurvature@GU_Measure@@")
        );
    }

    #[test]
    fn group_sop_subject_is_version_pinned_and_fail_closed() {
        let subject = houdini_reverse_subject("group-sop").unwrap();
        assert_eq!(subject.artifact_slug, "group-sop-family");
        assert_eq!(subject.host_version, "22.0.368");
        assert_eq!(subject.default_binary, r"F:\Houdini22\bin\libSOP.dll");
        assert_eq!(
            subject
                .targets
                .iter()
                .filter(|target| target.tier == 0)
                .count(),
            4
        );
        assert_eq!(subject.targets.len(), 8);
        for label in [
            "promote_cook_verb",
            "range_cook_verb",
            "expand_cook_verb",
            "find_path_cook_verb",
            "promote_build_from_op",
            "range_build_from_op",
            "expand_build_from_op",
            "find_path_build_from_op",
        ] {
            assert!(
                subject.targets.iter().any(|target| target.label == label),
                "missing required Group SOP reverse target {label}"
            );
        }
    }

    #[test]
    fn group_degenerate_subject_captures_all_group_domains() {
        let subject = houdini_reverse_subject("group-degenerate").unwrap();
        assert_eq!(subject.artifact_slug, "group-degenerate-bridges");
        assert_eq!(subject.default_binary, r"F:\Houdini22\bin\libGU.dll");
        assert_eq!(subject.targets.len(), 4);
    }

    #[test]
    fn group_path_gu_subject_captures_loop_helper_and_cost_variants() {
        let subject = houdini_reverse_subject("group-path-gu").unwrap();
        assert_eq!(subject.artifact_slug, "group-path-gu");
        assert_eq!(subject.default_binary, r"F:\Houdini22\bin\libGU.dll");
        assert_eq!(subject.targets.len(), 15);
        assert_eq!(
            subject
                .targets
                .iter()
                .filter(|target| target.tier == 0)
                .count(),
            9
        );
        for label in [
            "edge_loop_path",
            "edge_ring_path",
            "point_loop_path",
            "primitive_loop_path",
            "vertex_loop_path",
            "shortest_path_find",
            "edge_ring_find_dual_path",
        ] {
            assert!(subject.targets.iter().any(|target| target.label == label));
        }
    }

    #[test]
    fn deep_reverse_reuses_existing_analysis_unless_reanalysis_is_requested() {
        assert!(should_run_ghidra_analysis(false, true, false));
        assert!(!should_run_ghidra_analysis(true, true, false));
        assert!(should_run_ghidra_analysis(true, true, true));
        assert!(!should_run_ghidra_analysis(false, false, false));
    }
}
