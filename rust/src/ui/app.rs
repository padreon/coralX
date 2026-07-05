use crate::ui::welcome_screen::{self, Mode};

enum Screen {
    Welcome,
    PointCount,
    Measurement,
}

pub struct CoralXApp {
    screen: Screen,
}

impl Default for CoralXApp {
    fn default() -> Self {
        CoralXApp { screen: Screen::Welcome }
    }
}

impl eframe::App for CoralXApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        match self.screen {
            Screen::Welcome => {
                if let Some(mode) = welcome_screen::show(ui) {
                    self.screen = match mode {
                        Mode::PointCount => Screen::PointCount,
                        Mode::Measurement => Screen::Measurement,
                    };
                }
            }
            Screen::PointCount => {
                ui.label("Coral Point Count mode — main_window.py not yet ported.");
            }
            Screen::Measurement => {
                ui.label("Fragment Measurement mode — measurement_window.py not yet ported.");
            }
        }
    }
}
