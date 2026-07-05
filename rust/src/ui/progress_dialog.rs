//! Reusable progress dialog + background worker for coralX.
//!
//! Usage: spawn a job with [`spawn_worker`], keep the returned
//! [`ProgressDialog`] in your screen's state, call [`ProgressDialog::poll`]
//! once per frame, and [`ProgressDialog::show`] while `is_open()`.

use std::sync::mpsc::{Receiver, TryRecvError};

use egui::{Color32, Context, ProgressBar, RichText};

pub enum WorkerEvent<T> {
    Progress { done: usize, total: usize, message: String },
    Succeeded(T),
    Failed(String),
}

pub type ProgressCb<'a> = dyn FnMut(usize, usize, &str) + 'a;

/// Run `job` on a background thread, reporting progress through a callback.
pub fn spawn_worker<T: Send + 'static>(job: impl FnOnce(&mut ProgressCb) -> anyhow::Result<T> + Send + 'static) -> Receiver<WorkerEvent<T>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let tx2 = tx.clone();
        let mut cb = move |done: usize, total: usize, msg: &str| {
            let _ = tx2.send(WorkerEvent::Progress { done, total, message: msg.to_string() });
        };
        match job(&mut cb) {
            Ok(result) => {
                let _ = tx.send(WorkerEvent::Succeeded(result));
            }
            Err(e) => {
                let _ = tx.send(WorkerEvent::Failed(e.to_string()));
            }
        }
    });
    rx
}

enum State<T> {
    Running,
    Done(T),
    Error(String),
}

/// Modal-style progress dialog with a status line, counter, and progress bar.
pub struct ProgressDialog<T> {
    title: String,
    status: String,
    done: usize,
    total: usize,
    indeterminate: bool,
    cancellable: bool,
    cancelled: bool,
    rx: Receiver<WorkerEvent<T>>,
    state: State<T>,
}

impl<T> ProgressDialog<T> {
    pub fn new(title: impl Into<String>, total: usize, cancellable: bool, rx: Receiver<WorkerEvent<T>>) -> Self {
        ProgressDialog {
            title: title.into(),
            status: "Starting...".to_string(),
            done: 0,
            total,
            indeterminate: total == 0,
            cancellable,
            cancelled: false,
            rx,
            state: State::Running,
        }
    }

    pub fn is_open(&self) -> bool {
        matches!(self.state, State::Running)
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    /// Drain any pending events; call once per frame while `is_open()`.
    pub fn poll(&mut self) {
        loop {
            match self.rx.try_recv() {
                Ok(WorkerEvent::Progress { done, total, message }) => {
                    if total != self.total && total > 0 {
                        self.total = total;
                        self.indeterminate = false;
                    }
                    self.done = done;
                    self.status = message;
                }
                Ok(WorkerEvent::Succeeded(v)) => {
                    self.state = State::Done(v);
                    break;
                }
                Ok(WorkerEvent::Failed(msg)) => {
                    self.state = State::Error(msg);
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
    }

    /// Take the finished result (`Ok(value)` on success, `Err(message)` on
    /// failure); only meaningful once `is_open()` is false.
    pub fn take_result(self) -> Option<Result<T, String>> {
        match self.state {
            State::Done(v) => Some(Ok(v)),
            State::Error(e) => Some(Err(e)),
            State::Running => None,
        }
    }

    pub fn set_indeterminate(&mut self, msg: impl Into<String>) {
        self.indeterminate = true;
        self.status = msg.into();
    }

    pub fn show(&mut self, ctx: &Context) {
        egui::Window::new(&self.title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_min_width(400.0);
                ui.label(&self.status);
                if !self.indeterminate && self.total > 0 {
                    ui.label(RichText::new(format!("{} / {}", self.done, self.total)).color(Color32::from_rgb(0x88, 0x88, 0x88)).size(11.0));
                }

                let bar = if self.indeterminate {
                    ProgressBar::new(0.0).animate(true)
                } else {
                    ProgressBar::new(self.done as f32 / self.total.max(1) as f32).show_percentage()
                };
                ui.add(bar);

                if self.cancellable {
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let label = if self.cancelled { "Cancelling..." } else { "Cancel" };
                            if ui.add_enabled(!self.cancelled, egui::Button::new(label)).clicked() {
                                self.cancelled = true;
                            }
                        });
                    });
                }
            });
        ctx.request_repaint();
    }
}
