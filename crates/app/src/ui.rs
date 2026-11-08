//! The GPUI window: device picker, live view, timed runs, result and export.

use std::time::Duration;

use analysis::{DeviceKey, MouseEvent};
use capture::DeviceChange;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme, Disableable, h_flex, v_flex};
use gpui_kit::*;

use crate::export;
use crate::format;
use crate::live::{self, LiveWindow};
use crate::run::{DURATIONS, RunState};
use crate::selection::Selection;
use crate::source::{self, Source};

const FRAME: Duration = Duration::from_millis(16);

pub struct AppView {
    source: Option<Source>,
    error: Option<String>,
    selection: Selection,
    live: LiveWindow,
    run: RunState,
    duration_secs: u32,
    show_about: bool,
    status: Option<String>,
    scratch: Vec<MouseEvent>,
    _frames: Task<()>,
}

impl AppView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let (source, error) = match Source::start() {
            Ok(s) => (Some(s), None),
            Err(e) => (None, Some(e)),
        };
        let freq = source.as_ref().map_or(1, Source::qpc_frequency);
        let mut selection = Selection::default();
        selection.set_devices(source::list_devices());

        let frames = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(FRAME).await;
                if this.update(cx, |view, cx| view.frame(cx)).is_err() {
                    break;
                }
            }
        });

        Self {
            source,
            error,
            selection,
            live: LiveWindow::new(freq),
            run: RunState::Idle,
            duration_secs: DURATIONS[1],
            show_about: false,
            status: None,
            scratch: Vec::with_capacity(4096),
            _frames: frames,
        }
    }

    /// Runs once per frame: drain capture, update state, redraw.
    fn frame(&mut self, cx: &mut Context<Self>) {
        let Some(src) = self.source.as_mut() else {
            return;
        };
        let changes = src.changes();
        self.scratch.clear();
        src.drain(&mut self.scratch);
        let now = src.now_ticks();
        let dropped = src.dropped_reports();

        if !changes.is_empty() {
            self.selection.set_devices(source::list_devices());
            for c in &changes {
                if let DeviceChange::Removed(key) = c {
                    self.run.device_removed(*key, dropped);
                }
            }
        }

        if self.selection.is_detecting() {
            let mut seen: Vec<DeviceKey> = Vec::new();
            for e in &self.scratch {
                if !seen.contains(&e.device) {
                    seen.push(e.device);
                    if self.selection.on_activity(e.device) {
                        break;
                    }
                }
            }
        }

        self.live.watch(self.selection.selected_key());
        self.live.push(&self.scratch);
        self.live.trim(now);
        self.run.feed(&self.scratch);
        self.run.tick(now, dropped);
        cx.notify();
    }

    fn start_run(&mut self, cx: &mut Context<Self>) {
        let (Some(src), Some(device)) = (self.source.as_ref(), self.selection.selected_key())
        else {
            return;
        };
        self.status = None;
        self.run = RunState::start(
            device,
            self.duration_secs,
            src.now_ticks(),
            src.qpc_frequency(),
            src.dropped_reports(),
        );
        cx.notify();
    }

    fn export(&mut self, csv: bool, cx: &mut Context<Self>) {
        let RunState::Finished(finished) = &self.run else {
            return;
        };
        let Some(device) = self.selection.selected().cloned().or_else(|| {
            self.selection
                .devices()
                .iter()
                .find(|d| d.key == finished.run.settings.device)
                .cloned()
        }) else {
            self.status = Some("The measured mouse is no longer listed.".into());
            return;
        };
        let run = finished.run.clone();
        let result = finished.result.clone();
        let ext = if csv { "csv" } else { "json" };
        let dir = std::env::home_dir().unwrap_or_default();
        let picked = cx.prompt_for_new_path(&dir, Some(&export::suggested_name(ext)));

        cx.spawn(async move |this, cx| {
            let message = match picked.await {
                Ok(Ok(Some(path))) => match std::fs::File::create(&path) {
                    Ok(file) => {
                        let w = std::io::BufWriter::new(file);
                        let written = if csv {
                            export::write_csv(w, &device, &run).map_err(|e| e.to_string())
                        } else {
                            export::write_json(w, &device, &run, &result).map_err(|e| e.to_string())
                        };
                        match written {
                            Ok(()) => format!("Saved {}", path.display()),
                            Err(e) => format!("Export failed: {e}"),
                        }
                    }
                    Err(e) => format!("Export failed: {e}"),
                },
                Ok(Ok(None)) => return,
                Ok(Err(e)) => format!("Export failed: {e}"),
                Err(_) => return,
            };
            let _ = this.update(cx, |view, cx| {
                view.status = Some(message);
                cx.notify();
            });
        })
        .detach();
    }

    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        h_flex()
            .justify_between()
            .items_center()
            .child(
                v_flex()
                    .child(div().text_xl().child(export::APP_NAME))
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("Raw Input delivery timing"),
                    ),
            )
            .child(
                Button::new("about")
                    .ghost()
                    .label(if self.show_about { "Close" } else { "About" })
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.show_about = !view.show_about;
                        cx.notify();
                    })),
            )
    }

    fn device_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let running = self.run.is_running();
        let mut row = h_flex().gap_2().flex_wrap().items_center();
        if self.selection.devices().is_empty() {
            row = row.child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child("No mouse found. Connect one; the list updates by itself."),
            );
        }
        for (i, d) in self.selection.devices().iter().enumerate() {
            let key = d.key;
            row = row.child(
                Button::new(("device", i))
                    .label(d.name.clone())
                    .toggled(self.selection.selected_key() == Some(key))
                    .disabled(running)
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.selection.select(key);
                        cx.notify();
                    })),
            );
        }
        if self.selection.devices().len() > 1 {
            let detecting = self.selection.is_detecting();
            row = row.child(
                Button::new("detect")
                    .outline()
                    .label(if detecting {
                        "Move the mouse to measure…"
                    } else {
                        "Detect"
                    })
                    .disabled(running)
                    .on_click(cx.listener(move |view, _, _, cx| {
                        if detecting {
                            view.selection.cancel_detect();
                        } else {
                            view.selection.start_detect();
                        }
                        cx.notify();
                    })),
            );
        }
        row
    }

    fn live_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let now = self.source.as_ref().map_or(0, Source::now_ticks);
        let points = self.live.points(now);
        let rate = format::live_rate(self.live.current_rate_hz(now));
        let plot_color = theme.chart_1;
        let bar_color = theme.chart_2;
        let grid = theme.border;
        let hist = live::histogram(&points, 48);

        v_flex()
            .gap_2()
            .child(
                h_flex()
                    .gap_3()
                    .items_baseline()
                    .child(div().text_3xl().child(rate))
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("current report rate"),
                    ),
            )
            .child(
                div()
                    .h(px(180.))
                    .w_full()
                    .border_1()
                    .border_color(grid)
                    .child(interval_plot(points, plot_color).size_full()),
            )
            .child(
                div()
                    .h(px(100.))
                    .w_full()
                    .border_1()
                    .border_color(grid)
                    .child(histogram_plot(hist, bar_color).size_full()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("Top: interval between reports over the last 2 s. Bottom: how often each interval occurs (0 to 3× the median)."),
            )
    }

    fn run_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let can_start = self.source.is_some() && self.selection.selected_key().is_some();
        let mut row = h_flex().gap_2().items_center();
        match &self.run {
            RunState::Running(active) => {
                let now = self.source.as_ref().map_or(0, Source::now_ticks);
                row = row
                    .child(
                        div()
                            .text_2xl()
                            .child(format!("{} s", active.seconds_left(now))),
                    )
                    .child(
                        div().child("Keep moving the mouse in circles until the countdown ends."),
                    )
                    .child(
                        Button::new("cancel")
                            .danger()
                            .label("Cancel")
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.run.cancel();
                                cx.notify();
                            })),
                    );
            }
            _ => {
                for secs in DURATIONS {
                    row = row.child(
                        Button::new(("duration", secs as usize))
                            .label(format!("{secs} s"))
                            .toggled(self.duration_secs == secs)
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.duration_secs = secs;
                                cx.notify();
                            })),
                    );
                }
                row = row.child(
                    Button::new("start")
                        .primary()
                        .label("Start measurement")
                        .disabled(!can_start)
                        .on_click(cx.listener(|view, _, _, cx| view.start_run(cx))),
                );
            }
        }
        row
    }

    fn result_panel(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let RunState::Finished(finished) = &self.run else {
            return None;
        };
        let theme = cx.theme();
        let result = &finished.result;
        let mut panel = v_flex().gap_2().p_3().border_1().border_color(theme.border);
        panel = panel.child(div().text_lg().child("Result"));
        if let Some(v) = format::verdict(result) {
            panel = panel.child(div().text_color(theme.warning).child(v));
        }
        for row in format::rows(result) {
            panel = panel.child(
                h_flex()
                    .justify_between()
                    .child(div().text_color(theme.muted_foreground).child(row.label))
                    .child(div().child(row.value)),
            );
        }
        panel = panel
            .child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(format::SCOPE_STATEMENT),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("export-json")
                            .label("Export JSON")
                            .on_click(cx.listener(|view, _, _, cx| view.export(false, cx))),
                    )
                    .child(
                        Button::new("export-csv")
                            .label("Export CSV")
                            .on_click(cx.listener(|view, _, _, cx| view.export(true, cx))),
                    )
                    .child(
                        Button::new("clear")
                            .ghost()
                            .label("Clear")
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.run.reset();
                                view.status = None;
                                cx.notify();
                            })),
                    ),
            );
        if let Some(status) = &self.status {
            panel = panel.child(div().text_sm().child(status.clone()));
        }
        Some(panel)
    }

    fn about_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        v_flex()
            .gap_2()
            .p_3()
            .border_1()
            .border_color(theme.border)
            .child(div().text_lg().child(format!(
                "{} {}",
                export::APP_NAME,
                export::APP_VERSION
            )))
            .child(div().child(format::SCOPE_STATEMENT))
            .child(div().text_sm().text_color(theme.muted_foreground).child(
                "Runs entirely on this PC. It opens no network connections and changes no settings.",
            ))
    }
}

