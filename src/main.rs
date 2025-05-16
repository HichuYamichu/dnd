#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use egui_plot::{Bar, BarChart, Corner, Legend, Line, Plot, PlotPoint, PlotPoints, Text, VLine};
use std::collections::HashMap;

use eframe::egui::{self, Align2, Color32, RichText, Vec2};
use eframe::egui::{Stroke, Ui};

const AC_MIN: u8 = 10;
const AC_MAX: u8 = 24;

fn main() -> eframe::Result {
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
                build_a: Build {
                    attacks: vec![Attack {
                        ab: 10,
                        flat: 4,
                        dice: [
                            // (Die::D4, 2),
                            (Die::D4, 40),
                            // (Die::D6, 0),
                            (Die::D6, 20),
                            (Die::D8, 1),
                            (Die::D10, 0),
                            (Die::D20, 0),
                        ],
                    }],
                    savage: false,
                    crit_enabled: true,
                    pmf: PMF::default(),
                    cdf: CDF::default(),
                    mean: 0.0,
                    std_dev: 0.0,
                    greater_then_chance: 0.0,
                    min_dmg_chance: 0.0,
                },
                build_b: Build {
                    attacks: vec![Attack {
                        ab: 10,
                        flat: 4,
                        dice: [
                            // (Die::D4, 2),
                            (Die::D4, 40),
                            // (Die::D6, 0),
                            (Die::D6, 20),
                            (Die::D8, 1),
                            (Die::D10, 0),
                            (Die::D20, 0),
                        ],
                    }],
                    savage: false,
                    crit_enabled: true,
                    pmf: PMF::default(),
                    cdf: CDF::default(),
                    mean: 0.0,
                    std_dev: 0.0,
                    greater_then_chance: 0.0,
                    min_dmg_chance: 0.0,
                },
                sim_ac: 18,
                desired_min_dmg: 15,
                state_changed: true,
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

impl Attack {
    fn dmg(&self) -> f64 {
        let mut total = self.flat;
        for die in self.dice {
            total += (die.0 as u8 / 2) * die.1;
        }
        total as f64
    }
}

impl Default for Attack {
    fn default() -> Self {
        Self {
            ab: 10,
            flat: 5,
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

struct Dnd {
    build_a: Build,
    build_b: Build,
    sim_ac: u8,
    desired_min_dmg: u32,
    state_changed: bool,
}

#[derive(Debug)]
struct Build {
    attacks: Vec<Attack>,
    savage: bool,
    crit_enabled: bool,
    pmf: PMF,
    cdf: CDF,
    mean: f64,
    std_dev: f64,
    greater_then_chance: f64,
    min_dmg_chance: f64,
}

impl eframe::App for Dnd {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.state_changed {
            self.state_changed = false;

            self.build_a.pmf = convolve_many(
                &self
                    .build_a
                    .attacks
                    .iter()
                    .map(|a| {
                        attack_pmf(
                            a,
                            self.sim_ac,
                            self.build_a.crit_enabled,
                            self.build_a.savage,
                        )
                    })
                    .collect::<Vec<_>>(),
            );
            self.build_a.cdf = cdf(&self.build_a.pmf);
            self.build_a.mean = mean(&self.build_a.pmf);
            self.build_a.std_dev = std_dev(&self.build_a.pmf);
            self.build_a.min_dmg_chance = chance_at_least(&self.build_a.pmf, self.desired_min_dmg);

            self.build_b.pmf = convolve_many(
                &self
                    .build_b
                    .attacks
                    .iter()
                    .map(|a| {
                        attack_pmf(
                            a,
                            self.sim_ac,
                            self.build_b.crit_enabled,
                            self.build_b.savage,
                        )
                    })
                    .collect::<Vec<_>>(),
            );
            self.build_b.cdf = cdf(&self.build_b.pmf);
            self.build_b.mean = mean(&self.build_b.pmf);
            self.build_b.std_dev = std_dev(&self.build_b.pmf);
            self.build_b.min_dmg_chance = chance_at_least(&self.build_b.pmf, self.desired_min_dmg);

            self.build_a.greater_then_chance = greater_than(&self.build_a.pmf, &self.build_b.pmf);
            self.build_b.greater_then_chance = greater_than(&self.build_b.pmf, &self.build_a.pmf);
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
                        self.desired_min_dmg,
                        &mut self.state_changed,
                    );
                    build_box(
                        ui,
                        item_width,
                        "Build B",
                        &mut self.build_b,
                        self.desired_min_dmg,
                        &mut self.state_changed,
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
                        self.state_changed |=
                            ui.add(egui::DragValue::new(&mut self.sim_ac)).changed();
                        ui.add_space(10.0);
                        ui.label("Min desired dmg:");
                        self.state_changed |= ui
                            .add(egui::DragValue::new(&mut self.desired_min_dmg))
                            .changed();
                    });
                });
                ui.add_space(20.0);

                let plot_width = (total_width - gap) / 2.0;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    let plot_size = Vec2::new(plot_width, 500.0);
                    plot_pmf(ui, "Damage Distribution A", &self.build_a.pmf, plot_size);
                    plot_pmf(ui, "Damage Distribution B", &self.build_b.pmf, plot_size);
                });

                ui.add_space(15.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    let plot_size = Vec2::new(plot_width, 500.0);
                    plot_cdf(
                        ui,
                        "Cumulative Distribution A",
                        &self.build_a.cdf,
                        plot_size,
                    );
                    plot_cdf(
                        ui,
                        "Cumulative Distrbuition B",
                        &self.build_b.cdf,
                        plot_size,
                    );
                });

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    let plot_size = Vec2::new(plot_width, 500.0);
                    plot_mean_for_ac(ui, "Mean DMG per AC for Build A", plot_size, &self.build_a);
                    plot_mean_for_ac(ui, "Mean DMG per AC for Build B", plot_size, &self.build_b);
                });

                ui.separator();
            });
        });
    }
}

