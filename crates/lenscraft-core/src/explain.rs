//! L2 long-form analysis in 30 s / 3 min / 10 min lengths with sourced facts marked,
//! plus clean sentence splitting for L3 speech output.
use crate::error::LensError;
use crate::grounding::GroundingInfo;
use crate::identify::IdentifyResult;
use crate::speech::split_sentences;
use crate::vision::{capabilities, validate_model, VisionTransport, DEFAULT_MODEL};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LengthPreset {
    Brief30s,
    #[default]
    Standard3m,
    Deep10m,
}

impl LengthPreset {
    pub fn parse(raw: &str) -> Result<Self, LensError> {
        match raw.trim().to_lowercase().as_str() {
            "30s" | "brief" | "short" | "brief_30s" => Ok(Self::Brief30s),
            "3m" | "3min" | "standard" | "medium" | "standard_3m" => Ok(Self::Standard3m),
            "10m" | "10min" | "deep" | "long" | "deep_10m" => Ok(Self::Deep10m),
            other => Err(LensError::InvalidInput(format!(
                "unknown length preset '{other}'; expected 30s, 3m, or 10m"
            ))),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Brief30s => "30s",
            Self::Standard3m => "3m",
            Self::Deep10m => "10m",
        }
    }

    fn word_target(self) -> (usize, u32) {
        match self {
            Self::Brief30s => (75, 400),
            Self::Standard3m => (420, 1200),
            Self::Deep10m => (1300, 2800),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactBasis {
    Sourced,
    Visual,
    Model,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExplanationSection {
    pub heading: String,
    pub text: String,
    pub basis: FactBasis,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Explanation {
    pub title: String,
    pub creator: String,
    pub length: LengthPreset,
    pub language: String,
    pub sections: Vec<ExplanationSection>,
    pub full_text: String,
    pub sentences: Vec<String>,
    pub sourced_count: usize,
    pub model_count: usize,
}

fn explain_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "sections": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "heading": {"type": "string"},
                        "text": {"type": "string"},
                        "basis": {"type": "string", "enum": ["sourced", "visual", "model"]},
                        "source_ref": {"type": "string"}
                    },
                    "required": ["heading", "text", "basis"]
                }
            }
        },
        "required": ["sections"]
    })
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawExplain {
    sections: Vec<RawSection>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawSection {
    heading: String,
    text: String,
    basis: String,
    source_ref: Option<String>,
}

fn verify_basis(
    claimed: &str,
    source_ref: Option<String>,
    text: &str,
    grounding: Option<&GroundingInfo>,
    label_text: &str,
) -> (FactBasis, Option<String>) {
    let text_lc = text.to_lowercase();
    let has_grounding_overlap = grounding.is_some_and(|g| {
        g.sourced_facts.iter().any(|f| {
            let v = f.value.to_lowercase();
            v.split_whitespace()
                .filter(|w| w.len() >= 5)
                .any(|w| text_lc.contains(w))
        })
    });
    let has_label_overlap = !label_text.trim().is_empty()
        && label_text
            .to_lowercase()
            .split_whitespace()
            .filter(|w| w.len() >= 5)
            .any(|w| text_lc.contains(w));

    match claimed.trim().to_lowercase().as_str() {
        "sourced" if has_grounding_overlap || has_label_overlap => {
            let ref_str = source_ref
                .filter(|s| !s.trim().is_empty())
                .or_else(|| grounding.map(|g| format!("wikidata:{}", g.qid)));
            (FactBasis::Sourced, ref_str)
        }
        "visual" => (FactBasis::Visual, None),
        // Downgrade unverified "sourced" claims to "model" so we never overstate provenance.
        _ => (FactBasis::Model, None),
    }
}

/// Produce a structured museum guide explanation in 30s / 3m / 10m length with sourced facts marked.
pub fn explain(
    t: &dyn VisionTransport,
    result: &IdentifyResult,
    grounding: Option<&GroundingInfo>,
    length: LengthPreset,
    language: &str,
    model: &str,
) -> Result<Explanation, LensError> {
    let model = if model.trim().is_empty() {
        DEFAULT_MODEL.to_string()
    } else {
        validate_model(model)?
    };
    let lang = if language.trim().is_empty() {
        "en"
    } else {
        language.trim()
    };
    let top = result.candidates.first();
    let title = grounding
        .map(|g| g.label.clone())
        .or_else(|| top.map(|c| c.title.clone()))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Unidentified work".to_string());
    let creator = grounding
        .and_then(|g| g.creator.clone())
        .or_else(|| top.map(|c| c.creator.clone()))
        .unwrap_or_default();

    let (words, max_tokens) = length.word_target();
    let caps = capabilities(t, &model).unwrap_or_default();

    let mut ctx = format!(
        "Work: {title}\nCreator: {creator}\nSubject type: {}\nVerdict: {:?} ({:?})\nVisual description: {}\nWall label text: {}\n",
        result.subject_type, result.verdict, result.basis, result.description, result.label_text
    );
    if let Some(g) = grounding {
        ctx.push_str(&format!(
            "Sourced Wikidata ({qid}): {desc}\nInception: {inc}\nCollection: {col}\n",
            qid = g.qid,
            desc = g.description,
            inc = g.inception.as_deref().unwrap_or("unknown"),
            col = g.collection.as_deref().unwrap_or("unknown"),
        ));
        if let Some(ext) = &g.wikipedia_extract {
            ctx.push_str(&format!("Wikipedia extract: {ext}\n"));
        }
    }

    let system =
        "You are a museum audio-guide writer. Write clear spoken prose divided into short sections.
Mark each section's basis:
- 'sourced': facts directly backed by the provided Wikidata/Wikipedia context or the wall label.
- 'visual': observations directly visible in the artwork description.
- 'model': broader art-historical context from your training knowledge.
Never invent dates, inventory numbers, or quotes.";

    let user = format!(
        "{ctx}\nWrite an engaging {preset} spoken guide (~{words} words total) in language '{lang}'.",
        preset = length.label()
    );

    let mut body = json!({
        "model": model,
        "stream": false,
        "format": explain_schema(),
        "options": {"temperature": 0.3, "num_ctx": 8192, "num_predict": max_tokens},
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ]
    });
    if caps.iter().any(|x| x == "thinking") {
        if let Some(o) = body.as_object_mut() {
            o.insert("think".to_string(), Value::Bool(false));
        }
    }

    let resp = t.chat(&body)?;
    let content = resp
        .pointer("/message/content")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            LensError::Protocol("ollama explain reply missing message.content".to_string())
        })?;
    let raw: RawExplain = serde_json::from_str(content)
        .map_err(|e| LensError::Protocol(format!("explain reply is not valid JSON ({e})")))?;

    let mut sections = Vec::new();
    let mut sourced_count = 0usize;
    let mut model_count = 0usize;
    for s in raw.sections {
        let text = s.text.trim().to_string();
        if text.is_empty() {
            continue;
        }
        let heading = if s.heading.trim().is_empty() {
            "Overview".to_string()
        } else {
            s.heading.trim().to_string()
        };
        let (basis, source_ref) =
            verify_basis(&s.basis, s.source_ref, &text, grounding, &result.label_text);
        match basis {
            FactBasis::Sourced => sourced_count += 1,
            FactBasis::Model => model_count += 1,
            FactBasis::Visual => {}
        }
        sections.push(ExplanationSection {
            heading,
            text,
            basis,
            source_ref,
        });
    }
    if sections.is_empty() {
        return Err(LensError::Protocol(
            "explain reply contained no usable sections".to_string(),
        ));
    }

    let full_text = sections
        .iter()
        .map(|s| {
            let tag = match s.basis {
                FactBasis::Sourced => "[sourced]",
                FactBasis::Visual => "[visual]",
                FactBasis::Model => "[model]",
            };
            format!("{} {}: {}", tag, s.heading, s.text)
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let spoken_plain = sections
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let sentences = split_sentences(&spoken_plain);

    Ok(Explanation {
        title,
        creator,
        length,
        language: lang.to_string(),
        sections,
        full_text,
        sentences,
        sourced_count,
        model_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grounding::SourcedFact;
    use crate::identify::{Basis, Candidate, Certainty, ImageInfo, Verdict};
    use serde_json::json;

    struct FakeExplain;
    impl VisionTransport for FakeExplain {
        fn base_url(&self) -> String {
            "fake".to_string()
        }
        fn show(&self, _model: &str) -> Result<Value, LensError> {
            Ok(json!({"capabilities": ["completion", "vision"]}))
        }
        fn chat(&self, _body: &Value) -> Result<Value, LensError> {
            Ok(json!({
                "message": {
                    "content": json!({
                        "sections": [
                            {
                                "heading": "The Work",
                                "text": "Pieter Bruegel painted The Tower of Babel in 1563 for the Kunsthistorisches Museum.",
                                "basis": "sourced",
                                "source_ref": "wikidata:Q167794"
                            },
                            {
                                "heading": "What to Notice",
                                "text": "Notice the tiny figures hauling stone up the spiral ramp.",
                                "basis": "visual"
                            },
                            {
                                "heading": "Context",
                                "text": "Sixteenth-century Netherlandish painters often contrasted human ambition with the vastness of nature.",
                                "basis": "sourced"
                            }
                        ]
                    }).to_string()
                }
            }))
        }
        fn version(&self) -> Result<String, LensError> {
            Ok("test".to_string())
        }
    }

    #[test]
    fn explain_verifies_sourced_claims_and_splits_sentences() -> Result<(), LensError> {
        let res = IdentifyResult {
            verdict: Verdict::Identified,
            basis: Basis::Label,
            subject_type: "painting".to_string(),
            description: "A massive tower.".to_string(),
            label_text: "Bruegel Turmbau zu Babel 1563".to_string(),
            candidates: vec![Candidate {
                title: "The Tower of Babel".to_string(),
                creator: "Pieter Bruegel".to_string(),
                date: "1563".to_string(),
                certainty: Certainty::High,
                evidence: vec![],
                label_match: None,
            }],
            model: DEFAULT_MODEL.to_string(),
            image: ImageInfo::default(),
            elapsed_ms: 100,
            notes: vec![],
        };
        let grounding = GroundingInfo {
            qid: "Q167794".to_string(),
            label: "The Tower of Babel".to_string(),
            creator: Some("Pieter Bruegel".to_string()),
            sourced_facts: vec![SourcedFact {
                field: "collection".to_string(),
                value: "Kunsthistorisches Museum".to_string(),
                source: "wikidata:Q167794".to_string(),
            }],
            ..GroundingInfo::default()
        };
        let out = explain(
            &FakeExplain,
            &res,
            Some(&grounding),
            LengthPreset::Brief30s,
            "en",
            "",
        )?;
        assert_eq!(out.sections.len(), 3);
        assert_eq!(out.sections[0].basis, FactBasis::Sourced);
        assert_eq!(out.sections[1].basis, FactBasis::Visual);
        // Unbacked third section claiming "sourced" is downgraded to Model
        assert_eq!(out.sections[2].basis, FactBasis::Model);
        assert_eq!(out.sentences.len(), 3);
        Ok(())
    }
}
