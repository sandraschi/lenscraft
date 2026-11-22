//! M2 Multi-level floorplan engine: OSM `indoor=room` Overpass parser,
//! normalized `[0..1]` plan coordinates, point-in-polygon room hit-testing,
//! and curated room-to-artwork overlays for flagship museums.
use crate::error::LensError;
use crate::museums::find_museum;
use crate::venue::VenueTransport;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FloorRoom {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub room_number: String,
    /// Normalized `[0.0 .. 1.0]` polygon vertices `(x, y)` for canvas/SVG rendering.
    pub polygon: Vec<(f64, f64)>,
    pub center: (f64, f64),
    #[serde(default)]
    pub artwork_qids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_qid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FloorLevel {
    pub level_id: String,
    pub label: String,
    pub rooms: Vec<FloorRoom>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MuseumFloorplan {
    pub museum_slug: String,
    pub museum_name: String,
    pub levels: Vec<FloorLevel>,
    pub source: String,
}

/// Ray-casting 2D point-in-polygon check in normalized `[0..1]` floorplan coordinates.
pub fn point_in_polygon(x: f64, y: f64, poly: &[(f64, f64)]) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        let intersect =
            ((yi > y) != (yj > y)) && (x < (xj - xi) * (y - yi) / ((yj - yi) + 1e-12) + xi);
        if intersect {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Find which room on a floor level contains `(x, y)` in normalized `[0..1]` space.
pub fn room_at_point(level: &FloorLevel, x: f64, y: f64) -> Option<&FloorRoom> {
    level
        .rooms
        .iter()
        .find(|r| point_in_polygon(x, y, &r.polygon))
}

/// Build an Overpass QL query for OSM `indoor=room` / `indoor=area` within `radius_m`.
pub fn build_indoor_overpass_ql(lat: f64, lon: f64, radius_m: u32) -> String {
    let r = radius_m.clamp(40, 400);
    format!(
        "[out:json][timeout:12];\
(\
  way(around:{r},{lat:.6},{lon:.6})[\"indoor\"~\"^(room|area|hall)$\"];\
  relation(around:{r},{lat:.6},{lon:.6})[\"indoor\"~\"^(room|area|hall)$\"];\
);\
out geom tags 200;"
    )
}

/// Parse OSM Overpass indoor rooms and normalize coordinates into `[0.05..0.95]`.
pub fn parse_overpass_floorplan(slug: &str, museum_name: &str, raw: &Value) -> MuseumFloorplan {
    let Some(elements) = raw.get("elements").and_then(Value::as_array) else {
        return MuseumFloorplan {
            museum_slug: slug.to_string(),
            museum_name: museum_name.to_string(),
            levels: Vec::new(),
            source: "osm-indoor".to_string(),
        };
    };

    struct RawRoom {
        id: String,
        level: String,
        name: String,
        ref_num: String,
        pts: Vec<(f64, f64)>,
    }

    let mut raw_rooms = Vec::new();
    let mut min_lat = f64::MAX;
    let mut max_lat = f64::MIN;
    let mut min_lon = f64::MAX;
    let mut max_lon = f64::MIN;

    for (idx, el) in elements.iter().enumerate() {
        let Some(tags) = el.get("tags").and_then(Value::as_object) else {
            continue;
        };
        let Some(geom) = el.get("geometry").and_then(Value::as_array) else {
            continue;
        };
        let pts: Vec<(f64, f64)> = geom
            .iter()
            .filter_map(|p| {
                Some((
                    p.get("lon").and_then(Value::as_f64)?,
                    p.get("lat").and_then(Value::as_f64)?,
                ))
            })
            .collect();
        if pts.len() < 3 {
            continue;
        }
        for &(lo, la) in &pts {
            min_lon = min_lon.min(lo);
            max_lon = max_lon.max(lo);
            min_lat = min_lat.min(la);
            max_lat = max_lat.max(la);
        }
        let level = tags
            .get("level")
            .and_then(Value::as_str)
            .unwrap_or("0")
            .trim()
            .to_string();
        let ref_num = tags
            .get("ref")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        let name = tags
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| {
                if ref_num.is_empty() {
                    format!("Room {}", idx + 1)
                } else {
                    format!("Room {ref_num}")
                }
            });
        raw_rooms.push(RawRoom {
            id: format!(
                "osm-{}",
                el.get("id").and_then(Value::as_i64).unwrap_or(idx as i64)
            ),
            level,
            name,
            ref_num,
            pts,
        });
    }

    let span_lon = (max_lon - min_lon).max(1e-6);
    let span_lat = (max_lat - min_lat).max(1e-6);
    let mut by_level: BTreeMap<String, Vec<FloorRoom>> = BTreeMap::new();

    for r in raw_rooms {
        let poly: Vec<(f64, f64)> = r
            .pts
            .iter()
            .map(|&(lo, la)| {
                let x = 0.05 + 0.90 * ((lo - min_lon) / span_lon);
                let y = 0.95 - 0.90 * ((la - min_lat) / span_lat);
                (x, y)
            })
            .collect();
        let n = poly.len() as f64;
        let cx = poly.iter().map(|p| p.0).sum::<f64>() / n;
        let cy = poly.iter().map(|p| p.1).sum::<f64>() / n;
        by_level.entry(r.level).or_default().push(FloorRoom {
            id: r.id,
            name: r.name,
            room_number: r.ref_num,
            polygon: poly,
            center: (cx, cy),
            artwork_qids: Vec::new(),
            world_qid: None,
        });
    }

    let levels = by_level
        .into_iter()
        .map(|(lvl, rooms)| FloorLevel {
            label: format!("Level {lvl}"),
            level_id: lvl,
            rooms,
        })
        .collect();

    MuseumFloorplan {
        museum_slug: slug.to_string(),
        museum_name: museum_name.to_string(),
        levels,
        source: "osm-indoor".to_string(),
    }
}

fn rect_room(
    id: &str,
    name: &str,
    num: &str,
    bounds: (f64, f64, f64, f64),
    qids: &[&str],
    world: Option<&str>,
) -> FloorRoom {
    let (x, y, w, h) = bounds;
    FloorRoom {
        id: id.to_string(),
        name: name.to_string(),
        room_number: num.to_string(),
        polygon: vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)],
        center: (x + w * 0.5, y + h * 0.5),
        artwork_qids: qids.iter().map(|s| (*s).to_string()).collect(),
        world_qid: world.map(str::to_string),
    }
}