type PMF = HashMap<u32, f64>;
type CDF = Vec<(u32, f64)>;

fn hit_chance(ab: i32, ac: i32) -> f64 {
    let needed_roll = (ac - ab).clamp(2, 20);

    let possible_hits = 21 - needed_roll;
    let mut chance = possible_hits as f64 / 20.0;

    if ab + 1 >= ac {
        chance -= 1.0 / 20.0;
    }
    if ab + 20 < ac {
        chance += 1.0 / 20.0;
    }

    chance.clamp(0.0, 1.0)
}

fn convolve(a: &PMF, b: &PMF) -> PMF {
    let mut result = HashMap::new();
    for (&x, &px) in a {
        for (&y, &py) in b {
            *result.entry(x + y).or_insert(0.0) += px * py;
        }
    }
    result
}

fn convolve_many(pmfs: &[PMF]) -> PMF {
    pmfs.iter()
        .cloned()
        .reduce(|a, b| convolve(&a, &b))
        .unwrap_or_default()
}

fn scale(pmf: &PMF, factor: f64) -> PMF {
    pmf.iter().map(|(&k, &v)| (k, v * factor)).collect()
}

fn shift(pmf: &PMF, offset: u32) -> PMF {
    pmf.iter().map(|(&k, &v)| (k + offset, v)).collect()
}

fn die_pmf(die: Die) -> PMF {
    let mut pmf = HashMap::new();
    let sides = die as u32;
    for i in 1..=sides {
        pmf.insert(i, 1.0 / sides as f64);
    }
    pmf
}

