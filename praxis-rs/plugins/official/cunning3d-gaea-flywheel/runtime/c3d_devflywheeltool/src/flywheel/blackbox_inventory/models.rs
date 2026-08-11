#[derive(Debug, Deserialize)]
struct Ledger {
    schema_version: u32,
    architecture_authority: LedgerArchitectureAuthority,
    entries: Vec<LedgerEntry>,
}

#[derive(Debug, Deserialize, serde::Serialize)]
struct LedgerArchitectureAuthority {
    document: String,
    section: u32,
    policy: String,
    required_flow: String,
    promotion_gate: String,
}

#[derive(Debug, Deserialize, serde::Serialize)]
struct LedgerEntry {
    operator: String,
    node: String,
    layer: String,
    status: String,
    native_evidence: Vec<String>,
    rust_implementation: Vec<String>,
    evidence_summary: String,
    open_risk: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct FlywheelGraph {
    schema_version: u32,
    contracts: Vec<FlywheelContract>,
    nodes: Vec<FlywheelNode>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct FlywheelContract {
    id: String,
    label: String,
    kind: String,
    layer: String,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    ledger_operators: Vec<String>,
    #[serde(default)]
    owner_nodes: Vec<String>,
    #[serde(default)]
    reusable: bool,
    #[serde(default)]
    unlocks: Vec<String>,
    #[serde(default)]
    implementation: Vec<String>,
    #[serde(default)]
    evidence: Vec<String>,
    #[serde(default)]
    next_commands: Vec<String>,
    #[serde(default)]
    notes: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct FlywheelNode {
    id: String,
    label: String,
    domain: String,
    kind: String,
    priority: String,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    outputs: Vec<String>,
    #[serde(default)]
    input_ports: Vec<FlywheelPort>,
    #[serde(default)]
    output_ports: Vec<FlywheelPort>,
    #[serde(default)]
    shared_operators: Vec<String>,
    #[serde(default)]
    recipe_families: Vec<String>,
    #[serde(default)]
    next_commands: Vec<String>,
    #[serde(default)]
    notes: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct FlywheelPort {
    name: String,
    #[serde(default)]
    role: String,
    #[serde(default)]
    required: Option<bool>,
    #[serde(default)]
    slot: Option<usize>,
    #[serde(default)]
    source_slot: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct BlackboxInventory {
    schema_version: u32,
    generated_by: String,
    generated_from: String,
    node_count: usize,
    operator_count: usize,
    contract_count: usize,
    relation_count: usize,
    family_count: usize,
    nodes: Vec<FlywheelNode>,
    contracts: Vec<FlywheelContract>,
    operators: Vec<BlackboxOperator>,
    relations: Vec<BlackboxRelation>,
    families: Vec<BlackboxFamily>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct BlackboxOperator {
    id: String,
    label: String,
    class: String,
    method: String,
    file: String,
    contract_id: String,
    status: String,
    layer: String,
    called_operators: Vec<String>,
    called_by_nodes: Vec<String>,
    called_by_operators: Vec<String>,
    notes: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct BlackboxRelation {
    from: String,
    to: String,
    kind: String,
    depth: usize,
    #[serde(default)]
    via: Vec<String>,
    source: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct BlackboxFamily {
    id: String,
    node_count: usize,
    operator_count: usize,
    contract_count: usize,
    nodes: Vec<String>,
    operators: Vec<String>,
    contracts: Vec<String>,
}

#[derive(Debug, Clone)]
struct CatalogNode {
    id: String,
    label: String,
    family: String,
    public_node: bool,
    file: String,
}

#[derive(Debug, Clone)]
struct CatalogOperatorMethod {
    class: String,
    method: String,
    file: String,
}
