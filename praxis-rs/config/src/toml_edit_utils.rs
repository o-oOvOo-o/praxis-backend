use toml::Value as TomlValue;
use toml_edit::Item as TomlItem;

pub fn toml_value_to_edit_item(value: &TomlValue) -> anyhow::Result<TomlItem> {
    match value {
        TomlValue::Table(table) => {
            let mut table_item = toml_edit::Table::new();
            table_item.set_implicit(false);
            for (key, val) in table {
                table_item.insert(key, toml_value_to_edit_item(val)?);
            }
            Ok(TomlItem::Table(table_item))
        }
        other => Ok(TomlItem::Value(toml_value_to_edit_value(other)?)),
    }
}

pub fn toml_value_to_edit_value(value: &TomlValue) -> anyhow::Result<toml_edit::Value> {
    match value {
        TomlValue::String(val) => Ok(toml_edit::Value::from(val.clone())),
        TomlValue::Integer(val) => Ok(toml_edit::Value::from(*val)),
        TomlValue::Float(val) => Ok(toml_edit::Value::from(*val)),
        TomlValue::Boolean(val) => Ok(toml_edit::Value::from(*val)),
        TomlValue::Datetime(val) => Ok(toml_edit::Value::from(*val)),
        TomlValue::Array(items) => {
            let mut array = toml_edit::Array::new();
            for item in items {
                array.push(toml_value_to_edit_value(item)?);
            }
            Ok(toml_edit::Value::Array(array))
        }
        TomlValue::Table(table) => {
            let mut inline = toml_edit::InlineTable::new();
            for (key, val) in table {
                inline.insert(key, toml_value_to_edit_value(val)?);
            }
            Ok(toml_edit::Value::InlineTable(inline))
        }
    }
}
