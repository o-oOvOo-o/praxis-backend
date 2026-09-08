pub use praxis_app_core::backend_slash_commands::SlashCommand;
pub use praxis_app_core::backend_slash_commands::built_in_slash_commands;

pub(crate) fn available_during_task(command: SlashCommand) -> bool {
    command == SlashCommand::Namegen || command.available_during_task()
}
