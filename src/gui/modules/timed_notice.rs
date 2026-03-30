use egui::Color32;

use std::time::{Duration, Instant};

use crate::gui::new_app;

/*
Timed notice lifecycle overview (higher layers override lower ones):
  INIT                                            base_ttl
   |                                                 |
   [create_dur] [hold_dur] [fade_dur] [destroy_dur]

At any point the notice can be extended by calling extend():

extend()            reset to hold
   |                   |
   x<-extend_duration-> [hold_dur] -> ...

Extend overrides the normal lifecycle to fade the notice back in
over extend_duration (duration!), resets the notice to the start of the Hold phase,
and cedes control back to the regular lifecycle. Creation phase is not reapplied on reset.
*/

/// Phase of the TimedNotice's lifecycle
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Idle phase, notice is not visible until init() is called
    Idle,
    /// Fade in over create_dur
    Create,
    /// Full opacity for hold_dur
    Hold,
    /// Fade out for fade_dur
    FadeOut,
    /// Final fade for destroy_dur
    Destroy,
    /// Temporarily override normal lifecycle - fade back in for extend_duration and return to Hold
    Extend,
}

impl Default for Phase {
    fn default() -> Self {
        Phase::Idle
    }
}

/// Notice that appears for a limited time.
/// Durations are absolute; if a duration is zero, the phase is skipped.
#[derive(Debug, Clone)]
pub struct TimedNotice {
    /// Base lifecycle length
    base_ttl: Duration,

    /// Concrete durations for phases
    create_dur: Duration,
    hold_dur: Duration,
    fade_dur: Duration,
    destroy_dur: Duration,

    /// Duration of the extend action (fading back in)
    extend_duration: Option<Duration>,

    /// Message to display
    message: String,
    /// Base color of notice
    base_color: Color32,
    /// Whether the notice should be dismissed when clicked
    dismiss_on_click: bool,

    /// Easing function for the fading out phase, defaults to f(t) = t (linear)
    easing_fade_out: fn(f32) -> f32,
    /// Easing function for extending the lifetime of the notice (linear by default)
    easing_extend: fn(f32) -> f32,
    /// Easing function for the creation phase (linear by default)
    easing_create: fn(f32) -> f32,
    /// Easing function for the destruction phase (linear by default)
    easing_destroy: fn(f32) -> f32,

    // Declarative styling properties for frame and text
    frame_fill: Color32,
    frame_stroke: egui::Stroke,
    text_bold: bool,
    text_size: Option<f32>,

    /// Current phase of the notice
    phase: Phase,
    phase_start: Option<Instant>,
    phase_end: Option<Instant>,
}

impl Default for TimedNotice {
    fn default() -> Self {
        Self {
            base_ttl: Default::default(),
            create_dur: Default::default(),
            hold_dur: Default::default(),
            fade_dur: Default::default(),
            destroy_dur: Default::default(),
            extend_duration: Default::default(),
            message: Default::default(),
            base_color: Default::default(),
            dismiss_on_click: Default::default(),
            easing_fade_out: |t| t,
            easing_extend: |t| t,
            easing_create: |t| t,
            easing_destroy: |t| t,
            frame_fill: Color32::TRANSPARENT,
            frame_stroke: egui::Stroke::new(0.0, Color32::TRANSPARENT),
            text_bold: false,
            text_size: None,
            phase: Default::default(),
            phase_start: Default::default(),
            phase_end: Default::default(),
        }
    }
}

impl TimedNotice {
    pub fn new(base_ttl: Duration, message: String) -> Self {
        Self {
            base_ttl,
            message,
            create_dur: Duration::ZERO,
            hold_dur: Duration::ZERO,
            fade_dur: base_ttl, // default: all remaining time is mid fade
            destroy_dur: Duration::ZERO,
            extend_duration: None,
            base_color: new_app::get_theme().theme().text,
            dismiss_on_click: false,
            easing_fade_out: |t| t,
            easing_extend: |t| t,
            easing_create: |t| t,
            easing_destroy: |t| t,
            frame_fill: Color32::TRANSPARENT,
            frame_stroke: egui::Stroke::new(0.0, Color32::TRANSPARENT),
            text_bold: false,
            text_size: None,
            phase: Phase::Idle,
            phase_start: None,
            phase_end: None,
        }
    }

