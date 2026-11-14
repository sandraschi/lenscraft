//! Identify: prepared photo plus context in, ranked candidates with evidence out.
//!
//! Honesty rules. The model may say it does not know (an empty candidate list), and
//! that is reported as `Verdict::Uncertain`. There are no numeric confidence scores:
//! certainty is the model's own high/medium/low. The one thing this layer adds is a
//! deterministic check of each candidate against the wall label the model
//! transcribed, and `basis` states what the verdict rests on.
use crate::error::LensError;
use crate::image_prep::PreparedImage;
use crate::vision::{capabilities, validate_model, VisionTransport, DEFAULT_MODEL};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Instant;

pub const MAX_HINT_CHARS: usize = 500;
pub const MAX_PLACE_CHARS: usize = 200;
pub const MAX_CANDIDATES: usize = 3;
const MAX_LONG_CHARS: usize = 1500;
const MAX_SHORT_CHARS: usize = 300;
const MAX_EVIDENCE: usize = 6;
/// Output token cap. A full answer is a few hundred tokens; without a cap a model that
/// falls into a repetition loop writes until the context is full.
const MAX_OUTPUT_TOKENS: u32 = 1500;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IdentifyOptions {
    /// Ollama model; empty means `DEFAULT_MODEL`.
    #[serde(default)]
    pub model: String,
    /// The visitor's own note: "KHM, Bruegel room", "the big one on the left".
    #[serde(default)]
    pub hint: Option<String>,
    /// Venue, if known: "Kunsthistorisches Museum Wien".
    #[serde(default)]
    pub place: Option<String>,
    #[serde(default)]
    pub lat: Option<f64>,
    #[serde(default)]
    pub lon: Option<f64>,
    /// Language for description and evidence: "en", "de", "ja". Default "en".
    #[serde(default)]
    pub language: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Certainty {
    #[default]
    Low,
    Medium,
    High,
}

/// What the transcribed label confirms. Ordered weakest to strongest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LabelMatch {
    Creator,
    Title,
    TitleAndCreator,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    #[default]
    Uncertain,
    Likely,
    Identified,
}

/// What the verdict rests on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Basis {
    #[default]
    None,
    Recognition,
    Label,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    pub title: String,
    pub creator: String,
    pub date: String,
    pub certainty: Certainty,
    pub evidence: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_match: Option<LabelMatch>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub source_format: String,
    pub jpeg_bytes: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IdentifyResult {
    pub verdict: Verdict,
    pub basis: Basis,
    pub subject_type: String,
    pub description: String,
    pub label_text: String,
    pub candidates: Vec<Candidate>,
    pub model: String,
    pub image: ImageInfo,
    pub elapsed_ms: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

struct Checked {
    model: String,
    hint: Option<String>,
    place: Option<String>,
    latlon: Option<(f64, f64)>,
    language: String,
}

fn clean(s: &str, max: usize) -> String {
    let kept: String = s
        .chars()
        .map(|c| if c.is_control() && c != '\n' { ' ' } else { c })
        .take(max)
        .collect();
    kept.trim().to_string()
}

fn optional_text(
    label: &str,
    raw: &Option<String>,
    max: usize,
) -> Result<Option<String>, LensError> {
    match raw.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some(t) if t.chars().count() > max => Err(LensError::InvalidInput(format!(
            "{label} exceeds {max} characters"
        ))),
        Some(t) => Ok(Some(clean(t, max))),
    }
}

fn check(opts: &IdentifyOptions) -> Result<Checked, LensError> {
    let model = if opts.model.trim().is_empty() {
        DEFAULT_MODEL.to_string()
    } else {
        validate_model(&opts.model)?
    };
    let hint = optional_text("hint", &opts.hint, MAX_HINT_CHARS)?;
    let place = optional_text("place", &opts.place, MAX_PLACE_CHARS)?;
    let latlon = match (opts.lat, opts.lon) {
        (None, None) => None,
        (Some(lat), Some(lon)) => {
            let ok = lat.is_finite()
                && lon.is_finite()
                && (-90.0..=90.0).contains(&lat)
                && (-180.0..=180.0).contains(&lon);
            if !ok {
                return Err(LensError::InvalidInput(format!(
                    "lat/lon out of range: {lat}, {lon}"
                )));
            }
            Some((lat, lon))
        }
        _ => {
            return Err(LensError::InvalidInput(
                "give both lat and lon, or neither".to_string(),
            ))
        }
    };
    let language = match opts.language.as_deref().map(str::trim) {
        None | Some("") => "en".to_string(),
        Some(l) if l.len() <= 12 && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') => {
            l.to_string()
        }
        Some(l) => {
            return Err(LensError::InvalidInput(format!(
                "language must be a short code like en, de or ja; got '{l}'"
            )))
        }
    };
    Ok(Checked {
        model,
        hint,
        place,
        latlon,
        language,
    })
}

