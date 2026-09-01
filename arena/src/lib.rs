//! alashi arena: off-chain движок партий как продукт (v0, без блокчейна).
//! Ядро — rules::sim::Simulator, поверх — стратегии ботов, прогонщик
//! серий и (позже) HTTP API. Правила не дублируются: один источник
//! истины alashi-rules, replay-эквивалентность уже доказана тестом.

pub mod runner;
pub mod strategies;
