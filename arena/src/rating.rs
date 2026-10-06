//! Plackett-Luce рейтинг (Weng-Lin, Algorithm 4) для лидерборда между
//! партиями. Канон: исследование владельца «Оценка LLM в ончейн-играх»
//! (01.09, приоритет 1): парный Elo математически неверен для непрерывного
//! распределения 50/30/15/5, правильная модель — Plackett-Luce.
//! Параметры и формулы — байт-в-байт с openskill.py 6.2.0
//! (models/weng_lin/plackett_luce.py), тест-векторы сняты с него же.

use std::collections::HashMap;

pub const MU: f64 = 25.0;
pub const SIGMA: f64 = 25.0 / 3.0;
const BETA: f64 = 25.0 / 6.0;
const KAPPA: f64 = 0.0001;
const TAU: f64 = 25.0 / 300.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rating {
    pub mu: f64,
    pub sigma: f64,
}

impl Rating {
    pub fn new() -> Self {
        Rating { mu: MU, sigma: SIGMA }
    }
    /// ordinal openskill (z=3): консервативная оценка навыка
    pub fn ordinal(&self) -> f64 {
        self.mu - 3.0 * self.sigma
    }
}

impl Default for Rating {
    fn default() -> Self {
        Self::new()
    }
}

/// Обновление по одной партии: players — (ключ, место), место 0 = лучшее.
/// Места должны быть различны (наш settle всегда даёт строгий порядок).
/// Команда из одного игрока, веса нет, margin нет — как в сим-метриках.
pub fn rate_party(players: &[(String, u64)], ratings: &mut HashMap<String, Rating>) {
    if players.len() < 2 {
        return;
    }
    // tau-инфляция сигмы на партию (аддитивная динамика)
    for (k, _) in players {
        if let Some(r) = ratings.get_mut(k) {
            r.sigma = (r.sigma * r.sigma + TAU * TAU).sqrt();
        }
    }
    let c = {
        let mut s = 0.0;
        for (k, _) in players {
            let r = ratings[k];
            s += r.sigma * r.sigma + BETA * BETA;
        }
        s.sqrt()
    };
    // sum_q[q] = Σ по игрокам не хуже q exp(mu/c)
    let mu_exp: Vec<f64> = players
        .iter()
        .map(|(k, _)| (ratings[k].mu / c).exp())
        .collect();
    let sum_q: Vec<f64> = players
        .iter()
        .enumerate()
        .map(|(_, (_, rq))| {
            let mut s = 0.0;
            for (i, (_, ri)) in players.iter().enumerate() {
                if *ri >= *rq {
                    s += mu_exp[i];
                }
            }
            s
        })
        .collect();
    let mut new_mu: Vec<f64> = Vec::with_capacity(players.len());
    let mut new_sigma: Vec<f64> = Vec::with_capacity(players.len());
    for (i, (k, ri)) in players.iter().enumerate() {
        let r = ratings[k];
        let sig2 = r.sigma * r.sigma;
        let mut omega = 0.0;
        let mut delta = 0.0;
        for (q, (_, rq)) in players.iter().enumerate() {
            if rq <= ri {
                let e = mu_exp[i] / sum_q[q];
                delta += e * (1.0 - e);
                if q == i {
                    omega += 1.0 - e;
                } else {
                    omega -= e;
                }
            }
        }
        omega *= sig2 / c;
        delta *= sig2 / (c * c);
        let gamma = r.sigma / c; // дефолт openskill: sqrt(sigma_team^2)/c
        delta *= gamma;
        new_mu.push(r.mu + omega);
        new_sigma.push(r.sigma * (1.0 - (delta).max(KAPPA)).sqrt().max(0.0));
    }
    for ((k, _), i) in players.iter().zip(0..) {
        ratings.insert(k.clone(), Rating { mu: new_mu[i], sigma: new_sigma[i] });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    /// Векторы сняты с openskill.py 6.2.0 (см. tools/реплику в коммите):
    /// партия 1: A>B>C; партия 2 (A,C): C выигрывает.
    #[test]
    fn matches_openskill_reference() {
        let mut rt: HashMap<String, Rating> = HashMap::new();
        for k in ["A", "B", "C"] {
            rt.insert(k.into(), Rating::new());
        }
        rate_party(&[("A".into(), 0), ("B".into(), 1), ("C".into(), 2)], &mut rt);
        assert!(close(rt["A"].mu, 27.869048680749426), "A.mu {}", rt["A"].mu);
        assert!(close(rt["A"].sigma, 8.205243377397993), "A.sigma {}", rt["A"].sigma);
        assert!(close(rt["B"].mu, 25.717262170187357));
        assert!(close(rt["B"].sigma, 8.058224222802378));
        assert!(close(rt["C"].mu, 21.413689149063217));
        assert!(close(rt["C"].sigma, 8.058224222802378));
        rate_party(&[("A".into(), 1), ("C".into(), 0)], &mut rt);
        assert!(close(rt["A"].mu, 24.62633959554817), "A2.mu {}", rt["A"].mu);
        assert!(close(rt["A"].sigma, 7.954956364568482));
        assert!(close(rt["C"].mu, 24.541247336022387), "C2.mu {}", rt["C"].mu);
        assert!(close(rt["C"].sigma, 7.825628259691499));
    }

    #[test]
    fn winner_rises_loser_falls() {
        let mut rt: HashMap<String, Rating> = HashMap::new();
        rt.insert("w".into(), Rating::new());
        rt.insert("l".into(), Rating::new());
        rate_party(&[("w".into(), 0), ("l".into(), 1)], &mut rt);
        assert!(rt["w"].mu > MU && rt["l"].mu < MU);
        assert!(rt["w"].ordinal() > rt["l"].ordinal());
    }
}
