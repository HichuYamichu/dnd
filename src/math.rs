use std::collections::HashMap;

use crate::AC_MAX;
use crate::AC_MIN;
use crate::Attack;
use crate::Build;
use crate::Die;

pub type PMF = HashMap<u32, f64>;
pub type CDF = Vec<(u32, f64)>;

#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub pmf: PMF,
    pub cdf: CDF,
    pub mean: f64,
    pub std_dev: f64,
    pub greater_then_chance: f64,
    pub min_dmg_chance: f64,
}

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

pub fn greater_than(a: &PMF, b: &PMF) -> f64 {
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

pub fn calc_build_stats(build: &Build, sim_ac: u8, desired_min_dmg: u32) -> Stats {
    let mut stats = Stats::default();
    stats.pmf = convolve_many(
        &build
            .attacks
            .iter()
            .map(|a| attack_pmf(a, sim_ac, build.crit_enabled, build.savage))
            .collect::<Vec<_>>(),
    );
    stats.cdf = cdf(&stats.pmf);
    stats.mean = mean(&stats.pmf);
    stats.std_dev = std_dev(&stats.pmf);
    stats.min_dmg_chance = chance_at_least(&stats.pmf, desired_min_dmg);

    return stats;
}

pub fn calc_build_means(build: &Build) -> Vec<f64> {
    let means = (AC_MIN..AC_MAX)
        .map(|ac| {
            let pmf = convolve_many(
                &build
                    .attacks
                    .iter()
                    .map(|a| attack_pmf(a, ac, build.crit_enabled, build.savage))
                    .collect::<Vec<_>>(),
            );
            mean(&pmf)
        })
        .collect();

    return means;
}