const SYSTEM_PROMPT: &str = "You identify what a photo shows for a visitor standing in front of it: usually a painting or sculpture in a museum, sometimes a building, monument or object. Answer only from what is visible in the photo and from what you actually know.
Rules:
1. label_text: transcribe any readable text in the photo exactly (wall label, plaque, signature, inscription). Empty string if there is none.
2. candidates: up to 3 specific identifications, most likely first. certainty is high only if you recognise this specific work or the label names it; medium if subject, style and context strongly point to it; low if it is a guess.
3. evidence: short concrete reasons, each tied to something visible (composition, figures, technique, text) or to the label. No generic praise.
4. If you cannot name the specific work, return an empty candidates list. Never invent titles, dates, inventory numbers or attributions.
5. description: what is visibly there, in two to four sentences.";

fn user_prompt(c: &Checked) -> String {
    let mut p = String::from("What is this?");
    if let Some(place) = &c.place {
        p.push_str("\nThe visitor is at: ");
        p.push_str(place);
    }
    if let Some((lat, lon)) = c.latlon {
        p.push_str(&format!("\nPhone position: {lat:.5}, {lon:.5}"));
    }
    if let Some(hint) = &c.hint {
        p.push_str("\nVisitor's note: ");
        p.push_str(hint);
    }
    p.push_str(&format!(
        "\nWrite description and evidence in language '{}'. Keep titles and names in their usual published form.",
        c.language
    ));
    p
}

/// JSON schema the model must answer in (Ollama structured output).
pub fn answer_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "subject_type": {"type": "string", "enum": ["painting", "sculpture", "drawing_or_print", "building", "monument", "object", "text", "scene", "other"]},
            "description": {"type": "string"},
            "label_text": {"type": "string"},
            "candidates": {
                "type": "array",
                "maxItems": MAX_CANDIDATES,
                "items": {
                    "type": "object",
                    "properties": {
                        "title": {"type": "string"},
                        "creator": {"type": "string"},
                        "date": {"type": "string"},
                        "certainty": {"type": "string", "enum": ["high", "medium", "low"]},
                        "evidence": {"type": "array", "items": {"type": "string"}}
                    },
                    "required": ["title", "creator", "date", "certainty", "evidence"]
                }
            }
        },
        "required": ["subject_type", "description", "label_text", "candidates"]
    })
}

fn build_request(c: &Checked, caps: &[String], img: &PreparedImage) -> Value {
    let b64 = base64::engine::general_purpose::STANDARD.encode(&img.jpeg);
    let mut body = json!({
        "model": c.model,
        "stream": false,
        "format": answer_schema(),
        "options": {"temperature": 0.1, "num_ctx": 8192, "num_predict": MAX_OUTPUT_TOKENS},
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": user_prompt(c), "images": [b64]}
        ]
    });
    if caps.iter().any(|x| x == "thinking") {
        if let Some(o) = body.as_object_mut() {
            o.insert("think".to_string(), Value::Bool(false));
        }
    }
    body
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawAnswer {
    subject_type: Option<String>,
    description: Option<String>,
    label_text: Option<String>,
    candidates: Option<Vec<RawCandidate>>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawCandidate {
    title: Option<String>,
    creator: Option<String>,
    date: Option<String>,
    certainty: Option<String>,
    evidence: Option<Vec<String>>,
}

fn norm(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

const CREATOR_STOP: &[&str] = &[
    "elder",
    "younger",
    "workshop",
    "circle",
    "follower",
    "attributed",
    "school",
    "master",
    "manner",
    "after",
    "copy",
];
const TITLE_STOP: &[&str] = &[
    "with", "from", "into", "over", "under", "portrait", "painting", "study", "view", "scene",
    "eine", "einer", "beim", "nach", "bild", "dans", "avec", "pour", "sous", "della", "dell",
];

fn has_phrase(hay: &str, needle: &str) -> bool {
    !needle.is_empty() && format!(" {hay} ").contains(&format!(" {needle} "))
}

fn distinctive<'a>(s: &'a str, stop: &[&str]) -> Vec<&'a str> {
    s.split(' ')
        .filter(|w| w.chars().count() >= 4 && !stop.contains(w))
        .collect()
}

