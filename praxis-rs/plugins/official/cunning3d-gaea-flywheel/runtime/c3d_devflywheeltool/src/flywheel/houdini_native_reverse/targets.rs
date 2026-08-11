#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HoudiniReverseTarget {
    label: &'static str,
    symbol_fragment: &'static str,
    tier: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HoudiniReverseSubject {
    artifact_slug: &'static str,
    host_version: &'static str,
    default_binary: &'static str,
    binary_env: &'static str,
    targets: &'static [HoudiniReverseTarget],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct PeExport {
    name: String,
    rva: String,
}

const POLYREDUCE_TARGETS: &[HoudiniReverseTarget] = &[
    HoudiniReverseTarget {
        label: "reduce",
        symbol_fragment: "?reduce@?$DecimatorT@M@GU_PolyReduce2@@",
        tier: 0,
    },
    HoudiniReverseTarget {
        label: "build_triangulation",
        symbol_fragment: "?buildTriangulation@?$DecimatorT@M@GU_PolyReduce2@@",
        tier: 0,
    },
    HoudiniReverseTarget {
        label: "build_wedge_bundles",
        symbol_fragment: "?buildWedgeBundles@?$DecimatorT@M@GU_PolyReduce2@@",
        tier: 0,
    },
    HoudiniReverseTarget {
        label: "calc_initial_hedge_data",
        symbol_fragment: "?calcInitialHedgeCollapseData@?$DecimatorT@M@GU_PolyReduce2@@",
        tier: 0,
    },
    HoudiniReverseTarget {
        label: "calc_hedge_data",
        symbol_fragment: "?calcHedgeCollapseData@?$DecimatorT@M@GU_PolyReduce2@@AEAAXVGEO_Hedge@@@Z",
        tier: 0,
    },
    HoudiniReverseTarget {
        label: "hedge_collapse_cost",
        symbol_fragment: "?hedgeCollapseCost@?$DecimatorT@M@GU_PolyReduce2@@",
        tier: 0,
    },
    HoudiniReverseTarget {
        label: "fetch_next_batch",
        symbol_fragment: "?fetchNextCollapseBatch@?$DecimatorT@M@GU_PolyReduce2@@",
        tier: 0,
    },
    HoudiniReverseTarget {
        label: "collapse",
        symbol_fragment: "?collapse@?$DecimatorT@M@GU_PolyReduce2@@",
        tier: 0,
    },
    HoudiniReverseTarget {
        label: "has_reached_target",
        symbol_fragment: "?hasReachedTarget@?$DecimatorT@M@GU_PolyReduce2@@",
        tier: 0,
    },
    HoudiniReverseTarget {
        label: "reduce_to_target",
        symbol_fragment: "?reduceToReachTarget@?$DecimatorT@M@GU_PolyReduce2@@",
        tier: 1,
    },
    HoudiniReverseTarget {
        label: "populate_queue",
        symbol_fragment: "?populateQueue@?$DecimatorT@M@GU_PolyReduce2@@",
        tier: 1,
    },
    HoudiniReverseTarget {
        label: "push_edge",
        symbol_fragment: "?pushEdge@?$DecimatorT@M@GU_PolyReduce2@@",
        tier: 1,
    },
    HoudiniReverseTarget {
        label: "edge_collapse_allowed",
        symbol_fragment: "?isEdgeCollapseAllowed@?$DecimatorT@M@GU_PolyReduce2@@",
        tier: 1,
    },
    HoudiniReverseTarget {
        label: "edge_collapse_inversion",
        symbol_fragment: "?hedgeCollapseCausesInversion@?$DecimatorT@M@GU_PolyReduce2@@",
        tier: 1,
    },
    HoudiniReverseTarget {
        label: "point_vertex_wedge_representatives",
        symbol_fragment: "?getPointVtxWedgeRepVtxs@?$DecimatorT@M@GU_PolyReduce2@@",
        tier: 1,
    },
    HoudiniReverseTarget {
        label: "hedge_collapse_position",
        symbol_fragment: "?hedgeCollapsePos@?$DecimatorT@M@GU_PolyReduce2@@",
        tier: 1,
    },
    HoudiniReverseTarget {
        label: "attribute_collapse_cost",
        symbol_fragment: "?calcAttribCollapseCost@?$DecimatorT@M@GU_PolyReduce2@@",
        tier: 1,
    },
    HoudiniReverseTarget {
        label: "add_boundary_quadrics",
        symbol_fragment: "?addBoundaryQuadrics@?$DecimatorT@M@GU_PolyReduce2@@",
        tier: 1,
    },
    HoudiniReverseTarget {
        label: "add_seam_quadrics",
        symbol_fragment: "?addSeamQuadrics@?$DecimatorT@M@GU_PolyReduce2@@",
        tier: 1,
    },
    HoudiniReverseTarget {
        label: "find_initial_quad_rings",
        symbol_fragment: "?findInitialQuadRings@?$DecimatorT@M@GU_PolyReduce2@@",
        tier: 1,
    },
    HoudiniReverseTarget {
        label: "ring_collapse_cost",
        symbol_fragment: "?calcRingCollapseCost@?$DecimatorT@M@GU_PolyReduce2@@",
        tier: 1,
    },
    HoudiniReverseTarget {
        label: "collapse_hedges",
        symbol_fragment: "?collapseHedges@?$DecimatorT@M@GU_PolyReduce2@@",
        tier: 1,
    },
    HoudiniReverseTarget {
        label: "refresh_collapse_data",
        symbol_fragment: "?refreshCollapseData@?$DecimatorT@M@GU_PolyReduce2@@",
        tier: 1,
    },
    HoudiniReverseTarget {
        label: "find_view_groups",
        symbol_fragment: "?findViewGroups@?$DecimatorT@M@GU_PolyReduce2@@",
        tier: 2,
    },
];

