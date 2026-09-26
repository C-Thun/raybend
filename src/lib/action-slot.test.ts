import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import test from "node:test";

test("工具栏先读取空动作槽，注册后的定稿资格仍会随编辑与卸载更新", () => {
  const url = new URL("../features/editor/actions.ts", import.meta.url).href;
  const result = spawnSync(process.execPath, ["--conditions=browser", "--input-type=module", "-e", `
    import assert from 'node:assert/strict';
    import { createRoot, createMemo, createSignal } from 'solid-js';
    import { editorActions, registerEditorActions } from ${JSON.stringify(url)};
    createRoot(dispose => {
      const [eligible, setEligible] = createSignal(false);
      const buttonEnabled = createMemo(() => editorActions()?.canFinalize() ?? false);
      assert.equal(buttonEnabled(), false, '工具栏先于工作区挂载');
      registerEditorActions({ canFinalize: eligible });
      assert.equal(buttonEnabled(), false);
      setEligible(true);
      assert.equal(buttonEnabled(), true, 'latest 自动保存后仍能定稿');
      setEligible(false);
      assert.equal(buttonEnabled(), false, '读取已保存稿件/撤销回相同配置');
      setEligible(true);
      assert.equal(buttonEnabled(), true, '重做或再次修改');
      registerEditorActions(null);
      assert.equal(buttonEnabled(), false, '卸载必须立刻禁用');
      setEligible(false);
      registerEditorActions({ canFinalize: () => true });
      assert.equal(buttonEnabled(), true, '再次进入编辑工作区');
      registerEditorActions(null);
      dispose();
    });
  `], { encoding: "utf8", timeout: 5000 });
  assert.equal(result.status, 0, result.stderr || String(result.error));
});

test("编辑、浏览、导入与看图动作槽共用注册响应，替换和清空都能刷新消费者", () => {
  const modules = [
    ["../features/editor/actions.ts", "editorActions", "registerEditorActions"],
    ["../features/browse/actions.ts", "browseActions", "registerBrowseActions"],
    ["../features/import/actions.ts", "importActions", "registerImportActions"],
    ["../components/ui/viewer/actions.ts", "viewerActions", "registerViewerActions"],
  ].map(([path, read, register]) => [new URL(path, import.meta.url).href, read, register]);
  const result = spawnSync(process.execPath, ["--conditions=browser", "--input-type=module", "-e", `
    import assert from 'node:assert/strict';
    import { createRoot, createMemo } from 'solid-js';
    for (const [url, reader, writer] of ${JSON.stringify(modules)}) {
      const module = await import(url);
      createRoot(dispose => {
        let evaluations = 0;
        const observed = createMemo(() => { evaluations++; return module[reader](); });
        assert.equal(observed(), null);
        const first = { marker: 1 }, second = { marker: 2 };
        module[writer](first);
        assert.equal(observed(), first);
        const previous = evaluations;
        module[writer](first);
        assert.equal(evaluations, previous, '同一实现不重复发更新');
        module[writer](second);
        assert.equal(observed(), second);
        module[writer](null);
        assert.equal(observed(), null);
        dispose();
      });
    }
  `], { encoding: "utf8", timeout: 5000 });
  assert.equal(result.status, 0, result.stderr || String(result.error));
});