fn label_match(label: &str, c: &Candidate) -> Option<LabelMatch> {
    if label.is_empty() {
        return None;
    }
    let creator = norm(&c.creator);
    let creator_hit = distinctive(&creator, CREATOR_STOP)
        .iter()
        .any(|w| has_phrase(label, w));
    let title = norm(&c.title);
    let words = distinctive(&title, TITLE_STOP);
    let found = words.iter().filter(|w| has_phrase(label, w)).count();
    let title_hit = (title.chars().count() >= 4 && has_phrase(label, &title))
        || found >= 2
        || (words.len() == 1 && found == 1);
    match (title_hit, creator_hit) {
        (true, true) => Some(LabelMatch::TitleAndCreator),
        (true, false) => Some(LabelMatch::Title),
        (false, true) => Some(LabelMatch::Creator),
        (false, false) => None,
    }
}

/// Calibrated on live runs (2026-11-14, gemma4:12b): recognition alone named a
/// well-known painting wrongly with "high" certainty in about one run in five, so
/// only text in the photo that names the title earns `Identified`. A signature that
/// names the artist confirms the artist, not the work.
fn judge(c: &[Candidate]) -> (Verdict, Basis) {
    let Some(top) = c.first() else {
        return (Verdict::Uncertain, Basis::None);
    };
    match (top.label_match, top.certainty) {
        (Some(LabelMatch::TitleAndCreator), _) => (Verdict::Identified, Basis::Label),
        (Some(LabelMatch::Title), Certainty::High | Certainty::Medium) => {
            (Verdict::Identified, Basis::Label)
        }
        (Some(LabelMatch::Title), Certainty::Low) => (Verdict::Likely, Basis::Label),
        (Some(LabelMatch::Creator), Certainty::High | Certainty::Medium) => {
            (Verdict::Likely, Basis::Label)
        }
        (Some(LabelMatch::Creator), Certainty::Low) => (Verdict::Uncertain, Basis::Label),
        (None, Certainty::High | Certainty::Medium) => (Verdict::Likely, Basis::Recognition),
        (None, Certainty::Low) => (Verdict::Uncertain, Basis::Recognition),
    }
}
fn parse_answer(
    resp: &Value,
    model: &str,
    img: &PreparedImage,
    elapsed_ms: u64,
) -> Result<IdentifyResult, LensError> {
    let content = resp
        .pointer("/message/content")
        .and_then(Value::as_str)
        .ok_or_else(|| LensError::Protocol("ollama reply has no message.content".to_string()))?;
    let hit_limit = resp.get("done_reason").and_then(Value::as_str) == Some("length");
    let raw: RawAnswer = serde_json::from_str(content).map_err(|e| {
        if hit_limit {
            return LensError::Model(format!(
                "the model ran on to the {MAX_OUTPUT_TOKENS}-token limit without finishing its answer (often a repetition loop); try again or use another model"
            ));
        }
        let head: String = content.chars().take(200).collect();
        LensError::Protocol(format!(
            "model answer is not the requested JSON ({e}): {head}"
        ))
    })?;
    let mut notes = Vec::new();
    if hit_limit {
        notes.push("model output hit the length limit; the answer may be cut short".to_string());
    }
    let label_text = clean(raw.label_text.as_deref().unwrap_or(""), MAX_LONG_CHARS);
    let label = norm(&label_text);
    let mut candidates: Vec<Candidate> = Vec::new();
    for rc in raw.candidates.unwrap_or_default() {
        let title = clean(rc.title.as_deref().unwrap_or(""), MAX_SHORT_CHARS);
        let creator = clean(rc.creator.as_deref().unwrap_or(""), MAX_SHORT_CHARS);
        if title.is_empty() && creator.is_empty() {
            notes.push("dropped a candidate without title or creator".to_string());
            continue;
        }
        let certainty = match rc
            .certainty
            .as_deref()
            .map(|s| s.trim().to_lowercase())
            .as_deref()
        {
            Some("high") => Certainty::High,
            Some("medium") => Certainty::Medium,
            Some("low") => Certainty::Low,
            other => {
                notes.push(format!(
                    "candidate '{title}': certainty {other:?} is not high/medium/low, treated as low"
                ));
                Certainty::Low
            }
        };
        let mut cand = Candidate {
            title,
            creator,
            date: clean(rc.date.as_deref().unwrap_or(""), MAX_SHORT_CHARS),
            certainty,
            evidence: rc
                .evidence
                .unwrap_or_default()
                .iter()
                .map(|e| clean(e, MAX_SHORT_CHARS))
                .filter(|e| !e.is_empty())
                .take(MAX_EVIDENCE)
                .collect(),
            label_match: None,
        };
        cand.label_match = label_match(&label, &cand);
        if let Some(m) = cand.label_match {
            cand.evidence.push(
                match m {
                    LabelMatch::Creator => {
                        "text read in the photo (label or signature) names this creator"
                    }
                    LabelMatch::Title => {
                        "text read in the photo (label or signature) names this title"
                    }
                    LabelMatch::TitleAndCreator => {
                        "text read in the photo (label or signature) names this title and creator"
                    }
                }
                .to_string(),
            );
        }
        candidates.push(cand);
        if candidates.len() == MAX_CANDIDATES {
            break;
        }
    }
    // Label agreement first, then the model's own certainty; ties keep model order.
    candidates.sort_by(|a, b| {
        b.label_match
            .cmp(&a.label_match)
            .then(b.certainty.cmp(&a.certainty))
    });
    let (verdict, basis) = judge(&candidates);
    match (basis, candidates.first().and_then(|c| c.label_match)) {
        (Basis::Recognition, _) if verdict == Verdict::Likely => notes.push(
            "recognition only: nothing in the photo confirms the title; include the wall label in the photo to confirm".to_string(),
        ),
        (Basis::Label, Some(LabelMatch::Creator)) => notes.push(
            "the text in the photo confirms the artist, not the title".to_string(),
        ),
        _ => {}
    }
    let subject_type = clean(raw.subject_type.as_deref().unwrap_or(""), 40);
    Ok(IdentifyResult {
        verdict,
        basis,
        subject_type: if subject_type.is_empty() {
            "other".to_string()
        } else {
            subject_type
        },
        description: clean(raw.description.as_deref().unwrap_or(""), MAX_LONG_CHARS),
        label_text,
        candidates,
        model: model.to_string(),
        image: ImageInfo {
            width: img.width,
            height: img.height,
            source_width: img.source_width,
            source_height: img.source_height,
            source_format: img.source_format.clone(),
            jpeg_bytes: img.jpeg.len(),
        },
        elapsed_ms,
        notes,
    })
}

