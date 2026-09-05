use crate::types::NextData;
use serde::Deserialize;

#[derive(Deserialize)]
struct NextDataRaw<'a> {
    #[serde(default, borrow)]
    page: &'a str,
    #[serde(default, borrow, rename = "buildId")]
    build_id: &'a str,
    #[serde(default, borrow)]
    query: Option<&'a str>,
}

pub fn parse_next_data(json: &[u8]) -> Result<NextData, sonic_rs::Error> {
    let raw: NextDataRaw<'_> = sonic_rs::from_slice(json)?;
    Ok(NextData {
        page: compact_str::CompactString::new(raw.page),
        build_id: compact_str::CompactString::new(raw.build_id),
        query: raw.query.map(compact_str::CompactString::new),
    })
}