const GEO_POLY_INTERFACE_TARGETS: &[HoudiniReverseTarget] = &[
    HoudiniReverseTarget {
        label: "find_primary_hedge",
        symbol_fragment: "?findPrimary@GEO_PolyInterface@@",
        tier: 0,
    },
    HoudiniReverseTarget {
        label: "contract_hedge",
        symbol_fragment: "?contract@GEO_PolyInterface@@",
        tier: 0,
    },
    HoudiniReverseTarget {
        label: "sym_link",
        symbol_fragment: "?symLink@GEO_PolyInterface@@",
        tier: 0,
    },
];

const MEASURE_CURVATURE_TARGETS: &[HoudiniReverseTarget] = &[HoudiniReverseTarget {
    label: "compute_curvature",
    symbol_fragment: "?computeCurvature@GU_Measure@@",
    tier: 0,
}];

macro_rules! reverse_target {
    ($label:literal, $symbol:literal, $tier:literal) => {
        HoudiniReverseTarget {
            label: $label,
            symbol_fragment: $symbol,
            tier: $tier,
        }
    };
}

const GROUP_SOP_TARGETS: &[HoudiniReverseTarget] = &[
    reverse_target!("promote_cook_verb", "?cookVerb@SOP_GroupPromote@@", 0),
    reverse_target!("range_cook_verb", "?cookVerb@SOP_GroupRange@@", 0),
    reverse_target!("expand_cook_verb", "?cookVerb@SOP_GroupExpand@@", 0),
    reverse_target!("find_path_cook_verb", "?cookVerb@SOP_GroupFindPath@@", 0),
    reverse_target!(
        "promote_build_from_op",
        "?buildFromOp@SOP_GroupPromoteParms@@",
        1
    ),
    reverse_target!(
        "range_build_from_op",
        "?buildFromOp@SOP_GroupRangeParms@@",
        1
    ),
    reverse_target!(
        "expand_build_from_op",
        "?buildFromOp@SOP_GroupExpandParms@@",
        1
    ),
    reverse_target!(
        "find_path_build_from_op",
        "?buildFromOp@SOP_GroupFindPathParms@@",
        1
    ),
];