    pub fn dismiss_on_click(mut self, dismiss_on_click: bool) -> Self {
        self.dismiss_on_click = dismiss_on_click;
        self
    }

    pub fn colored(mut self, color: Color32) -> Self {
        self.base_color = color;
        self
    }

    /// Configure the frame fill color.
    pub fn with_frame_fill(mut self, fill: Color32) -> Self {
        self.frame_fill = fill;
        self
    }

    /// Configure the frame stroke.
    pub fn with_frame_stroke(mut self, stroke: egui::Stroke) -> Self {
        self.frame_stroke = stroke;
        self
    }

    /// Make the notice text bold.
    pub fn with_text_bold(mut self, bold: bool) -> Self {
        self.text_bold = bold;
        self
    }

    /// Override the notice text size.
    pub fn with_text_size(mut self, size: Option<f32>) -> Self {
        self.text_size = size;
        self
    }

    pub fn with_fade_out_easing(mut self, easing_fade_out: fn(f32) -> f32) -> Self {
        self.easing_fade_out = easing_fade_out;
        self
    }

    /// Configure destruction easing and explicit duration.
    pub fn with_destroy_easing(
        mut self,
        easing_destroy: fn(f32) -> f32,
        destroy_dur: Duration,
    ) -> Self {
        self.easing_destroy = easing_destroy;
        self.destroy_dur = destroy_dur;
        self
    }

    /// Configure creation easing and explicit duration.
    pub fn with_create_easing(
        mut self,
        easing_create: fn(f32) -> f32,
        create_dur: Duration,
    ) -> Self {
        self.easing_create = easing_create;
        self.create_dur = create_dur;
        self
    }

    pub fn with_extend_easing(
        mut self,
        easing_extend: fn(f32) -> f32,
        extend_for: Duration,
    ) -> Self {
        self.easing_extend = easing_extend;
        self.extend_duration = Some(extend_for);
        self
    }

