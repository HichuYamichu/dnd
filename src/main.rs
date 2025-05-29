#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use egui_plot::{Bar, BarChart, Corner, Legend, Line, Plot, PlotPoint, PlotPoints, Text, VLine};
use std::collections::HashMap;
use std::sync::mpsc::{self, *};

use eframe::egui::{self, Align2, Color32, RichText, Vec2};
use eframe::egui::{Stroke, Ui};

mod math;
use math::*;

const AC_MIN: u8 = 10;
const AC_MAX: u8 = 24;

fn main() -> eframe::Result {
    // MaybeUninit this.
    // Oh wait you cant move out of array.
    let mut stats_senders = Vec::new();
    let mut stats_receivers = Vec::new();
    for _ in 0..2 {
        let (enque_stats_tx, enque_stats_rx) = mpsc::channel();
        let (stats_tx, stats_rx) = mpsc::channel();
        // We mainly do this because calculating mean dmg for a range of possible ACs
        // is terribly slow and we might as well offload all the math to a separate thread.
        std::thread::spawn(move || {
            loop {
                match enque_stats_rx.recv() {
                    Ok((build, ac, min_dmg)) => {
                        let stats = calc_build_stats(&build, ac, min_dmg);
                        stats_tx.send(stats).unwrap();
                    }
                    Err(_) => return,
                }
            }
        });
        stats_senders.push(enque_stats_tx);
        stats_receivers.push(stats_rx);
    }

    let mut means_senders = Vec::new();
    let mut means_receivers = Vec::new();
    for _ in 0..2 {
        let (enque_means_tx, enque_means_rx) = mpsc::channel();
        let (means_tx, means_rx) = mpsc::channel();
        std::thread::spawn(move || {
            loop {
                match enque_means_rx.recv() {
                    Ok(build) => {
                        // This could have been faster if it didn't recompute everything
                        // from scratch for no reason.
                        let stats = calc_build_means(&build);
                        means_tx.send(stats).unwrap();
                    }
                    Err(_) => return,
                }
            }
        });
        means_senders.push(enque_means_tx);
        means_receivers.push(means_rx);
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_maximized(true),
        ..Default::default()
    };
    eframe::run_native(
        "dnd",
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);

            Ok(Box::new(Dnd {
                build_a: Build::default(),
                build_b: Build::default(),

                stats_a: Stats::default(),
                stats_b: Stats::default(),

                means_a: Vec::new(),
                means_b: Vec::new(),

                stats_rx_a: stats_receivers.pop().unwrap(),
                stats_rx_b: stats_receivers.pop().unwrap(),

                stats_tx_a: stats_senders.pop().unwrap(),
                stats_tx_b: stats_senders.pop().unwrap(),

                means_rx_a: means_receivers.pop().unwrap(),
                means_rx_b: means_receivers.pop().unwrap(),

                means_tx_a: means_senders.pop().unwrap(),
                means_tx_b: means_senders.pop().unwrap(),

                sim_ac: 18,
                desired_min_dmg: 15,
                changed_a: true,
                changed_b: true,
            }))
        }),
    )
}

#[derive(Debug, Copy, Clone)]
enum Die {
    D4 = 4,
    D6 = 6,
    D8 = 8,
    D10 = 10,
    D20 = 20,
}

#[derive(Debug, Copy, Clone)]
struct Attack {
    ab: i32,
    flat: u8,
    dice: [(Die, u8); 5],
}

impl Default for Attack {
    fn default() -> Self {
        Self {
            ab: 10,
            flat: 4,
            dice: [
                (Die::D4, 2),
                (Die::D6, 0),
                (Die::D8, 1),
                (Die::D10, 0),
                (Die::D20, 0),
            ],
        }
    }
}

#[derive(Debug, Clone)]
struct Build {
    attacks: Vec<Attack>,
    savage: bool,
    crit_enabled: bool,
}

impl Default for Build {
    fn default() -> Self {
        Self {
            attacks: vec![Attack::default()],
            savage: false,
            crit_enabled: true,
        }
    }
}

