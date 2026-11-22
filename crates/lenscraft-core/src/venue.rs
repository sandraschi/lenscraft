//! L2 venue narrowing: resolve GPS coordinates to nearby museums, galleries,
//! monuments, and historic buildings via Overpass (OpenStreetMap).
use crate::error::LensError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::time::Duration;

pub const DEFAULT_OVERPASS_URL: &str = "https://overpass-api.de/api/interpreter";
const DEFAULT_RADIUS_M: u32 = 250;
const MAX_RADIUS_M: u32 = 5_000;
const TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VenueCandidate {
    pub name: String,
    pub category: String,
    pub distance_m: f64,
    pub lat: f64,
    pub lon: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wikidata_qid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wikipedia: Option<String>,
}

pub trait VenueTransport {
    fn query(&self, overpass_ql: &str) -> Result<Value, LensError>;
}

pub struct OverpassHttp {
    url: String,
    client: reqwest::blocking::Client,
}

impl OverpassHttp {
    pub fn from_env() -> Result<Self, LensError> {
        let url = std::env::var("LENSCRAFT_OVERPASS_URL")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_OVERPASS_URL.to_string());
        Self::new(&url)
    }

    pub fn new(url: &str) -> Result<Self, LensError> {
        let url = url.trim().to_string();
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(LensError::InvalidInput(format!(
                "overpass url must start with http:// or https://, got '{url}'"
            )));
        }
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(TIMEOUT)
            .timeout(TIMEOUT)
            .user_agent("lenscraft/0.1 (local museum lens)")
            .build()
            .map_err(|e| LensError::Offline(format!("overpass client: {e}")))?;
        Ok(Self { url, client })
    }
}

impl VenueTransport for OverpassHttp {
    fn query(&self, overpass_ql: &str) -> Result<Value, LensError> {
        let resp = self
            .client
            .post(&self.url)
            .form(&[("data", overpass_ql)])
            .send()
            .map_err(|e| LensError::Offline(format!("overpass request failed: {e}")))?;
        let status = resp.status();
        let text = resp
            .text()
            .map_err(|e| LensError::Protocol(format!("overpass read failed: {e}")))?;
        if !status.is_success() {
            return Err(LensError::Protocol(format!(
                "overpass {status}: {}",
                text.chars().take(200).collect::<String>()
            )));
        }
        serde_json::from_str(&text)
            .map_err(|e| LensError::Protocol(format!("overpass invalid json: {e}")))
    }
}

