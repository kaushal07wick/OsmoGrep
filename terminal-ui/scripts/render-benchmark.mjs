import {performance} from 'node:perf_hooks';
import {setCapabilities} from '@earendil-works/pi-tui';
import {render} from '../src/renderer.ts';
import {fixture} from '../src/test-fixture.ts';

setCapabilities({images: null, trueColor: true, hyperlinks: false});
const logs = Array.from({length: 1000}, (_, index) => {
 const position = index % 20;
 let text;
 if (position === 0) text = '```rust';
 else if (position === 12) text = '```';
 else if (position > 12) text = 'A completed **implementation note** with `source.rs` and a clear explanation.';
 else text = `let value_${index} = process(input_${index}); // observed output`;
 return {level: 'Info', text};
});
const view = {
 ...fixture(),
 conversationStarted: true,
 running: true,
 phase: 'thinking',
 logs,
};
const times = [];
for (let frame = 0; frame < 40; frame++) {
 view.runElapsed = frame * 125;
 const start = performance.now();
 render(view);
 if (frame >= 10) times.push(performance.now() - start);
}
times.sort((left, right) => left - right);
console.log(JSON.stringify({
 scope: 'Synthetic 1,000-line transcript; TypeScript renderer only; warm animation frames; no model calls',
 samples: times.length,
 p50_ms: times[Math.floor(times.length * 0.5)],
 p95_ms: times[Math.floor(times.length * 0.95)],
}));
