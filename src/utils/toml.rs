use serde::Serialize;
use toml_edit::Item;

/// Serializes a value to a `toml_edit::Item` using its `Serialize` impl.
pub fn serialize_to_item(value: impl Serialize) -> Item {
  #[derive(Serialize)]
  struct Wrapper<T: Serialize> {
    v: T,
  }

  let toml_str = toml_edit::ser::to_string(&Wrapper { v: value }).expect("failed to serialize value");
  let doc: toml_edit::DocumentMut = toml_str.parse().expect("serialized value is valid TOML");

  return doc["v"].clone();
}
