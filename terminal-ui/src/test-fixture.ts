import type {View} from './renderer.ts';
export function fixture(): View {
  return {
    id: 1, width: 160, height: 44, version: 'test', cwd: '~/project', model: 'test-model', provider: 'local',
    branch: 'main', indexed: true, indexing: false, theme: 'dark', accent: 'orange', density: 'standard', session: null, elapsed: 0,
    profile: 'workspace-auto', autoApprove: false, planMode: false, logs: [], plan: [], diffs: [],
    input: '', cursor: 0, selection: null, masked: false, inputMode: 'AgentText', attachments: [],
    running: false, phase: 'idle', tool: null, followTail: true, scroll: 0, stream: '', permission: null,
    update: null, voice: null, commands: [], selectedCommand: 0, tokens: 0,
  };
}