const GROUP_DEGENERATE_TARGETS: &[HoudiniReverseTarget] = &[
    reverse_target!("point_group_degenerate", "?degenerate@GU_PointGroup@@", 0),
    reverse_target!("edge_group_degenerate", "?degenerate@GU_EdgeGroup@@", 0),
    reverse_target!("vertex_group_degenerate", "?degenerate@GU_VertexGroup@@", 0),
    reverse_target!(
        "primitive_group_degenerate",
        "?degenerate@GU_PrimGroup@@",
        0
    ),
];

const GROUP_PATH_GU_TARGETS: &[HoudiniReverseTarget] = &[
    reverse_target!(
        "edge_loop_path",
        "?edgeLoop@GU_LoopHelper@@QEAA_NVGU_PathHedge@@0W4GU_LoopType@@_N2AEA_NAEAV?$UT_Array@VGU_PathSHedge@@@@@Z",
        0
    ),
    reverse_target!(
        "edge_ring_path",
        "?edgeRing@GU_LoopHelper@@QEAA_NVGU_PathHedge@@0W4GU_LoopType@@AEAV?$UT_Array@VGU_PathSHedge@@@@@Z",
        0
    ),
    reverse_target!(
        "extend_loop",
        "?extendLoop@GU_LoopHelper@@QEAA_NAEAV?$UT_Array@VGU_PathSHedge@@@@@Z",
        0
    ),
    reverse_target!(
        "extend_ring",
        "?extendRing@GU_LoopHelper@@QEAA_NAEAV?$UT_Array@VGU_PathSHedge@@@@_N@Z",
        0
    ),
    reverse_target!(
        "point_loop_path",
        "?pointLoop@GU_LoopHelper@@QEAA_N_J0W4GU_LoopType@@AEAV?$UT_Array@VGU_PathSHedge@@@@@Z",
        0
    ),
    reverse_target!(
        "primitive_loop_path",
        "?primLoop@GU_LoopHelper@@QEAA_N_J0W4GU_LoopType@@AEAV?$UT_Array@VGU_PathSHedge@@@@@Z",
        0
    ),
    reverse_target!(
        "vertex_loop_path",
        "?vertexLoop@GU_LoopHelper@@QEAA_N_JH0HW4GU_LoopType@@AEAV?$UT_Array@VGU_PathSHedge@@@@@Z",
        0
    ),
    reverse_target!(
        "set_collision_group",
        "?setCollisionGroup@GU_LoopHelper@@QEAAXPEBVGA_Group@@_N1@Z",
        0
    ),
    reverse_target!(
        "set_previous_path",
        "?setPreviousPath@GU_LoopHelper@@QEAAXPEBVGA_Group@@_N@Z",
        0
    ),
    reverse_target!(
        "find_edge_loop",
        "?findEdgeLoop@GU_LoopHelper@@AEAA_NAEAV?$UT_Array@VGU_PathSHedge@@@@W4GU_LoopType@@_N@Z",
        1
    ),
    reverse_target!(
        "find_edge_ring",
        "?findEdgeRing@GU_LoopHelper@@AEAA_NAEAV?$UT_Array@VGU_PathSHedge@@@@W4GU_LoopType@@_N@Z",
        1
    ),
    reverse_target!(
        "shortest_path_find",
        "?findPath@?$GU_PathFinder@Vgu_ShortestPathCost@@@@QEAA?AVgu_ShortestPathCost@@AEAV?$UT_Array@VGU_PathSHedge@@@@W4Mask@GU_EdgeSuccessor@@@Z",
        1
    ),
    reverse_target!(
        "edge_loop_find_path",
        "?findPath@?$GU_PathFinder@Vgu_EdgeLoopCost@@@@QEAA?AVgu_EdgeLoopCost@@AEAV?$UT_Array@VGU_PathSHedge@@@@W4Mask@GU_EdgeSuccessor@@@Z",
        1
    ),
    reverse_target!(
        "edge_ring_find_path",
        "?findPath@?$GU_PathFinder@Vgu_EdgeRingCost@@@@QEAA?AVgu_EdgeRingCost@@AEAV?$UT_Array@VGU_PathSHedge@@@@W4Mask@GU_EdgeSuccessor@@@Z",
        1
    ),
    reverse_target!(
        "edge_ring_find_dual_path",
        "?findDualPath@?$GU_PathFinder@Vgu_EdgeRingCost@@@@QEAA?AVgu_EdgeRingCost@@AEAV?$UT_Array@VGU_PathSHedge@@@@W4Mask@GU_EdgeSuccessor@@@Z",
        1
    ),
];

