//! L5 follow-up Q&A grounded in the identified work and its Wikidata/Wikipedia facts.
use crate::error::LensError;
use crate::grounding::GroundingInfo;
use crate::identify::{IdentifyResult, Verdict};
use crate::speech::split_sentences;
use crate::vision::{capabilities, validate_model, VisionTransport, DEFAULT_MODEL};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QaTurn {
    pub role: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FollowupAnswer {
    pub answer: String,
    pub sentences: Vec<String>,
    pub grounded_in: String,
}

/// Answer a visitor's follow-up question grounded in the identified work and conversation history.
pub fn answer_followup(
    t: &dyn VisionTransport,
    result: &IdentifyResult,
    grounding: Option<&GroundingInfo>,
    history: &[QaTurn],
    question: &str,
    language: &str,
    model: &str,
) -> Result<FollowupAnswer, LensError> {
    let question = question.trim();
    if question.is_empty() {
        return Err(LensError::InvalidInput(
            "question must not be empty".to_string(),
        ));
    }
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
    let grounded_in = grounding
        .map(|g| format!("{} ({})", g.label, g.qid))
        .or_else(|| top.map(|c| format!("{} - {}", c.title, c.creator)))
        .unwrap_or_else(|| result.subject_type.clone());

    let correction_rule = if result.verdict == Verdict::Identified {
        "The work was confirmed by its wall label."
    } else {
        "IMPORTANT: The initial identification below is only an unconfirmed visual guess, not confirmed by a wall label. If the visitor tells you it is a different work or artist, immediately accept their correction, drop the initial guess, and answer about the work they named."
    };

    let mut sys = format!(
        "You are a knowledgeable museum companion answering a visitor's spoken follow-up question in language '{lang}'.
Keep answers concise (2 to 5 spoken sentences), concrete, and grounded in the work below. If a detail is unknown, say so rather than inventing it.
{correction_rule}
Work: {grounded_in}
Verdict: {:?} ({:?})
Visual description: {}
Wall label: {}",
        result.verdict, result.basis, result.description, result.label_text
    );
    if let Some(g) = grounding {
        sys.push_str(&format!(
            "\nWikidata ({}): {}\nInception: {}\nCollection: {}",
            g.qid,
            g.description,
            g.inception.as_deref().unwrap_or("unknown"),
            g.collection.as_deref().unwrap_or("unknown")
        ));
        if let Some(ext) = &g.wikipedia_extract {
            sys.push_str(&format!("\nWikipedia summary: {ext}"));
        }
    }

    let mut messages = vec![json!({"role": "system", "content": sys})];
    for turn in history.iter().rev().take(8).rev() {
        let role = if turn.role == "assistant" {
            "assistant"
        } else {
            "user"
        };
        messages.push(json!({"role": role, "content": turn.text}));
    }
    messages.push(json!({"role": "user", "content": question}));

    let caps = capabilities(t, &model).unwrap_or_default();
    let mut body = json!({
        "model": model,
        "stream": false,
        "options": {"temperature": 0.3, "num_ctx": 8192, "num_predict": 450},
        "messages": messages
    });
    if caps.iter().any(|x| x == "thinking") {
        if let Some(o) = body.as_object_mut() {
            o.insert("think".to_string(), Value::Bool(false));
        }
    }

    let resp = t.chat(&body)?;
    let answer = resp
        .pointer("/message/content")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| LensError::Protocol("followup reply missing message.content".to_string()))?
        .to_string();

    let sentences = split_sentences(&answer);
    Ok(FollowupAnswer {
        answer,
        sentences,
        grounded_in,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identify::{Basis, Candidate, Certainty, ImageInfo, Verdict};
    use serde_json::json;

    struct FakeQa;
    impl VisionTransport for FakeQa {
        fn base_url(&self) -> String {
            "fake".to_string()
        }
        fn show(&self, _model: &str) -> Result<Value, LensError> {
            Ok(json!({"capabilities": ["completion"]}))
        }
        fn chat(&self, body: &Value) -> Result<Value, LensError> {
            let msgs = body
                .get("messages")
                .and_then(Value::as_array)
                .ok_or_else(|| LensError::Protocol("missing messages".to_string()))?;
            assert!(msgs.len() >= 3);
            let sys = msgs[0].get("content").and_then(Value::as_str).unwrap_or("");
            assert!(sys.contains("confirmed by its wall label"));
            Ok(json!({
                "message": {
                    "content": "It is painted on oak panel. Bruegel used thin oil glazes."
                }
            }))
        }
        fn version(&self) -> Result<String, LensError> {
            Ok("test".to_string())
        }
    }

    #[test]
    fn followup_uses_history_and_splits_sentences() -> Result<(), LensError> {
        let res = IdentifyResult {
            verdict: Verdict::Identified,
            basis: Basis::Label,
            subject_type: "painting".to_string(),
            description: "Tower".to_string(),
            label_text: "Bruegel".to_string(),
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
            elapsed_ms: 10,
            notes: vec![],
        };
        let ans = answer_followup(
            &FakeQa,
            &res,
            None,
            &[QaTurn {
                role: "user".to_string(),
                text: "Who painted it?".to_string(),
            }],
            "What is it painted on?",
            "en",
            "",
        )?;
        assert_eq!(ans.sentences.len(), 2);
        assert!(ans.grounded_in.contains("The Tower of Babel"));
        Ok(())
    }
}
