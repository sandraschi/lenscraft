//! M1 World Museum Directory: flagship art museums with QIDs, coordinates, and pack metadata.
use crate::venue::haversine_m;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MuseumEntry {
    pub slug: &'static str,
    pub name: &'static str,
    pub city: &'static str,
    pub country: &'static str,
    pub qid: &'static str,
    pub lat: f64,
    pub lon: f64,
    pub pack_limit: usize,
    pub highlights: &'static [&'static str],
}

pub const WORLD_MUSEUMS: &[MuseumEntry] = &[
    // Vienna flagships
    MuseumEntry {
        slug: "khm",
        name: "Kunsthistorisches Museum Wien",
        city: "Vienna",
        country: "Austria",
        qid: "Q95569",
        lat: 48.2038,
        lon: 16.3618,
        pack_limit: 250,
        highlights: &["The Peasant Wedding", "The Tower of Babel", "Kunstkammer"],
    },
    MuseumEntry {
        slug: "belvedere",
        name: "Belvedere",
        city: "Vienna",
        country: "Austria",
        qid: "Q303139",
        lat: 48.1915,
        lon: 16.3809,
        pack_limit: 250,
        highlights: &["The Kiss (Klimt)", "Schiele", "Baroque Marble Hall"],
    },
    MuseumEntry {
        slug: "albertina",
        name: "Albertina",
        city: "Vienna",
        country: "Austria",
        qid: "Q371908",
        lat: 48.2047,
        lon: 16.3682,
        pack_limit: 250,
        highlights: &["Young Hare (Durer)", "Monet to Picasso", "State Rooms"],
    },
    MuseumEntry {
        slug: "leopold",
        name: "Leopold Museum",
        city: "Vienna",
        country: "Austria",
        qid: "Q59435",
        lat: 48.2026,
        lon: 16.3591,
        pack_limit: 200,
        highlights: &["Egon Schiele Collection", "Gustav Klimt", "Vienna 1900"],
    },
    MuseumEntry {
        slug: "mumok",
        name: "mumok",
        city: "Vienna",
        country: "Austria",
        qid: "Q689951",
        lat: 48.2037,
        lon: 16.3577,
        pack_limit: 150,
        highlights: &["Pop Art", "Fluxus", "Viennese Actionism"],
    },
    MuseumEntry {
        slug: "liechtenstein",
        name: "Liechtenstein Museum",
        city: "Vienna",
        country: "Austria",
        qid: "Q1824069",
        lat: 48.2225,
        lon: 16.3595,
        pack_limit: 150,
        highlights: &["Rubens", "Van Dyck", "Golden Carriage"],
    },
    // Global flagships
    MuseumEntry {
        slug: "louvre",
        name: "Louvre",
        city: "Paris",
        country: "France",
        qid: "Q19675",
        lat: 48.8606,
        lon: 2.3376,
        pack_limit: 500,
        highlights: &["Mona Lisa", "Winged Victory of Samothrace", "Venus de Milo"],
    },
    MuseumEntry {
        slug: "orsay",
        name: "Musee d'Orsay",
        city: "Paris",
        country: "France",
        qid: "Q23402",
        lat: 48.8600,
        lon: 2.3266,
        pack_limit: 300,
        highlights: &[
            "Starry Night Over the Rhone",
            "Olympia",
            "Bal du moulin de la Galette",
        ],
    },
    MuseumEntry {
        slug: "prado",
        name: "Museo del Prado",
        city: "Madrid",
        country: "Spain",
        qid: "Q160112",
        lat: 40.4138,
        lon: -3.6921,
        pack_limit: 350,
        highlights: &[
            "Las Meninas",
            "The Garden of Earthly Delights",
            "The Third of May 1808",
        ],
    },
    MuseumEntry {
        slug: "uffizi",
        name: "Uffizi Gallery",
        city: "Florence",
        country: "Italy",
        qid: "Q51252",
        lat: 43.7678,
        lon: 11.2553,
        pack_limit: 350,
        highlights: &["The Birth of Venus", "Primavera", "Medusa"],
    },
    MuseumEntry {
        slug: "rijksmuseum",
        name: "Rijksmuseum",
        city: "Amsterdam",
        country: "Netherlands",
        qid: "Q190804",
        lat: 52.3600,
        lon: 4.8852,
        pack_limit: 350,
        highlights: &[
            "The Night Watch",
            "The Milkmaid",
            "Self-Portrait (Van Gogh)",
        ],
    },
    MuseumEntry {
        slug: "british-museum",
        name: "British Museum",
        city: "London",
        country: "United Kingdom",
        qid: "Q6373",
        lat: 51.5194,
        lon: -0.1270,
        pack_limit: 350,
        highlights: &["Rosetta Stone", "Parthenon Sculptures", "Sutton Hoo"],
    },
    MuseumEntry {
        slug: "national-gallery-london",
        name: "National Gallery",
        city: "London",
        country: "United Kingdom",
        qid: "Q180788",
        lat: 51.5089,
        lon: -0.1283,
        pack_limit: 350,
        highlights: &[
            "The Arnolfini Portrait",
            "Sunflowers",
            "The Fighting Temeraire",
        ],
    },
    MuseumEntry {
        slug: "met",
        name: "Metropolitan Museum of Art",
        city: "New York",
        country: "United States",
        qid: "Q160236",
        lat: 40.7794,
        lon: -73.9632,
        pack_limit: 400,
        highlights: &[
            "Temple of Dendur",
            "Washington Crossing the Delaware",
            "Wheat Field with Cypresses",
        ],
    },
    MuseumEntry {
        slug: "moma",
        name: "Museum of Modern Art",
        city: "New York",
        country: "United States",
        qid: "Q188740",
        lat: 40.7614,
        lon: -73.9776,
        pack_limit: 300,
        highlights: &[
            "The Starry Night",
            "Les Demoiselles d'Avignon",
            "The Persistence of Memory",
        ],
    },
    MuseumEntry {
        slug: "hermitage",
        name: "Hermitage Museum",
        city: "Saint Petersburg",
        country: "Russia",
        qid: "Q132783",
        lat: 59.9398,
        lon: 30.3146,
        pack_limit: 350,
        highlights: &[
            "The Return of the Prodigal Son",
            "Peacock Clock",
            "The Dance",
        ],
    },
    MuseumEntry {
        slug: "gemaeldegalerie-berlin",
        name: "Gemaldegalerie Berlin",
        city: "Berlin",
        country: "Germany",
        qid: "Q165631",
        lat: 52.5086,
        lon: 13.3647,
        pack_limit: 250,
        highlights: &[
            "Netherlandish Proverbs",
            "The Man with the Golden Helmet",
            "Caravaggio",
        ],
    },
    MuseumEntry {
        slug: "kunsthaus-zurich",
        name: "Kunsthaus Zurich",
        city: "Zurich",
        country: "Switzerland",
        qid: "Q685038",
        lat: 47.3702,
        lon: 8.5484,
        pack_limit: 200,
        highlights: &["Giacometti", "Monet Water Lilies", "Chagall"],
    },
];