/// Haversine distance in meters between two WGS84 coordinates.
pub fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const EARTH_R_M: f64 = 6_371_000.0;
    let d_lat = (lat2 - lat1).to_radians();
    let d_lon = (lon2 - lon1).to_radians();
    let a = (d_lat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (d_lon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    EARTH_R_M * c
}

fn validate_coords(lat: f64, lon: f64) -> Result<(), LensError> {
    if !lat.is_finite()
        || !lon.is_finite()
        || !(-90.0..=90.0).contains(&lat)
        || !(-180.0..=180.0).contains(&lon)
    {
        return Err(LensError::InvalidInput(format!(
            "lat/lon out of range: {lat}, {lon}"
        )));
    }
    Ok(())
}

/// Build an Overpass QL query for cultural venues and landmarks around `(lat, lon)`.
pub fn build_overpass_ql(lat: f64, lon: f64, radius_m: u32) -> String {
    let r = radius_m.clamp(25, MAX_RADIUS_M);
    format!(
        "[out:json][timeout:10];\
(\
  nwr(around:{r},{lat:.6},{lon:.6})[\"tourism\"~\"^(museum|gallery|artwork|attraction)$\"];\
  nwr(around:{r},{lat:.6},{lon:.6})[\"historic\"~\"^(monument|memorial|castle|palace|church|cathedral|ruins)$\"];\
  nwr(around:{r},{lat:.6},{lon:.6})[\"amenity\"~\"^(arts_centre|place_of_worship|theatre)$\"];\
);\
out center tags 40;"
    )
}

/// Query nearby cultural venues, sorted nearest-first and deduplicated by name.
pub fn nearby_venues(
    t: &dyn VenueTransport,
    lat: f64,
    lon: f64,
    radius_m: Option<u32>,
) -> Result<Vec<VenueCandidate>, LensError> {
    validate_coords(lat, lon)?;
    let r = radius_m.unwrap_or(DEFAULT_RADIUS_M).clamp(25, MAX_RADIUS_M);
    let ql = build_overpass_ql(lat, lon, r);
    let raw = t.query(&ql)?;
    parse_overpass_venues(&raw, lat, lon)
}

pub fn parse_overpass_venues(
    raw: &Value,
    origin_lat: f64,
    origin_lon: f64,
) -> Result<Vec<VenueCandidate>, LensError> {
    let Some(elements) = raw.get("elements").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for el in elements {
        let Some(tags) = el.get("tags").and_then(Value::as_object) else {
            continue;
        };
        let Some(name) = tags.get("name").and_then(Value::as_str).map(str::trim) else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        let (v_lat, v_lon) = if let (Some(la), Some(lo)) = (
            el.get("lat").and_then(Value::as_f64),
            el.get("lon").and_then(Value::as_f64),
        ) {
            (la, lo)
        } else if let (Some(la), Some(lo)) = (
            el.pointer("/center/lat").and_then(Value::as_f64),
            el.pointer("/center/lon").and_then(Value::as_f64),
        ) {
            (la, lo)
        } else {
            continue;
        };
        let category = tags
            .get("tourism")
            .or_else(|| tags.get("historic"))
            .or_else(|| tags.get("amenity"))
            .and_then(Value::as_str)
            .unwrap_or("landmark")
            .to_string();
        let wikidata_qid = tags
            .get("wikidata")
            .and_then(Value::as_str)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let wikipedia = tags
            .get("wikipedia")
            .and_then(Value::as_str)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        out.push(VenueCandidate {
            name: name.to_string(),
            category,
            distance_m: haversine_m(origin_lat, origin_lon, v_lat, v_lon),
            lat: v_lat,
            lon: v_lon,
            wikidata_qid,
            wikipedia,
        });
    }
    out.sort_by(|a, b| a.distance_m.total_cmp(&b.distance_m));
    let mut seen = HashSet::new();
    out.retain(|v| seen.insert(v.name.to_lowercase()));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct FakeVenue(Value);
    impl VenueTransport for FakeVenue {
        fn query(&self, _ql: &str) -> Result<Value, LensError> {
            Ok(self.0.clone())
        }
    }

    #[test]
    fn haversine_is_sane_for_vienna() {
        let d = haversine_m(48.2038, 16.3614, 48.2046, 16.3618);
        assert!(d > 80.0 && d < 120.0, "got {d}");
    }

    #[test]
    fn nearby_venues_sorts_dedups_and_extracts_wikidata() -> Result<(), LensError> {
        let fake = FakeVenue(json!({
            "elements": [
                {
                    "type": "way",
                    "center": {"lat": 48.2052, "lon": 16.3596},
                    "tags": {"name": "Naturhistorisches Museum", "tourism": "museum", "wikidata": "Q688704"}
                },
                {
                    "type": "node",
                    "lat": 48.2039,
                    "lon": 16.3617,
                    "tags": {"name": "Kunsthistorisches Museum Wien", "tourism": "museum", "wikidata": "Q95569"}
                },
                {
                    "type": "node",
                    "lat": 48.2039,
                    "lon": 16.3617,
                    "tags": {"name": "Kunsthistorisches Museum Wien", "tourism": "museum"}
                }
            ]
        }));
        let venues = nearby_venues(&fake, 48.2038, 16.3616, Some(300))?;
        assert_eq!(venues.len(), 2);
        assert_eq!(venues[0].name, "Kunsthistorisches Museum Wien");
        assert_eq!(venues[0].wikidata_qid.as_deref(), Some("Q95569"));
        assert!(venues[0].distance_m < venues[1].distance_m);
        Ok(())
    }

    #[test]
    fn invalid_coords_are_rejected() {
        let fake = FakeVenue(json!({}));
        let err = nearby_venues(&fake, 95.0, 16.0, None);
        assert_eq!(err.map_err(|e| e.kind()), Err("invalid"));
    }
}
