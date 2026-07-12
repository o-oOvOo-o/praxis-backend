
fn print_usage() {
    println!("c3d-devflywheeltool");
    println!("  toolbox [--json]");
    println!("  toolchain doctor|list|sync [--json] [--strict] [--offline] [--repair]");
    println!("  reverse --node <Node> [--json]  # includes node_surface_contract");
    println!("  ledger [--operator <Name>] [--node <Node>] [--all] [--json]");
    println!("  ledger-hygiene [--json] [--strict]");
    println!("  open-frontier [--node <Node>] [--all] [--json]");
    println!("  contracts --node <Node> [--json]");
    println!("  status --node <Node> [--json]");
    println!("  praxis-panel [--node <Node>] [--json]");
    println!(
        "  goal-chain-status [--nodes ThermalShaper,Weathering,Snowfield,Glacier,Debris] [--json]"
    );
    println!(
        "  goal-chain-bench [--resolution N] [--source cone|ridge|basin|sine|ramp-x] [--thermal-backend native-fast|native-reference] [--weathering-backend native-fast|native-preview] [--snowfield-backend native-fast|native-preview|gaea-bridge] [--glacier-backend native-fast|native-preview|gaea-bridge] [--debris-point-cloud] [--repeat N] [--target-total-ms N] [--require-all-pass] [--require-performance] [--run] [--json] [--direct-bin]"
    );
    println!("  acceptance-matrix [--node <Node>] [--json]");
    println!(
        "  frontier-health [--suite quick|foundation|frontier|all] [--epsilon N(default 0)] [--case-timeout-seconds N(default 90)] [--run] [--json] [--direct-bin]"
    );
    println!("  graph [--json]");
    println!("  impact --operator <ContractOrSubstrate> [--json]");
    println!("  plan --node <Node> [--json]");
    println!("  export-ui [--json]");
    println!("  blackbox-scan [--json] [--dry-run]");
    println!("  verify --node <Node> [--json]");
    println!("  certify --node Mountain [--run] [--json] [--direct-bin]");
    println!(
        "  sweep --node Mountain [--samples N|--seconds N] [--rng-seed N] [--run] [--json] [--direct-bin] [--keep-going] [-- fixed Mountain params]"
    );
    println!(
        "  raw-gate --node Mountain [--samples N|--seconds N] [--candidates native_gpu_wave,...] [--epsilon N(default 0)] [--rng-seed N] [--resolution-choices A,B] [--run] [--json] [--direct-bin] [--fresh-bridge-cache] [--require-exact] [--require-gpu-active] [--keep-going] [-- fixed Mountain params]"
    );
    println!(
        "  gaea-project --preset volcano-snow-material [--template PATH] [--output PATH] [--resolution N] [--volcano-scale N] [--volcano-height N] [--mouth N] [--bulk N] [--snow-intensity N] [--snow-mass N] [--snow-direction E] [--tree-count N] [--tree-size N] [--tree-altitude-max N] [--tree-slope-max N] [--open] [--run] [--json]"
    );
    println!("  gaea-viewport-reverse [--gaea-dir PATH] [--run] [--json]");
    println!(
        "  gaea-app-bench --node Mountain|Debris [--terrain PATH] [--node-id N] [--resolution N] [--buildpath PATH] [--gaea-dir PATH] [--timeout-seconds N] [--no-new-console] [--run] [--json]"
    );
    println!(
        "  perf-migrate --node Mountain [--candidates native_live,native_gpu_wave,...] [--samples N|--seconds N] [--gaea-app-baseline-ms N] [--target-speedup N(default 5)] [--require-speedup] [--run] [--json] [--direct-bin] [--fresh-bridge-cache]"
    );
    println!(
        "  gpu-sweep --node Mountain [--lhs native_live|native_gpu_wave|native_gpu_exact] [--rhs gaea_bridge] [--samples N|--seconds N] [--run] [--json] [--direct-bin] [--release-bin] [--fresh-bridge-cache] [--allow-stale-direct-bin] [--skip-native-preflight] [--gpu-wave-policy auto|force|off] [--gpu-wave-min-packets N] [--gpu-exact-barrier|--cpu-commit-barrier] [--cpu-trace-barrier] [--require-gpu-active] [--resident-wave-count N] [--resident-min-level N] [--gaea-app-baseline-ms N] [--min-gaea-app-speedup N] [--min-bridge-speedup N(diagnostic-only)] [--max-gpu-readbacks N] [--max-gpu-submits N] [--mean-abs-norm-limit N] [--rmse-norm-limit N] [--max-abs-norm-limit N]"
    );
    println!(
        "  live-heightfield-audit [--bridge-addr HOST:PORT] [--source-type Mountain] [--target Scree,Stratify,Outcrops,RockMap] [--resolution N] [--timeout-ms N] [--keep-nodes] [--require-all-pass] [--run] [--json]"
    );
    println!(
        "  gpu-preview --node Mountain [--samples N] [--repeat N] [--preview-axis N] [--preview-ms-budget N] [--prewarm] [--run] [--json] [--direct-bin] [--require-all-pass]"
    );
    println!(
        "  gpu-candidate-sweep --node Mountain [--candidates native_gpu_exact,native_gpu_wave,native_gpu_shader_ridge,native_gpu_resident_basic] [--rhs gaea_bridge] [--samples N|--seconds N] [--style-choices basic,old] [--run] [--json] [--direct-bin] [--require-all-pass]"
    );
    println!(
        "  gpu-stage-audit --node Mountain [--stage all|voronoi_base,...] [--run] [--json] [--direct-bin] [--require-exact]"
    );
    println!(
        "  gpu-substrate --node Mountain [--source-resolution 512x384] [--target-resolution 128x96] [--layers 4] [--epsilon N] [--skip-seed-packets] [--seed-packets-only] [--run] [--json] [--direct-bin] [--require-all-pass]"
    );
    println!(
        "  gpu-wave --node Mountain [--case old_baseline|all|custom] [--epsilon N] [--run] [--json] [--direct-bin] [--require-all-pass] [--require-exact] [--require-gpu-active] [--gpu-wave-policy auto|force|off] [--gpu-wave-min-packets N] [--gpu-exact-barrier|--cpu-commit-barrier] [--cpu-trace-barrier] [--trace-probe] [--resident-wave-loop] [--resident-layer-loop] [--resident-layer-cpu-shape-loop] [--resident-wave-count N(default 1)] [--resident-wave-counts A,B] [--resident-min-level N(default 4)] [--resident-min-levels A,B] [--wave-writeback-min-level N(default resident-min when resident)] [--max-gpu-readbacks N] [--max-gpu-submits N] [--max-gpu-cpu-ratio N] [--policy-gpu-cpu-ratio N(default 0.95)] [-- Mountain params]"
    );
    println!(
        "  gpu-resident-replay --node Mountain [--case old_baseline] [--resident-wave-count 1] [--resident-min-level 4] [--trace-probe] [--cpu-trace-barrier] [--cpu-commit-barrier] [--epsilon N] [--pe-profile] [--run] [--json] [--direct-bin] [--require-all-pass]"
    );
    println!(
        "  heightfield-art-status [--target Scree,Stratify,Outcrops,RockMap|GroundTexture|all] [--require-all-pass] [--require-goal-complete] [--json]  # includes default Mountain display gate"
    );
    println!(
        "  heightfield-art-gaea-baseline [--target Scree,Stratify,Outcrops,RockMap|all] [--samples N] [--require-all-pass] [--run] [--json]  # official Gaea inner timing baseline"
    );
    println!("  mountain-display-log-audit [--log PATH] [--require-all-pass] [--json]");
    println!(
        "  island-process-probe --node Island [--case NAME] [--resolution N] [--size N] [--chaos N] [--seed N] [--input-map TOKEN] [--epsilon N] [--require-pass] [--run] [--json] [--direct-bin] [--gaea-dir PATH]"
    );
    println!(
        "  island-process-sweep --node Island [--case NAME] [--samples N] [--rng-seed N] [--resolution-choices A,B] [--epsilon N] [--require-all-pass] [--keep-going] [--run] [--json] [--direct-bin] [--gaea-dir PATH]"
    );
    println!(
        "  probe-bin --bin gaea_<probe_bin> [--direct-bin] [--release-bin] [--no-incremental] [--file-capture] [--run] [--json] [-- probe args]"
    );
    println!(
        "  canyon-bridge-probe --node Canyon [--case NAME] [--resolution N] [--style Classic|Eroded|Eroded2|Strata|Both] [--alternate-style true|false] [--run] [--json] [--gaea-dir PATH]"
    );
    println!(
        "  canyon-compare --node Canyon [--resolution N] [--style Classic|Eroded|Eroded2|Strata|Both] [--epsilon N] [--matrix focused] [--direct-bin] [--run] [--json]"
    );
    println!(
        "  easy-erosion-compare --node EasyErosion [--matrix focused|examples|all] [--resolution N] [--case LABEL] [--epsilon N] [--repeat N] [--target-speedup N] [--require-all-pass] [--require-exact] [--require-speedup] [--direct-bin] [--run] [--json]"
    );
    println!(
            "  debris-compare --node Debris [--matrix focused|single] [--resolution N] [--source ramp-x|ramp-y|cone|sine|checker] [--emitter none|left-band|center-disk|checker] [--debris-amount N] [--repeat N] [--target-speedup N] [--gaea-app-baseline-ms N] [--target-gaea-speedup N] [--compare-bridge] [--gaea-harness-exe PATH] [--require-exact] [--require-speedup] [--require-gaea-speedup] [--require-bridge-exact] [--direct-bin] [--run] [--json]"
        );
    println!(
        "  rugged-stage-compare --node Rugged [--matrix focused|examples|all] [--surface m3|m4|m5|m6] [--resolution N] [--terrain-width N] [--terrain-height N] [--scale N] [--seed N] [--epsilon N] [--repeat N] [--target-speedup N] [--require-all-pass] [--require-exact] [--require-speedup] [--direct-bin] [--run] [--json]"
    );
    println!(
        "  mountain-side-compare --node MountainSide [--resolution N] [--scale N] [--detail N] [--type Slope|Peak] [--style Basic|Eroded|Old|Alpine|Strata] [--direction DEG] [--seed N] [--epsilon N] [--matrix focused|full-promotion] [--require-exact] [--require-all-pass] [--direct-bin] [--run] [--json]"
    );
    println!(
        "  stratify-compare [--node Stratify] [--resolution N] [--input-map TOKEN] [--spacing F] [--octaves N] [--intensity F] [--shape F] [--seed N] [--tilt-amount F] [--direction N] [--sweep N] [--native-only] [--require-exact|--require-accepted] [--direct-bin] [--run] [--json]"
    );
    println!(
        "  fractal-terrace-internals --node FractalTerraces [--matrix focused|production] [--input-map TOKEN] [--spacing F] [--octaves N] [--intensity F] [--shape F] [--seed N] [--tilt-amount F] [--tilt-seed N] [--direction N] [--epsilon N] [--run] [--json] [--direct-bin] [--require-all-pass]"
    );
    eprintln!(
        "  fractal-terraces-bridge-probe [--node FractalTerraces] [--matrix focused|production] [--resolution N] [--source rampx|rampy|checker|checkerfine|cone|radial|impulse|cornerimpulse|edgeimpulse] [--modulator-source none|rampx|rampy|checker|checkerfine|cone|radial|impulse|cornerimpulse|edgeimpulse] [--spacing F] [--octaves N] [--intensity F] [--shape F] [--seed N] [--shapes-separation true|false] [--macro-octaves N] [--micro-shape F] [--character F] [--thickness-uniformity F] [--hardness-uniformity F] [--strata-details F] [--protect-range true|false] [--apply-tilt true|false] [--tilt-amount F] [--tilt-seed N] [--direction N] [--warp-amount F] [--warp-size F] [--warp-style A|B|C|D] [--native-repeat N] [--target-speedup N] [--require-speedup] [--epsilon N] [--direct-bin] [--run] [--json]"
    );
    println!(
        "  terraces-compare --node Terraces [--matrix focused] [--resolution N] [--input-map TOKEN] [--num N] [--uniformity F] [--steepness F] [--intensity F] [--seed N] [--force-zero true|false] [--epsilon N] [--repeat N] [--target-speedup N] [--run] [--json] [--direct-bin] [--require-all-pass] [--require-speedup]"
    );
    println!(
        "  ridge-compare --node Ridge [--resolution N] [--terrain-width N] [--terrain-height N] [--scale F] [--height F] [--definition F] [--seed N] [--scale-x F] [--scale-y F] [--sweep N] [--sweep-seed N] [--native-only] [--repeat N] [--direct-bin] [--run] [--json] [--require-exact]"
    );
    println!(
        "  crumble-compare --node Crumble [--matrix focused] [--resolution N] [--input KIND] [--duration F] [--strength F] [--coverage F] [--horizontal F] [--vertical F] [--rock-hardness F] [--edge F] [--downcutting F] [--depth F] [--epsilon N] [--repeat N] [--run] [--json] [--require-all-pass]"
    );
    println!(
        "  slump-compare --node Slump [--matrix focused|production] [--resolution N] [--scale F] [--style A|B|C|D] [--seed N] [--epsilon N] [--repeat N] [--target-speedup N] [--run] [--json] [--direct-bin] [--require-all-pass] [--require-speedup]"
    );
    println!(
        "  stones-compare --node Stones [--matrix focused] [--resolution N] [--input-map TOKEN] [--scale F] [--height F] [--density F] [--seed N] [--epsilon N] [--repeat N] [--run] [--json] [--direct-bin] [--require-all-pass]"
    );
    println!(
        "  scree-compare --node Scree [--bridge-dir PATH] [--prefix NAME] [--source flat|cone|rampy|checker] [--height-map TOKEN] [--resolution N] [--scale F] [--height F] [--density N] [--spread F] [--edge F] [--seed N] [--epsilon N] [--repeat N] [--native-only] [--run] [--json] [--direct-bin]"
    );
    println!(
        "  rock-core-compare --node RockCore|Outcrops [--matrix focused] [--case large_default_32|noise_defaults_32|outcrops_core_defaults_cone_32|outcrops_node_defaults_cone_32|outcrops_node_defaults_flat_16] [--oracle-root PATH] [--epsilon N] [--repeat N] [--dump-dir PATH] [--direct-bin] [--run] [--json] [--require-all-pass] [--require-exact]"
    );
    println!(
        "  rock-noise-compare --node RockNoise [--matrix focused|examples|all] [--height-map TOKEN] [--resolution N] [--terrain-width N] [--terrain-height N] [--size-x N] [--size-y N] [--variety N] [--octaves N] [--seed N] [--epsilon N] [--repeat N] [--target-speedup N] [--dump-dir PATH] [--direct-bin] [--run] [--json] [--require-all-pass] [--require-exact] [--require-speedup]"
    );
    println!(
        "  combiner-mountain-connected-probe [--node Combiner] [--resolution N] [--mountain-style Basic|Eroded|Old|Alpine|Strata] [--mountain-bulk Low|Medium|High] [--mountain-scale N] [--mountain-height N] [--mountain-seed N] [--epsilon 0] [--repeat N] [--direct-bin] [--run] [--json]"
    );
    println!(
        "  combiner-compare [--node Combiner|Mix|Combiner.Insert|Combiner.SpectralBlend|ClassicCombiner|Masking.Mask] [--op mix|embed|insert|spectralblend|mask|classic] [--mode Blend|Add|Screen|...] [--classic-mode N] [--ratio N] [--extend N] [--threshold N] [--flatten N] [--boundary N] [--spectral-max true|false] [--resolution N] [--a-source TOKEN] [--b-source TOKEN] [--mask-source TOKEN] [--matrix p0|p1|classic|embed|insert|transpose|spectral|mountain-connected|acceptance|all] [--epsilon 0] [--repeat N] [--verify-gpu] [--run] [--json] [--direct-bin] [--require-pass]"
    );
    println!(
        "  slope-warp-compare --node SlopeWarp [--matrix focused|acceptance] [--input-map TOKEN] [--guide-map TOKEN] [--intensity N] [--iterations N] [--direction DEG] [--normalized true|false] [--quality 0|1|2|3] [--antialiasing 0|1|2] [--epsilon 0] [--repeat N] [--run] [--json] [--direct-bin] [--require-pass]"
    );
    println!(
        "  thermal-shaper-compare --node ThermalShaper [--matrix degenerate|focused|acceptance] [--map TOKEN] [--intensity TOKEN|null] [--terrain-width N] [--terrain-height N] [--scale N] [--influence N] [--shape N] [--microdetail-preservation N] [--epsilon 0] [--repeat N] [--target-speedup N] [--shape-step-multipliers CSV] [--shape-step-sweep START:END:COUNT] [--pass-budget-multipliers CSV] [--slope-multipliers CSV] [--slope-powers CSV] [--diagonal-weights CSV] [--mean-weights CSV] [--gradient-weights CSV] [--drop-diagonal-weights CSV] [--reconstruction-child-multipliers CSV] [--reconstruction-detail-multipliers CSV] [--edge-modes clamp,mirror,wrap] [--response-modes slope-relief-log,relief-log,slope-log,slope-relief-linear,relief-linear,slope-linear] [--terminal-pass-modes bridge-hybrid,fractional,full,ceil-fractional] [--run] [--json] [--direct-bin] [--release-bin] [--require-pass] [--require-exact] [--require-speedup]"
    );
    println!(
        "  thermal2-compare|thermal2-bridge-native-compare [--node Thermal2] [--matrix focused|all] [--map TOKEN] [--area TOKEN|null] [--sediment-removal-map TOKEN|null] [--terrain-width N] [--terrain-height N] [--duration N] [--strength N] [--anisotropy N] [--angle N] [--feature-scale N] [--sediment-removal N] [--epsilon N] [--repeat N] [--first] [--run] [--json] [--direct-bin] [--require-pass|--require-exact]"
    );
    println!(
        "  thermal2-bridge-probe|thermal2-probe [--node Thermal2] [--map TOKEN] [--area TOKEN|null] [--sediment-removal-map TOKEN|null] [--terrain-width N] [--terrain-height N] [--duration N] [--strength N] [--anisotropy N] [--angle N] [--feature-scale N] [--sediment-removal N] [--use-area-mask true|false] [--use-sediment-removal-mask true|false] [--dump-dir PATH] [--dump-prefix NAME] [--direct-bin] [--run] [--json]"
    );
    println!(
        "  directional-warp-compare --node DirectionalWarp [--matrix focused] [--resolution N] [--input-map TOKEN] [--control-map TOKEN] [--strength F] [--direction DEG] [--edge-mode Edge|Mirror] [--epsilon N] [--repeat N] [--verify-gpu] [--verify-handle-gpu] [--run] [--json] [--direct-bin] [--require-all-pass]"
    );
    println!(
        "  warp-compare --node Warp [--matrix focused|production] [--resolution N] [--input-map TOKEN] [--modulator-map TOKEN] [--size F] [--strength F] [--z-scale F] [--noise-type NAME] [--perturbation F] [--complexity N] [--roughness F] [--normalized true|false] [--edge-mode Edge|Mirror] [--modulation F] [--modulation-direction DEG] [--seed N] [--iterations N] [--mode Real|Virtual|Integral] [--epsilon N] [--repeat N] [--run] [--json] [--direct-bin] [--require-all-pass]"
    );
    println!(
        "  erosion-classic-bridge-probe --node Erosion [--resolution N] [--input-map TOKEN] [--duration F] [--rock-softness F] [--strength F] [--downcutting F] [--inhibition F] [--base-level F] [--real-scale true|false] [--feature-scale N] [--terrain-scale N] [--verticality N] [--debris F] [--volume F] [--sediment-removal F] [--area-effect None|ErosionStrength|RockSoftness|PrecipitationAmount] [--bias-type Altitude|Slope] [--bias F] [--reverse-bias true|false] [--seed N] [--aggressive-mode true|false] [--deterministic true|false] [--area-mask TOKEN] [--sediment-removal-mask TOKEN] [--run] [--json]"
    );
    println!(
        "  erosion-classic-substrate-compare --node Erosion [--resolution N] [--source flat|rampx|rampy|cone] [--duration F] [--strength F] [--terrain-scale N] [--verticality N] [--layer-iteration-scale F] [--max-steps N] [--post-schedule none|rivers-entry] [--include-traces] [--run] [--json]"
    );
    println!(
        "  erosion2-inhibitor-probe --node Erosion2 [--matrix focused] [--resolution N] [--source cone|radial|rampx|rampy|sine|checker|flat] [--mask none|flat|rampx|checker] [--enable true|false] [--slope-min N] [--slope-max N] [--altitude-min N] [--altitude-max N] [--reverse true|false] [--epsilon N] [--run] [--json] [--direct-bin] [--require-all-pass]"
    );
    println!(
        "  erosion2-compare --node Erosion2 [--matrix focused] [--resolution N] [--source cone|radial|rampx|rampy|sine|checker|flat] [--mask none|flat|rampx|checker] [--enable true|false] [--slope-min N] [--slope-max N] [--altitude-min N] [--altitude-max N] [--reverse true|false] [--epsilon N] [--run] [--json] [--direct-bin] [--require-all-pass]"
    );
    println!(
        "  crater-compare [--node Crater] [--resolution N] [--scale F] [--formation F] [--height F] [--seed N] [--x F] [--y F] [--sweep N] [--target-speedup N] [--native-only] [--require-all-pass] [--require-exact|--require-accepted] [--require-speedup] [--direct-bin] [--run] [--json]"
    );
    println!(
        "  craterfield-compare [--node CraterField] [--resolution N] [--scale N] [--depth N] [--density N] [--seed N] [--x N] [--y N] [--warp-row N] [--sweep N] [--native-only] [--profile-native] [--require-exact|--require-accepted] [--direct-bin] [--run] [--json]"
    );
    println!(
        "  transform-compare [--node Transform] [--matrix focused] [--resolution N] [--offset-x N] [--offset-y N] [--offset-z N] [--scale N] [--base-map TOKEN] [--rotate DEG] [--epsilon N] [--direct-bin] [--run] [--json] [--require-all-pass]"
    );
    println!(
        "  recurve-bridge-probe [--node Recurve] [--matrix focused] [--resize-only --resize-target N|--resize-target-width W --resize-target-height H] [--resolution N] [--source rampx|rampy|cone|basin|checker|flat] [--epsilon N] [--direct-bin] [--run] [--json]"
    );
    println!(
        "  blur-bridge-probe [--node Blur] [--matrix focused] [--resolution N] [--source rampx|rampy|cone|basin|checker|flat|impulse] [--radius N] [--epsilon N] [--repeat N] [--direct-bin] [--run] [--json] [--require-pass]"
    );
    println!(
        "  graphic-eq-bridge-probe [--node GraphicEQ] [--matrix focused] [--resolution N] [--source rampx|rampy|cone|basin|checker|flat|impulse] [--band1 N] ... [--band7 N] [--epsilon N] [--direct-bin] [--run] [--json]"
    );
    println!(
        "  deflate-bridge-probe [--node Deflate] [--matrix focused] [--resolution N] [--source rampx|rampy|cone|basin|checker|flat|impulse] [--amount N] [--epsilon N] [--direct-bin] [--run] [--json]"
    );
    println!(
        "  peaks-bridge-probe [--node Peaks] [--matrix focused] [--resolution N] [--source rampx|rampy|cone|basin|checker|flat|impulse] [--falloff N] [--precise true|false] [--epsilon N] [--direct-bin] [--run] [--json]"
    );
    println!(
        "  uplift-bridge-probe [--node Uplift] [--matrix focused] [--resolution N] [--passes N] [--scale N] [--height N] [--octaves N] [--direction N] [--jitter N] [--seed N] [--input-source none|rampx|rampy|cone|basin|checker|flat] [--epsilon N] [--direct-bin] [--run] [--json]"
    );
    println!(
        "  weathering-native-probe|weathering-probe [--node Weathering] [--resolution N] [--input sine|ramp-x|ramp-y|cone|checker|flat] [--scale N] [--creep N] [--amount N] [--dirt N] [--inverse] [--darker] [--compare-bridge] [--fresh-bridge-cache] [--ao-only] [--ao-timing-only] [--ao-normal-z-scale-sweep] [--ao-normal-z-scales CSV] [--prewarm] [--native-repeat N] [--epsilon N] [--target-speedup N] [--matrix focused] [--dump-dir PATH] [--require-all-pass] [--require-exact] [--require-speedup] [--direct-bin] [--file-capture] [--run] [--json]"
    );
    println!(
        "  dune-sea-native-probe|dune-sea-probe [--node DuneSea] [--resolution N] [--dune-type A|B|C|D] [--scale N] [--height N] [--direction N] [--chaos none|low|medium|high] [--undulation none|low|medium|high] [--softness N] [--sharpness N] [--seed N] [--require-pass] [--direct-bin] [--run] [--json]"
    );
    println!(
        "  dune-sea-compare|dune-sea-bridge-native-compare [--node DuneSea] [--resolution N] [--dune-type A|B|C|D] [--scale N] [--height N] [--direction N] [--chaos none|low|medium|high] [--undulation none|low|medium|high] [--softness N] [--sharpness N] [--seed N] [--terrain-width N] [--terrain-height N] [--epsilon N] [--repeat N] [--bridge-dump-dir PATH] [--height-sweep-min N] [--height-sweep-max N] [--height-sweep-step N] [--sweep-dump-root PATH] [--require-pass] [--require-exact] [--direct-bin] [--run] [--json]"
    );
    println!(
        "  flow-map-classic-compare [--node FlowClassic] [--map TOKEN] [--matrix focused|all] [--rainfall N] [--primary true|false] [--secondary true|false] [--tertiary true|false] [--quaternary true|false] [--simulate2x true|false] [--enhance N] [--quality 0|1|2|3] [--terrain-width N] [--terrain-height N] [--epsilon N] [--repeat N] [--dump-dir PATH] [--harness-exe PATH] [--require-pass] [--require-exact] [--direct-bin] [--run] [--json]"
    );
    println!(
        "  denoise-bridge-probe [--node Denoise] [--matrix focused] [--include-pixels] [--resolution N] [--source rampx|rampy|cone|basin|checker|flat|impulse] [--type OnePass|TwoPass|Pixels] [--amount N] [--passes N] [--epsilon N] [--direct-bin] [--run] [--json]"
    );
    println!(
        "  sharpen-bridge-probe [--node Sharpen] [--matrix focused] [--resolution N] [--source rampx|rampy|cone|basin|checker|flat|impulse] [--method Edge|Frequency] [--amount N] [--epsilon N] [--direct-bin] [--run] [--json]"
    );
    println!(
        "  gabor-bridge-probe [--node Gabor] [--matrix focused] [--resolution N] [--size N] [--entropy N] [--anisotropy N] [--azimuth DEG] [--gain N] [--seed N] [--input-source none|flat|rampx|rampy|checker|cone|radial|sine|impulse] [--aniso-source none|flat|rampx|rampy|checker|cone|radial|sine|impulse] [--epsilon N] [--direct-bin] [--run] [--json]"
    );
    println!(
        "  distress-bridge-probe [--node Distress] [--matrix focused] [--case LABEL] [--resolution N] [--epsilon N] [--require-exact] [--direct-bin] [--run] [--json]"
    );
    println!(
        "  sea-bridge-probe [--node Sea] [--resolution N] [--source rampx|rampy|cone|basin|checker|flat|radial] [--edge-source none|same|...] [--arrangement Global|Surrounding] [--level N] [--compare-native] [--epsilon N] [--matrix focused|surrounding-no-coastal|surrounding-coastal|coastal-diagnostic|branch-diagnostic|full-promotion] [--require-all-pass] [--require-exact] [--run] [--json] [--direct-bin]"
    );
    println!(
        "  flow-map-bridge-probe [--node FlowMap] [--matrix focused] [--resolution N] [--source rampx|rampy|cone|basin|checker|flat|radial] [--precipitation-source none|...] [--flow-length N] [--flow-volume N] [--seed N] [--compare-native] [--epsilon N] [--dump-dir PATH] [--require-all-pass] [--require-exact] [--run] [--json] [--direct-bin]"
    );
    println!(
        "  cracks-bridge-probe [--node Cracks] [--matrix focused] [--case LABEL|custom] [--resolution N] [--style Normal|Hard|Classic] [--octaves N] [--scale N] [--depth N] [--jitter N] [--warp-size N] [--warp-strength N] [--scale-x N] [--scale-y N] [--seed N] [--input-source none|rampx|rampy|checker|cone|radial|sine] [--epsilon N] [--repeat N] [--require-all-pass] [--require-exact] [--run] [--json] [--direct-bin]"
    );
    println!(
        "  distance-bridge-probe [--node Distance] [--matrix focused] [--case LABEL|custom] [--resolution N] [--method classic|rt] [--mode Asterisk|Pyramid] [--directions N] [--skew N] [--angle DEG] [--angular-jitter N] [--falloff N] [--threshold N] [--falloff-jitter N] [--invert-input true|false] [--invert-output true|false] [--multiply-by-input true|false] [--input-source rampx|rampy|cone|basin|checker|flat|radial|sine] [--epsilon N] [--repeat N] [--trace-directions] [--require-all-pass] [--require-exact] [--run] [--json] [--direct-bin]"
    );
    println!(
        "  plates-bridge-probe [--node Plates] [--matrix focused] [--case LABEL|custom] [--resolution N] [--scale N] [--range N] [--falloff N] [--warp N] [--angle DEG] [--seed N] [--input-source none|rampx|rampy|cone|basin|checker|flat|radial|sine] [--epsilon N] [--repeat N] [--require-all-pass] [--require-exact] [--run] [--json] [--direct-bin]"
    );
    println!(
        "  lake-bridge-probe [--node Lake] [--matrix focused] [--resolution N] [--source rampx|rampy|cone|basin|checker|flat] [--precipitation F] [--small-lakes F] [--fixed-threads true|false] [--compare-native] [--epsilon N] [--dump-dir PATH] [--require-all-pass] [--require-exact] [--run] [--json] [--direct-bin]"
    );
    println!(
        "  hydro-fix-bridge-probe [--node HydroFix] [--resolution N] [--source rampx|rampy|cone|basin|checker|flat|radial] [--downcutting F] [--compare-native] [--epsilon N] [--run] [--json] [--direct-bin]"
    );
    println!(
        "  snow-bridge-probe [--node Snow] [--matrix focused|examples|mountain-connected] [--resolution N] [--source rampx|rampy|cone|basin|checker|flat] [--height-input-json PATH|--mountain-bridge-input] [--mountain-style Basic|Eroded|Old|Alpine|Strata] [--mountain-bulk Low|Medium|High] [--mountain-scale N] [--mountain-height N] [--seed N] [--duration N] [--intensity N] [--compare-native] [--epsilon N] [--fresh-bridge-cache] [--target-speedup N] [--diagnostics-dir PATH] [--dump-dir PATH] [--require-all-pass] [--require-exact] [--require-speedup] [--run] [--json] [--direct-bin]"
    );
    println!(
        "  snow-mountain-connected-probe [--node Snow] [--matrix mountain-connected] [--resolution N] [--mountain-style Basic|Eroded|Old|Alpine|Strata] [--duration N] [--intensity N] [--epsilon N] [--fresh-bridge-cache] [--target-speedup N] [--require-all-pass] [--require-exact] [--require-speedup] [--run] [--json] [--direct-bin]"
    );
    println!(
        "  snowfield-bridge-probe [--node Snowfield] [--matrix focused|examples|mountain-connected] [--resolution N] [--source rampx|rampy|cone|basin|checker|flat] [--cascades N] [--duration N] [--intensity N] [--compare-native] [--epsilon N] [--fresh-bridge-cache] [--target-speedup N] [--require-all-pass] [--require-exact] [--require-speedup] [--run] [--json] [--direct-bin]"
    );
    println!(
        "  glacier-bridge-probe [--node Glacier] [--matrix focused|examples|branches|mountain-connected] [--resolution N] [--source rampx|rampy|cone|basin|checker|flat|radial] [--reference-source none|same|...] [--scale N] [--scale2 N] [--thickness N] [--height N] [--direction DEG] [--breakage N] [--rough-edges BOOL] [--chipped BOOL] [--secondary-breakage BOOL] [--diagonal-breakage BOOL] [--flow-breakage BOOL] [--substructure BOOL] [--target-speedup N] [--require-speedup] [--compare-native] [--fresh-bridge-cache] [--epsilon N] [--run] [--json] [--direct-bin]"
    );
    println!(
        "  aspect-bridge-probe [--node Aspect|Height|Slope|Angle|Curvature] [--operator height|slope|angle|curvature] [--matrix focused] [--mode bridge|native|compare] [--resolution N] [--source rampx|rampy|cone|basin|checker|flat|radial|sine|impulse] [--epsilon N] [--run] [--json] [--direct-bin]"
    );
    println!(
        "  gradient-bridge-probe --node LinearGradient|RadialGradient|Cone|Hemisphere [--matrix focused] [--resolution N] [--scale N] [--height N] [--x N] [--y N] [--flatten bool] [--direction DEG] [--edge Clip|Repeat|Mirror] [--input-source none|flat|rampx|checker|cone] [--epsilon N] [--direct-bin] [--run] [--json] [--require-all-pass]"
    );
    println!(
        "  slope-mask-bridge-probe --node SlopeMask [--matrix focused] [--resolution N] [--height-source flat|rampx|rampy|cone|basin|checker|sine|impulse] [--layer-source SOURCE] [--slope-type Accurate|Normalized] [--flow-mode SlopeOnly|Mask|Heightfield] [--min N] [--max N] [--falloff N] [--micro-accent N] [--epsilon N] [--direct-bin] [--run] [--json] [--require-all-pass]"
    );
    println!(
        "  mask-bridge-probe --node Mask [--matrix focused] [--resolution N] [--base-source SOURCE] [--layer-source SOURCE] [--mask-source zero|one|half|negative|high] [--epsilon N] [--direct-bin] [--run] [--json] [--require-all-pass]"
    );
    println!(
        "  mask-flow-mountain-connected-probe --node LinearGradient|SlopeMask|Mask [--resolution N] [--run] [--json] [--direct-bin] [-- Mountain and target params]"
    );
    println!(
        "  ground-texture-bridge-probe [--node GroundTexture] [--matrix focused] [--resolution N] [--source rampx|rampy|cone|basin|checker|flat|radial] [--method harsh|rocky|rough] [--strength N] [--coverage N] [--compare-native] [--epsilon N] [--run] [--json] [--direct-bin]"
    );
    println!(
        "  volcano-stage-parity [--node Volcano] [--case NAME|all] [--stage NAME[,..]] [--kind stage|replay|all] [--only-mismatch] [--direct-bin] [--run] [--json]"
    );
    println!(
        "  river-connected-probe --node River [--case NAME] [--resolution N] [--run] [--json] [--gaea-dir PATH] [-- mountain/river params]"
    );
    println!("  matrix --node Mountain [--suite frontier] [--run] [--json] [--direct-bin]");
    println!("  capture --node Mountain|Thermal2 --case <Case> [--run] [--json] [--direct-bin]");
    println!(
        "  diff --node Mountain|Thermal2 --case <Case> [--first] [--coord x,y --level N] [--serial N] [--run] [--json] [--direct-bin] [-- extra case flags]"
    );
    println!("  audit --node Mountain|Thermal2 [--case all|Case] [--run] [--json] [--direct-bin]");
    println!();
    println!(
        "Defaults: node=Mountain, case=old_baseline. Heavy commands are dry-run unless --run is passed; --direct-bin reuses compiled Cunning3D probe executables when present; use --no-incremental only to retry probe cargo runs after stale or corrupted incremental link artifacts."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dune_sea_compare_summary_exposes_residual_family_summary() {
        let value = json!({
            "node": "DuneSea",
            "case_id": "test_case",
            "mode": "bridge_native_compare",
            "resolution": [128, 128],
            "terrain_width": 1000.0,
            "terrain_height": 500.0,
            "exact": false,
            "passed": true,
            "bridge_available": true,
            "bridge_error": null,
            "timing_native_avg_ms": 1.0,
            "timing_native_min_ms": 1.0,
            "timing_native_max_ms": 1.0,
            "bridge_timing_ms": 5.0,
            "stage_checks": [],
            "stage_compare": [
                {
                    "stage": "height_scaled",
                    "bit_mismatch_count": 0,
                    "max_abs_diff": 0.0,
                    "mean_abs_diff": 0.0,
                    "rmse": 0.0,
                    "sample_count": 4,
                    "exact_bit_count": 4,
                    "native_to_bridge_mean_ratio": 1.0,
                },
                {
                    "stage": "thermal_shaped",
                    "bit_mismatch_count": 2,
                    "max_abs_diff": 0.125,
                    "mean_abs_diff": 0.0625,
                    "rmse": 0.0901,
                    "sample_count": 4,
                    "exact_bit_count": 2,
                    "native_to_bridge_mean_ratio": 0.75,
                },
                {
                    "stage": "final_precommit",
                    "bit_mismatch_count": 4,
                    "max_abs_diff": 0.25,
                    "mean_abs_diff": 0.125,
                    "rmse": 0.1768,
                    "sample_count": 4,
                    "exact_bit_count": 0,
                    "native_to_bridge_mean_ratio": 0.5,
                },
            ],
            "managed_stage_dump_status": [],
            "thermal_replay_diagnostics": null,
            "final_commit_diagnostics": null,
            "spatial_diagnostics": null,
            "terminal_stage_noop": false,
            "softened_to_final_mean_delta": 0.125,
            "bridge_to_softened_mean_ratio": 0.75,
            "focused_diagnostic_verdict": "test",
            "stage_family_summary": {
                "exact_stage_names": ["height_scaled"],
                "non_exact_stage_names": ["thermal_shaped", "final_precommit"],
                "exact_stage_count": 1,
                "non_exact_stage_count": 2,
            },
            "bridge_dump_errors": [],
        });

        let summary = summary_view(&value).expect("summary view");
        let residual = summary
            .get("residual_family_summary")
            .expect("residual_family_summary");
        assert_eq!(
            residual
                .get("exact_stage_names")
                .and_then(Value::as_array)
                .unwrap()
                .iter()
                .map(|stage| stage.get("stage").and_then(Value::as_str))
                .collect::<Option<Vec<_>>>(),
            Some(vec!["height_scaled"])
        );
        assert_eq!(
            residual
                .get("non_exact_stage_names")
                .and_then(Value::as_array)
                .unwrap()
                .iter()
                .map(|stage| stage.get("stage").and_then(Value::as_str))
                .collect::<Option<Vec<_>>>(),
            Some(vec!["thermal_shaped", "final_precommit"])
        );
        assert_eq!(
            residual
                .get("first_non_exact_stage")
                .and_then(|stage| stage.get("stage"))
                .and_then(Value::as_str),
            Some("thermal_shaped")
        );
        assert_eq!(residual.get("stage_count").and_then(Value::as_u64), Some(3));
    }

    #[test]
    fn thermal_shaper_compare_summary_exposes_first_kernel_blocker() {
        let value = json!({
            "node": "ThermalShaper",
            "matrix": "focused",
            "epsilon": 0.0,
            "repeat": 1,
            "exact": false,
            "passed": false,
            "speedup_gate_passed": false,
            "speedup_20x_gate_passed": false,
            "suggested_next_command": "thermal-shaper-compare --node ThermalShaper --map map:sine:32:5:0.35:0.5",
            "cases": [
                {
                    "name": "sine32_shape0",
                    "exact": true,
                    "passed": true,
                    "parity_status": "DegenerateIdentityBranchClosed",
                    "promotion_status": "gated_kernel",
                    "native_elapsed_ms": 0.002,
                    "speedup_vs_bridge_method": 1000.0,
                    "speedup_vs_bridge_process": 100000.0,
                    "diff": null,
                },
                {
                    "name": "sine32_default_open_kernel",
                    "exact": false,
                    "passed": false,
                    "parity_status": "NativeReferenceCandidateKernelOpen",
                    "promotion_status": "gated_kernel",
                    "native_elapsed_ms": 0.1,
                    "speedup_vs_bridge_method": 150.0,
                    "speedup_vs_bridge_process": 6000.0,
                    "schedule_diagnostics": {
                        "basis": "test",
                        "current_rust": {
                            "per_level": [
                                {"level_index": 0, "pass_budget": 16.0, "iteration_count": 16}
                            ]
                        },
                        "decompiled_native_expected_hints": {
                            "per_level": [
                                {"level_index": 0, "layer_fraction": 1.0, "iteration_count_estimate": 27}
                            ]
                        },
                        "mismatch_flags": ["schedule mismatch"]
                    },
                    "diff": {
                        "mismatch_count": 938,
                        "max_abs_diff": 0.0032886267,
                    },
                    "bridge_derived_stage_reports": [
                        {
                            "stage": "root_post_kernel",
                            "reference": "bridge_derived_root_post_kernel_from_output_height",
                            "reference_raw": "derived.rawf32",
                            "reference_sha256_f32": "bridge",
                            "raw_sha256_f32": "native",
                            "resolution": [32, 32],
                            "diff": {
                                "mismatch_count": 1024,
                                "bit_mismatch_count": 1024,
                                "max_abs_diff": 0.0065772533,
                                "mean_abs_diff": 0.001,
                                "rmse": 0.002,
                                "first_bit_mismatch": {"coord": [1, 0]},
                                "worst_cell": {"coord": [24, 31]}
                            }
                        }
                    ],
                }
            ],
            "diagnostics": {
                "first_failing": {
                    "name": "sine32_default_open_kernel",
                    "parity_status": "NativeReferenceCandidateKernelOpen",
                    "shortest_blocker": "bit_mismatch",
                    "mismatch_count": 938,
                    "max_abs_diff": 0.0032886267,
                    "boundary_mismatch_count": 111,
                    "interior_mismatch_count": 827,
                    "first_mismatch_coord": [1, 0],
                    "first_bit_mismatch": {
                        "coord": [1, 0],
                        "bridge_bits": "3f26190c",
                        "native_bits": "3f25e3f1",
                    },
                    "first_native_stage_mismatch": {
                        "stage": "root_post_kernel",
                        "bit_mismatch_count": 951,
                        "max_abs_diff": 0.09663129,
                    },
                    "bridge_derived_stage_reports": [
                        {
                            "stage": "root_post_kernel",
                            "reference": "bridge_derived_root_post_kernel_from_output_height",
                            "reference_raw": "derived.rawf32",
                            "reference_sha256_f32": "bridge",
                            "raw_sha256_f32": "native",
                            "resolution": [32, 32],
                            "diff": {
                                "mismatch_count": 1024,
                                "bit_mismatch_count": 1024,
                                "max_abs_diff": 0.0065772533,
                                "mean_abs_diff": 0.001,
                                "rmse": 0.002,
                                "first_bit_mismatch": {"coord": [1, 0]},
                                "worst_cell": {"coord": [24, 31]}
                            }
                        }
                    ],
                    "kernel_candidate_sweep": {
                        "parameter": "shape_step_multiplier",
                        "candidate_count": 3,
                        "best_by_mean_abs_diff": {
                            "shape_step_multiplier": 0.97,
                            "exact": false,
                            "passed": false,
                            "mismatch_count": 900,
                            "bit_mismatch_count": 900,
                            "max_abs_diff": 0.001,
                            "mean_abs_diff": 0.0005,
                            "rmse": 0.0007,
                            "first_mismatch_coord": [1, 0],
                        },
                        "candidates": [],
                    },
                    "schedule_diagnostics": {
                        "basis": "test",
                        "current_rust": {
                            "per_level": [
                                {"level_index": 0, "pass_budget": 16.0, "iteration_count": 16}
                            ]
                        },
                        "decompiled_native_expected_hints": {
                            "per_level": [
                                {"level_index": 0, "layer_fraction": 1.0, "iteration_count_estimate": 27}
                            ]
                        },
                        "mismatch_flags": ["schedule mismatch"]
                    },
                    "stage_family_summary": {
                        "exact_stage_names": ["working_signed_input"],
                        "non_exact_stage_names": ["root_post_kernel", "finalized"],
                        "first_non_exact_stage": "root_post_kernel",
                    },
                    "residual_family_summary": {
                        "exact_case_names": ["sine32_shape0"],
                        "non_exact_case_names": ["sine32_default_open_kernel"],
                        "first_failing_stage": "root_post_kernel",
                    }
                }
            }
        });

        let summary = summary_view(&value).expect("summary view");
        assert_eq!(
            summary
                .pointer("/run_summary/case_count")
                .and_then(Value::as_u64),
            Some(2)
        );
        assert_eq!(
            summary
                .pointer("/first_failing/first_native_stage_mismatch/stage")
                .and_then(Value::as_str),
            Some("root_post_kernel")
        );
        assert_eq!(
            summary
                .pointer("/residual_family_summary/first_failing_stage")
                .and_then(Value::as_str),
            Some("root_post_kernel")
        );
        assert_eq!(
            summary
                .pointer("/first_failing/kernel_candidate_sweep/best_by_mean_abs_diff/shape_step_multiplier")
                .and_then(Value::as_f64),
            Some(0.97)
        );
        assert_eq!(
            summary
                .pointer("/first_failing/schedule/native_per_level/0/iteration_count_estimate")
                .and_then(Value::as_u64),
            Some(27)
        );
        assert_eq!(
            summary
                .pointer("/first_failing/bridge_derived_stage_reports/0/reference")
                .and_then(Value::as_str),
            Some("bridge_derived_root_post_kernel_from_output_height")
        );
        assert!(summary
            .get("suggested_next_command")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .contains("thermal-shaper-compare"));
    }

    #[test]
    fn combine_surface_override_keeps_recovered_ports() {
        let (inputs, outputs) = combine_node_ports();
        let input_names = inputs
            .iter()
            .map(|port| port.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(input_names, vec!["Input", "Input2", "Mask"]);
        assert_eq!(inputs[0].required, Some(true));
        assert_eq!(inputs[2].role, "mask");
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].name, "Output");
    }

    #[test]
    fn surface_contract_gate_marks_raw_buffer_as_insufficient() {
        let gate = flywheel_surface_contract_gate();
        assert_eq!(
            gate.get("required_for_100_percent")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert!(gate
            .get("raw_buffer")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .contains("insufficient"));
        assert!(gate.get("parameter_surface").is_some());
        assert!(gate.get("port_surface").is_some());
    }

    #[test]
    fn obfuscated_constant_detection_catches_escaped_and_private_use_forms() {
        assert!(has_obfuscated_constants(r"\ue0003.\ue000(3)"));
        assert!(has_obfuscated_constants("\u{e000}3.\u{e000}(3)"));
        assert!(!has_obfuscated_constants("PortCount = 3"));
    }

    #[test]
    fn matching_source_lines_preserves_line_numbers_and_limit() {
        let text = "a\n[Parameter]\nbase.Ports.Add\n[Parameter]\n";
        assert_eq!(
            matching_source_lines(text, &["[Parameter"], 1),
            vec!["2: [Parameter]".to_string()]
        );
    }

    #[test]
    fn exact_node_class_match_rejects_core_suffixes() {
        assert!(!line_declares_exact_node_class(
            "internal class DebrisCore",
            "Debris"
        ));
        assert!(line_declares_exact_node_class(
            "public class Debris : Node",
            "Debris"
        ));
    }

    #[test]
    fn exact_node_class_match_handles_generic_class_declarations() {
        assert!(line_declares_exact_node_class(
            "internal sealed class Combine<T> : Node",
            "Combine"
        ));
        assert!(!line_declares_exact_node_class(
            "internal sealed class CombineMask<T> : Node",
            "Combine"
        ));
    }

    #[test]
    fn runtime_plan_view_reads_mountain_style_case_report() {
        let value = json!({
            "cases": [{
                "report": {
                    "lhs_runtime_plan_summary": {
                        "stage_count": 2,
                        "gpu_stage_count": 1
                    },
                    "rhs_runtime_plan_summary": {
                        "stage_count": 1,
                        "gpu_stage_count": 0
                    },
                    "lhs_runtime_stage_profiles": [{
                        "id": "mountain.ridge",
                        "policy": "GpuDense",
                        "backend_key": "native_gpu",
                        "elapsed_ms": 1.5,
                        "cache_hit": false,
                        "gpu_expected": true,
                        "cpu_expected": false,
                        "shipped": true
                    }]
                }
            }]
        });

        let view = backend_compare_runtime_plan_view(&value).expect("runtime view should exist");
        assert_eq!(
            view.pointer("/stage_summary/lhs/stage_count")
                .and_then(Value::as_u64),
            Some(2)
        );
        assert_eq!(
            view.pointer("/stage_profile_summary/lhs/total/gpu_expected_count")
                .and_then(Value::as_u64),
            Some(1)
        );
    }

    #[test]
    fn runtime_plan_view_reads_voronoi_nested_compare_report() {
        let value = json!({
            "cases": [{
                "report": {
                    "tag": { "case_label": "default_complex_p_64" },
                    "compare": {
                        "lhs_runtime_plan_summary": {
                            "stage_count": 1,
                            "backend_key": "gaea_bridge"
                        },
                        "rhs_runtime_plan_summary": {
                            "stage_count": 3,
                            "backend_key": "native"
                        },
                        "rhs_runtime_stage_profiles": [{
                            "id": "voronoi.raw_sample",
                            "policy": "CpuParallel",
                            "backend_key": "native",
                            "elapsed_ms": 2.0,
                            "cache_hit": null,
                            "gpu_expected": false,
                            "cpu_expected": true,
                            "shipped": true
                        }]
                    }
                }
            }]
        });

        let view = backend_compare_runtime_plan_view(&value).expect("runtime view should exist");
        assert_eq!(
            view.pointer("/stage_summary/rhs/backend_key")
                .and_then(Value::as_str),
            Some("native")
        );
        assert_eq!(
            view.pointer("/stage_profile_summary/rhs/by_stage/voronoi.raw_sample/count")
                .and_then(Value::as_u64),
            Some(1)
        );
    }

    #[test]
    fn runtime_plan_view_reads_perlin_case_level_report_after_raw_case() {
        let value = json!({
            "cases": [
                {
                    "case": "fbm_nowarp_default_128",
                    "contract": "raw"
                },
                {
                    "case": "fbm_nowarp_default_128",
                    "contract": "wrapped",
                    "report": {
                        "lhs_runtime_plan_summary": {
                            "stage_count": 2,
                            "backend_key": "gaea_bridge"
                        },
                        "rhs_runtime_plan_summary": {
                            "stage_count": 2,
                            "backend_key": "native"
                        }
                    }
                }
            ]
        });

        let view = backend_compare_runtime_plan_view(&value).expect("runtime view should exist");
        assert_eq!(
            view.pointer("/stage_summary/lhs/backend_key")
                .and_then(Value::as_str),
            Some("gaea_bridge")
        );
        assert_eq!(
            view.pointer("/stage_summary/rhs/backend_key")
                .and_then(Value::as_str),
            Some("native")
        );
    }
}
