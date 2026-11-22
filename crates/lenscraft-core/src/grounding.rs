//! L2 grounding: verify and enrich candidates against Wikidata and Wikipedia.
use crate::error::LensError;
use crate::identify::IdentifyResult;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(12);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourcedFact {
    pub field: String,
    pub value: String,
    pub source: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroundingInfo {
    pub query: String,
    pub qid: String,
    pub label: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inception: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wikipedia_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wikipedia_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wikipedia_extract: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sourced_facts: Vec<SourcedFact>,
}

pub trait KnowledgeTransport {
    fn get_json(&self, url: &str) -> Result<Value, LensError>;
}

pub struct HttpKnowledgeClient {
    client: reqwest::blocking::Client,
}

impl HttpKnowledgeClient {
    pub fn new() -> Result<Self, LensError> {
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(TIMEOUT)
            .timeout(TIMEOUT)
            .user_agent(
                "lenscraft/0.1 (local museum lens; https://github.com/sandraschi/lenscraft)",
            )
            .build()
            .map_err(|e| LensError::Offline(format!("knowledge client: {e}")))?;
        Ok(Self { client })
    }
}

impl KnowledgeTransport for HttpKnowledgeClient {
    fn get_json(&self, url: &str) -> Result<Value, LensError> {
        let resp =
            self.client.get(url).send().map_err(|e| {
                LensError::Offline(format!("knowledge request failed ({url}): {e}"))
            })?;
        let status = resp.status();
        let text = resp
            .text()
            .map_err(|e| LensError::Protocol(format!("knowledge read failed: {e}")))?;
        if !status.is_success() {
            return Err(LensError::Protocol(format!(
                "knowledge {status} for {url}: {}",
                text.chars().take(200).collect::<String>()
            )));
        }
        serde_json::from_str(&text)
            .map_err(|e| LensError::Protocol(format!("knowledge invalid json: {e}")))
    }
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

fn first_claim_qid(entity: &Value, prop: &str) -> Option<String> {
    entity
        .pointer(&format!("/claims/{prop}/0/mainsnak/datavalue/value/id"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn first_claim_time(entity: &Value, prop: &str) -> Option<String> {
    let raw = entity
        .pointer(&format!("/claims/{prop}/0/mainsnak/datavalue/value/time"))
        .and_then(Value::as_str)?;
    let trimmed = raw.trim_start_matches('+');
    Some(
        trimmed
            .split('T')
            .next()
            .unwrap_or(trimmed)
            .trim_end_matches("-00")
            .to_string(),
    )
}

fn fetch_entity_label(t: &dyn KnowledgeTransport, qid: &str, lang: &str) -> Option<String> {
    let url = format!(
        "https://www.wikidata.org/w/api.php?action=wbgetentities&ids={qid}&props=labels&languages={lang}|en&format=json"
    );
    let v = t.get_json(&url).ok()?;
    v.pointer(&format!("/entities/{qid}/labels/{lang}/value"))
        .or_else(|| v.pointer(&format!("/entities/{qid}/labels/en/value")))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Ground a candidate work (title + optional creator) against Wikidata and Wikipedia.
pub fn ground_work(
    t: &dyn KnowledgeTransport,
    title: &str,
    creator: Option<&str>,
    language: &str,
) -> Result<Option<GroundingInfo>, LensError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(LensError::InvalidInput(
            "title must not be empty for grounding".to_string(),
        ));
    }
    let lang = if language.trim().is_empty() {
        "en"
    } else {
        language.trim()
    };
    let query = match creator.map(str::trim).filter(|s| !s.is_empty()) {
        Some(c) => format!("{title} {c}"),
        None => title.to_string(),
    };
    let search_url = format!(
        "https://www.wikidata.org/w/api.php?action=wbsearchentities&search={}&language={}&uselang={}&limit=5&format=json",
        percent_encode(&query),
        percent_encode(lang),
        percent_encode(lang),
    );
    let search = t.get_json(&search_url)?;
    let Some(hits) = search.get("search").and_then(Value::as_array) else {
        return Ok(None);
    };
    let Some(top_qid) = hits
        .first()
        .and_then(|h| h.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return Ok(None);
    };

    let entity_url = format!(
        "https://www.wikidata.org/w/api.php?action=wbgetentities&ids={top_qid}&props=labels|descriptions|claims|sitelinks&languages={lang}|en&format=json"
    );
    let entity_resp = t.get_json(&entity_url)?;
    let Some(entity) = entity_resp.pointer(&format!("/entities/{top_qid}")) else {
        return Ok(None);
    };

    let label = entity
        .pointer(&format!("/labels/{lang}/value"))
        .or_else(|| entity.pointer("/labels/en/value"))
        .and_then(Value::as_str)
        .unwrap_or(title)
        .to_string();
    let description = entity
        .pointer(&format!("/descriptions/{lang}/value"))
        .or_else(|| entity.pointer("/descriptions/en/value"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let mut sourced_facts = Vec::new();
    let wd_source = format!("wikidata:{top_qid}");
    if !description.is_empty() {
        sourced_facts.push(SourcedFact {
            field: "description".to_string(),
            value: description.clone(),
            source: wd_source.clone(),
        });
    }

    // P170 = creator, P571 = inception, P195 = collection
    let creator_label =
        first_claim_qid(entity, "P170").and_then(|q| fetch_entity_label(t, &q, lang));
    if let Some(ref c) = creator_label {
        sourced_facts.push(SourcedFact {
            field: "creator".to_string(),
            value: c.clone(),
            source: wd_source.clone(),
        });
    }
    let inception = first_claim_time(entity, "P571");
    if let Some(ref inc) = inception {
        sourced_facts.push(SourcedFact {
            field: "inception".to_string(),
            value: inc.clone(),
            source: wd_source.clone(),
        });
    }
    let collection = first_claim_qid(entity, "P195").and_then(|q| fetch_entity_label(t, &q, lang));
    if let Some(ref col) = collection {
        sourced_facts.push(SourcedFact {
            field: "collection".to_string(),
            value: col.clone(),
            source: wd_source.clone(),
        });
    }

    let wiki_key = format!("{lang}wiki");
    let sitelink = entity
        .pointer(&format!("/sitelinks/{wiki_key}/title"))
        .or_else(|| entity.pointer("/sitelinks/enwiki/title"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let wiki_lang = if entity
        .pointer(&format!("/sitelinks/{wiki_key}/title"))
        .is_some()
    {
        lang
    } else {
        "en"
    };

    let (wikipedia_url, wikipedia_extract) = if let Some(ref wp_title) = sitelink {
        let summary_url = format!(
            "https://{wiki_lang}.wikipedia.org/api/rest_v1/page/summary/{}",
            percent_encode(wp_title)
        );
        match t.get_json(&summary_url) {
            Ok(wp) => {
                let page_url = wp
                    .pointer("/content_urls/desktop/page")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let extract = wp
                    .get("extract")
                    .and_then(Value::as_str)
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                if let Some(ref ext) = extract {
                    sourced_facts.push(SourcedFact {
                        field: "wikipedia_extract".to_string(),
                        value: ext.clone(),
                        source: format!("wikipedia:{wiki_lang}"),
                    });
                }
                (page_url, extract)
            }
            Err(_) => (None, None),
        }
    } else {
        (None, None)
    };

    Ok(Some(GroundingInfo {
        query,
        qid: top_qid,
        label,
        description,
        creator: creator_label,
        inception,
        collection,
        wikipedia_title: sitelink,
        wikipedia_url,
        wikipedia_extract,
        sourced_facts,
    }))
}

/// Ground the top candidate of an `IdentifyResult`, if present.
pub fn ground_result(
    t: &dyn KnowledgeTransport,
    result: &IdentifyResult,
    language: &str,
) -> Result<Option<GroundingInfo>, LensError> {
    if let Some(top) = result.candidates.first() {
        let title = if top.title.is_empty() {
            top.creator.as_str()
        } else {
            top.title.as_str()
        };
        let creator = if top.creator.is_empty() {
            None
        } else {
            Some(top.creator.as_str())
        };
        if !title.is_empty() {
            return ground_work(t, title, creator, language);
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct FakeKnowledge;
    impl KnowledgeTransport for FakeKnowledge {
        fn get_json(&self, url: &str) -> Result<Value, LensError> {
            if url.contains("wbsearchentities") {
                Ok(json!({
                    "search": [{"id": "Q167794", "label": "The Tower of Babel"}]
                }))
            } else if url.contains("ids=Q167794") && url.contains("claims") {
                Ok(json!({
                    "entities": {
                        "Q167794": {
                            "labels": {"en": {"value": "The Tower of Babel"}},
                            "descriptions": {"en": {"value": "painting by Pieter Bruegel the Elder (1563), KHM"}},
                            "claims": {
                                "P170": [{"mainsnak": {"datavalue": {"value": {"id": "Q43270"}}}}],
                                "P571": [{"mainsnak": {"datavalue": {"value": {"time": "+1563-00-00T00:00:00Z"}}}}],
                                "P195": [{"mainsnak": {"datavalue": {"value": {"id": "Q95569"}}}}]
                            },
                            "sitelinks": {
                                "enwiki": {"title": "The Tower of Babel (Bruegel)"}
                            }
                        }
                    }
                }))
            } else if url.contains("ids=Q43270") {
                Ok(
                    json!({"entities": {"Q43270": {"labels": {"en": {"value": "Pieter Bruegel the Elder"}}}}}),
                )
            } else if url.contains("ids=Q95569") {
                Ok(
                    json!({"entities": {"Q95569": {"labels": {"en": {"value": "Kunsthistorisches Museum"}}}}}),
                )
            } else if url.contains("wikipedia.org/api/rest_v1/page/summary") {
                Ok(json!({
                    "content_urls": {"desktop": {"page": "https://en.wikipedia.org/wiki/The_Tower_of_Babel_(Bruegel)"}},
                    "extract": "The Tower of Babel was the subject of three oil paintings by Pieter Bruegel the Elder."
                }))
            } else {
                Err(LensError::Protocol(format!("unexpected url: {url}")))
            }
        }
    }

    #[test]
    fn ground_work_collects_wikidata_and_wikipedia_facts() -> Result<(), LensError> {
        let g = ground_work(&FakeKnowledge, "The Tower of Babel", Some("Bruegel"), "en")?
            .ok_or_else(|| LensError::Protocol("expected grounding".to_string()))?;
        assert_eq!(g.qid, "Q167794");
        assert_eq!(g.creator.as_deref(), Some("Pieter Bruegel the Elder"));
        assert_eq!(g.inception.as_deref(), Some("1563"));
        assert_eq!(g.collection.as_deref(), Some("Kunsthistorisches Museum"));
        assert!(g.wikipedia_extract.is_some());
        assert!(g
            .sourced_facts
            .iter()
            .any(|f| f.source == "wikidata:Q167794"));
        assert!(g.sourced_facts.iter().any(|f| f.source == "wikipedia:en"));
        Ok(())
    }
}