/// Identify what a prepared photo shows.
pub fn identify(
    t: &dyn VisionTransport,
    img: &PreparedImage,
    opts: &IdentifyOptions,
) -> Result<IdentifyResult, LensError> {
    let c = check(opts)?;
    if img.jpeg.is_empty() {
        return Err(LensError::InvalidInput(
            "prepared image is empty".to_string(),
        ));
    }
    let caps = capabilities(t, &c.model)?;
    if !caps.iter().any(|x| x == "vision") {
        return Err(LensError::Model(format!(
            "model '{}' cannot see images (capabilities: {}); use a vision model such as {DEFAULT_MODEL}",
            c.model,
            if caps.is_empty() { "none reported".to_string() } else { caps.join(", ") }
        )));
    }
    let mut body = build_request(&c, &caps, img);
    let started = Instant::now();
    let mut resp = t.chat(&body)?;
    let retried = is_runaway(&resp);
    if retried {
        tune_for_retry(&mut body);
        resp = t.chat(&body)?;
    }
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let mut result = parse_answer(&resp, &c.model, img, elapsed_ms)?;
    if retried {
        result.notes.push(
            "the first answer ran into a repetition loop; this is the answer from one retry"
                .to_string(),
        );
    }
    Ok(result)
}

/// A reply cut off at the token cap that is not valid JSON: the model looped.
/// Text-dense photos (maps, wall labels) trigger this in gemma4:12b.
fn is_runaway(resp: &Value) -> bool {
    resp.get("done_reason").and_then(Value::as_str) == Some("length")
        && resp
            .pointer("/message/content")
            .and_then(Value::as_str)
            .is_some_and(|c| serde_json::from_str::<RawAnswer>(c).is_err())
}

