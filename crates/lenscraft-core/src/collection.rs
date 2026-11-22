//! L8 collection packs: per-museum image signature + metadata packs (KHM first)
//! for fast offline nearest-neighbour matching alongside Wikidata/Commons.
use crate::error::LensError;
use crate::grounding::KnowledgeTransport;
use crate::image_prep::PreparedImage;
use image::GenericImageView as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

pub const KHM_QID: &str = "Q95569";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionItem {
    pub qid: String,
    pub title: String,
    pub creator: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub image_url: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    /// 24-dim normalized spatial RGB signature (2x2 quadrants x 6 channel/luma stats).
    pub signature: [f32; 24],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionPack {
    pub museum: String,
    pub qid: String,
    pub version: u32,
    pub items: Vec<CollectionItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionMatch {
    pub qid: String,
    pub title: String,
    pub creator: String,
    pub date: String,
    pub score: f32,
    pub image_score: f32,
    pub text_score: f32,
}

/// Compute a 24-dim normalized spatial color signature from a prepared JPEG.
pub fn compute_signature(img: &PreparedImage) -> Result<[f32; 24], LensError> {
    let dyn_img = image::load_from_memory(&img.jpeg)
        .map_err(|e| LensError::Image(format!("decode prepared image failed: {e}")))?;
    let small = dyn_img.resize_exact(32, 32, image::imageops::FilterType::Triangle);
    let mut vec = [0.0f32; 24];
    for qy in 0..2u32 {
        for qx in 0..2u32 {
            let base = ((qy * 2 + qx) * 6) as usize;
            let mut sum = [0.0f32; 3];
            let mut sum_sq = [0.0f32; 3];
            let mut count = 0.0f32;
            for y in (qy * 16)..((qy + 1) * 16) {
                for x in (qx * 16)..((qx + 1) * 16) {
                    let p = small.get_pixel(x, y);
                    let rgb = [
                        f32::from(p[0]) / 255.0,
                        f32::from(p[1]) / 255.0,
                        f32::from(p[2]) / 255.0,
                    ];
                    for c in 0..3 {
                        sum[c] += rgb[c];
                        sum_sq[c] += rgb[c] * rgb[c];
                    }
                    count += 1.0;
                }
            }
            if count > 0.0 {
                for c in 0..3 {
                    let mean = sum[c] / count;
                    let var = (sum_sq[c] / count - mean * mean).max(0.0);
                    vec[base + c] = mean;
                    vec[base + 3 + c] = var.sqrt();
                }
            }
        }
    }
    let norm = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-6 {
        for x in &mut vec {
            *x /= norm;
        }
    }
    Ok(vec)
}

fn cosine(a: &[f32; 24], b: &[f32; 24]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let na = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na > 1e-6 && nb > 1e-6 {
        (dot / (na * nb)).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Nearest-neighbour match of a photo against a museum collection pack.
pub fn match_collection(
    pack: &CollectionPack,
    img: &PreparedImage,
    hint_text: Option<&str>,
    top_k: usize,
) -> Result<Vec<CollectionMatch>, LensError> {
    let sig = compute_signature(img)?;
    let hint_tokens: Vec<String> = hint_text
        .unwrap_or("")
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 4)
        .map(str::to_string)
        .collect();

    let mut matches: Vec<CollectionMatch> = pack
        .items
        .iter()
        .map(|item| {
            let image_score = cosine(&sig, &item.signature);
            let text_score = if hint_tokens.is_empty() {
                0.0
            } else {
                let hay = format!(
                    "{} {} {}",
                    item.title.to_lowercase(),
                    item.creator.to_lowercase(),
                    item.keywords.join(" ").to_lowercase()
                );
                let hits = hint_tokens
                    .iter()
                    .filter(|t| hay.contains(t.as_str()))
                    .count();
                (hits as f32 / hint_tokens.len() as f32).min(1.0)
            };
            let score = if hint_tokens.is_empty() {
                image_score
            } else {
                0.6 * image_score + 0.4 * text_score
            };
            CollectionMatch {
                qid: item.qid.clone(),
                title: item.title.clone(),
                creator: item.creator.clone(),
                date: item.date.clone(),
                score,
                image_score,
                text_score,
            }
        })
        .collect();
    matches.sort_by(|a, b| b.score.total_cmp(&a.score));
    matches.truncate(top_k.max(1));
    Ok(matches)
}

/// Build a collection pack from Wikidata SPARQL for a museum QID (default KHM `Q95569`).
pub fn build_wikidata_pack(
    t: &dyn KnowledgeTransport,
    museum_name: &str,
    museum_qid: &str,
    limit: usize,
) -> Result<CollectionPack, LensError> {
    let qid = museum_qid.trim();
    if !qid.starts_with('Q') || qid.len() < 2 {
        return Err(LensError::InvalidInput(format!(
            "invalid Wikidata QID: {qid}"
        )));
    }
    let lim = limit.clamp(1, 1000);
    // Order by Wikidata sitelinks count so iconic works (e.g. Peasant Wedding, Tower of Babel)
    // are always included first in large museums like KHM and the Louvre.
    let sparql = format!(
        "SELECT ?item ?itemLabel ?creatorLabel ?inception ?image ?sitelinks WHERE {{\
  ?item wdt:P195 wd:{qid} .\
  ?item wdt:P18 ?image .\
  OPTIONAL {{ ?item wdt:P170 ?creator . }}\
  OPTIONAL {{ ?item wdt:P571 ?inception . }}\
  OPTIONAL {{ ?item wikibase:sitelinks ?sitelinks . }}\
  SERVICE wikibase:label {{ bd:serviceParam wikibase:language \"en,de\". }}\
}} ORDER BY DESC(?sitelinks) LIMIT {lim}"
    );
    let mut encoded = String::with_capacity(sparql.len());
    for b in sparql.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(b as char)
            }
            b' ' => encoded.push_str("%20"),
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    let url = format!("https://query.wikidata.org/sparql?format=json&query={encoded}");
    let v = t.get_json(&url)?;
    let bindings = v
        .pointer("/results/bindings")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut items = Vec::with_capacity(bindings.len());
    for b in bindings {
        let item_uri = b
            .pointer("/item/value")
            .and_then(Value::as_str)
            .unwrap_or("");
        let item_qid = item_uri.rsplit('/').next().unwrap_or("").to_string();
        let title = b
            .pointer("/itemLabel/value")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if item_qid.is_empty() || title.is_empty() {
            continue;
        }
        let creator = b
            .pointer("/creatorLabel/value")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let date = b
            .pointer("/inception/value")
            .and_then(Value::as_str)
            .map(|s| s.split('T').next().unwrap_or(s).to_string())
            .unwrap_or_default();
        let image_url = b
            .pointer("/image/value")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let keywords: Vec<String> = format!("{title} {creator}")
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() >= 4)
            .map(str::to_string)
            .collect();
        items.push(CollectionItem {
            qid: item_qid,
            title,
            creator,
            date,
            image_url,
            keywords,
            signature: [0.0f32; 24],
        });
    }
    Ok(CollectionPack {
        museum: museum_name.to_string(),
        qid: qid.to_string(),
        version: 1,
        items,
    })
}

/// Download Commons thumbnails (`?width=256`) and populate 24-dim `signature` vectors
/// for overnight/offline pack preparation. Individual download failures are skipped.
pub fn hydrate_pack_signatures(pack: &mut CollectionPack) -> Result<usize, LensError> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(20))
        .user_agent("lenscraft/0.2 (local museum lens; https://github.com/sandraschi/lenscraft)")
        .build()
        .map_err(|e| LensError::Offline(format!("signature client: {e}")))?;
    let mut hydrated = 0usize;
    for item in &mut pack.items {
        if item.image_url.trim().is_empty() {
            continue;
        }
        let thumb_url = if item.image_url.contains('?') {
            format!("{}&width=256", item.image_url)
        } else {
            format!("{}?width=256", item.image_url)
        };
        let Ok(resp) = client.get(&thumb_url).send() else {
            continue;
        };
        if !resp.status().is_success() {
            continue;
        }
        let Ok(bytes) = resp.bytes() else {
            continue;
        };
        if let Ok(prep) = crate::image_prep::prepare_bytes(&bytes) {
            if let Ok(sig) = compute_signature(&prep) {
                item.signature = sig;
                hydrated += 1;
            }
        }
    }
    Ok(hydrated)
}

