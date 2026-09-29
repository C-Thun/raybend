import assert from "node:assert/strict";
import test from "node:test";
import { spawnSync } from "node:child_process";
import type { RepositoryConnection, RepositoryView, Volume } from "../../api/types.ts";
import { createRepositoryState } from "./state.ts";
import { createRepositoryMonitor, PROBE_DEBOUNCE_MS, RETRY_MS, VOLUME_POLL_MS, type RepositoryMonitorTimers } from "./monitor.ts";

const status = (revision: string, state: RepositoryConnection["state"] = "offline", generation = "1"): RepositoryConnection => ({
  repositoryId: "中文库", revision, state, generation, root: state === "online" ? "E:/照片" : null, reason: null, observedAt: 0,
});
const row = (connection = status("1")): RepositoryView => ({
  id: "中文库", name: "库", importTemplate: null, createdAt: 0, lastOpenedAt: null, online: connection.state === "online",
  root: connection.root, displayPath: "E:/照片", paths: [], photosCount: 7, imagesCount: 9, triedPaths: 1, connection,
});
const tick = async () => { for (let i = 0; i < 12; i++) await Promise.resolve(); };

test("浏览器响应式调用探测不订阅库状态，状态回写不能重置有界重试", () => {
  const result = spawnSync(process.execPath, ["--conditions=browser", "--input-type=module", "-e", `
    import assert from "node:assert/strict";
    import { createComputed, createRoot, createSignal } from "solid-js";
    import { createRepositoryState } from "./src/features/repositories/state.ts";
    import { createRepositoryMonitor } from "./src/features/repositories/monitor.ts";
    await createRoot(async dispose => {
      const repository = {id:"库", name:"库", online:false, root:null, paths:[], photosCount:0, imagesCount:0, displayPath:"D:/库", triedPaths:1};
      const repositories = createRepositoryState({api:{listRepositories:async()=>[repository], remountRepository:async()=>repository, setRepositoryTemplate:async()=>({importTemplate:""})}});
      await repositories.load();
      const monitor = createRepositoryMonitor({repositories, listVolumes:async()=>[], subscribe:async()=>()=>{}, timers:{after:()=>()=>{}, repeat:()=>()=>{}}});
      const [flow,setFlow] = createSignal("browse"); let calls=0;
      createComputed(() => { flow(); monitor.request(); calls++; });
      assert.equal(calls,1);
      repositories.patch("库",{online:true,root:"D:/库"});
      assert.equal(calls,1,"状态更新不能触发新探测");
      setFlow("export"); assert.equal(calls,2,"进入工作流仍触发探测");
      monitor.dispose(); repositories.dispose(); dispose();
    });
  `], { cwd: process.cwd(), encoding: "utf8", timeout: 10_000 });
  assert.equal(result.status, 0, result.stderr || result.stdout);
});
function clock() {
  let serial = 0;
  const tasks = new Map<number, { fn: () => void; ms: number; repeat: boolean }>();
  const add = (fn: () => void, ms: number, repeat: boolean) => { const id = ++serial; tasks.set(id, { fn, ms, repeat }); return () => { tasks.delete(id); }; };
  const timers: RepositoryMonitorTimers = { after: (fn, ms) => add(fn, ms, false), repeat: (fn, ms) => add(fn, ms, true) };
  return { timers, tasks, async fire(ms: number) {
    for (const [id, task] of [...tasks]) if (task.ms === ms) { if (!task.repeat) tasks.delete(id); task.fn(); }
    await tick();
  } };
}

test("订阅先于快照；离线只有三次重试，重复 focus 合并且销毁后停止", async () => {
  const time = clock(), calls: string[] = [];
  const repositories = createRepositoryState({ api: {
    listRepositories: async () => { calls.push("snapshot"); return [row()]; },
    remountRepository: async () => { calls.push("probe"); return row(status(String(calls.length))); },
    setRepositoryTemplate: async () => ({ importTemplate: "" }),
  } });
  const monitor = createRepositoryMonitor({ repositories, timers: time.timers,
    listVolumes: async () => [], subscribe: async () => { calls.push("subscribe"); return () => { calls.push("unsubscribe"); }; },
  });
  await monitor.start(); assert.deepEqual(calls.slice(0, 2), ["subscribe", "snapshot"]);
  for (let i = 0; i < 100; i++) monitor.request();
  assert.equal([...time.tasks.values()].filter(task => !task.repeat).length, 1);
  await time.fire(PROBE_DEBOUNCE_MS);
  for (const delay of RETRY_MS) await time.fire(delay);
  assert.equal(calls.filter(call => call === "probe").length, 4);
  assert.equal([...time.tasks.values()].filter(task => !task.repeat).length, 0);
  assert.equal(repositories.byId("中文库")?.photosCount, 7);
  monitor.dispose(); monitor.request(); assert.equal(time.tasks.size, 0); assert.equal(calls[calls.length - 1], "unsubscribe");
});

