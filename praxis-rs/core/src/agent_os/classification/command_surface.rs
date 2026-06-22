pub(in crate::agent_os) fn denylist_surface(command: &[String]) -> String {
    if command
        .first()
        .is_some_and(|program| program.eq_ignore_ascii_case("apply_patch"))
    {
        return "apply_patch".to_string();
    }
    command.join(" ")
}
