use super::super::*;

pub(super) struct BeginManagedCommandRequest<'a> {
    pub(super) ticket: &'a ExecutionTicket,
    pub(super) command: String,
    pub(super) argv: &'a [String],
    pub(super) cwd: PathBuf,
    pub(super) process_id: Option<i32>,
    pub(super) runtime_kind: Option<String>,
    pub(super) runtime_owner_id: Option<String>,
}

impl BeginManagedCommandRequest<'_> {
    pub(super) fn command_fingerprint(&self) -> PraxisResult<String> {
        let command_fingerprint =
            action_fingerprint(self.argv, &self.cwd, self.ticket.allowed_intent);
        if command_fingerprint != self.ticket.command_fingerprint {
            return Err(PraxisErr::UnsupportedOperation(
                "execution ticket command fingerprint does not match requested command".to_string(),
            ));
        }
        if normalize_path_for_scope(&self.cwd) != normalize_path_for_scope(&self.ticket.cwd) {
            return Err(PraxisErr::UnsupportedOperation(
                "execution ticket cwd does not match requested command".to_string(),
            ));
        }
        Ok(command_fingerprint)
    }
}
