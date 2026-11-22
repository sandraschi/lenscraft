//! Main egui workstation for LensCraft following CRAFT_MENU_BAR_STANDARD & EGUI_DESKTOP_UI_AND_PARITY.
use crate::help::{HelpTab, HelpWindows, PaletteAction};
use egui::{Color32, Context, CornerRadius, Key, Modifiers, ScrollArea, Stroke, StrokeKind, Vec2};
use lenscraft_core::{
    answer_followup, build_wikidata_pack, explain, fetch_floorplan_image_bytes, ground_result,
    identify, load_pack, match_collection, nearby_venues, prepare_file, resolve_floorplan,
    save_pack, search_museums, split_sentences, status, CollectionMatch, CollectionPack,
    Explanation, FactBasis, FollowupAnswer, GroundingInfo, HttpKnowledgeClient, IdentifyMode,
    IdentifyOptions, IdentifyResult, LengthPreset, ModelStatus, MuseumFloorplan, MuseumSearchHit,
    OllamaHttp, OverpassHttp, PreparedImage, QaTurn, SpeechMcpHttp, VenueCandidate, WorldDb,
    DEFAULT_MODEL, DEFAULT_WORLD_DB_PATH, KHM_QID,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct LensApp {
    dark_mode: bool,
    show_photo_pane: bool,
    show_candidates_pane: bool,
    show_guide_pane: bool,

    photo_path: Option<PathBuf>,
    recent_files: Vec<PathBuf>,
    prepared: Option<PreparedImage>,
    photo_texture: Option<egui::TextureHandle>,
    photo_tex_dirty: bool,
    speak_cancel: Arc<AtomicBool>,

    model: String,
    place: String,
    hint: String,
    lat_text: String,
    lon_text: String,
    language: String,
    mode: IdentifyMode,
    length: LengthPreset,

    backend_status: Option<ModelStatus>,
    venues: Vec<VenueCandidate>,
    identify_res: Option<IdentifyResult>,
    grounding: Option<GroundingInfo>,
    explanation: Option<Explanation>,

    pack: Option<CollectionPack>,
    pack_matches: Vec<CollectionMatch>,

    qa_input: String,
    qa_history: Vec<QaTurn>,
    last_answer: Option<FollowupAnswer>,

    status_msg: String,
    help: HelpWindows,

    show_museums: bool,
    museum_query: String,
    museums_list: Vec<MuseumSearchHit>,
    active_floorplan: Option<MuseumFloorplan>,
    active_level_idx: usize,
    floorplan_texture: Option<egui::TextureHandle>,
    floorplan_tex_dirty: bool,
    selected_room_name: String,
    selected_room_world: Option<String>,
}

impl LensApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        apply_theme(&cc.egui_ctx, true);
        Self {
            dark_mode: true,
            show_photo_pane: true,
            show_candidates_pane: true,
            show_guide_pane: true,
            photo_path: None,
            recent_files: Vec::new(),
            prepared: None,
            photo_texture: None,
            photo_tex_dirty: false,
            speak_cancel: Arc::new(AtomicBool::new(false)),
            model: DEFAULT_MODEL.to_string(),
            place: String::new(),
            hint: String::new(),
            lat_text: String::new(),
            lon_text: String::new(),
            language: "en".to_string(),
            mode: IdentifyMode::Auto,
            length: LengthPreset::Standard3m,
            backend_status: None,
            venues: Vec::new(),
            identify_res: None,
            grounding: None,
            explanation: None,
            pack: None,
            pack_matches: Vec::new(),
            qa_input: String::new(),
            qa_history: Vec::new(),
            last_answer: None,
            status_msg: "Ready. Open a photo (Ctrl+O) or check backend status.".to_string(),
            help: HelpWindows::default(),
            show_museums: false,
            museum_query: String::new(),
            museums_list: search_museums(None, None),
            active_floorplan: None,
            active_level_idx: 0,
            floorplan_texture: None,
            floorplan_tex_dirty: false,
            selected_room_name: String::new(),
            selected_room_world: None,
        }
    }

    fn parsed_coords(&self) -> (Option<f64>, Option<f64>) {
        let lat = self.lat_text.trim().parse::<f64>().ok();
        let lon = self.lon_text.trim().parse::<f64>().ok();
        (lat, lon)
    }

    fn open_photo_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("Images", &["jpg", "jpeg", "png", "webp"])
            .pick_file()
        {
            self.load_photo(p);
        }
    }

    fn take_webcam_photo(&mut self) {
        let out_path = std::env::temp_dir().join("lenscraft_webcam.jpg");
        let _ = std::fs::remove_file(&out_path);
        // Probe first DirectShow video device via ffmpeg, or launch Windows Camera app.
        let list_out = std::process::Command::new("ffmpeg")
            .args(["-list_devices", "true", "-f", "dshow", "-i", "dummy"])
            .output();
        let device_name = list_out.ok().and_then(|o| {
            let stderr = String::from_utf8_lossy(&o.stderr);
            stderr
                .lines()
                .find(|l| l.contains("(video)"))
                .and_then(|l| l.split('"').nth(1))
                .map(str::to_string)
        });
        if let Some(dev) = device_name {
            let cap = std::process::Command::new("ffmpeg")
                .args([
                    "-y",
                    "-f",
                    "dshow",
                    "-i",
                    &format!("video={dev}"),
                    "-frames:v",
                    "1",
                ])
                .arg(&out_path)
                .status();
            if cap.is_ok_and(|s| s.success()) && out_path.exists() {
                self.load_photo(out_path);
                return;
            }
        }
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "microsoft.windows.camera:"])
            .spawn();
        self.status_msg = "No DirectShow webcam captured via ffmpeg; opened Windows Camera (use Open Photo after shooting).".to_string();
    }

    fn load_photo(&mut self, path: PathBuf) {
        match prepare_file(&path) {
            Ok(img) => {
                self.status_msg = format!(
                    "Loaded {} ({}x{} -> {}x{} JPEG)",
                    path.display(),
                    img.source_width,
                    img.source_height,
                    img.width,
                    img.height
                );
                self.prepared = Some(img);
                self.photo_texture = None;
                self.photo_tex_dirty = true;
                self.photo_path = Some(path.clone());
                self.recent_files.retain(|x| x != &path);
                self.recent_files.insert(0, path);
                self.recent_files.truncate(8);
                self.identify_res = None;
                self.grounding = None;
                self.explanation = None;
                self.qa_history.clear();
                self.last_answer = None;
                self.pack_matches.clear();
            }
            Err(e) => self.status_msg = format!("Open failed: {e}"),
        }
    }

    fn export_json(&mut self) {
        let Some(ref res) = self.identify_res else {
            self.status_msg = "Nothing to export yet. Identify a photo first.".to_string();
            return;
        };
        if let Some(p) = rfd::FileDialog::new()
            .set_file_name("lenscraft-result.json")
            .save_file()
        {
            let bundle = serde_json::json!({
                "identify": res,
                "grounding": self.grounding,
                "explanation": self.explanation,
                "qa_history": self.qa_history,
            });
            match serde_json::to_string_pretty(&bundle)
                .map_err(|e| e.to_string())
                .and_then(|s| std::fs::write(&p, s).map_err(|e| e.to_string()))
            {
                Ok(()) => self.status_msg = format!("Exported JSON to {}", p.display()),
                Err(e) => self.status_msg = format!("Export failed: {e}"),
            }
        }
    }

    fn check_status(&mut self) {
        match OllamaHttp::from_env().and_then(|t| status(&t, &self.model)) {
            Ok(s) => {
                self.status_msg = if s.ready {
                    format!("Backend ready: {} ({})", s.model, s.ollama_url)
                } else {
                    format!(
                        "Backend not ready: {}",
                        s.problem.as_deref().unwrap_or("unknown")
                    )
                };
                self.backend_status = Some(s);
            }
            Err(e) => self.status_msg = format!("Status check failed: {e}"),
        }
    }

    fn run_venues(&mut self) {
        let (Some(lat), Some(lon)) = self.parsed_coords() else {
            self.status_msg = "Enter valid numeric lat and lon first.".to_string();
            return;
        };
        match OverpassHttp::from_env().and_then(|t| nearby_venues(&t, lat, lon, Some(300))) {
            Ok(v) => {
                if let Some(first) = v.first() {
                    if self.place.trim().is_empty() {
                        self.place = first.name.clone();
                    }
                }
                self.status_msg = format!("Found {} nearby cultural venues.", v.len());
                self.venues = v;
            }
            Err(e) => self.status_msg = format!("Venue lookup failed: {e}"),
        }
    }

    fn run_identify(&mut self, mode: IdentifyMode) {
        let Some(ref img) = self.prepared else {
            self.status_msg = "Open a photo first (Ctrl+O).".to_string();
            return;
        };
        self.mode = mode;
        let (lat, lon) = self.parsed_coords();
        let opts = IdentifyOptions {
            model: self.model.clone(),
            hint: (!self.hint.trim().is_empty()).then(|| self.hint.clone()),
            place: (!self.place.trim().is_empty()).then(|| self.place.clone()),
            lat,
            lon,
            language: Some(self.language.clone()),
            mode,
        };
        match OllamaHttp::from_env().and_then(|t| identify(&t, img, &opts)) {
            Ok(r) => {
                self.status_msg = format!(
                    "Identified: {:?} ({:?}) in {:.1} s",
                    r.verdict,
                    r.basis,
                    r.elapsed_ms as f64 / 1000.0
                );
                self.identify_res = Some(r);
            }
            Err(e) => self.status_msg = format!("Identify failed: {e}"),
        }
    }

    fn run_ground(&mut self) {
        let Some(ref res) = self.identify_res else {
            self.status_msg = "Identify a photo first (F5).".to_string();
            return;
        };
        match HttpKnowledgeClient::new().and_then(|kc| ground_result(&kc, res, &self.language)) {
            Ok(Some(g)) => {
                self.status_msg = format!(
                    "Grounded to {} ({}) with {} sourced facts.",
                    g.label,
                    g.qid,
                    g.sourced_facts.len()
                );
                self.grounding = Some(g);
            }
            Ok(None) => self.status_msg = "No matching Wikidata entry found.".to_string(),
            Err(e) => self.status_msg = format!("Grounding failed: {e}"),
        }
    }

    fn run_explain(&mut self, preset: LengthPreset) {
        let Some(ref res) = self.identify_res else {
            self.status_msg = "Identify a photo first (F5).".to_string();
            return;
        };
        self.length = preset;
        match OllamaHttp::from_env().and_then(|t| {
            explain(
                &t,
                res,
                self.grounding.as_ref(),
                preset,
                &self.language,
                &self.model,
            )
        }) {
            Ok(exp) => {
                self.status_msg = format!(
                    "Generated {} guide ({} sourced, {} model sections, {} sentences).",
                    preset.label(),
                    exp.sourced_count,
                    exp.model_count,
                    exp.sentences.len()
                );
                self.explanation = Some(exp);
            }
            Err(e) => self.status_msg = format!("Explain failed: {e}"),
        }
    }

    fn run_speak(&mut self) {
        let text = self
            .last_answer
            .as_ref()
            .map(|a| a.answer.clone())
            .or_else(|| {
                self.explanation.as_ref().map(|e| {
                    e.sections
                        .iter()
                        .map(|s| s.text.as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
            })
            .or_else(|| self.identify_res.as_ref().map(|r| r.description.clone()))
            .unwrap_or_default();
        if text.trim().is_empty() {
            self.status_msg = "Nothing to speak yet.".to_string();
            return;
        }
        let sentences = split_sentences(&text);
        if sentences.is_empty() {
            return;
        }
        self.speak_cancel.store(true, Ordering::SeqCst);
        let cancel = Arc::new(AtomicBool::new(false));
        self.speak_cancel = Arc::clone(&cancel);
        let count = sentences.len();
        self.status_msg = format!("Speaking {count} sentences...");
        std::thread::spawn(move || {
            let speech = SpeechMcpHttp::from_env().ok();
            for s in sentences {
                if cancel.load(Ordering::SeqCst) {
                    break;
                }
                let ok = speech
                    .as_ref()
                    .is_some_and(|sp| sp.speak_aloud(&s, "windows", "default").is_ok());
                if !ok {
                    let _ = std::process::Command::new("powershell")
                        .args([
                            "-NoProfile",
                            "-Command",
                            "Add-Type -AssemblyName System.Speech; (New-Object System.Speech.Synthesis.SpeechSynthesizer).Speak($env:LENS_SPEAK_TEXT)",
                        ])
                        .env("LENS_SPEAK_TEXT", &s)
                        .status();
                }
            }
        });
    }

    fn stop_speak(&mut self) {
        self.speak_cancel.store(true, Ordering::SeqCst);
        if let Ok(sp) = SpeechMcpHttp::from_env() {
            let _ = sp.stop_aloud();
        }
        self.status_msg = "Speech stopped.".to_string();
    }

    fn run_ask(&mut self) {
        let Some(ref res) = self.identify_res else {
            self.status_msg = "Identify a photo first (F5).".to_string();
            return;
        };
        let q = self.qa_input.trim().to_string();
        if q.is_empty() {
            return;
        }
        match OllamaHttp::from_env().and_then(|t| {
            answer_followup(
                &t,
                res,
                self.grounding.as_ref(),
                &self.qa_history,
                &q,
                &self.language,
                &self.model,
            )
        }) {
            Ok(ans) => {
                self.qa_history.push(QaTurn {
                    role: "user".to_string(),
                    text: q,
                });
                self.qa_history.push(QaTurn {
                    role: "assistant".to_string(),
                    text: ans.answer.clone(),
                });
                self.qa_input.clear();
                self.status_msg =
                    format!("Answered follow-up ({} sentences).", ans.sentences.len());
                self.last_answer = Some(ans);
            }
            Err(e) => self.status_msg = format!("Follow-up failed: {e}"),
        }
    }

    fn build_khm_pack(&mut self) {
        match HttpKnowledgeClient::new()
            .and_then(|kc| build_wikidata_pack(&kc, "Kunsthistorisches Museum Wien", KHM_QID, 50))
        {
            Ok(p) => {
                self.status_msg = format!("Built KHM pack with {} items.", p.items.len());
                if let Some(out) = rfd::FileDialog::new()
                    .set_file_name("khm-pack.json")
                    .save_file()
                {
                    if let Err(e) = save_pack(&p, &out) {
                        self.status_msg = format!("Save pack failed: {e}");
                    }
                }
                self.pack = Some(p);
            }
            Err(e) => self.status_msg = format!("Build KHM pack failed: {e}"),
        }
    }

    fn load_pack_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("JSON Pack", &["json"])
            .pick_file()
        {
            match load_pack(&p) {
                Ok(pack) => {
                    self.status_msg = format!(
                        "Loaded pack '{}' ({} items).",
                        pack.museum,
                        pack.items.len()
                    );
                    self.pack = Some(pack);
                }
                Err(e) => self.status_msg = format!("Load pack failed: {e}"),
            }
        }
    }

    fn match_against_pack(&mut self) {
        let (Some(ref pack), Some(ref img)) = (&self.pack, &self.prepared) else {
            self.status_msg = "Load a collection pack and open a photo first.".to_string();
            return;
        };
        let hint = if self.hint.trim().is_empty() {
            None
        } else {
            Some(self.hint.as_str())
        };
        match match_collection(pack, img, hint, 5) {
            Ok(m) => {
                self.status_msg = format!("Matched top {} items in '{}'.", m.len(), pack.museum);
                self.pack_matches = m;
            }
            Err(e) => self.status_msg = format!("Pack match failed: {e}"),
        }
    }

    fn load_museum_floorplan(&mut self, slug: &str) {
        let overpass = OverpassHttp::from_env().ok();
        match resolve_floorplan(
            slug,
            std::path::Path::new("floorplans"),
            overpass
                .as_ref()
                .map(|o| o as &dyn lenscraft_core::VenueTransport),
        ) {
            Ok(fp) => {
                self.place = fp.museum_name.clone();
                self.active_level_idx = 0;
                self.floorplan_texture = None;
                self.floorplan_tex_dirty = true;
                self.selected_room_name.clear();
                self.selected_room_world = None;
                self.status_msg =
                    format!("Loaded floorplan for {} ({}).", fp.museum_name, fp.source);
                self.active_floorplan = Some(fp);
            }
            Err(e) => self.status_msg = format!("Floorplan error: {e}"),
        }
    }

    fn handle_shortcuts(&mut self, ctx: &Context) {
        handle_zoom(ctx);
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::O)) {
            self.open_photo_dialog();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::E)) {
            self.export_json();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::K))
            || ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::P))
        {
            self.help.show_palette = !self.help.show_palette;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Comma)) {
            self.help.show_preferences = true;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Num0)) {
            ctx.set_zoom_factor(1.0);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F1))
            || ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Slash))
        {
            self.help.show_shortcuts = true;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F5)) {
            self.run_identify(IdentifyMode::Auto);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F6)) {
            self.run_identify(IdentifyMode::Label);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F7)) {
            self.run_ground();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F8)) {
            self.run_explain(LengthPreset::Standard3m);
        }
    }

    fn draw_menu_bar(&mut self, ctx: &Context, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            // Front Bookend: File, Edit, View
            ui.menu_button("File", |ui| {
                if ui.button("Open Photo... (Ctrl+O)").clicked() {
                    ui.close();
                    self.open_photo_dialog();
                }
                ui.menu_button("Open Recent", |ui| {
                    if self.recent_files.is_empty() {
                        ui.weak("No recent photos");
                    } else {
                        let recents = self.recent_files.clone();
                        for p in recents {
                            if ui.button(p.display().to_string()).clicked() {
                                ui.close();
                                self.load_photo(p);
                            }
                        }
                        ui.separator();
                        if ui.button("Clear Recent").clicked() {
                            self.recent_files.clear();
                            ui.close();
                        }
                    }
                });
                if ui.button("Export Result JSON... (Ctrl+E)").clicked() {
                    ui.close();
                    self.export_json();
                }
                ui.separator();
                if ui.button("Close Photo").clicked() {
                    self.photo_path = None;
                    self.prepared = None;
                    self.identify_res = None;
                    self.grounding = None;
                    self.explanation = None;
                    ui.close();
                }
                if ui.button("Quit (Ctrl+Q)").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });

            ui.menu_button("Edit", |ui| {
                if ui.button("Copy Identification Summary").clicked() {
                    if let Some(ref r) = self.identify_res {
                        let s = serde_json::to_string_pretty(r).unwrap_or_default();
                        ctx.copy_text(s);
                    }
                    ui.close();
                }
                if ui.button("Clear Context & Q&A").clicked() {
                    self.place.clear();
                    self.hint.clear();
                    self.qa_history.clear();
                    self.last_answer = None;
                    ui.close();
                }
                ui.separator();
                if ui.button("Preferences... (Ctrl+,)").clicked() {
                    self.help.show_preferences = true;
                    ui.close();
                }
            });

            ui.menu_button("View", |ui| {
                if ui.button("Zoom In (Ctrl+=)").clicked() {
                    let next = (ctx.zoom_factor() * 1.1).clamp(0.6, 2.5);
                    ctx.set_zoom_factor(next);
                    ui.close();
                }
                if ui.button("Zoom Out (Ctrl+-)").clicked() {
                    let next = (ctx.zoom_factor() / 1.1).clamp(0.6, 2.5);
                    ctx.set_zoom_factor(next);
                    ui.close();
                }
                if ui.button("Reset Zoom 100% (Ctrl+0)").clicked() {
                    ctx.set_zoom_factor(1.0);
                    ui.close();
                }
                ui.separator();
                ui.menu_button("Interface Theme", |ui| {
                    if ui
                        .selectable_label(self.dark_mode, "Dark (High Contrast)")
                        .clicked()
                    {
                        self.dark_mode = true;
                        apply_theme(ctx, true);
                        ui.close();
                    }
                    if ui
                        .selectable_label(!self.dark_mode, "Light (High Contrast)")
                        .clicked()
                    {
                        self.dark_mode = false;
                        apply_theme(ctx, false);
                        ui.close();
                    }
                });
            });

            // Domain Menus: Lens, Packs
            ui.menu_button("Lens", |ui| {
                if ui.button("Check Backend Status").clicked() {
                    ui.close();
                    self.check_status();
                }
                if ui.button("Nearby Venues from GPS").clicked() {
                    ui.close();
                    self.run_venues();
                }
                ui.separator();
                if ui.button("Identify Photo - Auto (F5)").clicked() {
                    ui.close();
                    self.run_identify(IdentifyMode::Auto);
                }
                if ui
                    .button("Identify Wall Label - Label-First (F6)")
                    .clicked()
                {
                    ui.close();
                    self.run_identify(IdentifyMode::Label);
                }
                if ui.button("Ground in Wikidata & Wikipedia (F7)").clicked() {
                    ui.close();
                    self.run_ground();
                }
                ui.separator();
                if ui.button("Guide: 30s Brief").clicked() {
                    ui.close();
                    self.run_explain(LengthPreset::Brief30s);
                }
                if ui.button("Guide: 3m Standard (F8)").clicked() {
                    ui.close();
                    self.run_explain(LengthPreset::Standard3m);
                }
                if ui.button("Guide: 10m Deep").clicked() {
                    ui.close();
                    self.run_explain(LengthPreset::Deep10m);
                }
                ui.separator();
                if ui.button("Speak Guide via speech-mcp").clicked() {
                    ui.close();
                    self.run_speak();
                }
            });

            ui.menu_button("Packs", |ui| {
                if ui.button("Build KHM Collection Pack (Q95569)...").clicked() {
                    ui.close();
                    self.build_khm_pack();
                }
                if ui.button("Load Collection Pack...").clicked() {
                    ui.close();
                    self.load_pack_dialog();
                }
                if ui.button("Match Photo Against Loaded Pack").clicked() {
                    ui.close();
                    self.match_against_pack();
                }
            });

            ui.menu_button("Museums", |ui| {
                if ui
                    .button("World Museum Directory & Floorplans...")
                    .clicked()
                {
                    self.show_museums = true;
                    ui.close();
                }
                if ui.button("Open KHM Floorplan").clicked() {
                    self.show_museums = true;
                    self.load_museum_floorplan("khm");
                    ui.close();
                }
                if ui.button("Open Louvre Floorplan").clicked() {
                    self.show_museums = true;
                    self.load_museum_floorplan("louvre");
                    ui.close();
                }
            });

            // Tail Bookend: Window, Help
            ui.menu_button("Window", |ui| {
                if ui.button("Reset Panel Layout").clicked() {
                    self.show_photo_pane = true;
                    self.show_candidates_pane = true;
                    self.show_guide_pane = true;
                    ui.close();
                }
                ui.separator();
                ui.checkbox(&mut self.show_photo_pane, "Photo & Context");
                ui.checkbox(&mut self.show_candidates_pane, "Candidates & Grounding");
                ui.checkbox(&mut self.show_guide_pane, "Guide & Follow-up Q&A");
            });

            ui.menu_button("Help", |ui| {
                if ui.button("Command Palette... (Ctrl+K)").clicked() {
                    self.help.show_palette = true;
                    ui.close();
                }
                if ui.button("Keyboard Shortcuts... (F1)").clicked() {
                    self.help.show_shortcuts = true;
                    ui.close();
                }
                ui.separator();
                if ui.button("Getting Started...").clicked() {
                    self.help.open_help(HelpTab::GetStarted);
                    ui.close();
                }
                if ui.button("Usage (GUI, CLI & MCP)...").clicked() {
                    self.help.open_help(HelpTab::Usage);
                    ui.close();
                }
                if ui.button("Tips & Tricks...").clicked() {
                    self.help.open_help(HelpTab::Tips);
                    ui.close();
                }
                if ui.button("Common Pitfalls & Safety...").clicked() {
                    self.help.open_help(HelpTab::Pitfalls);
                    ui.close();
                }
                if ui.button("Planned Extensions...").clicked() {
                    self.help.open_help(HelpTab::Planned);
                    ui.close();
                }
                if ui.button("Project Files & Docs...").clicked() {
                    self.help.open_help(HelpTab::Functions);
                    ui.close();
                }
                ui.separator();
                if ui.button("Licenses & Notices...").clicked() {
                    self.help.show_licenses = true;
                    ui.close();
                }
                if ui.button("About LensCraft").clicked() {
                    self.help.show_about = true;
                    ui.close();
                }
            });

            // Right-aligned status + zoom pill + vector theme toggle
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if theme_toggle_button(ui, &mut self.dark_mode) {
                    apply_theme(ctx, self.dark_mode);
                }
                let zoom_pct = (ctx.zoom_factor() * 100.0).round() as i32;
                if ui.button(format!("{zoom_pct}%")).clicked() {
                    ctx.set_zoom_factor(1.0);
                }
                ui.weak("Safe Rust (0 unsafe)");
            });
        });
    }

    fn apply_palette_action(&mut self, ctx: &Context, act: PaletteAction) {
        match act {
            PaletteAction::OpenPhoto => self.open_photo_dialog(),
            PaletteAction::ExportJson => self.export_json(),
            PaletteAction::IdentifyAuto => self.run_identify(IdentifyMode::Auto),
            PaletteAction::IdentifyLabel => self.run_identify(IdentifyMode::Label),
            PaletteAction::FindVenues => self.run_venues(),
            PaletteAction::GroundWork => self.run_ground(),
            PaletteAction::Explain3m => self.run_explain(LengthPreset::Standard3m),
            PaletteAction::SpeakGuide => self.run_speak(),
            PaletteAction::ToggleTheme => {
                self.dark_mode = !self.dark_mode;
                apply_theme(ctx, self.dark_mode);
            }
            PaletteAction::ResetZoom => ctx.set_zoom_factor(1.0),
            PaletteAction::OpenHelp => self.help.open_help(HelpTab::GetStarted),
            PaletteAction::OpenShortcuts => self.help.show_shortcuts = true,
        }
    }
}