const APEX_CORE_TARGETS: &[HoudiniReverseTarget] = &[
    reverse_target!("compile_program", "?compileProgram@APEX_Graph@apex@@", 0),
    reverse_target!("execute_program", "?executeProgram@APEX_Graph@apex@@", 0),
    reverse_target!("evaluate_output", "?evaluateOutput@APEX_Graph@apex@@", 0),
    reverse_target!("evaluate_outputs", "?evaluateOutputs@APEX_Graph@apex@@", 0),
    reverse_target!(
        "evaluate_graph_partial",
        "?evaluateGraphPartial@APEX_Graph@apex@@",
        0
    ),
    reverse_target!(
        "build_invocations",
        "?buildInvocations@APEX_Graph@apex@@",
        0
    ),
    reverse_target!(
        "compute_allocation",
        "?computeAllocation@APEX_Graph@apex@@",
        0
    ),
    reverse_target!(
        "compute_in_place_chains",
        "?computeInPlaceChains@APEX_Graph@apex@@",
        0
    ),
    reverse_target!(
        "build_parm_dependencies",
        "?buildParmDependencies@APEX_Graph@apex@@",
        0
    ),
    reverse_target!(
        "dirty_nodes_pre_evaluation",
        "?getDirtyNodesPreEvaluation@APEX_Graph@apex@@",
        0
    ),
    reverse_target!(
        "update_dirty_pre_evaluation",
        "?updateDirtyPreEvaluation@APEX_Graph@apex@@",
        0
    ),
    reverse_target!(
        "update_dirty_post_evaluation",
        "?updateDirtyPostEvaluation@APEX_Graph@apex@@",
        0
    ),
    reverse_target!("needs_recompile", "?needsRecompile@APEX_Graph@apex@@", 0),
    reverse_target!(
        "program_evaluate",
        "?evaluate@APEX_Program@apex@@QEAAXXZ",
        0
    ),
    reverse_target!(
        "program_evaluate_subprogram",
        "?evaluate@APEX_Program@apex@@QEAAXAEBVAPEX_SubProgram@2@@Z",
        0
    ),
    reverse_target!(
        "program_evaluate_counter",
        "?evaluateProgramCounter@APEX_Program@apex@@",
        0
    ),
    reverse_target!(
        "program_counter_invoke",
        "?invoke@APEX_ProgramCounter@apex@@",
        0
    ),
    reverse_target!(
        "program_counter_advance",
        "?advance@APEX_ProgramCounter@apex@@",
        0
    ),
    reverse_target!("callback_compile", "?compile@APEX_FunctionBase@apex@@", 0),
    reverse_target!("callback_execute", "?execute@APEX_FunctionBase@apex@@", 0),
    reverse_target!("set_inputs_dirty", "?setInputsDirty@APEX_Graph@apex@@", 0),
    reverse_target!("set_program_dirty", "?setDirty@APEX_Program@apex@@", 0),
    reverse_target!(
        "build_all_output_partial",
        "?buildAllOutputPartial@APEX_Graph@apex@@",
        1
    ),
    reverse_target!(
        "build_all_node_partial",
        "?buildAllNodePartial@APEX_Graph@apex@@",
        1
    ),
    reverse_target!("build_subgraph", "?buildSubGraph@APEX_Graph@apex@@", 1),
    reverse_target!(
        "compute_poisoned_ports",
        "?computePoisonedPorts@APEX_Graph@apex@@",
        1
    ),
    reverse_target!(
        "compile_callback_instances",
        "?compileCallbackInstances@APEX_Graph@apex@@",
        1
    ),
    reverse_target!(
        "get_dirty_parameters",
        "?getDirtyParameters@APEX_Graph@apex@@",
        1
    ),
    reverse_target!(
        "set_program_all_dirty",
        "?setAllDirty@APEX_Program@apex@@",
        1
    ),
    reverse_target!(
        "append_invocation",
        "?appendInvocation@APEX_Program@apex@@",
        1
    ),
    reverse_target!("bind_dicts", "?bindDicts@APEX_Program@apex@@", 1),
    reverse_target!(
        "registry_add_callback",
        "?addCallbackImpl@APEX_Registry@apex@@",
        1
    ),
    reverse_target!(
        "registry_get_callback",
        "?getCallback@APEX_Registry@apex@@",
        1
    ),
    reverse_target!(
        "registry_get_subgraph",
        "?getSubGraph@APEX_Registry@apex@@",
        1
    ),
    reverse_target!(
        "registry_get_signature",
        "?getSignature@APEX_Registry@apex@@",
        1
    ),
    reverse_target!("buffer_allocate", "?allocate@APEX_Buffer@apex@@", 1),
    reverse_target!("buffer_append", "?append@APEX_Buffer@apex@@", 1),
    reverse_target!(
        "buffer_find_typed",
        "?findTypedBuffer@APEX_Buffer@apex@@",
        1
    ),
    reverse_target!(
        "set_port_in_place_condition",
        "?setPortInPlaceCondition@APEX_GraphData@apex@@",
        1
    ),
    reverse_target!(
        "validate_dynamic_ports",
        "?validateDynamicPorts@APEX_Graph@apex@@",
        1
    ),
    reverse_target!("validate_inputs", "?validateInputs@APEX_Graph@apex@@", 1),
    reverse_target!(
        "find_in_place_end_ports",
        "?findInPlaceEndPorts@APEX_Graph@apex@@",
        1
    ),
    reverse_target!(
        "partial_graph_in_place_children",
        "?partialGraphInplaceChildren@APEX_Graph@apex@@",
        1
    ),
];

