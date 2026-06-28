use crate::function_tool::FunctionCallError;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

pub(crate) fn parse_arguments<T>(arguments: &str) -> Result<T, FunctionCallError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_str(arguments).map_err(|err| {
        FunctionCallError::RespondToModel(format!("failed to parse function arguments: {err}"))
    })
}

pub(crate) fn parse_optional_value_arguments(
    raw_args: &str,
) -> Result<Option<Value>, FunctionCallError> {
    if raw_args.trim().is_empty() {
        return Ok(None);
    }

    let value: Value = parse_arguments(raw_args)?;
    if value.is_null() {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

pub(crate) fn parse_value<T>(arguments: Option<Value>) -> Result<T, FunctionCallError>
where
    T: DeserializeOwned,
{
    match arguments {
        Some(value) => serde_json::from_value(value).map_err(|err| {
            FunctionCallError::RespondToModel(format!("failed to parse function arguments: {err}"))
        }),
        None => Err(FunctionCallError::RespondToModel(
            "failed to parse function arguments: expected value".to_string(),
        )),
    }
}

pub(crate) fn parse_value_with_default<T>(arguments: Option<Value>) -> Result<T, FunctionCallError>
where
    T: DeserializeOwned + Default,
{
    match arguments {
        Some(value) => parse_value(Some(value)),
        None => Ok(T::default()),
    }
}