struct Dnd {
    build_a: Build,
    build_b: Build,
    stats_a: Stats,
    stats_b: Stats,
    // We store these outside of core stats because it takes much longer to compute and we dont
    // want to stall the other data.
    means_a: Vec<f64>,
    means_b: Vec<f64>,

    stats_rx_a: Receiver<Stats>,
    stats_rx_b: Receiver<Stats>,

    stats_tx_a: Sender<(Build, u8, u32)>,
    stats_tx_b: Sender<(Build, u8, u32)>,

    means_rx_a: Receiver<Vec<f64>>,
    means_rx_b: Receiver<Vec<f64>>,

    means_tx_a: Sender<Build>,
    means_tx_b: Sender<Build>,

    sim_ac: u8,
    desired_min_dmg: u32,
    changed_a: bool,
    changed_b: bool,
}

impl eframe::App for Dnd {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.changed_a {
            self.changed_a = false;
            self.stats_tx_a
                .send((self.build_a.clone(), self.sim_ac, self.desired_min_dmg))
                .unwrap();
            self.means_tx_a.send(self.build_a.clone()).unwrap();
        }

        if self.changed_b {
            self.changed_b = false;
            self.stats_tx_b
                .send((self.build_b.clone(), self.sim_ac, self.desired_min_dmg))
                .unwrap();
            self.means_tx_b.send(self.build_b.clone()).unwrap();
        }

        if let Ok(stats) = self.stats_rx_a.try_recv() {
            self.stats_a = stats;
        }

        if let Ok(stats) = self.stats_rx_b.try_recv() {
            self.stats_b = stats;
        }

        if let Ok(means) = self.means_rx_a.try_recv() {
            self.means_a = means;
            self.stats_a.greater_then_chance = greater_than(&self.stats_a.pmf, &self.stats_b.pmf);
        }

        if let Ok(means) = self.means_rx_b.try_recv() {
            self.stats_b.greater_then_chance = greater_than(&self.stats_b.pmf, &self.stats_a.pmf);
            self.means_b = means;
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.label(RichText::new("DND build calculator").size(35.0));
                ui.add_space(10.0);

                let gap = 30.0;
                let total_width = ui.max_rect().width() - 10.0;
                let item_width = (total_width - gap) / 2.0;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    build_box(
                        ui,
                        item_width,
                        "Build A",
                        &mut self.build_a,
                        &self.stats_a,
                        self.desired_min_dmg,
                        &mut self.changed_a,
                    );
                    build_box(
                        ui,
                        item_width,
                        "Build B",
                        &mut self.build_b,
                        &self.stats_b,
                        self.desired_min_dmg,
                        &mut self.changed_b,
                    );
                });

                ui.add_space(20.0);
                ui.scope(|ui| {
                    let style = ui.style_mut();
                    for (_text_style, font_id) in style.text_styles.iter_mut() {
                        font_id.size = 20.0;
                    }

                    ui.horizontal(|ui| {
                        ui.label("Sim AC:");
                        let changed = ui.add(egui::DragValue::new(&mut self.sim_ac)).changed();
                        self.changed_a |= changed;
                        self.changed_b |= changed;

                        ui.add_space(10.0);
                        ui.label("Min desired dmg:");
                        let changed = ui
                            .add(egui::DragValue::new(&mut self.desired_min_dmg))
                            .changed();
                        self.changed_a |= changed;
                        self.changed_b |= changed;
                    });
                });
                ui.add_space(20.0);

                let plot_width = (total_width - gap) / 2.0;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    let plot_size = Vec2::new(plot_width, 500.0);
                    plot_pmf(ui, "Damage Distribution A", &self.stats_a.pmf, plot_size);
                    plot_pmf(ui, "Damage Distribution B", &self.stats_b.pmf, plot_size);
                });

                ui.add_space(15.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    let plot_size = Vec2::new(plot_width, 500.0);
                    plot_cdf(
                        ui,
                        "Cumulative Distribution A",
                        &self.stats_a.cdf,
                        plot_size,
                    );
                    plot_cdf(
                        ui,
                        "Cumulative Distrbuition B",
                        &self.stats_b.cdf,
                        plot_size,
                    );
                });

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    let plot_size = Vec2::new(plot_width, 500.0);
                    plot_mean_for_ac(
                        ui,
                        "Mean DMG for given AC for Build A",
                        plot_size,
                        &self.means_a,
                    );
                    plot_mean_for_ac(
                        ui,
                        "Mean DMG for given AC for Build B",
                        plot_size,
                        &self.means_b,
                    );
                });

                ui.separator();
            });
        });
    }
}

