//! This is a clone of the error types from the `validator` crate but with
//! the `utoipa::ToSchema` derive.
//! TODO: clean this up to something simpler to expose in APIs
//!
use deps::*;

use serde::{Deserialize, Serialize};

use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
};

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(crate = "serde", rename_all = "camelCase")]
pub struct ValidationError {
    pub code: Cow<'static, str>,
    pub message: Option<Cow<'static, str>>,
    #[schema(value_type = HashMap<String, Object>)]
    pub params: HashMap<Cow<'static, str>, serde_json::Value>,
}

impl From<validator::ValidationError> for ValidationError {
    fn from(err: validator::ValidationError) -> Self {
        Self {
            code: err.code,
            message: err.message,
            params: err.params,
        }
    }
}

#[derive(Debug, Serialize, Clone, PartialEq, utoipa::ToSchema)]
#[serde(untagged)]
#[serde(crate = "serde", rename_all = "camelCase")]
pub enum ValidationErrorsKind {
    Object(Box<ValidationErrors>),
    List(BTreeMap<usize, Box<ValidationErrors>>),
    Field(Vec<ValidationError>),
}

impl From<validator::ValidationErrorsKind> for ValidationErrorsKind {
    fn from(errs: validator::ValidationErrorsKind) -> Self {
        match errs {
            validator::ValidationErrorsKind::Struct(b) => Self::Object(Box::new((*b).into())),
            validator::ValidationErrorsKind::List(map) => Self::List(
                map.into_iter()
                    .map(|(key, val)| (key, Box::new((*val).into())))
                    .collect(),
            ),
            validator::ValidationErrorsKind::Field(vec) => {
                Self::Field(vec.into_iter().map(|err| err.into()).collect())
            }
        }
    }
}

#[derive(Default, Debug, Serialize, Clone, PartialEq, utoipa::ToSchema)]
#[serde(crate = "serde", rename_all = "camelCase")]
pub struct ValidationErrors(HashMap<&'static str, ValidationErrorsKind>);

impl fmt::Display for ValidationError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(msg) = self.message.as_ref() {
            write!(fmt, "{}", msg)
        } else {
            write!(fmt, "Validation error: {} [{:?}]", self.code, self.params)
        }
    }
}

use std::fmt::{self, Write};

fn display_errors(
    fmt: &mut fmt::Formatter<'_>,
    errs: &ValidationErrorsKind,
    path: &str,
) -> fmt::Result {
    fn display_struct(
        fmt: &mut fmt::Formatter<'_>,
        errs: &ValidationErrors,
        path: &str,
    ) -> fmt::Result {
        let mut full_path = String::new();
        write!(&mut full_path, "{}.", path)?;
        let base_len = full_path.len();
        for (path, err) in &errs.0 {
            write!(&mut full_path, "{}", path)?;
            display_errors(fmt, err, &full_path)?;
            full_path.truncate(base_len);
        }
        Ok(())
    }
    match errs {
        ValidationErrorsKind::Field(errs) => {
            write!(fmt, "{}: ", path)?;
            let len = errs.len();
            for (idx, err) in errs.iter().enumerate() {
                if idx + 1 == len {
                    write!(fmt, "{}", err)?;
                } else {
                    write!(fmt, "{}, ", err)?;
                }
            }
            Ok(())
        }
        ValidationErrorsKind::Object(errs) => display_struct(fmt, errs, path),
        ValidationErrorsKind::List(errs) => {
            let mut full_path = String::new();
            write!(&mut full_path, "{}", path)?;
            let base_len = full_path.len();
            for (idx, err) in errs.iter() {
                write!(&mut full_path, "[{}]", idx)?;
                display_struct(fmt, err, &full_path)?;
                full_path.truncate(base_len);
            }
            Ok(())
        }
    }
}

impl fmt::Display for ValidationErrors {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (idx, (path, err)) in self.0.iter().enumerate() {
            display_errors(fmt, err, path)?;
            if idx + 1 < self.0.len() {
                writeln!(fmt)?;
            }
        }
        Ok(())
    }
}

impl std::error::Error for ValidationErrors {
    fn description(&self) -> &str {
        "Validation failed"
    }
    fn cause(&self) -> Option<&dyn std::error::Error> {
        None
    }
}

impl From<validator::ValidationErrors> for ValidationErrors {
    fn from(errs: validator::ValidationErrors) -> Self {
        Self(
            errs.into_errors()
                .into_iter()
                .map(|(key, val)| (key, val.into()))
                .collect(),
        )
    }
}
