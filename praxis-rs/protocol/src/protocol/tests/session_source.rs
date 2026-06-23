use super::*;

#[test]
fn session_source_from_startup_arg_maps_known_values() {
    assert_eq!(
        SessionSource::from_startup_arg("vscode").unwrap(),
        SessionSource::VSCode
    );
    assert_eq!(
        SessionSource::from_startup_arg("app-gateway").unwrap(),
        SessionSource::AppGateway
    );
}

#[test]
fn session_source_from_startup_arg_normalizes_custom_values() {
    assert_eq!(
        SessionSource::from_startup_arg("atlas").unwrap(),
        SessionSource::Custom("atlas".to_string())
    );
    assert_eq!(
        SessionSource::from_startup_arg(" Atlas ").unwrap(),
        SessionSource::Custom("atlas".to_string())
    );
}

#[test]
fn session_source_restriction_product_defaults_non_subagent_sources_to_praxis() {
    assert_eq!(
        SessionSource::Cli.restriction_product(),
        Some(Product::Praxis)
    );
    assert_eq!(
        SessionSource::VSCode.restriction_product(),
        Some(Product::Praxis)
    );
    assert_eq!(
        SessionSource::Exec.restriction_product(),
        Some(Product::Praxis)
    );
    assert_eq!(
        SessionSource::AppGateway.restriction_product(),
        Some(Product::Praxis)
    );
    assert_eq!(
        SessionSource::Mcp.restriction_product(),
        Some(Product::Praxis)
    );
    assert_eq!(
        SessionSource::Unknown.restriction_product(),
        Some(Product::Praxis)
    );
}

#[test]
fn session_source_restriction_product_does_not_guess_subagent_products() {
    assert_eq!(
        SessionSource::SubAgent(SubAgentSource::Review).restriction_product(),
        None
    );
}

#[test]
fn session_source_restriction_product_maps_custom_sources_to_products() {
    assert_eq!(
        SessionSource::Custom("chatgpt".to_string()).restriction_product(),
        Some(Product::Chatgpt)
    );
    assert_eq!(
        SessionSource::Custom("ATLAS".to_string()).restriction_product(),
        Some(Product::Atlas)
    );
    assert_eq!(
        SessionSource::Custom("cunning3d".to_string()).restriction_product(),
        Some(Product::Cunning3d)
    );
    assert_eq!(
        SessionSource::Custom("c3d".to_string()).restriction_product(),
        Some(Product::Cunning3d)
    );
    assert_eq!(
        SessionSource::Custom("praxis".to_string()).restriction_product(),
        Some(Product::Praxis)
    );
    assert_eq!(
        SessionSource::Custom("codex".to_string()).restriction_product(),
        Some(Product::Praxis)
    );
    assert_eq!(
        SessionSource::Custom("atlas-dev".to_string()).restriction_product(),
        None
    );
}

#[test]
fn session_source_matches_product_restriction() {
    assert!(
        SessionSource::Custom("chatgpt".to_string())
            .matches_product_restriction(&[Product::Chatgpt])
    );
    assert!(
        !SessionSource::Custom("chatgpt".to_string())
            .matches_product_restriction(&[Product::Praxis])
    );
    assert!(SessionSource::VSCode.matches_product_restriction(&[Product::Praxis]));
    assert!(
        !SessionSource::Custom("atlas-dev".to_string())
            .matches_product_restriction(&[Product::Atlas])
    );
    assert!(
        SessionSource::Custom("cunning3d".to_string())
            .matches_product_restriction(&[Product::Cunning3d])
    );
    assert!(SessionSource::Custom("atlas-dev".to_string()).matches_product_restriction(&[]));
}