impl Render for AppView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let mut root = v_flex()
            .size_full()
            .gap_4()
            .p_4()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(self.header(cx));
        if let Some(err) = &self.error {
            root = root.child(div().text_color(cx.theme().danger).child(err.clone()));
        }
        if self.show_about {
            root = root.child(self.about_panel(cx));
        }
        root.child(self.device_row(cx))
            .child(self.live_panel(cx))
            .child(self.run_panel(cx))
            .children(self.result_panel(cx))
    }
}

/// Interval-over-time plot: one min/max bar per pixel column.
fn interval_plot(points: Vec<live::Point>, color: Hsla) -> Canvas<()> {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            if points.is_empty() {
                return;
            }
            let width = f32::from(bounds.size.width).max(1.0) as usize;
            let height = f64::from(bounds.size.height);
            let span = (live::WINDOW_SECS * 1_000_000) as f64;
            let mut sorted: Vec<f64> = points.iter().map(|p| p.interval_us).collect();
            sorted.sort_by(f64::total_cmp);
            let median = sorted[(sorted.len() - 1) / 2];
            let y_max = (median * 3.0).max(1.0);
            for (x, col) in live::min_max_columns(&points, span, width)
                .into_iter()
                .enumerate()
            {
                let Some((lo, hi)) = col else { continue };
                let to_y = |v: f64| height - (v.min(y_max) / y_max) * height;
                let (top, bottom) = (to_y(hi), to_y(lo));
                let h = (bottom - top).max(1.5);
                window.paint_quad(fill(
                    Bounds::new(
                        point(
                            bounds.origin.x + px(x as f32),
                            bounds.origin.y + px(top as f32),
                        ),
                        size(px(1.0), px(h as f32)),
                    ),
                    color,
                ));
            }
        },
    )
}

fn histogram_plot(hist: Option<live::Histogram>, color: Hsla) -> Canvas<()> {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let Some(h) = hist else { return };
            let peak = h.bins.iter().copied().max().unwrap_or(0).max(1) as f32;
            let n = h.bins.len() as f32;
            let w = f32::from(bounds.size.width);
            let height = f32::from(bounds.size.height);
            let bar_w = (w / n).max(1.0);
            for (i, &c) in h.bins.iter().enumerate() {
                if c == 0 {
                    continue;
                }
                let bh = (c as f32 / peak) * height;
                window.paint_quad(fill(
                    Bounds::new(
                        point(
                            bounds.origin.x + px(i as f32 * bar_w),
                            bounds.origin.y + px(height - bh),
                        ),
                        size(px((bar_w - 1.0).max(1.0)), px(bh)),
                    ),
                    color,
                ));
            }
        },
    )
}
