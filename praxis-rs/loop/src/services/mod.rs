mod event;
mod history;
mod model;
mod steering;
mod tool;

pub use event::EventSink;
pub use history::HistorySink;
pub use model::ModelEventStream;
pub use model::ModelRequest;
pub use model::ModelService;
pub use model::RoundSettings;
pub use steering::SteeringControl;
pub use steering::SteeringDrain;
pub use steering::SteeringInbox;
pub use tool::ToolAccess;

pub trait TurnServices:
    ModelService + EventSink + HistorySink + SteeringInbox + ToolAccess
{
}

impl<T> TurnServices for T where
    T: ModelService + EventSink + HistorySink + SteeringInbox + ToolAccess
{
}