fn attack_pmf(attack: &Attack, ac: u8, crit_enabled: bool, savage_attacker: bool) -> PMF {
    let base_pmfs: Vec<_> = attack
        .dice
        .iter()
        .flat_map(|&(die, count)| {
            let single = die_pmf(die);
            std::iter::repeat(single).take(count as usize)
        })
        .collect();

    let base_dmg_dist = convolve_many(&base_pmfs);
    let base_pmf = shift(&base_dmg_dist, attack.flat as u32);

    let base_pmf = if savage_attacker {
        best_of_two(&base_pmf)
    } else {
        base_pmf
    };

    let crit_pmfs: Vec<_> = attack
        .dice
        .iter()
        .flat_map(|&(die, count)| {
            let single = die_pmf(die);
            std::iter::repeat(single).take((2 * count) as usize)
        })
        .collect();

    let crit_dmg_dist = convolve_many(&crit_pmfs);
    let crit_pmf = shift(&crit_dmg_dist, attack.flat as u32);

    let crit_pmf = if savage_attacker {
        best_of_two(&crit_pmf)
    } else {
        crit_pmf
    };

    let hit_chance = hit_chance(attack.ab, ac as _);
    let crit_chance = if crit_enabled { 1.0 / 20.0 } else { 0.0 };

    let split_hit_chance = hit_chance - crit_chance;
    let mut pmf = scale(&base_pmf, split_hit_chance);
    let crit_pmf = scale(&crit_pmf, crit_chance);

    for (k, v) in crit_pmf {
        *pmf.entry(k).or_default() += v;
    }

    *pmf.entry(0).or_insert(0.0) += 1.0 - hit_chance; // 0 dmg on miss.
    pmf
}

fn best_of_two(pmf: &PMF) -> PMF {
    let mut result = PMF::new();
    for (&x, &px) in pmf {
        for (&y, &py) in pmf {
            let max = x.max(y);
            *result.entry(max).or_default() += px * py;
        }
    }
    result
}

fn mean(pmf: &PMF) -> f64 {
    pmf.iter().map(|(&val, &prob)| val as f64 * prob).sum()
}

fn variance(pmf: &PMF) -> f64 {
    let mean = mean(pmf);
    pmf.iter()
        .map(|(&val, &prob)| {
            let diff = val as f64 - mean;
            diff * diff * prob
        })
        .sum()
}

fn std_dev(pmf: &PMF) -> f64 {
    variance(pmf).sqrt()
}

fn greater_than(a: &PMF, b: &PMF) -> f64 {
    let mut prob = 0.0;
    for (&a_val, &a_prob) in a {
        for (&b_val, &b_prob) in b {
            if a_val > b_val {
                prob += a_prob * b_prob;
            }
        }
    }
    prob
}

fn chance_at_least(pmf: &PMF, threshold: u32) -> f64 {
    pmf.iter()
        .filter(|&(&val, _)| val >= threshold)
        .map(|(_, &prob)| prob)
        .sum()
}

fn cdf(pmf: &PMF) -> Vec<(u32, f64)> {
    let mut cumulative = 0.0;
    let mut result = Vec::new();

    let mut values: Vec<_> = pmf.iter().collect();
    values.sort_by_key(|&(&val, _)| val);

    for (&val, &prob) in values {
        cumulative += prob;
        result.push((val, cumulative));
    }

    result
}

fn build_box(
    ui: &mut Ui,
    item_width: f32,
    build_name: &str,
    build: &mut Build,
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
            }
            *changed |= ui
                .checkbox(&mut build.crit_enabled, "Crits Enabled")
                .changed();
            *changed |= ui.checkbox(&mut build.savage, "Savage Attacker").changed();
            ui.add_space(10.0);
            ui.label(RichText::new(format!("Mean damage: {:.2}", build.mean)).size(15.0));
            ui.label(RichText::new(format!("Standard deviation: {:.2}", build.std_dev)).size(15.0));
            ui.add_space(10.0);
            ui.label(format!(
                "There is {:.1}% chance that {} will out damage the other build.",
                build.greater_then_chance * 100.0,
                build_name
            ));

            ui.label(RichText::new(format!(
                "There is {:.1}% chance to deal at least {} damage.",
                build.min_dmg_chance * 100.0,
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

fn plot_mean_for_ac(ui: &mut Ui, title: &str, size: Vec2, build: &Build) {
    let bars: Vec<Bar> = (10..24)
        .map(|ac| {
            let pmf = convolve_many(
                &build
                    .attacks
                    .iter()
                    .map(|a| attack_pmf(a, ac, build.crit_enabled, build.savage))
                    .collect::<Vec<_>>(),
            );
            let mean = mean(&pmf);
            Bar::new(ac as f64, mean)
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