pub fn save_pack(pack: &CollectionPack, path: &Path) -> Result<(), LensError> {
    let json = serde_json::to_string_pretty(pack)
        .map_err(|e| LensError::Protocol(format!("serialize pack failed: {e}")))?;
    std::fs::write(path, json).map_err(|e| LensError::Io(format!("write pack failed: {e}")))
}

pub fn load_pack(path: &Path) -> Result<CollectionPack, LensError> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| LensError::Io(format!("read pack failed: {e}")))?;
    serde_json::from_str(&raw).map_err(|e| LensError::Protocol(format!("parse pack failed: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};
    use serde_json::json;
    use std::io::Cursor;

    fn solid_jpeg(r: u8, g: u8, b: u8) -> Result<PreparedImage, LensError> {
        let img = ImageBuffer::from_pixel(32, 32, Rgb([r, g, b]));
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Jpeg)
            .map_err(|e| LensError::Image(format!("encode failed: {e}")))?;
        Ok(PreparedImage {
            jpeg: buf,
            width: 32,
            height: 32,
            source_width: 32,
            source_height: 32,
            source_format: "jpeg".to_string(),
        })
    }

    #[test]
    fn nearest_neighbour_ranks_matching_color_and_text_first() -> Result<(), LensError> {
        let warm = solid_jpeg(220, 120, 40)?;
        let blue = solid_jpeg(30, 80, 210)?;
        let warm_sig = compute_signature(&warm)?;
        let blue_sig = compute_signature(&blue)?;

        let pack = CollectionPack {
            museum: "KHM".to_string(),
            qid: KHM_QID.to_string(),
            version: 1,
            items: vec![
                CollectionItem {
                    qid: "Q1".to_string(),
                    title: "Blue Madonna".to_string(),
                    creator: "Artist A".to_string(),
                    date: "1500".to_string(),
                    image_url: String::new(),
                    keywords: vec!["blue".to_string()],
                    signature: blue_sig,
                },
                CollectionItem {
                    qid: "Q2".to_string(),
                    title: "The Tower of Babel".to_string(),
                    creator: "Pieter Bruegel".to_string(),
                    date: "1563".to_string(),
                    image_url: String::new(),
                    keywords: vec!["babel".to_string(), "bruegel".to_string()],
                    signature: warm_sig,
                },
            ],
        };
        let hits = match_collection(&pack, &warm, Some("Bruegel Babel"), 2)?;
        assert_eq!(hits[0].qid, "Q2");
        assert!(hits[0].score > hits[1].score);
        Ok(())
    }

    struct FakeSparql;
    impl KnowledgeTransport for FakeSparql {
        fn get_json(&self, _url: &str) -> Result<Value, LensError> {
            Ok(json!({
                "results": {
                    "bindings": [{
                        "item": {"value": "http://www.wikidata.org/entity/Q167794"},
                        "itemLabel": {"value": "The Tower of Babel"},
                        "creatorLabel": {"value": "Pieter Bruegel the Elder"},
                        "inception": {"value": "1563-01-01T00:00:00Z"},
                        "image": {"value": "https://commons.wikimedia.org/wiki/Special:FilePath/Bruegel.jpg"}
                    }]
                }
            }))
        }
    }

    #[test]
    fn build_wikidata_pack_parses_sparql_bindings() -> Result<(), LensError> {
        let pack = build_wikidata_pack(&FakeSparql, "Kunsthistorisches Museum", KHM_QID, 10)?;
        assert_eq!(pack.items.len(), 1);
        assert_eq!(pack.items[0].qid, "Q167794");
        assert_eq!(pack.items[0].date, "1563-01-01");
        Ok(())
    }
}