impl eframe::App for LensApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_shortcuts(&ctx);

        egui::Panel::top("lens_menubar").show(ui, |ui| {
            self.draw_menu_bar(&ctx, ui);
        });

        egui::Panel::bottom("lens_statusbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status_msg);
            });
        });

        if self.show_photo_pane {
            egui::Panel::left("photo_context")
                .default_size(310.0)
                .resizable(true)
                .show(ui, |ui| {
                    ui.heading("Photo & Context");
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Take Photo").clicked() {
                            self.take_webcam_photo();
                        }
                        if ui.button("Open Photo... (Ctrl+O)").clicked() {
                            self.open_photo_dialog();
                        }
                        if ui.button("Status").clicked() {
                            self.check_status();
                        }
                    });
                    if let Some(ref p) = self.photo_path {
                        ui.small(p.display().to_string());
                    }
                    ui.separator();
                    ui.label("Model:");
                    ui.text_edit_singleline(&mut self.model);
                    ui.horizontal(|ui| {
                        ui.label("Venue / Place:");
                        if ui.small_button("KHM").clicked() {
                            self.place = "Kunsthistorisches Museum Wien".to_string();
                            self.lat_text = "48.2038".to_string();
                            self.lon_text = "16.3618".to_string();
                        }
                        if ui.small_button("Museums").clicked() {
                            self.show_museums = true;
                        }
                    });
                    ui.text_edit_singleline(&mut self.place);
                    ui.label("Visitor Hint:");
                    ui.text_edit_singleline(&mut self.hint);
                    ui.horizontal(|ui| {
                        ui.label("Lat:");
                        ui.add(egui::TextEdit::singleline(&mut self.lat_text).desired_width(80.0));
                        ui.label("Lon:");
                        ui.add(egui::TextEdit::singleline(&mut self.lon_text).desired_width(80.0));
                        if ui.button("Venues").clicked() {
                            self.run_venues();
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Lang:");
                        ui.add(egui::TextEdit::singleline(&mut self.language).desired_width(50.0));
                        ui.selectable_value(&mut self.mode, IdentifyMode::Auto, "Auto");
                        ui.selectable_value(&mut self.mode, IdentifyMode::Label, "Label");
                    });
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Identify (F5)").clicked() {
                            self.run_identify(self.mode);
                        }
                        if ui.button("Wall Label (F6)").clicked() {
                            self.run_identify(IdentifyMode::Label);
                        }
                        if ui.button("Ground (F7)").clicked() {
                            self.run_ground();
                        }
                    });
                    if !self.venues.is_empty() {
                        ui.separator();
                        ui.label("Nearby Venues:");
                        ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
                            for v in self.venues.clone() {
                                if ui
                                    .button(format!("{} ({:.0} m)", v.name, v.distance_m))
                                    .clicked()
                                {
                                    self.place = v.name;
                                }
                            }
                        });
                    }
                });
        }

        if self.show_guide_pane {
            egui::Panel::right("guide_qa")
                .default_size(390.0)
                .resizable(true)
                .show(ui, |ui| {
                    ui.heading("Guide & Follow-up Q&A");
                    ui.horizontal(|ui| {
                        if ui.button("30s").clicked() {
                            self.run_explain(LengthPreset::Brief30s);
                        }
                        if ui.button("3m (F8)").clicked() {
                            self.run_explain(LengthPreset::Standard3m);
                        }
                        if ui.button("10m").clicked() {
                            self.run_explain(LengthPreset::Deep10m);
                        }
                        if ui.button("Speak").clicked() {
                            self.run_speak();
                        }
                        if ui.button("Stop").clicked() {
                            self.stop_speak();
                        }
                    });
                    ui.separator();
                    ScrollArea::vertical()
                        .id_salt("guide_scroll")
                        .max_height(300.0)
                        .show(ui, |ui| {
                            if let Some(ref exp) = self.explanation {
                                for sec in &exp.sections {
                                    let badge = match sec.basis {
                                        FactBasis::Sourced => "[sourced]",
                                        FactBasis::Visual => "[visual]",
                                        FactBasis::Model => "[model]",
                                    };
                                    ui.strong(format!("{badge} {}", sec.heading));
                                    if let Some(ref r) = sec.source_ref {
                                        ui.small(r);
                                    }
                                    ui.label(&sec.text);
                                    ui.add_space(6.0);
                                }
                            } else {
                                ui.weak("Run 30s / 3m / 10m guide after identifying a work.");
                            }
                        });
                    ui.separator();
                    ui.label("Ask Follow-up:");
                    let mut submit_ask = false;
                    ui.horizontal(|ui| {
                        let resp = ui.text_edit_singleline(&mut self.qa_input);
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            submit_ask = true;
                        }
                        if ui.button("Ask").clicked() {
                            submit_ask = true;
                        }
                    });
                    if submit_ask {
                        self.run_ask();
                    }
                    ScrollArea::vertical()
                        .id_salt("qa_chat_scroll")
                        .auto_shrink([false, false])
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for turn in &self.qa_history {
                                let prefix = if turn.role == "user" { "Q: " } else { "A: " };
                                ui.label(format!("{prefix}{}", turn.text));
                                ui.add_space(4.0);
                            }
                        });
                });
        }

        if self.show_candidates_pane {
            if self.photo_tex_dirty {
                self.photo_tex_dirty = false;
                if let Some(ref prep) = self.prepared {
                    if let Ok(dyn_img) = image::load_from_memory(&prep.jpeg) {
                        let rgba = dyn_img.to_rgba8();
                        let size = [rgba.width() as usize, rgba.height() as usize];
                        let color_img =
                            egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                        self.photo_texture = Some(ctx.load_texture(
                            "photo_preview",
                            color_img,
                            egui::TextureOptions::LINEAR,
                        ));
                    }
                }
            }
            egui::CentralPanel::default().show(ui, |ui| {
                ui.heading("Candidates & Grounding");
                ScrollArea::vertical().show(ui, |ui| {
                    if let Some(ref tex) = self.photo_texture {
                        let avail_w = ui.available_width().max(120.0);
                        let sz = tex.size_vec2();
                        let scale = (avail_w / sz.x.max(1.0))
                            .min(260.0 / sz.y.max(1.0))
                            .min(1.0);
                        ui.image((tex.id(), sz * scale));
                        ui.separator();
                    }
                    if let Some(ref res) = self.identify_res {
                        ui.strong(format!(
                            "Verdict: {:?} (Basis: {:?}) - {}",
                            res.verdict, res.basis, res.subject_type
                        ));
                        ui.label(&res.description);
                        if !res.label_text.is_empty() {
                            ui.separator();
                            ui.strong("Transcribed Wall Label:");
                            ui.monospace(&res.label_text);
                        }
                        ui.separator();
                        for (i, c) in res.candidates.iter().enumerate() {
                            ui.strong(format!(
                                "{}. {} | {} | {} [{:?}]",
                                i + 1,
                                c.title,
                                c.creator,
                                c.date,
                                c.certainty
                            ));
                            for e in &c.evidence {
                                ui.label(format!("  - {e}"));
                            }
                        }
                    } else {
                        ui.weak("No identification yet. Open a photo (Ctrl+O) and press F5.");
                    }

                    if let Some(ref g) = self.grounding {
                        ui.separator();
                        ui.strong(format!("Wikidata Grounding: {} ({})", g.label, g.qid));
                        if !g.description.is_empty() {
                            ui.label(&g.description);
                        }
                        if let Some(ref ext) = g.wikipedia_extract {
                            ui.label(ext);
                        }
                    }

                    if !self.pack_matches.is_empty() {
                        ui.separator();
                        ui.strong("Collection Pack Matches:");
                        for m in &self.pack_matches {
                            ui.label(format!(
                                "- {} ({}) [{:.2}] {}",
                                m.title, m.creator, m.score, m.qid
                            ));
                        }
                    }
                });
            });
        }

        if self.help.show_preferences {
            let mut open = self.help.show_preferences;
            egui::Window::new("Preferences")
                .open(&mut open)
                .collapsible(false)
                .show(&ctx, |ui| {
                    ui.checkbox(&mut self.dark_mode, "Dark Mode");
                    if ui.button("Apply Theme").clicked() {
                        apply_theme(&ctx, self.dark_mode);
                    }
                    ui.label("Default Ollama Model:");
                    ui.text_edit_singleline(&mut self.model);
                    ui.label("Default Language:");
                    ui.text_edit_singleline(&mut self.language);
                });
            self.help.show_preferences = open;
        }

        if let Some(act) = self.help.draw(&ctx) {
            self.apply_palette_action(&ctx, act);
        }

        if self.show_museums {
            let mut open = self.show_museums;
            let mut clicked_slug: Option<String> = None;
            egui::Window::new("World Museums & Floorplans")
                .open(&mut open)
                .default_size([820.0, 520.0])
                .resizable(true)
                .show(&ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Search:");
                        if ui.text_edit_singleline(&mut self.museum_query).changed() {
                            self.museums_list = search_museums(Some(&self.museum_query), None);
                        }
                    });
                    ui.separator();
                    ui.columns(2, |cols| {
                        ScrollArea::vertical().show(&mut cols[0], |ui| {
                            for m in &self.museums_list {
                                if ui
                                    .button(format!("{} ({}, {})", m.name, m.city, m.country))
                                    .clicked()
                                {
                                    clicked_slug = Some(m.slug.clone());
                                }
                            }
                        });
                        if let Some(ref fp) = self.active_floorplan {
                            cols[1].heading(&fp.museum_name);
                            cols[1].horizontal(|ui| {
                                for (idx, lv) in fp.levels.iter().enumerate() {
                                    if ui
                                        .selectable_label(self.active_level_idx == idx, &lv.label)
                                        .clicked()
                                    {
                                        self.active_level_idx = idx;
                                        self.floorplan_texture = None;
                                        self.floorplan_tex_dirty = true;
                                    }
                                }
                            });
                            if self.floorplan_tex_dirty {
                                self.floorplan_tex_dirty = false;
                                if let Some(lv) = fp.levels.get(self.active_level_idx) {
                                    if let Some(ref url) = lv.image_url {
                                        let cache = std::path::PathBuf::from(format!(
                                            "floorplans/{}_L{}.img",
                                            fp.museum_slug, self.active_level_idx
                                        ));
                                        if let Ok(bytes) = fetch_floorplan_image_bytes(url, &cache) {
                                            if let Ok(dyn_img) = image::load_from_memory(&bytes) {
                                                let rgba = dyn_img.to_rgba8();
                                                let sz = [rgba.width() as usize, rgba.height() as usize];
                                                let ci = egui::ColorImage::from_rgba_unmultiplied(sz, rgba.as_raw());
                                                self.floorplan_texture = Some(ctx.load_texture(
                                                    "floorplan_bg",
                                                    ci,
                                                    egui::TextureOptions::LINEAR,
                                                ));
                                            }
                                        }
                                    }
                                }
                            }
                            if let Some(lv) = fp.levels.get(self.active_level_idx) {
                                let (rect, resp) = cols[1].allocate_exact_size(
                                    Vec2::new(360.0, 260.0),
                                    egui::Sense::click(),
                                );
                                let p = cols[1].painter();
                                p.rect_filled(rect, CornerRadius::same(6), Color32::from_rgb(18, 24, 36));
                                if let Some(ref tex) = self.floorplan_texture {
                                    p.image(
                                        tex.id(),
                                        rect,
                                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                                        Color32::WHITE,
                                    );
                                }
                                for rm in &lv.rooms {
                                    let pts: Vec<egui::Pos2> = rm
                                        .polygon
                                        .iter()
                                        .map(|&(nx, ny)| {
                                            egui::pos2(
                                                rect.left() + (nx as f32) * rect.width(),
                                                rect.top() + (ny as f32) * rect.height(),
                                            )
                                        })
                                        .collect();
                                    let fill = if rm.world_qid.is_some() {
                                        Color32::from_rgba_unmultiplied(212, 168, 83, 70)
                                    } else {
                                        Color32::from_rgba_unmultiplied(100, 116, 139, 55)
                                    };
                                    p.add(egui::Shape::convex_polygon(
                                        pts,
                                        fill,
                                        Stroke::new(1.2, Color32::from_rgb(200, 210, 225)),
                                    ));
                                    let c = egui::pos2(
                                        rect.left() + (rm.center.0 as f32) * rect.width(),
                                        rect.top() + (rm.center.1 as f32) * rect.height(),
                                    );
                                    p.text(
                                        c,
                                        egui::Align2::CENTER_CENTER,
                                        if rm.room_number.is_empty() {
                                            rm.name.as_str()
                                        } else {
                                            rm.room_number.as_str()
                                        },
                                        egui::FontId::proportional(11.0),
                                        Color32::WHITE,
                                    );
                                }
                                if resp.clicked() {
                                    if let Some(pos) = resp.interact_pointer_pos() {
                                        let nx = ((pos.x - rect.left()) / rect.width()) as f64;
                                        let ny = ((pos.y - rect.top()) / rect.height()) as f64;
                                        if let Some(rm) = lenscraft_core::room_at_point(lv, nx, ny) {
                                            self.selected_room_name = rm.name.clone();
                                            self.selected_room_world = rm.world_qid.clone();
                                        }
                                    }
                                }
                                if !self.selected_room_name.is_empty() {
                                    cols[1].separator();
                                    cols[1].strong(format!("Room: {}", self.selected_room_name));
                                    if let Some(ref wq) = self.selected_room_world {
                                        if let Ok(db) = WorldDb::load(std::path::Path::new(DEFAULT_WORLD_DB_PATH)) {
                                            if let Some(w) = db.get_by_qid(wq) {
                                                cols[1].label(format!("3D Walk-In ready: {} ({})", w.title, w.splat_path));
                                            } else {
                                                cols[1].weak(format!("3D Walk-In QID {wq} not yet prepped (run scripts/prep-iconic-worlds.ps1)."));
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            cols[1].weak("Select a museum on the left to view its floorplan.");
                        }
                    });
                });
            self.show_museums = open;
            if let Some(slug) = clicked_slug {
                self.load_museum_floorplan(&slug);
            }
        }
    }
}

