fn execute_live_heightfield_audit(
    bridge_addr: &str,
    source_type: &str,
    source_output: &str,
    target_input: &str,
    target_output: &str,
    prefix: &str,
    targets: &[String],
    resolution: i64,
    timeout_ms: u64,
    keep_nodes: bool,
) -> Result<Value, String> {
    let timeout = Duration::from_millis(timeout_ms.max(1));
    let mut target_reports = Vec::new();
    let mut temp_nodes = Vec::new();
    let mut stale_deleted = Vec::new();
    let mut cleanup_errors = Vec::new();
    let mut original_display_name = None;
    let mut initial_node_count = None;

    let operation_error = {
        let result = (|| -> Result<(), String> {
            let initial_graph = c3d_live_graph_state(bridge_addr, timeout)?;
            initial_node_count = initial_graph.get("node_count").and_then(Value::as_u64);
            original_display_name = live_display_node_name(&initial_graph);

            for stale in live_nodes_with_prefix(&initial_graph, prefix) {
                let _ = c3d_graph_call(
                    bridge_addr,
                    "delete_node",
                    json!({ "node_name_or_id": stale }),
                    timeout,
                )?;
                stale_deleted.push(stale);
            }

            let source_name = format!("{prefix}{source_type}");
            c3d_graph_call(
                bridge_addr,
                "create_node",
                json!({ "node_type": source_type, "node_name": source_name }),
                timeout,
            )?;
            temp_nodes.push(source_name.clone());
            c3d_wait_live_node(bridge_addr, &source_name, timeout)?;
            if source_type.eq_ignore_ascii_case("Mountain") {
                c3d_graph_call(
                    bridge_addr,
                    "set_parameter",
                    json!({ "node_name": source_name, "param_name": "resolution", "value": resolution }),
                    timeout,
                )?;
            }

            for target in targets {
                let target_name = format!("{prefix}{target}");
                c3d_graph_call(
                    bridge_addr,
                    "create_node",
                    json!({ "node_type": target, "node_name": target_name }),
                    timeout,
                )?;
                temp_nodes.push(target_name.clone());
                c3d_wait_live_node(bridge_addr, &target_name, timeout)?;
                c3d_graph_call(
                    bridge_addr,
                    "connect_nodes",
                    json!({
                        "from_node": source_name,
                        "from_port": source_output,
                        "to_node": target_name,
                        "to_port": target_input,
                    }),
                    timeout,
                )?;
            }

            for target in targets {
                let target_name = format!("{prefix}{target}");
                c3d_graph_call(
                    bridge_addr,
                    "set_node_flag",
                    json!({ "node_name": target_name, "flag": "display", "active": true }),
                    timeout,
                )?;
                let report = c3d_wait_live_heightfield_ref(
                    bridge_addr,
                    &target_name,
                    target_output,
                    timeout,
                )?;
                target_reports.push(report);
            }
            Ok(())
        })();
        result.err()
    };

    if let Some(display_name) = original_display_name.as_deref() {
        if let Err(error) = c3d_graph_call(
            bridge_addr,
            "set_node_flag",
            json!({ "node_name": display_name, "flag": "display", "active": true }),
            timeout,
        ) {
            cleanup_errors.push(
                json!({ "operation": "restore_display", "node": display_name, "error": error }),
            );
        }
    }

    if !keep_nodes {
        for node_name in temp_nodes.iter().rev() {
            if let Err(error) = c3d_graph_call(
                bridge_addr,
                "delete_node",
                json!({ "node_name_or_id": node_name }),
                timeout,
            ) {
                cleanup_errors
                    .push(json!({ "operation": "delete_node", "node": node_name, "error": error }));
            }
        }
    }

    let final_graph = c3d_live_graph_state(bridge_addr, timeout).ok();
    let all_targets_passed = !target_reports.is_empty()
        && target_reports.iter().all(|report| {
            report
                .get("heightfield_ref")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                && report
                    .get("cook_error")
                    .map(Value::is_null)
                    .unwrap_or(false)
        });
    let success = operation_error.is_none() && all_targets_passed && cleanup_errors.is_empty();

    Ok(json!({
        "mode": "executed",
        "command": "live-heightfield-audit",
        "success": success,
        "bridge_addr": bridge_addr,
        "source": {
            "type": source_type,
            "output": source_output,
            "resolution": resolution,
        },
        "target_input": target_input,
        "target_output": target_output,
        "targets": targets,
        "target_reports": target_reports,
        "operation_error": operation_error,
        "cleanup": {
            "keep_nodes": keep_nodes,
            "stale_deleted": stale_deleted,
            "temp_nodes": temp_nodes,
            "errors": cleanup_errors,
        },
        "initial": {
            "node_count": initial_node_count,
            "display_node": original_display_name,
        },
        "final": {
            "node_count": final_graph.as_ref().and_then(|graph| graph.get("node_count")).cloned(),
            "display_node": final_graph.as_ref().and_then(live_display_node_name),
        },
        "truth_rule": "This live audit proves product graph HeightField runtime refs and cook-error health only; raw-buffer parity remains owned by node-specific Bridge/native compare commands."
    }))
}

fn live_heightfield_audit_with_artifact(mut report: Value, run_dir: &Path) -> Value {
    if let Some(map) = report.as_object_mut() {
        map.insert("artifact_dir".to_string(), json!(path_text(run_dir)));
    }
    report
}
