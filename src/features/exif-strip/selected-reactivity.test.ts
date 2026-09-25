import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { test } from 'node:test';

// The normal node export of Solid disables effects. Exercise the browser reactive runtime
// used by EditorWorkspace: a lens IPC response enriches the shared metadata inside an effect.
test('lens metadata enrichment neither subscribes its caller nor loops when the match arrives', () => {
  const moduleUrl = new URL('./selected.ts', import.meta.url).href;
  const result = spawnSync(process.execPath, ['--conditions=browser', '--input-type=module', '-e', `
    import assert from 'node:assert/strict';
    import {createRoot, createEffect, createSignal} from 'solid-js';
    import {createSelectedFileMetadata} from ${JSON.stringify(moduleUrl)};
    createRoot(dispose => {
      const metadata = createSelectedFileMetadata(() => new Promise(() => {}));
      metadata.select('sample.RW2');
      const [match, setMatch] = createSignal(null);
      let enrichRuns = 0, reads = 0;
      createEffect(() => {
        const lens = match()?.lensName;
        if (lens) {
          if (++enrichRuns > 4) throw new Error('metadata enrichment subscribed to itself');
          metadata.enrich({lens});
        }
      });
      createEffect(() => { metadata.data(); reads++; });
      queueMicrotask(() => {
        setMatch({lensName:'DG Vario-Elmarit 12-60mm F2.8-4'});
        assert.equal(enrichRuns, 1);
        metadata.enrich({cameraMake:'Panasonic'});
        assert.equal(enrichRuns, 1, 'unrelated metadata must not rerun the lens effect');
        const previousReads = reads;
        metadata.enrich({cameraMake:'Panasonic'});
        assert.equal(reads, previousReads, 'identical enrichment must not publish again');
        assert.equal(match().lensName, metadata.data().lens);
        dispose();
      });
    });
  `], { encoding: 'utf8', timeout: 5000 });
  assert.equal(result.status, 0, result.stderr || String(result.error));
});