const APEX_ANIMATION_TARGETS: &[HoudiniReverseTarget] = &[
    reverse_target!(
        "scene_invoke_evaluate_output",
        "?evaluateOutput@APEXA_SceneInvoke@@",
        0
    ),
    reverse_target!(
        "scene_invoke_update_time",
        "?updateEvaluationTime@APEXA_SceneInvoke@@",
        0
    ),
    reverse_target!("scene_add_rig", "?addRigToEvaluation@APEXA_Scene@@", 0),
    reverse_target!("scene_evaluate_only", "?evaluateOnly@APEXA_Scene@@", 0),
    reverse_target!(
        "scene_evaluate_animation_bindings",
        "?evaluateAnimationBindings@APEXA_Scene@@",
        0
    ),
    reverse_target!(
        "scene_evaluate_tracked_rig_data",
        "?evaluateTrackedRigData@APEXA_Scene@@",
        0
    ),
    reverse_target!(
        "scene_update_dirty_rigs",
        "?updateDirtyRigs@APEXA_Scene@@",
        0
    ),
    reverse_target!(
        "scene_update_evaluation_parms",
        "?updateEvaluationParms@APEXA_Scene@@",
        0
    ),
    reverse_target!(
        "scene_enable_parallel_evaluation",
        "?enableParallelEvaluation@APEXA_Scene@@",
        0
    ),
    reverse_target!(
        "scene_cache_animation_data",
        "?cacheAnimationData@APEXA_Scene@@",
        0
    ),
    reverse_target!("scene_graph_update_rig", "?updateRig@APEXA_SceneGraph@@", 0),
    reverse_target!(
        "scene_graph_evaluate_output_data",
        "?evaluateOutputData@APEXA_SceneGraph@@QEAA",
        0
    ),
    reverse_target!(
        "scene_graph_evaluate_tracked_outputs",
        "?evaluateTrackedOutputs@APEXA_SceneGraph@@",
        0
    ),
    reverse_target!("scene_graph_set_dirty", "?setDirty@APEXA_SceneGraph@@", 0),
    reverse_target!(
        "scene_graph_rig_needs_update",
        "?rigNeedsUpdate@APEXA_SceneGraph@@",
        0
    ),
    reverse_target!(
        "scene_graph_set_evaluation_parms",
        "?setEvaluationParms@APEXA_SceneGraph@@",
        0
    ),
    reverse_target!(
        "scene_graph_cache_animation_data",
        "?cacheAnimationData@APEXA_SceneGraph@@",
        0
    ),
    reverse_target!(
        "scene_graph_update_time_key",
        "?updateTimeKey@APEXA_SceneGraph@@",
        0
    ),
    reverse_target!(
        "scene_graph_update_frame_key",
        "?updateFrameKey@APEXA_SceneGraph@@",
        0
    ),
    reverse_target!(
        "channels_evaluate_animation_stack",
        "?evaluateAnimationStack@APEXA_ChannelPrimBindings@@",
        0
    ),
    reverse_target!(
        "channels_evaluate_animation_stack_parm",
        "?evaluateAnimationStackForParm@APEXA_ChannelPrimBindings@@",
        0
    ),
    reverse_target!(
        "channels_evaluate_primitives",
        "?evaluateChannelPrims@APEXA_ChannelPrimBindings@@",
        0
    ),
    reverse_target!(
        "anim_stack_evaluation_data",
        "?getEvaluationData@APEXA_AnimStack@@",
        0
    ),
    reverse_target!(
        "anim_layer_evaluation_data",
        "?getEvaluationData@APEXA_AnimLayer@@",
        0
    ),
    reverse_target!(
        "cache_manager_grab_scene",
        "?grabScene@APEXA_CacheManager@@",
        0
    ),
    reverse_target!(
        "cache_manager_update_scene",
        "?updateSceneGeometry@APEXA_CacheManager@@",
        0
    ),
    reverse_target!(
        "scene_graph_evaluate_output_data_internal",
        "?evaluateOutputData@APEXA_SceneGraph@@AEAA",
        1
    ),
    reverse_target!(
        "scene_graph_build_control_graph",
        "?buildControlGraph@APEXA_SceneGraph@@",
        1
    ),
    reverse_target!(
        "scene_graph_build_control_manager",
        "?buildControlManager@APEXA_SceneGraph@@",
        1
    ),
    reverse_target!(
        "scene_graph_build_control_skeleton",
        "?buildControlSkeleton@APEXA_SceneGraph@@",
        1
    ),
    reverse_target!(
        "scene_graph_build_control_template",
        "?buildControlTemplate@APEXA_SceneGraph@@",
        1
    ),
    reverse_target!("scene_load_rig_graph", "?loadRigGraph@APEXA_Scene@@", 1),
    reverse_target!(
        "scene_initialize_anim_stack",
        "?initializeAnimStack@APEXA_Scene@@",
        1
    ),
    reverse_target!(
        "scene_update_animation_bindings",
        "?updateAnimationBindings@APEXA_Scene@@",
        1
    ),
    reverse_target!(
        "scene_update_from_rig_parameters",
        "?updateFromRigParameterBindings@APEXA_Scene@@",
        1
    ),
    reverse_target!(
        "scene_update_constraints",
        "?updateConstraintManagers@APEXA_Scene@@",
        1
    ),
    reverse_target!("scene_prepare_caching", "?prepCaching@APEXA_Scene@@", 1),
    reverse_target!("scene_start_caching", "?startCaching@APEXA_Scene@@", 1),
    reverse_target!("scene_halt_caching", "?haltCaching@APEXA_Scene@@", 1),
    reverse_target!(
        "channels_compute_layer_value",
        "?computeLayerValueForParm@APEXA_ChannelPrimBindings@@",
        1
    ),
];
const HOUDINI_FAST_REVERSE_SCHEMA: u32 = 2;
