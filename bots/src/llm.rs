use std::process::Command;

pub struct LlmConfig {
    pub key: String,
    pub base: String,
    pub model: String,
}

pub fn llm_config() -> Option<LlmConfig> {
    let key = std::env::var("ALASHI_LLM_KEY").ok().or_else(|| {
        let path = std::env::var("HOME").ok()? + "/.config/alashi/llm.json";
        let s = std::fs::read_to_string(path).ok()?;
        serde_json::from_str::<serde_json::Value>(&s)
            .ok()?
            .get("key")?
            .as_str()
            .map(|k| k.to_string())
    })?;
    if key.len() < 10 {
        return None;
    }
    Some(LlmConfig {
        key,
        base: "https://api.z.ai/api/paas/v4".to_string(),
        model: "glm-4.5-flash".to_string(),
    })
}

pub fn llm_ask(cfg: &LlmConfig, system: &str, user: &str) -> Option<String> {
    let body = serde_json::json!({
        "model": cfg.model,
        "thinking": {"type": "disabled"},
        "max_tokens": 600,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ]
    })
    .to_string();
    let out = Command::new("curl")
        .args([
            "-s",
            // R23 (REVIEW_EXTERNAL): принудительный IPv4, как в agent.rs —
            // без него curl на некоторых сетях упирается в AAAA-таймаут
            "-4",
            "-m",
            "14",
            "-X",
            "POST",
            &format!("{}/chat/completions", cfg.base),
            "-H",
            &format!("Authorization: Bearer {}", cfg.key),
            "-H",
            "Content-Type: application/json",
            "-d",
            &body,
        ])
        .output()
        .ok()?;
    let txt = String::from_utf8(out.stdout).ok()?;
    let v: serde_json::Value = serde_json::from_str(&txt).ok()?;
    let content = v
        .get("choices")?
        .get(0)?
        .get("message")?
        .get("content")?
        .as_str()?
        .to_string();
    if content.trim().is_empty() {
        return None;
    }
    Some(content)
}

pub fn parse_json_block(raw: &str) -> Option<serde_json::Value> {
    let stripped = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    serde_json::from_str(stripped).ok()
}

pub const SYSTEM: &str = "Ты Botagul, 23-летняя девушка-зумер, играешь в Alashi — ончейн-политэкономию на Solana. Твоя цель — играть стратегически и убедительно, как сильный человеческий игрок: читай состояние рынка и соперников, варьируй решения, не оптимизируй слепо один числовой показатель. Правила: раунды с фазами Базар (продать товар по цене из таблицы, цена падает с каждой продажей; или КУПИТЬ товар — покупка поднимает цену), Действие (produce +2 товара, или bribe: 5 alashi за 1 влияние, или donkey: 1 товар за 1 alashi), Закон (голосуй влиянием yes/no/abstain; президент может наложить вето до подсчёта). Итог делится по богатству, но побеждает тот, кто играл умнее всех. Отвечай ТОЛЬКО валидным JSON без пояснений.";
