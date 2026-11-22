//! Standard Help Center (7 mandatory tabs), Command Palette, Shortcuts, Licenses, and About.
use egui::{Context, ScrollArea};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpTab {
    GetStarted,
    Functions,
    Usage,
    Tips,
    Pitfalls,
    Planned,
    OtherCrafts,
}

impl HelpTab {
    pub const ALL: [Self; 7] = [
        Self::GetStarted,
        Self::Functions,
        Self::Usage,
        Self::Tips,
        Self::Pitfalls,
        Self::Planned,
        Self::OtherCrafts,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::GetStarted => "Get Started",
            Self::Functions => "Functions",
            Self::Usage => "Usage (GUI, CLI & MCP)",
            Self::Tips => "Tips & Tricks",
            Self::Pitfalls => "Common Pitfalls & Safety",
            Self::Planned => "Planned Extensions",
            Self::OtherCrafts => "Other Crafts",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteAction {
    OpenPhoto,
    ExportJson,
    IdentifyAuto,
    IdentifyLabel,
    FindVenues,
    GroundWork,
    Explain3m,
    SpeakGuide,
    ToggleTheme,
    ResetZoom,
    OpenHelp,
    OpenShortcuts,
}

pub struct PaletteEntry {
    pub label: &'static str,
    pub shortcut: &'static str,
    pub action: PaletteAction,
}

pub const PALETTE_ENTRIES: &[PaletteEntry] = &[
    PaletteEntry {
        label: "File: Open Photo...",
        shortcut: "Ctrl+O",
        action: PaletteAction::OpenPhoto,
    },
    PaletteEntry {
        label: "File: Export Result JSON...",
        shortcut: "Ctrl+E",
        action: PaletteAction::ExportJson,
    },
    PaletteEntry {
        label: "Lens: Identify Photo (Auto)",
        shortcut: "F5",
        action: PaletteAction::IdentifyAuto,
    },
    PaletteEntry {
        label: "Lens: Identify Wall Label (Label-First)",
        shortcut: "F6",
        action: PaletteAction::IdentifyLabel,
    },
    PaletteEntry {
        label: "Lens: Find Nearby Venues (GPS)",
        shortcut: "",
        action: PaletteAction::FindVenues,
    },
    PaletteEntry {
        label: "Lens: Ground in Wikidata & Wikipedia",
        shortcut: "F7",
        action: PaletteAction::GroundWork,
    },
    PaletteEntry {
        label: "Lens: Generate 3m Museum Guide",
        shortcut: "F8",
        action: PaletteAction::Explain3m,
    },
    PaletteEntry {
        label: "Lens: Speak Guide (speech-mcp)",
        shortcut: "",
        action: PaletteAction::SpeakGuide,
    },
    PaletteEntry {
        label: "View: Toggle Dark / Light Theme",
        shortcut: "",
        action: PaletteAction::ToggleTheme,
    },
    PaletteEntry {
        label: "View: Reset Zoom (100%)",
        shortcut: "Ctrl+0",
        action: PaletteAction::ResetZoom,
    },
    PaletteEntry {
        label: "Help: Help Center & Docs",
        shortcut: "",
        action: PaletteAction::OpenHelp,
    },
    PaletteEntry {
        label: "Help: Keyboard Shortcuts",
        shortcut: "F1",
        action: PaletteAction::OpenShortcuts,
    },
];

pub struct HelpWindows {
    pub show_help: bool,
    pub help_tab: HelpTab,
    pub show_shortcuts: bool,
    pub show_licenses: bool,
    pub show_about: bool,
    pub show_palette: bool,
    pub palette_query: String,
    pub show_preferences: bool,
}

impl Default for HelpWindows {
    fn default() -> Self {
        Self {
            show_help: false,
            help_tab: HelpTab::GetStarted,
            show_shortcuts: false,
            show_licenses: false,
            show_about: false,
            show_palette: false,
            palette_query: String::new(),
            show_preferences: false,
        }
    }
}

impl HelpWindows {
    pub fn open_help(&mut self, tab: HelpTab) {
        self.help_tab = tab;
        self.show_help = true;
    }

    pub fn draw(&mut self, ctx: &Context) -> Option<PaletteAction> {
        let mut triggered = None;

        if self.show_palette {
            let mut open = self.show_palette;
            egui::Window::new("Command Palette")
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .default_width(480.0)
                .show(ctx, |ui| {
                    ui.text_edit_singleline(&mut self.palette_query);
                    ui.separator();
                    let q = self.palette_query.trim().to_lowercase();
                    ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                        for entry in PALETTE_ENTRIES {
                            if !q.is_empty() && !entry.label.to_lowercase().contains(&q) {
                                continue;
                            }
                            ui.horizontal(|ui| {
                                if ui.button(entry.label).clicked() {
                                    triggered = Some(entry.action);
                                }
                                if !entry.shortcut.is_empty() {
                                    ui.weak(entry.shortcut);
                                }
                            });
                        }
                    });
                });
            self.show_palette = open;
            if triggered.is_some() {
                self.show_palette = false;
                self.palette_query.clear();
            }
        }

        if self.show_help {
            let mut open = self.show_help;
            egui::Window::new("LensCraft Help & Documentation")
                .open(&mut open)
                .default_size([740.0, 520.0])
                .resizable(true)
                .show(ctx, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for tab in HelpTab::ALL {
                            if ui
                                .selectable_label(self.help_tab == tab, tab.title())
                                .clicked()
                            {
                                self.help_tab = tab;
                            }
                        }
                    });
                    ui.separator();
                    ScrollArea::vertical().show(ui, |ui| match self.help_tab {
                        HelpTab::GetStarted => {
                            ui.heading("Get Started (5-Step Tour)");
                            ui.label("1. Check Ollama status (default gemma4:12b) via Lens > Check Backend Status.");
                            ui.label("2. Open a museum or landmark photo (File > Open Photo, Ctrl+O).");
                            ui.label("3. Optionally enter a venue or GPS coordinates and run Lens > Nearby Venues.");
                            ui.label("4. Run Lens > Identify Photo (F5) or Identify Wall Label (F6) for label-first OCR.");
                            ui.label("5. Ground the top candidate in Wikidata/Wikipedia (F7), generate a 30s/3m/10m guide (F8), and ask follow-up questions or export JSON (Ctrl+E).");
                        }
                        HelpTab::Functions => {
                            ui.heading("Core Engine Functions");
                            ui.label("- image_prep: Reads JPEG/PNG/WebP, applies EXIF orientation, downscales to <= 1536 px.");
                            ui.label("- identify: Structured JSON vision call with repetition-loop retry and deterministic label verification.");
                            ui.label("- venue: Overpass GPS lookup for nearby museums, galleries, monuments, and historic buildings.");
                            ui.label("- grounding: Wikidata entity + claims (creator, inception, collection) and Wikipedia REST summary.");
                            ui.label("- explain: 30s, 3m, or 10m guide with verified [sourced] vs [visual] vs [model] fact tags.");
                            ui.label("- speech & qa: Sentence-streamed TTS and FunASR STT via speech-mcp (:10909) + grounded Q&A.");
                            ui.label("- collection: Per-museum Wikidata SPARQL packs (KHM Q95569) with 24-dim color signature matching.");
                        }
                        HelpTab::Usage => {
                            ui.heading("Usage: GUI, CLI & MCP");
                            ui.label("Desktop GUI: lenscraft-app (in-process calls to lenscraft-core).");
                            ui.label("CLI:");
                            ui.monospace("lenscraft-mcp status --json");
                            ui.monospace("lenscraft-mcp identify photo.jpg --mode label --place \"KHM Wien\" --json");
                            ui.monospace("lenscraft-mcp venues --lat 48.2039 --lon 16.3617");
                            ui.monospace("lenscraft-mcp ground \"The Tower of Babel\" --creator Bruegel");
                            ui.monospace("lenscraft-mcp explain photo.jpg --length 3m");
                            ui.monospace("lenscraft-mcp serve --port 11265");
                            ui.label("MCP stdio: lenscraft-mcp mcp (tools: lens_identify, lens_status, lens_venues, lens_ground, lens_explain, lens_speak, lens_pack).");
                        }
                        HelpTab::Tips => {
                            ui.heading("Tips & Tricks");
                            ui.label("- Always include the wall label in the frame when possible: label agreement upgrades verdict to 'identified'.");
                            ui.label("- Use --mode label (F6) when photographing dense plaques or inscriptions.");
                            ui.label("- Use Ctrl+MouseWheel or Ctrl+= / Ctrl+- to zoom the desktop UI; click 100% or Ctrl+0 to reset.");
                            ui.label("- For mobile museum use, run 'lenscraft-mcp serve' and 'scripts/tailscale-serve.ps1' for HTTPS camera/mic/GPS access on iPhone.");
                        }
                        HelpTab::Pitfalls => {
                            ui.heading("Common Pitfalls & Safety");
                            ui.label("- Recognition alone gave a wrong title in ~1 in 5 live tests on famous paintings, so LensCraft caps recognition-only verdicts at 'likely'.");
                            ui.label("- HEIC/AVIF files are rejected with a clear error: convert or shoot JPEG.");
                            ui.label("- Another GPU workload can starve Ollama; check 'Lens > Check Backend Status' if timeouts occur.");
                            ui.label("- All prose outputs avoid numeric confidence fabrication and mark unverified claims as [model] rather than [sourced].");
                        }
                        HelpTab::Planned => {
                            ui.heading("Planned Extensions");
                            ui.label("- Live end-to-end iPhone Safari verification over Tailscale serve.");
                            ui.label("- Full egui SightLens tab polish in travelcraft-app.");
                            ui.label("- Optional dense CLIP/SigLIP image embeddings for L8 museum collection packs.");
                        }
                        HelpTab::OtherCrafts => {
                            ui.heading("Other Crafts Ecosystem");
                            ui.label("- travelcraft: Consumes lenscraft-core in travelcraft-core/src/lens.rs for cartridge cross-matching and Universal Notebook logging.");
                            ui.label("- giscraft: Provides L7 bearing_lens.rs (position + compass heading ray-cast against OSM building footprints).");
                            ui.label("- speech-mcp (:10909): Provides sentence-streamed TTS (/api/v1/tts/wav) and FunASR STT (/api/v1/transcribe/file).");
                        }
                    });
                });
            self.show_help = open;
        }

        if self.show_shortcuts {
            let mut open = self.show_shortcuts;
            egui::Window::new("Keyboard Shortcuts")
                .open(&mut open)
                .collapsible(false)
                .default_width(420.0)
                .show(ctx, |ui| {
                    ui.label("Ctrl+O          Open Photo");
                    ui.label("Ctrl+E          Export Result JSON");
                    ui.label("Ctrl+K / Ctrl+Shift+P  Command Palette");
                    ui.label("Ctrl+,          Preferences");
                    ui.label("Ctrl+= / - / 0  Zoom In / Out / Reset (100%)");
                    ui.label("Ctrl+Wheel      Smooth UI Zoom");
                    ui.label("F1 / Ctrl+/     Keyboard Shortcuts");
                    ui.label("F5              Identify Photo (Auto)");
                    ui.label("F6              Identify Wall Label");
                    ui.label("F7              Ground in Wikidata/Wikipedia");
                    ui.label("F8              Generate 3m Museum Guide");
                });
            self.show_shortcuts = open;
        }

        if self.show_licenses {
            let mut open = self.show_licenses;
            egui::Window::new("Licenses & Notices")
                .open(&mut open)
                .collapsible(false)
                .default_width(480.0)
                .show(ctx, |ui| {
                    ui.heading("LensCraft");
                    ui.label("Dual-licensed under MIT OR Apache-2.0.");
                    ui.label("100% Safe Rust: #![forbid(unsafe_code)] and deny(unwrap_used, expect_used, panic, todo, unimplemented).");
                    ui.separator();
                    ui.label("Key third-party crates: eframe/egui (MIT/Apache-2.0), reqwest (MIT/Apache-2.0), serde/serde_json (MIT/Apache-2.0), image (MIT/Apache-2.0), axum/tokio (MIT), clap (MIT/Apache-2.0), rfd (MIT).");
                    ui.label("External data sources when queried: OpenStreetMap Overpass API (ODbL), Wikidata (CC0), Wikipedia (CC BY-SA).");
                });
            self.show_licenses = open;
        }

        if self.show_about {
            let mut open = self.show_about;
            egui::Window::new("About LensCraft")
                .open(&mut open)
                .collapsible(false)
                .default_width(400.0)
                .show(ctx, |ui| {
                    ui.heading(format!("LensCraft v{}", env!("CARGO_PKG_VERSION")));
                    ui.label("Local museum & landmark vision identification workstation.");
                    ui.label("Safe Rust (0 unsafe), Pure-Rust ArtCraft L1-L8.");
                    ui.label("Repository: https://github.com/sandraschi/lenscraft");
                });
            self.show_about = open;
        }

        triggered
    }
}
