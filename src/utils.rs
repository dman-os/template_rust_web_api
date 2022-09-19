pub use list_request::*;
mod list_request;

pub use validation_errs::*;
mod validation_errs;

#[cfg(test)]
pub mod testing;

/// This baby doesn't work on generic types
pub fn type_name_raw<T>() -> &'static str {
    let name = std::any::type_name::<T>();
    match &name.rfind(':') {
        Some(pos) => &name[pos + 1..name.len()],
        None => name,
    }
}

#[test]
fn test_type_name_macro() {
    struct Foo {}
    assert_eq!("Foo", type_name_raw::<Foo>());
}

/*
/// Serde deserialization decorator to map empty Strings to None,
fn empty_string_as_none<'de, D, T>(de: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    use serde::Deserialize;
    let opt = Option::<String>::deserialize(de)?;
    match opt.as_deref() {
        None | Some("") => Ok(None),
        Some(s) => std::str::FromStr::from_str(s).map_err(serde::de::Error::custom).map(Some),
    }
}
*/
