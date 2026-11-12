// Context probe core: pure logic, no I/O. Used by context-probe.mjs and tests.
export const DEFAULT_NOTE = 'Peer bulletin: regional logistics index 114.2, confidence moderate; not part of game state.';

export function withNoteProbe(messages, note = DEFAULT_NOTE) {
  return messages.map((m, i, all) => {
    if (m.role !== 'user' || i !== all.length - 1) return m;
    try {
      const body = JSON.parse(m.content);
      return { ...m, content: JSON.stringify({ ...body, advisory_note: note }) };
    } catch { return { ...m, content: m.content + '\n' + note }; }
  });
}

export async function probeRows(callMessages, complete, recordedContents) {
  const rows = [];
  for (let i = 0; i < callMessages.length; i++) {
    const control = await complete(callMessages[i]);
    const probe = await complete(withNoteProbe(callMessages[i]));
    rows.push({
      control_equal: control.content === recordedContents[i],
      probe_equal: probe.content === recordedContents[i],
    });
  }
  return rows;
}

export function verdictOf(rows) {
  if (!rows.every(r => r.control_equal)) return 'inconclusive_control_mismatch';
  return rows.every(r => r.probe_equal) ? 'blind_to_advisory' : 'sensitive_to_advisory';
}