test("事件先于快照；恢复只响应较新代次，真实故障不当拔盘反复重试", async () => {
  const time = clock(); let handler!: (connection: RepositoryConnection) => void; let probes = 0, recoveries = 0;
  const repositories = createRepositoryState({ api: {
    listRepositories: async () => [row(status("1"))],
    remountRepository: async () => { probes++; return row({ ...status("4", "unavailable"), reason: "catalog_invalid" }); },
    setRepositoryTemplate: async () => ({ importTemplate: "" }),
  } });
  const monitor = createRepositoryMonitor({ repositories, timers: time.timers, listVolumes: async () => [],
    subscribe: async fn => { handler = fn; fn(status("2", "online")); return () => {}; }, onRecovery: () => { recoveries++; },
  });
  await monitor.start(); assert.equal(repositories.byId("中文库")?.online, true);
  handler(status("3")); handler(status("5", "online", "2")); handler(status("4"));
  assert.equal(recoveries, 1); assert.equal(repositories.byId("中文库")?.online, true);
  assert.equal(repositories.lastVerifiedRoot("中文库"), "E:/照片");
  monitor.request(); await time.fire(PROBE_DEBOUNCE_MS);
  // The stale probe response cannot overwrite the newer event.
  assert.equal(repositories.byId("中文库")?.online, true);
  repositories.applyConnection({ ...status("6", "unavailable"), reason: "catalog_invalid" });
  monitor.request(); await time.fire(PROBE_DEBOUNCE_MS); await time.fire(RETRY_MS[0]);
  assert.equal(probes, 2); monitor.dispose();
});

test("卷枚举单飞，迟到结果不更新已销毁的观察者", async () => {
  const time = clock(); let release!: (rows: Volume[]) => void; let calls = 0, changes = 0;
  const repositories = createRepositoryState({ api: { listRepositories: async () => [], remountRepository: async () => row(), setRepositoryTemplate: async () => ({ importTemplate: "" }) } });
  const monitor = createRepositoryMonitor({ repositories, timers: time.timers, subscribe: async () => () => {},
    listVolumes: () => { calls++; return new Promise(resolve => { release = resolve; }); }, onVolumes: () => { changes++; },
  });
  await monitor.start(); const first = monitor.listVolumes(), second = monitor.listVolumes();
  assert.equal(first, second); await time.fire(VOLUME_POLL_MS); assert.equal(calls, 1);
  monitor.dispose(); release([{ path: "E:/", kind: "removable", kindLabel: "USB" }]); await first;
  assert.equal(changes, 0); assert.equal(time.tasks.size, 0); repositories.dispose();
});

test("同端点暂忙的 checking 状态按既有预算重试，预算释放后恢复且停止重试", async () => {
  const time = clock(); let probes = 0;
  const repositories = createRepositoryState({ api: {
    listRepositories: async () => [row()],
    remountRepository: async () => {
      probes++;
      return row(probes === 1 ? { ...status("2", "checking"), reason: "busy" } : status("3", "online", "2"));
    },
    setRepositoryTemplate: async () => ({ importTemplate: "" }),
  } });
  const monitor = createRepositoryMonitor({ repositories, timers: time.timers,
    listVolumes: async () => [], subscribe: async () => () => {},
  });
  await monitor.start(); await time.fire(PROBE_DEBOUNCE_MS);
  assert.equal(repositories.byId("中文库")?.connection?.reason, "busy");
  assert.equal([...time.tasks.values()].filter(task => !task.repeat).length, 1);
  await time.fire(RETRY_MS[0]);
  assert.equal(repositories.byId("中文库")?.online, true); assert.equal(probes, 2);
  assert.equal([...time.tasks.values()].filter(task => !task.repeat).length, 0);
  for (const delay of RETRY_MS) await time.fire(delay);
  assert.equal(probes, 2); monitor.dispose(); repositories.dispose();
});

test("事件订阅失败仍载入登记并有界探测；恢复刷新和清理继续生效", async () => {
  const time = clock(); let failures = 0, recoveries = 0, probes = 0;
  const repositories = createRepositoryState({ api: {
    listRepositories: async () => [row()],
    remountRepository: async () => { probes++; return row(status("2", "online", "2")); },
    setRepositoryTemplate: async () => ({ importTemplate: "" }),
  } });
  const monitor = createRepositoryMonitor({ repositories, timers: time.timers, listVolumes: async () => [],
    subscribe: async () => { throw new Error("event channel unavailable"); },
    onSubscriptionError: () => { failures++; }, onRecovery: () => { recoveries++; },
  });
  await monitor.start(); assert.equal(failures, 1); assert.equal(repositories.list().length, 1);
  await time.fire(PROBE_DEBOUNCE_MS); assert.equal(probes, 1); assert.equal(recoveries, 1);
  assert.equal(repositories.byId("中文库")?.online, true);
  monitor.dispose(); assert.equal(time.tasks.size, 0); repositories.dispose();
});

test("释放状态不被启动探测、回焦点或卷变化解除；明确重连才恢复", async () => {
  const time=clock(); let calls=0;
  const repositories=createRepositoryState({api:{listRepositories:async()=>[row(status("1","released"))],setRepositoryTemplate:async()=>({importTemplate:""}),remountRepository:async(_id,automatic)=>{assert.equal(automatic,false);calls++;return row(status("2","online","2"));}}});
  let volumes:Volume[]=[];
  const monitor=createRepositoryMonitor({repositories,timers:time.timers,subscribe:async()=>()=>{},listVolumes:async()=>volumes});
  await monitor.start(); await tick(); monitor.request(); await time.fire(PROBE_DEBOUNCE_MS);
  volumes=[{path:"E:/",name:"disk",kind:"removable",kindLabel:"Removable"} as Volume]; await monitor.listVolumes(); await time.fire(PROBE_DEBOUNCE_MS);
  assert.equal(calls,0); await repositories.remount("中文库"); assert.equal(calls,1); assert.equal(repositories.byId("中文库")?.online,true); monitor.dispose();
});
