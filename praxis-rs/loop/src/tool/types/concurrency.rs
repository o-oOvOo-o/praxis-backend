use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum ConcurrencyMode {
    #[default]
    Parallel,
    Exclusive,
    Blocking,
}
