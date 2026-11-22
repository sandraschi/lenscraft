//! 3D Walk-in Simulation DB & PLY relief-splat generator for iconic paintings.
//! Stores `.spz` / `.ply` worlds (from `splatcraft` Marble or local generation)
//! keyed by Wikidata QID so any identified work can be elicited anytime.
use crate::error::LensError;
use crate::image_prep::PreparedImage;
use image::GenericImageView as _;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const DEFAULT_WORLD_DB_PATH: &str = "worlds/worlds_db.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldStatus {
    Ready,
    Queued,
    Failed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldKind {
    #[default]
    Painting,
    GalleryRoom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldEntry {
    pub qid: String,
    pub title: String,
    pub creator: String,
    pub museum: String,
    #[serde(default)]
    pub kind: WorldKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub museum_slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room_id: Option<String>,
    pub splat_path: String,
    pub format: String,
    pub source: String,
    #[serde(default)]
    pub prompt: String,
    pub status: WorldStatus,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldDb {
    pub version: u32,
    pub entries: Vec<WorldEntry>,
}

impl WorldDb {
    pub fn load(path: &Path) -> Result<Self, LensError> {
        if !path.exists() {
            return Ok(Self {
                version: 1,
                entries: Vec::new(),
            });
        }
        let raw = std::fs::read_to_string(path)
            .map_err(|e| LensError::Io(format!("read world db failed: {e}")))?;
        serde_json::from_str(&raw)
            .map_err(|e| LensError::Protocol(format!("parse world db failed: {e}")))
    }

    pub fn save(&self, path: &Path) -> Result<(), LensError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| LensError::Io(format!("mkdir world db failed: {e}")))?;
            }
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| LensError::Protocol(format!("serialize world db failed: {e}")))?;
        let tmp = PathBuf::from(format!("{}.tmp", path.display()));
        std::fs::write(&tmp, json)
            .map_err(|e| LensError::Io(format!("write world db tmp failed: {e}")))?;
        std::fs::rename(&tmp, path)
            .map_err(|e| LensError::Io(format!("rename world db failed: {e}")))
    }

    pub fn upsert(&mut self, entry: WorldEntry) {
        if let Some(existing) = self.entries.iter_mut().find(|e| e.qid == entry.qid) {
            *existing = entry;
        } else {
            self.entries.push(entry);
        }
    }

    pub fn get_by_qid(&self, qid: &str) -> Option<&WorldEntry> {
        let q = qid.trim();
        self.entries
            .iter()
            .find(|e| e.qid.eq_ignore_ascii_case(q) && e.status == WorldStatus::Ready)
    }

    pub fn find_by_title(&self, title: &str) -> Option<&WorldEntry> {
        let t = title.trim().to_lowercase();
        if t.is_empty() {
            return None;
        }
        self.entries.iter().find(|e| {
            e.status == WorldStatus::Ready
                && (e.title.to_lowercase().contains(&t) || t.contains(&e.title.to_lowercase()))
        })
    }

    pub fn list_ready(&self) -> Vec<&WorldEntry> {
        self.entries
            .iter()
            .filter(|e| e.status == WorldStatus::Ready)
            .collect()
    }
}

/// Generate a local 3D relief point-cloud `.ply` from a painting image (luminance + perspective
/// depth) for offline walk-in preview when a full `splatcraft` Marble `.spz` is not yet built.
pub fn generate_relief_ply(
    img: &PreparedImage,
    out_ply: &Path,
    depth_scale: f32,
) -> Result<usize, LensError> {
    let dyn_img = image::load_from_memory(&img.jpeg)
        .map_err(|e| LensError::Image(format!("decode image for 3d relief failed: {e}")))?;
    let target_w = 160u32;
    let target_h = ((dyn_img.height() as f32 / dyn_img.width().max(1) as f32) * target_w as f32)
        .round()
        .clamp(40.0, 160.0) as u32;
    let small = dyn_img.resize_exact(target_w, target_h, image::imageops::FilterType::Triangle);

    let count = (target_w * target_h) as usize;
    let mut out = format!(
        "ply\nformat ascii 1.0\nelement vertex {count}\n\
         property float x\nproperty float y\nproperty float z\n\
         property uchar red\nproperty uchar green\nproperty uchar blue\nend_header\n"
    );

    let aspect = target_w as f32 / target_h.max(1) as f32;
    for y in 0..target_h {
        let ny = 0.5 - (y as f32 / target_h.max(1) as f32);
        for x in 0..target_w {
            let nx = ((x as f32 / target_w.max(1) as f32) - 0.5) * aspect;
            let p = small.get_pixel(x, y);
            let luma =
                (0.299 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.114 * f32::from(p[2]))
                    / 255.0;
            // Subtle perspective recession + luminance relief so you can step into the scene.
            let radial = (nx * nx + ny * ny).sqrt();
            let z = (luma - 0.5) * depth_scale - radial * (depth_scale * 0.4);
            out.push_str(&format!(
                "{nx:.4} {ny:.4} {z:.4} {} {} {}\n",
                p[0], p[1], p[2]
            ));
        }
    }

    if let Some(parent) = out_ply.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| LensError::Io(format!("mkdir ply failed: {e}")))?;
        }
    }
    std::fs::write(out_ply, out).map_err(|e| LensError::Io(format!("write ply failed: {e}")))?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};
    use std::io::Cursor;

    #[test]
    fn world_db_roundtrip_and_elicitation() -> Result<(), LensError> {
        let dir = std::env::temp_dir().join("lenscraft_world_test");
        let _ = std::fs::create_dir_all(&dir);
        let db_path = dir.join("worlds_db.json");
        let _ = std::fs::remove_file(&db_path);

        let mut db = WorldDb::load(&db_path)?;
        db.upsert(WorldEntry {
            qid: "Q153642".to_string(),
            title: "The Peasant Wedding".to_string(),
            creator: "Pieter Bruegel the Elder".to_string(),
            museum: "Kunsthistorisches Museum Wien".to_string(),
            kind: WorldKind::Painting,
            museum_slug: Some("khm".to_string()),
            room_id: Some("khm-x".to_string()),
            splat_path: "worlds/Q153642.ply".to_string(),
            format: "ply".to_string(),
            source: "splatcraft-marble".to_string(),
            prompt: "Walk inside Bruegel's Peasant Wedding hall".to_string(),
            status: WorldStatus::Ready,
        });
        db.save(&db_path)?;

        let loaded = WorldDb::load(&db_path)?;
        assert_eq!(loaded.list_ready().len(), 1);
        assert_eq!(
            loaded.get_by_qid("Q153642").map(|e| e.title.as_str()),
            Some("The Peasant Wedding")
        );
        assert!(loaded.find_by_title("Peasant Wedding").is_some());
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn generate_relief_ply_writes_valid_ascii_ply() -> Result<(), LensError> {
        let img = ImageBuffer::from_pixel(32, 24, Rgb([200u8, 140, 60]));
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Jpeg)
            .map_err(|e| LensError::Image(format!("encode failed: {e}")))?;
        let prep = PreparedImage {
            jpeg: buf,
            width: 32,
            height: 24,
            source_width: 32,
            source_height: 24,
            source_format: "jpeg".to_string(),
        };
        let out = std::env::temp_dir().join("lenscraft_relief_test.ply");
        let n = generate_relief_ply(&prep, &out, 0.35)?;
        assert!(n > 1000);
        let text = std::fs::read_to_string(&out).map_err(|e| LensError::Io(e.to_string()))?;
        assert!(text.starts_with("ply\nformat ascii 1.0\n"));
        let _ = std::fs::remove_file(&out);
        Ok(())
    }
}