/// Curated schematic floorplans for flagship museums when OSM indoor mapping is incomplete.
pub fn curated_floorplan(slug: &str) -> Option<MuseumFloorplan> {
    let m = find_museum(slug)?;
    match m.slug {
        "khm" => Some(MuseumFloorplan {
            museum_slug: m.slug.to_string(),
            museum_name: m.name.to_string(),
            source: "curated-schematic".to_string(),
            levels: vec![
                FloorLevel {
                    level_id: "0".to_string(),
                    label: "Ground Floor (Kunstkammer & Antiquities)".to_string(),
                    rooms: vec![
                        rect_room(
                            "khm-ant",
                            "Egyptian & Near Eastern",
                            "0.1",
                            (0.06, 0.12, 0.38, 0.72),
                            &[],
                            None,
                        ),
                        rect_room(
                            "khm-kk",
                            "Kunstkammer (Cellini Saliera)",
                            "0.2",
                            (0.52, 0.12, 0.42, 0.72),
                            &["Q698448"],
                            None,
                        ),
                    ],
                },
                FloorLevel {
                    level_id: "1".to_string(),
                    label: "First Floor (Picture Gallery)".to_string(),
                    rooms: vec![
                        rect_room(
                            "khm-x",
                            "Room X: Pieter Bruegel the Elder",
                            "X",
                            (0.06, 0.12, 0.28, 0.36),
                            &["Q153642", "Q167794"],
                            Some("Q153642"),
                        ),
                        rect_room(
                            "khm-xi",
                            "Room XI: Rubens & Van Dyck",
                            "XI",
                            (0.36, 0.12, 0.28, 0.36),
                            &[],
                            None,
                        ),
                        rect_room(
                            "khm-it",
                            "Italian Renaissance (Caravaggio, Titian)",
                            "V",
                            (0.66, 0.12, 0.28, 0.36),
                            &[],
                            None,
                        ),
                        rect_room(
                            "khm-nl",
                            "Dutch & Flemish Masters (Vermeer)",
                            "XII",
                            (0.06, 0.54, 0.88, 0.34),
                            &[],
                            None,
                        ),
                    ],
                },
            ],
        }),
        "louvre" => Some(MuseumFloorplan {
            museum_slug: m.slug.to_string(),
            museum_name: m.name.to_string(),
            source: "curated-schematic".to_string(),
            levels: vec![FloorLevel {
                level_id: "1".to_string(),
                label: "Level 1 (Denon, Sully & Richelieu)".to_string(),
                rooms: vec![
                    rect_room(
                        "louvre-711",
                        "Salle des Etats (Mona Lisa)",
                        "711",
                        (0.06, 0.15, 0.40, 0.68),
                        &["Q12418"],
                        Some("Q12418"),
                    ),
                    rect_room(
                        "louvre-sully",
                        "Sully Wing (French & Antiquities)",
                        "Sully",
                        (0.50, 0.15, 0.44, 0.32),
                        &[],
                        None,
                    ),
                    rect_room(
                        "louvre-richelieu",
                        "Richelieu Wing (Northern Schools)",
                        "Richelieu",
                        (0.50, 0.51, 0.44, 0.32),
                        &[],
                        None,
                    ),
                ],
            }],
        }),
        _ => Some(MuseumFloorplan {
            museum_slug: m.slug.to_string(),
            museum_name: m.name.to_string(),
            source: "curated-overview".to_string(),
            levels: vec![FloorLevel {
                level_id: "0".to_string(),
                label: "Main Galleries".to_string(),
                rooms: vec![rect_room(
                    "main-hall",
                    "Permanent Collection Highlights",
                    "Main",
                    (0.08, 0.15, 0.84, 0.70),
                    &[],
                    None,
                )],
            }],
        }),
    }
}

