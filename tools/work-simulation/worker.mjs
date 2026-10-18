import { pathToFileURL } from 'node:url';
import { handler, legacyContract } from './case.mjs';

export function work({ task, brief, reports }) {
  if (task.id === 'requirements') return { material: brief.work_context };
  if (task.id === 'client') return { contract: legacyContract };
  if (task.id === 'implement') {
    const rules = reports.requirements.material.business_rules;
    return { policy: { response_field: 'allowed', validate_user: false, validate_state: false,
      retain_ownership: rules.includes('retain_ownership'), honor_grace_window: rules.includes('honor_grace_window') } };
  }
  if (task.id === 'review') {
    const policy = { ...reports.implement.policy }, contract = reports.client.contract;
    const failures = [];
    if (typeof handler(policy, { userId: 'TEST-reader', subscription: 'active' })[contract.response_field] !== 'boolean') {
      failures.push('legacy_response'); policy.response_field = contract.response_field;
    }
    if (handler(policy, { subscription: 'active' }).status !== 400) { failures.push('missing_user'); policy.validate_user = true; }
    if (handler(policy, { userId: 'TEST-reader', subscription: 'unexpected' }).status !== 400) { failures.push('unknown_state'); policy.validate_state = true; }
    return { policy, detected: failures };
  }
  if (task.id === 'deliver') return { policy: Object.values(reports)[0].policy };
  throw new Error('unknown_test_task');
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  let raw = '';
  for await (const part of process.stdin) { raw += part; if (raw.length > 65536) throw new Error('test_input_too_large'); }
  const report = work(JSON.parse(raw));
  console.log(JSON.stringify({ report, usage: { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0, cached_tokens: 0 }, response_id: null, request_failed: false }));
}