fn build_box(
    ui: &mut Ui,
    item_width: f32,
    build_name: &str,
    build: &mut Build,
    stats: &Stats,
    desired_min_dmg: u32,
    changed: &mut bool,
) {
    let style = ui.style_mut();
    for (_text_style, font_id) in style.text_styles.iter_mut() {
        font_id.size = 18.0;
    }
    ui.vertical(|ui| {
        ui.group(|ui| {
            ui.set_width(item_width);
            ui.set_height(370.0);

            ui.label(RichText::new(build_name).size(24.0));
            if ui.button("Add attack").clicked() {
                let prev_or_def = build.attacks.last().cloned().unwrap_or(Attack::default());
                build.attacks.push(prev_or_def);
                *changed = true;
            }
            let mut remove_request = None;
            for (i, attack) in build.attacks.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
                    ui.label("AB:");
                    *changed |= ui.add(egui::DragValue::new(&mut attack.ab)).changed();

                    ui.label("Flat dmg:");
                    *changed |= ui.add(egui::DragValue::new(&mut attack.flat)).changed();

                    ui.label("D4:");
                    *changed |= ui
                        .add(egui::DragValue::new(&mut attack.dice[0].1))
                        .changed();

                    ui.label("D6:");
                    *changed |= ui
                        .add(egui::DragValue::new(&mut attack.dice[1].1))
                        .changed();

                    ui.label("D8:");
                    *changed |= ui
                        .add(egui::DragValue::new(&mut attack.dice[2].1))
                        .changed();

                    ui.label("D10:");
                    *changed |= ui
                        .add(egui::DragValue::new(&mut attack.dice[3].1))
                        .changed();

                    ui.label("D20:");
                    *changed |= ui
                        .add(egui::DragValue::new(&mut attack.dice[4].1))
                        .changed();

                    if ui.button("Remove").clicked() {
                        remove_request = Some(i);
                    }
                });
            }
            if let Some(idx) = remove_request {
                build.attacks.remove(idx);
                *changed = true;
            }
            *changed |= ui
                .checkbox(&mut build.crit_enabled, "Crits Enabled")
                .changed();
            *changed |= ui.checkbox(&mut build.savage, "Savage Attacker").changed();
            ui.add_space(10.0);
            ui.label(RichText::new(format!("Mean damage: {:.2}", stats.mean)).size(15.0));
            ui.label(RichText::new(format!("Standard deviation: {:.2}", stats.std_dev)).size(15.0));
            ui.add_space(10.0);
            ui.label(format!(
                "There is {:.1}% chance that {} will out damage the other build.",
                stats.greater_then_chance * 100.0,
                build_name
            ));

            ui.label(RichText::new(format!(
                "There is {:.1}% chance to deal at least {} damage.",
                stats.min_dmg_chance * 100.0,
                desired_min_dmg,
            )));
        });
    });
}

fn plot_pmf(ui: &mut Ui, title: &str, pmf: &PMF, size: Vec2) {
    let bars: Vec<Bar> = pmf
        .iter()
        .map(|(&dmg, &prob)| {
            Bar::new(dmg as f64, prob)
                .fill(Color32::from_rgb(70, 53, 177))
                .stroke(Stroke::new(0.1, Color32::WHITE))
        })
        .collect();
    let chart = BarChart::new(title, bars.clone()).width(1.0);
    ui.allocate_ui(size, |ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(title).size(20.0).strong());
            Plot::new(title)
                .view_aspect(2.0)
                .x_axis_label("dmg")
                .y_axis_label("chance")
                .allow_scroll(false)
                .allow_boxed_zoom(false)
                .allow_drag(false)
                .cursor_color(Color32::TRANSPARENT)
                .show(ui, |plot_ui| {
                    plot_ui.bar_chart(chart);
                });
        });
    });
}

