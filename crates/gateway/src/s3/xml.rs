use quick_xml::se::to_string;
use serde::Serialize;

pub fn to_xml<T: Serialize>(v: &T, root: &str) -> String {
    // Wrap with root manually to match S3 responses
    let body = to_string(v).unwrap_or_default();
    format!("<{root}>{body}</{root}>")
}

