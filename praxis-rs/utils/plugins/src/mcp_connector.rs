pub fn is_connector_id_allowed(connector_id: &str) -> bool {
    praxis_connectors::is_connector_id_allowed(connector_id)
}

pub fn sanitize_name(name: &str) -> String {
    praxis_connectors::sanitize_connector_name(name)
}
