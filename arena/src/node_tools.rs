//! Зависимости node-скриптов tools/agent-bootstrap. CI ставит их отдельным
//! шагом `npm ci --ignore-scripts --prefix tools/agent-bootstrap`; без этого
//! шага во свежем клоне тесты арены, запускающие эти скрипты, падают с
//! ERR_MODULE_NOT_FOUND (@solana/web3.js нужен уже на импорте alashi.mjs).

use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

static DEPS_LOCK: Mutex<()> = Mutex::new(());

fn web3_installed(bootstrap: &PathBuf) -> bool {
    bootstrap.join("node_modules/@solana/web3.js").exists()
}

/// Гарантирует, что node_modules для tools/agent-bootstrap existуют, прежде
/// чем тест запустит их node-скрипты. Идемпотентно: при готовых зависимостях
/// (например, после шага CI или прошлого запуска) ничего не делает.
pub fn ensure_agent_bootstrap_deps() {
    let bootstrap = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tools/agent-bootstrap");
    if web3_installed(&bootstrap) {
        return;
    }
    let _guard = DEPS_LOCK.lock().unwrap();
    if web3_installed(&bootstrap) {
        return;
    }
    let status = Command::new("npm")
        .arg("ci")
        .arg("--ignore-scripts")
        .arg("--prefix")
        .arg(&bootstrap)
        .status()
        .expect("npm не найден: нужен для зависимостей tools/agent-bootstrap");
    assert!(
        status.success(),
        "npm ci --ignore-scripts --prefix tools/agent-bootstrap не удался; \
         выполните его вручную и повторите тест"
    );
}
