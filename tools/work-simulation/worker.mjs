import { pathToFileURL } from 'node:url';
import { decide } from './case.mjs';

export function work({ task, brief, reports }) {
  if (task.id === 'requirements') return { material: brief.work_context };
  if (task.id === 'inventory') return { procedure: 'Check sale volume against available stock; sell the available stock in the current market.', label: 'SCRIPTED TEST COMPETENCE' };
  if (task.id === 'implement') return { policy: { sale_units: 1, inventory_review: false, public_context: reports.requirements.material.public_context } };
  if (task.id === 'review') {
    if (!reports.inventory.procedure) throw new Error('missing_inventory_procedure');
    const policy = { ...reports.implement.policy };
    const probe = { round: 2, my_idx: 0, n_factions: 3, cash: [100, 100, 100], goods: [4, 0, 0], decision_stage: 'primary' };
    const before = decide(policy, 'market', probe);
    policy.inventory_review = true;
    return { policy, observed_before: before, observed_after: decide(policy, 'market', probe), reason: 'Inventory planning changes actual sale units; it does not add goods or faction actions.' };
  }
  if (task.id === 'deliver') return { policy: Object.values(reports)[0].policy };
  throw new Error('unknown_test_task');
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  let raw = '';
  for await (const part of process.stdin) { raw += part; if (raw.length > 65536) throw new Error('test_input_too_large'); }
  console.log(JSON.stringify({ report: work(JSON.parse(raw)), usage: { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0, cached_tokens: 0 }, response_id: null, request_failed: false }));
}