fn handle_zoom(ctx: &Context) {
    let scroll_y = ctx.input(|i| {
        if i.modifiers.ctrl || i.modifiers.command {
            i.smooth_scroll_delta.y
        } else {
            0.0
        }
    });
    if scroll_y.abs() > 0.0 {
        let step = if scroll_y > 0.0 {
            1.1_f32
        } else {
            1.0 / 1.1_f32
        };
        let next = (ctx.zoom_factor() * step).clamp(0.6, 2.5);
        ctx.set_zoom_factor(next);
    }
}

fn apply_theme(ctx: &Context, dark: bool) {
    let mut v = if dark {
        let mut d = egui::Visuals::dark();
        d.panel_fill = Color32::from_rgb(10, 13, 18);
        d.window_fill = Color32::from_rgb(16, 22, 34);
        d.override_text_color = Some(Color32::from_rgb(245, 247, 250));
        d
    } else {
        let mut l = egui::Visuals::light();
        l.panel_fill = Color32::from_rgb(248, 249, 252);
        l.window_fill = Color32::from_rgb(255, 255, 255);
        // High-contrast dark ink on light mode (never light grey on white)
        l.override_text_color = Some(Color32::from_rgb(16, 20, 28));
        l
    };
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(56, 72, 102));
    ctx.set_visuals(v);
}

