use deps::*;

use crate::user::{User, UserSortingField};
use serde::{Deserialize, Serialize};

pub trait SortingField {
    fn sql_field_name(&self) -> String;
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(crate = "serde", rename_all = "camelCase")]
pub enum SortingOrder {
    Ascending,
    Descending,
}
impl SortingOrder {
    #[inline]
    pub fn sql_key_word(&self) -> &'static str {
        match self {
            Self::Ascending => "asc",
            Self::Descending => "desc",
        }
    }
}

pub const DEFAULT_LIST_LIMIT: usize = 25;

#[derive(Debug, Serialize, Deserialize, validator::Validate, utoipa::ToSchema)]
#[serde(crate = "serde", rename_all = "camelCase")]
#[validate(schema(function = "validate_list_req"))]
#[aliases(ListUsersRequest = ListRequest<UserSortingField>)]
pub struct ListRequest<S>
where
    S: SortingField + Clone + Copy + Serialize,
{
    #[serde(skip)]
    pub auth_token: Option<std::sync::Arc<str>>,
    #[validate(range(min = 1, max = 100))]
    pub limit: Option<usize>,
    pub after_cursor: Option<String>,
    pub before_cursor: Option<String>,
    pub filter: Option<String>,
    pub sorting_field: Option<S>,
    pub sorting_order: Option<SortingOrder>,
}

fn validate_list_req<S>(req: &ListRequest<S>) -> Result<(), validator::ValidationError>
where
    S: SortingField + Clone + Copy + Serialize,
{
    match (req.before_cursor.as_ref(), req.after_cursor.as_ref()) {
        (Some(before_cursor), Some(after_cursor)) => Err(validator::ValidationError {
            code: "before_and_after_cursors_at_once".into(),
            message: Some("both beforeCursor and afterCursor are present".into()),
            params: [
                (
                    "beforeCursor".into(),
                    serde_json::json!({ "value": before_cursor }),
                ),
                (
                    "afterCursor".into(),
                    serde_json::json!({ "value": after_cursor }),
                ),
            ]
            .into_iter()
            .collect(),
        }),
        (None, Some(cursor)) | (Some(cursor), None)
            if req.sorting_field.is_some()
                || req.sorting_order.is_some()
                || req.filter.is_some() =>
        {
            Err(validator::ValidationError {
                code: "both_cursor_and_sorting_or_filter".into(),
                message: Some("both beforeCursor and afterCursor are present".into()),
                params: [
                    Some((
                        if req.after_cursor.is_some() {
                            "afterCursor".into()
                        } else {
                            "beforeCursor".into()
                        },
                        serde_json::json!({ "value": cursor }),
                    )),
                    req.sorting_order
                        .map(|val| ("sortingOrder".into(), serde_json::json!({ "value": val }))),
                    req.sorting_field
                        .map(|val| ("sortingOrder".into(), serde_json::json!({ "value": val }))),
                    req.filter
                        .as_ref()
                        .map(|val| ("sortingOrder".into(), serde_json::json!({ "value": val }))),
                ]
                .into_iter()
                .flatten()
                .collect(),
            })
        }
        _ => Ok(()),
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(crate = "serde", rename_all = "camelCase")]
#[aliases(ListUsersResponse = ListResponse<User>)]
pub struct ListResponse<T>
where
    T: utoipa::ToSchema,
{
    pub cursor: Option<String>,
    pub items: Vec<T>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(crate = "serde", rename_all = "camelCase")]
pub struct Cursor<T, S>
where
    S: SortingField + Clone + Copy,
{
    pub value: T,
    pub field: S,
    pub order: SortingOrder,
    pub filter: Option<String>,
}

const CURSOR_VERSION: usize = 1;

impl<T, S> Cursor<T, S>
where
    S: Serialize + SortingField + Clone + Copy,
    T: Serialize,
{
    pub fn to_encoded_str(&self) -> String {
        use std::io::Write;
        // let mut out = format!("{CURSOR_VERSION}:");
        let mut out = Vec::new();
        {
            std::write!(&mut out, "{CURSOR_VERSION}:").unwrap_or_log();
            let mut b64_w = base64::write::EncoderWriter::new(&mut out, base64::STANDARD);
            let mut brotli_w = brotli::CompressorWriter::new(&mut b64_w, 4096, 5, 21);
            serde_json::to_writer(&mut brotli_w, &self).unwrap_or_log();
        }
        String::from_utf8(out).unwrap_or_log()
    }
}

impl<T, S> std::str::FromStr for Cursor<T, S>
where
    T: serde::de::DeserializeOwned,
    S: SortingField + Clone + Copy + serde::de::DeserializeOwned,
{
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (ver_str, payload_str) = s.split_once(':').ok_or(())?;
        let version: usize = ver_str.parse().map_err(|_| ())?;
        if version != CURSOR_VERSION {
            return Err(());
        }
        // let mut cursor = std::io::Cursor::new(payload_str);
        // let mut b64_r = base64::read::DecoderReader::new(&mut cursor, base64::STANDARD);
        // let mut brotil_r = brotli::CompressorReader::new(&mut b64_r, 4096, 5, 21);
        // serde_json::from_reader(&mut brotil_r).map_err(|err| tracing::error!(?err))
        let compressed = base64::decode_config(payload_str, base64::STANDARD).map_err(|_| ())?;
        let mut cursor = std::io::Cursor::new(&compressed);
        let mut json = Vec::new();
        brotli::BrotliDecompress(&mut cursor, &mut json).map_err(|_| ())?;
        tracing::info!("{}", std::str::from_utf8(&json).unwrap_or_log());
        serde_json::from_slice(&json[..]).map_err(|_| ())
    }
}