pub fn find_museum(slug_or_qid: &str) -> Option<&'static MuseumEntry> {
    let key = slug_or_qid.trim().to_lowercase();
    WORLD_MUSEUMS
        .iter()
        .find(|m| m.slug == key || m.qid.to_lowercase() == key)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MuseumSearchHit {
    pub slug: String,
    pub name: String,
    pub city: String,
    pub country: String,
    pub qid: String,
    pub lat: f64,
    pub lon: f64,
    pub pack_limit: usize,
    pub highlights: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance_km: Option<f64>,
}

/// Filter and optionally distance-sort the world museum directory.
pub fn search_museums(query: Option<&str>, origin: Option<(f64, f64)>) -> Vec<MuseumSearchHit> {
    let q = query.unwrap_or("").trim().to_lowercase();
    let mut hits: Vec<MuseumSearchHit> = WORLD_MUSEUMS
        .iter()
        .filter(|m| {
            if q.is_empty() {
                return true;
            }
            m.name.to_lowercase().contains(&q)
                || m.city.to_lowercase().contains(&q)
                || m.country.to_lowercase().contains(&q)
                || m.slug.contains(&q)
                || m.highlights.iter().any(|h| h.to_lowercase().contains(&q))
        })
        .map(|m| MuseumSearchHit {
            slug: m.slug.to_string(),
            name: m.name.to_string(),
            city: m.city.to_string(),
            country: m.country.to_string(),
            qid: m.qid.to_string(),
            lat: m.lat,
            lon: m.lon,
            pack_limit: m.pack_limit,
            highlights: m.highlights.iter().map(|s| (*s).to_string()).collect(),
            distance_km: origin.map(|(la, lo)| haversine_m(la, lo, m.lat, m.lon) / 1000.0),
        })
        .collect();
    if origin.is_some() {
        hits.sort_by(|a, b| {
            a.distance_km
                .unwrap_or(f64::MAX)
                .total_cmp(&b.distance_km.unwrap_or(f64::MAX))
        });
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_has_18_flagships_and_distance_sorts_from_vienna() {
        assert_eq!(WORLD_MUSEUMS.len(), 18);
        assert!(find_museum("khm").is_some());
        assert!(find_museum("Q19675").is_some());
        let sorted = search_museums(None, Some((48.2038, 16.3618)));
        assert_eq!(sorted[0].slug, "khm");
        let paris = search_museums(Some("Paris"), None);
        assert_eq!(paris.len(), 2);
    }
}