fn theme_toggle_button(ui: &mut egui::Ui, dark_mode: &mut bool) -> bool {
    let size = Vec2::new(26.0, 22.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&resp);
        ui.painter().rect(
            rect,
            CornerRadius::same(5),
            visuals.bg_fill,
            visuals.bg_stroke,
            StrokeKind::Inside,
        );
        let c = rect.center();
        if *dark_mode {
            // Vector Sun
            ui.painter()
                .circle_filled(c, 4.0, Color32::from_rgb(250, 204, 21));
            for k in 0..8 {
                let a = (k as f32) * std::f32::consts::TAU / 8.0;
                let p1 = c + Vec2::new(a.cos(), a.sin()) * 6.0;
                let p2 = c + Vec2::new(a.cos(), a.sin()) * 8.5;
                ui.painter()
                    .line_segment([p1, p2], Stroke::new(1.3, Color32::from_rgb(250, 204, 21)));
            }
        } else {
            // Vector Crescent Moon
            ui.painter()
                .circle_filled(c, 5.5, Color32::from_rgb(30, 41, 59));
            ui.painter()
                .circle_filled(c + Vec2::new(2.6, -1.8), 4.5, visuals.bg_fill);
        }
    }
    if resp.clicked() {
        *dark_mode = !*dark_mode;
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn glyph_allowlist() {
        for src in [include_str!("app.rs"), include_str!("help.rs")] {
            for ch in src.chars() {
                assert!(
                    ch.is_ascii()
                        || matches!(ch, '\u{00B0}' | '\u{00D7}' | '\u{00B7}' | '\u{2026}'),
                    "disallowed non-ASCII glyph '{ch}' (U+{:04X}) risks tofu box in egui",
                    ch as u32
                );
            }
        }
    }
}