fn plot_cdf(ui: &mut Ui, title: &str, cdf: &CDF, size: Vec2) {
    fn to_step_points(cdf: &CDF) -> PlotPoints {
        let mut points = Vec::new();
        if cdf.is_empty() {
            return points.into();
        }

        points.push([cdf[0].0 as f64, 0.0]);

        for window in cdf.windows(2) {
            let (x1, y1) = window[0];
            let (x2, _) = window[1];
            points.push([x1 as f64, y1]);
            points.push([x2 as f64, y1]);
        }

        if let Some(&(x_last, y_last)) = cdf.last() {
            points.push([x_last as f64, y_last]);
        }

        points.into()
    }

    let points: PlotPoints = to_step_points(cdf);
    let line = Line::new(title, points)
        .color(Color32::from_rgb(200, 100, 100))
        .name("Cumulative Distribution")
        .fill_alpha(0.0)
        .stroke(egui::Stroke::new(5.0, Color32::LIGHT_BLUE));

    let p95_x = cdf
        .iter()
        .find(|&&(_, p)| p >= 0.95)
        .map(|&(x, _)| x as f64)
        .unwrap_or(0.0);

    let vline_95 = VLine::new("95", p95_x)
        .color(Color32::RED)
        .name("95th percentile");

    let p25_x = cdf
        .iter()
        .find(|&&(_, p)| p >= 0.25)
        .map(|&(x, _)| x as f64)
        .unwrap_or(0.0);

    let vline_25 = VLine::new("25", p25_x)
        .color(Color32::GREEN)
        .name("25th percentile");

    let p75_x = cdf
        .iter()
        .find(|&&(_, p)| p >= 0.75)
        .map(|&(x, _)| x as f64)
        .unwrap_or(0.0);

    let vline_75 = VLine::new("75", p75_x)
        .color(Color32::ORANGE)
        .name("75th percentile");

    ui.allocate_ui(size, |ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(title).size(20.0).strong());
            Plot::new(title)
                .view_aspect(2.0)
                .x_axis_label("dmg")
                .y_axis_label("cumulative probability")
                .allow_scroll(false)
                .allow_boxed_zoom(false)
                .allow_drag(false)
                .auto_bounds([true, true])
                .default_y_bounds(0.0, 1.1)
                .legend(Legend::default().position(Corner::RightBottom))
                .show(ui, |plot_ui| {
                    plot_ui.line(line);
                    plot_ui.vline(vline_25);
                    plot_ui.vline(vline_75);
                    plot_ui.vline(vline_95);
                });
        });
    });
}

fn plot_mean_for_ac(ui: &mut Ui, title: &str, size: Vec2, means: &Vec<f64>) {
    let bars: Vec<Bar> = means
        .iter()
        .enumerate()
        .map(|(offset, mean)| {
            let ac = AC_MIN + offset as u8;
            Bar::new(ac as f64, *mean)
                .fill(Color32::from_rgb(70, 53, 177))
                .stroke(Stroke::new(0.2, Color32::BLACK))
        })
        .collect();

    let chart = BarChart::new(title, bars.clone()).width(1.0);
    ui.allocate_ui(size, |ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(title).size(20.0).strong());
            Plot::new(title)
                .view_aspect(2.0)
                .x_axis_label("AC")
                .y_axis_label("dmg")
                .allow_scroll(false)
                .allow_boxed_zoom(false)
                .allow_drag(false)
                .cursor_color(Color32::TRANSPARENT)
                .show(ui, |plot_ui| {
                    plot_ui.bar_chart(chart);
                });
        });
    });
}