/// Fetch a museum floorplan from `floorplans/<slug>.json`, or Overpass indoor, or curated schematic.
pub fn resolve_floorplan(
    slug: &str,
    floorplans_dir: &Path,
    overpass: Option<&dyn VenueTransport>,
) -> Result<MuseumFloorplan, LensError> {
    let m = find_museum(slug)
        .ok_or_else(|| LensError::InvalidInput(format!("unknown museum slug '{slug}'")))?;
    let cached = floorplans_dir.join(format!("{}.json", m.slug));
    if cached.exists() {
        let raw = std::fs::read_to_string(&cached)
            .map_err(|e| LensError::Io(format!("read floorplan failed: {e}")))?;
        return serde_json::from_str(&raw)
            .map_err(|e| LensError::Protocol(format!("parse floorplan failed: {e}")));
    }
    if let Some(t) = overpass {
        let ql = build_indoor_overpass_ql(m.lat, m.lon, 180);
        if let Ok(raw) = t.query(&ql) {
            let fp = parse_overpass_floorplan(m.slug, m.name, &raw);
            if !fp.levels.is_empty() {
                return Ok(fp);
            }
        }
    }
    curated_floorplan(m.slug)
        .ok_or_else(|| LensError::Protocol(format!("no floorplan for '{}'", m.slug)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn curated_khm_floorplan_supports_room_hit_test() -> Result<(), LensError> {
        let fp = curated_floorplan("khm")
            .ok_or_else(|| LensError::Protocol("missing khm floorplan".to_string()))?;
        assert_eq!(fp.levels.len(), 2);
        let first_floor = &fp.levels[1];
        let hit = room_at_point(first_floor, 0.20, 0.28)
            .ok_or_else(|| LensError::Protocol("expected Room X hit".to_string()))?;
        assert_eq!(hit.room_number, "X");
        assert_eq!(hit.world_qid.as_deref(), Some("Q153642"));
        Ok(())
    }

    #[test]
    fn overpass_indoor_parser_normalizes_levels() {
        let raw = json!({
            "elements": [{
                "type": "way",
                "id": 101,
                "tags": {"indoor": "room", "level": "1", "ref": "X", "name": "Bruegel Room"},
                "geometry": [
                    {"lat": 48.2035, "lon": 16.3610},
                    {"lat": 48.2035, "lon": 16.3614},
                    {"lat": 48.2038, "lon": 16.3614},
                    {"lat": 48.2038, "lon": 16.3610}
                ]
            }]
        });
        let fp = parse_overpass_floorplan("khm", "KHM", &raw);
        assert_eq!(fp.levels.len(), 1);
        assert_eq!(fp.levels[0].rooms[0].name, "Bruegel Room");
        assert!(fp.levels[0].rooms[0].center.0 > 0.0 && fp.levels[0].rooms[0].center.0 < 1.0);
    }
}