/// One retry: penalise repetition and loosen sampling a little. Only on the retry:
/// tested 2026-11-14, a penalty on the first attempt stopped the loops but also stopped
/// label transcription and produced a confident wrong title for a well-known painting.
fn tune_for_retry(body: &mut Value) {
    if let Some(o) = body.get_mut("options").and_then(Value::as_object_mut) {
        o.insert("repeat_penalty".to_string(), json!(1.2));
        o.insert("temperature".to_string(), json!(0.4));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Fake {
        caps: Vec<&'static str>,
        /// Replies in order; the last one repeats.
        replies: RefCell<Vec<Result<Value, LensError>>>,
        seen: RefCell<Option<Value>>,
        calls: RefCell<usize>,
    }

    impl Fake {
        fn answering(answer: Value) -> Self {
            Self::with(
                vec!["completion", "vision", "thinking"],
                Ok(json!({
                    "message": {"role": "assistant", "content": answer.to_string()},
                    "done_reason": "stop"
                })),
            )
        }
        fn with(caps: Vec<&'static str>, reply: Result<Value, LensError>) -> Self {
            Self::seq(caps, vec![reply])
        }
        fn seq(caps: Vec<&'static str>, replies: Vec<Result<Value, LensError>>) -> Self {
            Self {
                caps,
                replies: RefCell::new(replies),
                seen: RefCell::new(None),
                calls: RefCell::new(0),
            }
        }
    }

    impl VisionTransport for Fake {
        fn base_url(&self) -> String {
            "fake".to_string()
        }
        fn show(&self, _model: &str) -> Result<Value, LensError> {
            Ok(json!({ "capabilities": self.caps }))
        }
        fn chat(&self, body: &Value) -> Result<Value, LensError> {
            *self.seen.borrow_mut() = Some(body.clone());
            *self.calls.borrow_mut() += 1;
            let mut q = self.replies.borrow_mut();
            if q.len() > 1 {
                q.remove(0)
            } else {
                q.first()
                    .cloned()
                    .unwrap_or_else(|| Err(LensError::Offline("no reply queued".to_string())))
            }
        }
        fn version(&self) -> Result<String, LensError> {
            Ok("test".to_string())
        }
    }

    fn img() -> PreparedImage {
        PreparedImage {
            jpeg: vec![0xFF, 0xD8, 0xFF, 0xD9],
            width: 4,
            height: 3,
            source_width: 4000,
            source_height: 3000,
            source_format: "jpeg".to_string(),
        }
    }

    fn cand(title: &str, creator: &str, certainty: &str) -> Value {
        json!({"title": title, "creator": creator, "date": "", "certainty": certainty, "evidence": ["seen"]})
    }

    fn answer(label: &str, cands: Vec<Value>) -> Value {
        json!({"subject_type": "painting", "description": "A painting.", "label_text": label, "candidates": cands})
    }

    #[test]
    fn request_carries_image_schema_context_and_no_thinking() -> Result<(), LensError> {
        let fake = Fake::answering(answer("", vec![]));
        let opts = IdentifyOptions {
            place: Some("Kunsthistorisches Museum Wien".to_string()),
            hint: Some("Bruegel room".to_string()),
            lat: Some(48.2039),
            lon: Some(16.3617),
            language: Some("de".to_string()),
            ..IdentifyOptions::default()
        };
        identify(&fake, &img(), &opts)?;
        let body = fake.seen.borrow().clone().unwrap_or_default();
        assert_eq!(
            body.get("model").and_then(Value::as_str),
            Some(DEFAULT_MODEL)
        );
        assert_eq!(body.get("think"), Some(&Value::Bool(false)));
        assert!(body.pointer("/format/properties/candidates").is_some());
        let sent = body
            .pointer("/messages/1/images/0")
            .and_then(Value::as_str)
            .unwrap_or("");
        assert_eq!(sent, "/9j/2Q==");
        let user = body
            .pointer("/messages/1/content")
            .and_then(Value::as_str)
            .unwrap_or("");
        for part in ["Kunsthistorisches", "Bruegel room", "48.20390", "'de'"] {
            assert!(user.contains(part), "{part} missing from: {user}");
        }
        Ok(())
    }

    #[test]
    fn no_think_flag_for_models_without_thinking() -> Result<(), LensError> {
        let fake = Fake::with(
            vec!["vision"],
            Ok(json!({"message": {"content": answer("", vec![]).to_string()}})),
        );
        identify(&fake, &img(), &IdentifyOptions::default())?;
        assert!(fake
            .seen
            .borrow()
            .clone()
            .unwrap_or_default()
            .get("think")
            .is_none());
        Ok(())
    }

    #[test]
    fn label_agreement_outranks_model_order() -> Result<(), LensError> {
        let fake = Fake::answering(answer(
            "PIETER BRUEGEL D. Ä. | Turmbau zu Babel, 1563 | Öl auf Eichenholz",
            vec![
                cand("The Tower of Babel", "Lucas van Valckenborch", "medium"),
                cand("Turmbau zu Babel", "Pieter Bruegel the Elder", "medium"),
            ],
        ));
        let r = identify(&fake, &img(), &IdentifyOptions::default())?;
        assert_eq!(
            r.candidates.first().map(|c| c.creator.as_str()),
            Some("Pieter Bruegel the Elder")
        );
        assert_eq!(
            r.candidates.first().and_then(|c| c.label_match),
            Some(LabelMatch::TitleAndCreator)
        );
        assert_eq!((r.verdict, r.basis), (Verdict::Identified, Basis::Label));
        assert!(r.candidates.first().is_some_and(|c| c
            .evidence
            .iter()
            .any(|e| e.contains("text read in the photo"))));
        Ok(())
    }

    #[test]
    fn nothing_recognised_is_uncertain_not_a_guess() -> Result<(), LensError> {
        let r = identify(
            &Fake::answering(answer("", vec![])),
            &img(),
            &IdentifyOptions::default(),
        )?;
        assert_eq!((r.verdict, r.basis), (Verdict::Uncertain, Basis::None));
        assert!(r.candidates.is_empty());
        Ok(())
    }

    #[test]
    fn recognition_alone_is_at_most_likely() -> Result<(), LensError> {
        let one = identify(
            &Fake::answering(answer("", vec![cand("The Kiss", "Gustav Klimt", "high")])),
            &img(),
            &IdentifyOptions::default(),
        )?;
        assert_eq!(
            (one.verdict, one.basis),
            (Verdict::Likely, Basis::Recognition)
        );
        assert!(one.notes.iter().any(|n| n.contains("recognition only")));
        let low = identify(
            &Fake::answering(answer("", vec![cand("A", "Someone", "low")])),
            &img(),
            &IdentifyOptions::default(),
        )?;
        assert_eq!(low.verdict, Verdict::Uncertain);
        Ok(())
    }

    #[test]
    fn a_signature_confirms_the_artist_not_the_work() -> Result<(), LensError> {
        let r = identify(
            &Fake::answering(answer(
                "GUSTAV KLIMT",
                vec![cand("Portrait of a Couple", "Gustav Klimt", "high")],
            )),
            &img(),
            &IdentifyOptions::default(),
        )?;
        assert_eq!(
            r.candidates.first().and_then(|c| c.label_match),
            Some(LabelMatch::Creator)
        );
        assert_eq!((r.verdict, r.basis), (Verdict::Likely, Basis::Label));
        assert!(r.notes.iter().any(|n| n.contains("artist, not the title")));
        Ok(())
    }
    #[test]
    fn model_output_is_cleaned_capped_and_odd_certainty_flagged() -> Result<(), LensError> {
        let mut many = vec![cand("", "", "high")];
        for i in 0..5 {
            many.push(cand(&format!("Work\u{0007} {i}"), "Artist", "certain"));
        }
        let r = identify(
            &Fake::answering(answer("", many)),
            &img(),
            &IdentifyOptions::default(),
        )?;
        assert_eq!(r.candidates.len(), MAX_CANDIDATES);
        assert!(r.candidates.iter().all(|c| !c.title.contains('\u{0007}')));
        assert!(r.candidates.iter().all(|c| c.certainty == Certainty::Low));
        assert!(r.notes.iter().any(|n| n.contains("dropped")));
        assert!(r.notes.iter().any(|n| n.contains("treated as low")));
        Ok(())
    }

    #[test]
    fn model_without_vision_is_a_model_error() {
        let fake = Fake::with(vec!["completion", "tools"], Ok(json!({})));
        let out = identify(&fake, &img(), &IdentifyOptions::default());
        assert_eq!(out.map_err(|e| e.kind()), Err("model"));
        assert!(fake.seen.borrow().is_none(), "must not call chat");
    }

    #[test]
    fn broken_replies_are_protocol_errors() {
        let not_json = Fake::with(
            vec!["vision"],
            Ok(json!({"message": {"content": "Sure! It is a painting."}})),
        );
        assert_eq!(
            identify(&not_json, &img(), &IdentifyOptions::default()).map_err(|e| e.kind()),
            Err("protocol")
        );
        let no_content = Fake::with(vec!["vision"], Ok(json!({"done": true})));
        assert_eq!(
            identify(&no_content, &img(), &IdentifyOptions::default()).map_err(|e| e.kind()),
            Err("protocol")
        );
    }

    #[test]
    fn runaway_output_is_a_clear_model_error() {
        let cut = r#"{"candidates": [{"title": "Map", "evidence": ["Vienna Vienna Vienna"#;
        let fake = Fake::with(
            vec!["vision"],
            Ok(json!({"message": {"content": cut}, "done_reason": "length"})),
        );
        let out = identify(&fake, &img(), &IdentifyOptions::default());
        let msg = match &out {
            Err(LensError::Model(m)) => m.clone(),
            _ => String::new(),
        };
        assert!(
            msg.contains("token limit") && msg.contains("try again"),
            "got {out:?}"
        );
        let sent = fake.seen.borrow().clone().unwrap_or_default();
        assert_eq!(
            sent.pointer("/options/num_predict").and_then(Value::as_u64),
            Some(u64::from(MAX_OUTPUT_TOKENS))
        );
        assert_eq!(*fake.calls.borrow(), 2, "one retry, then give up");
    }

    #[test]
    fn one_retry_recovers_from_a_repetition_loop() -> Result<(), LensError> {
        let cut = r#"{"candidates": [{"title": "Map", "evidence": ["Wien Wien Wien"#;
        let good = answer("", vec![cand("Map of Vienna", "OpenStreetMap", "high")]);
        let fake = Fake::seq(
            vec!["vision"],
            vec![
                Ok(json!({"message": {"content": cut}, "done_reason": "length"})),
                Ok(json!({"message": {"content": good.to_string()}, "done_reason": "stop"})),
            ],
        );
        let r = identify(&fake, &img(), &IdentifyOptions::default())?;
        assert_eq!(*fake.calls.borrow(), 2);
        assert_eq!(r.candidates.len(), 1);
        assert!(r.notes.iter().any(|n| n.contains("one retry")));
        let second = fake.seen.borrow().clone().unwrap_or_default();
        assert!(second.pointer("/options/repeat_penalty").is_some());
        Ok(())
    }

    #[test]
    fn a_normal_answer_is_never_retried() -> Result<(), LensError> {
        let fake = Fake::answering(answer("", vec![]));
        identify(&fake, &img(), &IdentifyOptions::default())?;
        assert_eq!(*fake.calls.borrow(), 1);
        Ok(())
    }

    #[test]
    fn offline_backend_propagates() {
        let fake = Fake::with(vec!["vision"], Err(LensError::Offline("down".to_string())));
        assert_eq!(
            identify(&fake, &img(), &IdentifyOptions::default()).map_err(|e| e.kind()),
            Err("offline")
        );
    }

    #[test]
    fn hostile_options_are_rejected_before_any_call() {
        let bad = [
            IdentifyOptions {
                lat: Some(48.0),
                ..IdentifyOptions::default()
            },
            IdentifyOptions {
                lat: Some(91.0),
                lon: Some(0.0),
                ..IdentifyOptions::default()
            },
            IdentifyOptions {
                lat: Some(f64::NAN),
                lon: Some(0.0),
                ..IdentifyOptions::default()
            },
            IdentifyOptions {
                hint: Some("x".repeat(MAX_HINT_CHARS + 1)),
                ..IdentifyOptions::default()
            },
            IdentifyOptions {
                language: Some("english please".to_string()),
                ..IdentifyOptions::default()
            },
            IdentifyOptions {
                model: "x; rm -rf".to_string(),
                ..IdentifyOptions::default()
            },
        ];
        for opts in bad {
            let fake = Fake::answering(answer("", vec![]));
            assert_eq!(
                identify(&fake, &img(), &opts).map_err(|e| e.kind()),
                Err("invalid"),
                "{opts:?}"
            );
            assert!(fake.seen.borrow().is_none());
        }
        let empty = PreparedImage::default();
        assert_eq!(
            identify(
                &Fake::answering(answer("", vec![])),
                &empty,
                &IdentifyOptions::default()
            )
            .map_err(|e| e.kind()),
            Err("invalid")
        );
    }
}