    pub fn with_hold_duration(mut self, hold_dur: Duration) -> Self {
        self.hold_dur = hold_dur;
        self
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn color(&self) -> Color32 {
        self.base_color
    }

    pub fn is_idle(&self) -> bool {
        self.phase == Phase::Idle
    }

    /// Call this to initialize this Notice's lifecycle.
    pub fn init(&mut self, now: Instant) {
        // Derive fade_dur if left at default (i.e., not explicitly set)
        if self.fade_dur == self.base_ttl {
            let used = self
                .create_dur
                .saturating_add(self.hold_dur)
                .saturating_add(self.destroy_dur);
            self.fade_dur = self.base_ttl.saturating_sub(used);
        }

        // Pick starting phase (skip zero-length phases)
        if !self.create_dur.is_zero() {
            self.phase = Phase::Create;
            self.phase_start = Some(now);
            self.phase_end = Some(now + self.create_dur);
        } else if !self.hold_dur.is_zero() {
            self.phase = Phase::Hold;
            self.phase_start = Some(now);
            self.phase_end = Some(now + self.hold_dur);
        } else if !self.fade_dur.is_zero() {
            self.phase = Phase::FadeOut;
            self.phase_start = Some(now);
            self.phase_end = Some(now + self.fade_dur);
        } else if !self.destroy_dur.is_zero() {
            self.phase = Phase::Destroy;
            self.phase_start = Some(now);
            self.phase_end = Some(now + self.destroy_dur);
        } else {
            self.phase = Phase::Idle;
            self.phase_start = None;
            self.phase_end = None;
        }
    }

    /// Call this to extend the notice's ttl
    pub fn extend(&mut self, now: Instant) {
        if let Some(ext) = self.extend_duration {
            self.phase = Phase::Extend;
            self.phase_start = Some(now);
            self.phase_end = Some(now + ext);
        } else {
            // No extend configured: restart Hold immediately
            if !self.hold_dur.is_zero() {
                self.phase = Phase::Hold;
                self.phase_start = Some(now);
                self.phase_end = Some(now + self.hold_dur);
            } else if !self.fade_dur.is_zero() {
                self.phase = Phase::FadeOut;
                self.phase_start = Some(now);
                self.phase_end = Some(now + self.fade_dur);
            } else if !self.destroy_dur.is_zero() {
                self.phase = Phase::Destroy;
                self.phase_start = Some(now);
                self.phase_end = Some(now + self.destroy_dur);
            } else {
                self.phase = Phase::Idle;
                self.phase_start = None;
                self.phase_end = None;
            }
        }
    }

    /// Get the calculated opacity of the notice. Use when implementing custom rendering.
    pub fn opacity(&self) -> f32 {
        let (Some(phase_start), Some(phase_end)) = (self.phase_start, self.phase_end) else {
            return 0.0;
        };
        let phase_elapsed = phase_end.saturating_duration_since(phase_start);
        if phase_elapsed.is_zero() {
            return match self.phase {
                Phase::Idle => 0.0,
                Phase::Hold => 1.0,
                Phase::Destroy => 0.0,
                _ => 1.0,
            };
        }
        let now = Instant::now();
        let phase_progress_normalized = (now.saturating_duration_since(phase_start).as_secs_f32()
            / phase_elapsed.as_secs_f32())
        .clamp(0.0, 1.0);

        match self.phase {
            Phase::Idle => 0.0,
            Phase::Extend => (self.easing_extend)(phase_progress_normalized).clamp(0.0, 1.0),
            Phase::Create => {
                if self.create_dur.is_zero() {
                    1.0
                } else {
                    (self.easing_create)(phase_progress_normalized).clamp(0.0, 1.0)
                }
            }
            Phase::Hold => 1.0,
            Phase::FadeOut => {
                (1.0 - (self.easing_fade_out)(phase_progress_normalized)).clamp(0.0, 1.0)
            }
            Phase::Destroy => {
                if self.destroy_dur.is_zero() {
                    0.0
                } else {
                    (1.0 - (self.easing_destroy)(phase_progress_normalized)).clamp(0.0, 1.0)
                }
            }
        }
    }

    /// Internal: set a phase if its duration is non-zero, updating start/end timestamps.
    fn set_phase(&mut self, phase: Phase, now: Instant, dur: Duration) -> bool {
        if dur.is_zero() {
            return false;
        }
        self.phase = phase;
        self.phase_start = Some(now);
        self.phase_end = Some(now + dur);
        true
    }

    /// Resume normal lifecycle preferring Hold, then FadeOut, then Destroy; else Idle.
    fn resume_from_hold(&mut self, now: Instant) {
        if self.set_phase(Phase::Hold, now, self.hold_dur) {
            return;
        }
        if self.set_phase(Phase::FadeOut, now, self.fade_dur) {
            return;
        }
        if self.set_phase(Phase::Destroy, now, self.destroy_dur) {
            return;
        }
        self.phase = Phase::Idle;
        self.phase_start = None;
        self.phase_end = None;
    }

    /// Resume normal lifecycle preferring Create, then Hold, FadeOut, Destroy; else Idle.
    fn resume_from_create(&mut self, now: Instant) {
        if self.set_phase(Phase::Create, now, self.create_dur) {
            return;
        }
        if self.set_phase(Phase::Hold, now, self.hold_dur) {
            return;
        }
        if self.set_phase(Phase::FadeOut, now, self.fade_dur) {
            return;
        }
        if self.set_phase(Phase::Destroy, now, self.destroy_dur) {
            return;
        }
        self.phase = Phase::Idle;
        self.phase_start = None;
        self.phase_end = None;
    }

    /// Update the notice's state - call every frame.
    pub fn update(&mut self, now: Instant) {
        let Some(end) = self.phase_end else { return };
        if now < end {
            return;
        }

        match self.phase {
            Phase::Idle => {
                // Nothing to do
            }
            Phase::Extend => {
                // After extend, resume Hold (skip Create)
                self.resume_from_hold(now);
            }
            Phase::Create => {
                // Move to Hold → FadeOut → Destroy as applicable
                self.resume_from_hold(now);
            }
            Phase::Hold => {
                // Prefer FadeOut, then Destroy, else Idle
                if self.set_phase(Phase::FadeOut, now, self.fade_dur) {
                    return;
                }
                if self.set_phase(Phase::Destroy, now, self.destroy_dur) {
                    return;
                }
                self.phase = Phase::Idle;
                self.phase_start = None;
                self.phase_end = None;
            }
            Phase::FadeOut => {
                if self.set_phase(Phase::Destroy, now, self.destroy_dur) {
                    return;
                }
                self.phase = Phase::Idle;
                self.phase_start = None;
                self.phase_end = None;
            }
            Phase::Destroy => {
                // Terminal: auto-clear to Idle
                self.phase = Phase::Idle;
                self.phase_start = None;
                self.phase_end = None;
            }
        }
    }

    /// Render the notice as a clickable label with current opacity.
    /// Returns true if the caller should clear/dismiss the notice.
    pub fn show(&mut self, ui: &mut egui::Ui) -> bool {
        // Compute current opacity
        let alpha = self.opacity();
        if alpha <= 0.0 {
            return true; // effectively invisible; let caller clear
        }

        // Apply color with alpha and render
        let mut rgba = egui::epaint::Rgba::from(self.base_color);
        rgba[3] = alpha;

        // Build a frame around the notice using declarative properties
        let mut frame = egui::Frame::popup(ui.style())
            .fill(self.frame_fill)
            .stroke(self.frame_stroke);

        // Compose the label RichText using declarative text properties
        let mut rich = egui::RichText::new(self.message()).color(egui::Color32::from(rgba));
        if self.text_bold {
            rich = rich.strong();
        }
        if let Some(size) = self.text_size {
            rich = rich.size(size);
        }

        // Render inside the frame and use a pointer cursor
        let resp = frame
            .show(ui, |ui| {
                ui.add(egui::Label::new(rich).sense(egui::Sense::click_and_drag()))
            })
            .inner
            .on_hover_cursor(egui::CursorIcon::PointingHand);

        // Hover: extend (override phase)
        if resp.hovered() {
            self.extend(Instant::now());
        }

        // Click: dismiss if enabled
        if self.dismiss_on_click && resp.clicked() {
            return true;
        }

        // Request repaint while visible to keep animations updating
        ui.ctx().request_repaint();
        // Auto-clear when lifecycle has ended (Idle)
        self.is_idle()
    }
}

/// Create a timed notice in egui's temp storage.
/// Automatically initializes, shows and clears the notice.
/// Refactored to avoid holding the Context data RwLock during UI rendering.
pub fn notice_in_temp_storage<F>(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    id: egui::Id,
    init: F,
) -> bool
where
    F: FnOnce(Instant) -> TimedNotice,
{
    let now = Instant::now();

    // Take ownership for the duration of rendering to avoid holding the ctx lock while drawing
    let mut notice = ctx.data_mut(|d| {
        d.remove_temp(id).unwrap_or_else(|| {
            let mut n = init(now);
            n.init(now);
            n
        })
    });

    // Update lifecycle and render via the widget, outside of any ctx lock
    notice.update(now);
    let should_clear = notice.show(ui);

    // Reinsert or remove based on the result
    if should_clear {
        ctx.data_mut(|d| {
            d.remove::<TimedNotice>(id);
        });
    } else {
        ctx.data_mut(|d| {
            d.insert_temp(id, notice);
        });
    }

    should_clear
}
