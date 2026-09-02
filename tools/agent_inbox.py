#!/usr/bin/env python3
"""Автосбор артефактов внешних агентов в репо alashi (владелец от 02.09:
«чтобы я не ходил и не копировал»).

Что делает каждый тик (60с):
1. Сканирует источники — известные каталоги, где opencode-агенты пишут
   свои драйверы, логи и отчёты:
   - ~/Desktop/alashi/inbox/           (агенты, знающие про inbox, кладут сюда)
   - /tmp/alashi*                      (драйверы/состояния в /tmp)
   - /var/folders/.../T/opencode/      (auto.py, r4.py, st.py, *.log, *.sh)
2. Копирует новые или изменённые файлы в data/agents/<источник>/,
   обновляя data/agents/manifest.json (путь, mtime, размер, sha256).
3. При изменениях: git add + commit + push от имени владельца.

Запуск демона: nohup python3 tools/agent_inbox.py >> /tmp/agent_inbox.log 2>&1 &
Разовый прогон: python3 tools/agent_inbox.py --once
"""
import hashlib, json, os, shutil, subprocess, sys, time, glob

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
AGENTS_DIR = os.path.join(ROOT, "data", "agents")
MANIFEST = os.path.join(AGENTS_DIR, "manifest.json")
HOME = os.path.expanduser("~")

INBOX = os.path.join(ROOT, "inbox")
TMP_GLOBS = ["/tmp/alashi_*", "/tmp/wait_test.json"]
OPENCODE_DIRS = glob.glob(os.path.join(
    HOME, "Library/Caches", "TemporaryItems", "*", "T", "opencode")) or glob.glob(
    "/var/folders/*/*/*/T/opencode")
# только их игровые артефакты, не весь рабочий каталог
OPENCODE_PATTERNS = ["*.py", "*.log", "*.sh", "*.json", "*.md", "*.txt"]

def sha(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()

def load_manifest():
    if os.path.exists(MANIFEST):
        return json.load(open(MANIFEST))
    return {}

def collect_sources():
    srcs = []  # (абс_путь, метка_источника)
    if os.path.isdir(INBOX):
        for dp, _, fns in os.walk(INBOX):
            rel = os.path.relpath(dp, INBOX)
            label = os.path.join("inbox", rel) if rel != "." else "inbox"
            for fn in fns:
                srcs.append((os.path.join(dp, fn), label))
    for pat in TMP_GLOBS:
        for p in glob.glob(pat):
            if os.path.isfile(p):
                srcs.append((p, "tmp"))
    for d in OPENCODE_DIRS:
        if not os.path.isdir(d):
            continue
        label = "opencode"
        for pat in OPENCODE_PATTERNS:
            for p in glob.glob(os.path.join(d, pat)):
                if os.path.isfile(p):
                    srcs.append((p, label))
    return srcs

def sync_once(verbose=False):
    os.makedirs(AGENTS_DIR, exist_ok=True)
    man = load_manifest()
    changed = []
    for src, label in collect_sources():
        try:
            st = os.stat(src)
        except OSError:
            continue
        if st.st_size > 5_000_000:  # не тянем тяжёлые дампы
            continue
        key = src
        prev = man.get(key)
        fp = f"{st.st_mtime_ns}-{st.st_size}"
        if prev and prev["fingerprint"] == fp:
            continue
        dst_dir = os.path.join(AGENTS_DIR, label)
        os.makedirs(dst_dir, exist_ok=True)
        base = os.path.basename(src)
        dst = os.path.join(dst_dir, base)
        # не затираем разные файлы с одним именем из inbox-подпапок агентов
        if os.path.exists(dst) and sha(dst) != sha(src):
            stem, ext = os.path.splitext(base)
            dst = os.path.join(dst_dir, f"{stem}-{int(st.st_mtime)}{ext}")
        try:
            shutil.copy2(src, dst)
        except OSError as e:
            print(f"[skip] {src}: {e}", flush=True)
            continue
        man[key] = {
            "fingerprint": fp,
            "size": st.st_size,
            "mtime": int(st.st_mtime),
            "sha256": sha(src),
            "stored": os.path.relpath(dst, ROOT),
        }
        changed.append(os.path.relpath(dst, ROOT))
    if changed:
        json.dump(man, open(MANIFEST, "w"), ensure_ascii=False, indent=1)
        if verbose:
            for c in changed:
                print(f"[sync] {c}", flush=True)
        # автокоммит+пуш от имени владельца
        try:
            subprocess.run(["git", "add", "-A"], cwd=ROOT, check=True,
                           timeout=30)
            msg = f"AGENT-INBOX auto: {len(changed)} файл(ов) от внешних агентов"
            subprocess.run(["git", "commit", "-m", msg], cwd=ROOT,
                          check=True, timeout=30)
            subprocess.run(["git", "push"], cwd=ROOT, check=True, timeout=60)
            if verbose:
                print(f"[git] {msg}", flush=True)
        except subprocess.SubprocessError as e:
            print(f"[ERROR] git: {e}", flush=True)
    return len(changed)

def main():
    once = "--once" in sys.argv
    while True:
        n = sync_once(verbose=True)
        if once:
            print(f"готово: {n} изменений")
            return
        time.sleep(60)

if __name__ == "__main__":
    main()
