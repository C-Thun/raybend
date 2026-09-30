import { DatabaseSync } from 'node:sqlite';
import assert from 'node:assert/strict';
import { performance } from 'node:perf_hooks';

// Analysis only: synthetic in-memory database, no application data or dependencies.
const n = Number(process.argv[2] ?? 100000);
assert(Number.isInteger(n) && n >= 1000 && n <= 1000000);
const db = new DatabaseSync(':memory:');
db.exec(`
  CREATE TABLE assets(id INTEGER PRIMARY KEY, taken_at INTEGER NOT NULL);
  CREATE INDEX assets_time ON assets(taken_at DESC,id DESC);
  CREATE TABLE files(asset_id INTEGER, parent TEXT, role TEXT);
  CREATE INDEX files_parent ON files(parent,asset_id);
  CREATE INDEX files_asset ON files(asset_id,parent);
  CREATE TABLE direct(asset_id INTEGER,tag TEXT,source INTEGER,
    PRIMARY KEY(asset_id,tag,source)) WITHOUT ROWID;
  CREATE INDEX direct_tag ON direct(tag,asset_id,source);
  CREATE TABLE dirs(parent TEXT,tag TEXT,PRIMARY KEY(parent,tag)) WITHOUT ROWID;
  CREATE INDEX dirs_tag ON dirs(tag,parent);
  CREATE TABLE masks(asset_id INTEGER,tag TEXT,PRIMARY KEY(asset_id,tag)) WITHOUT ROWID;
  CREATE TABLE materialized(asset_id INTEGER,tag TEXT,PRIMARY KEY(asset_id,tag)) WITHOUT ROWID;
  CREATE INDEX materialized_tag ON materialized(tag,asset_id);
  BEGIN;
`);
const a = db.prepare('INSERT INTO assets VALUES (?,?)');
const f = db.prepare('INSERT INTO files VALUES (?,?,?)');
const t = db.prepare('INSERT OR IGNORE INTO direct VALUES (?,?,?)');
const d = db.prepare('INSERT INTO dirs VALUES (?,?)');
const m = db.prepare('INSERT INTO masks VALUES (?,?)');
for (let i=1;i<=n;i++) {
  a.run(i, (i * 7919) % n);
  const dir = `照片/${Math.floor((i-1)/1000)}`;
  f.run(i,dir,'bitmap');
  if(i%2===0) f.run(i,dir+'/_RAW','raw');
  t.run(i,`手动${i%100}`,1);
  t.run(i,`AI${i%20}`,2);
  if(i%1000===0) t.run(i,'稀有',1);
  if(i%3===0) t.run(i,'常见',1);
  if(i%5===0) t.run(i,'常见',2);
  if(i%7===0) m.run(i,'常见');
}
for(let i=0;i<Math.ceil(n/1000);i++) {
  d.run(`照片/${i}`,`目录${i%10}`);
  if(i%2===0) d.run(`照片/${i}`,'常见');
}
db.exec('COMMIT; ANALYZE;');

const correlated = `SELECT a.id FROM assets a WHERE
  (EXISTS(SELECT 1 FROM direct t WHERE t.asset_id=a.id AND t.tag=?)
   OR EXISTS(SELECT 1 FROM files f JOIN dirs d ON d.parent=f.parent
             WHERE f.asset_id=a.id AND d.tag=?))
  AND NOT EXISTS(SELECT 1 FROM masks m WHERE m.asset_id=a.id AND m.tag=?)`;
const indexed = `WITH candidate AS (
  SELECT asset_id FROM direct WHERE tag=?
  UNION
  SELECT f.asset_id FROM dirs d JOIN files f ON f.parent=d.parent WHERE d.tag=?
) SELECT a.id FROM candidate c JOIN assets a ON a.id=c.asset_id
WHERE NOT EXISTS(SELECT 1 FROM masks m WHERE m.asset_id=a.id AND m.tag=?)`;
const order = ' ORDER BY a.taken_at DESC,a.id DESC';
function time(fn,reps=7) {
  fn();
  const v=[];
  for(let i=0;i<reps;i++) {const start=performance.now();fn();v.push(performance.now()-start);}
  v.sort((x,y)=>x-y);
  return +v[Math.floor(v.length/2)].toFixed(3);
}
const result={node:process.version,sqlite:db.prepare('SELECT sqlite_version() AS v').get().v,
  assets:n,files:db.prepare('SELECT COUNT(*) AS n FROM files').get().n,
  direct:db.prepare('SELECT COUNT(*) AS n FROM direct').get().n,queries:{}};
for(const tag of ['稀有','常见','不存在']) {
  const p=[tag,tag,tag];
  const ca=db.prepare(correlated+order), ix=db.prepare(indexed+order);
  const expected=ca.all(...p), actual=ix.all(...p);
  assert.deepEqual(actual,expected);
  const cp=db.prepare(correlated+order+' LIMIT 200');
  const ip=db.prepare(indexed+order+' LIMIT 200');
  const cc=db.prepare(`SELECT COUNT(*) AS n FROM (${correlated})`);
  const ic=db.prepare(`SELECT COUNT(*) AS n FROM (${indexed})`);
  const bounded = db.prepare(correlated.replace('FROM assets a WHERE',
    'FROM (SELECT id,taken_at FROM assets ORDER BY taken_at DESC,id DESC LIMIT 4096) a WHERE')
    +order+' LIMIT 200');
  function hybridPage() {const rows=bounded.all(...p);return rows.length>=200?rows:ip.all(...p);}
  assert.deepEqual(hybridPage(),actual.slice(0,200));
  result.queries[tag]={matches:actual.length,
    correlatedPageMs:time(()=>cp.all(...p)),indexedPageMs:time(()=>ip.all(...p)),
    boundedHybridPageMs:time(hybridPage),
    correlatedCountMs:time(()=>cc.get(...p)),indexedCountMs:time(()=>ic.get(...p))};
}
// Updating one tag for a directory with N photos vs writing N derived relationships.
result.directoryWrite={
  virtualMs:time(()=>{db.exec('BEGIN');d.run('巨型目录','新标签');db.exec('ROLLBACK');}),
  materializedMs:time(()=>{db.exec(`BEGIN; INSERT INTO materialized SELECT id,'新标签' FROM assets; ROLLBACK;`);},3),
  virtualRows:1,materializedRows:n,
};
// Source/mask semantics: another source survives removal; masks suppress every source.
db.exec(`BEGIN;
  INSERT INTO assets VALUES (-1,0);
  INSERT INTO files VALUES(-1,'测试目录','bitmap');
  INSERT INTO dirs VALUES('测试目录','鸟');
  INSERT INTO direct VALUES(-1,'鸟',1),(-1,'鸟',2),(-1,'城市',1);
`);
const effective=db.prepare(`SELECT tag FROM (
  SELECT tag FROM direct WHERE asset_id=-1
  UNION SELECT d.tag FROM dirs d JOIN files f ON f.parent=d.parent WHERE f.asset_id=-1
) t WHERE NOT EXISTS(SELECT 1 FROM masks WHERE asset_id=-1 AND masks.tag=t.tag)
ORDER BY tag`);
const words=()=>effective.all().map(r=>r.tag);
assert.deepEqual(words(),['城市','鸟']);
db.exec("DELETE FROM direct WHERE asset_id=-1 AND tag='鸟' AND source=1");
assert.deepEqual(words(),['城市','鸟']);
db.exec("INSERT INTO masks VALUES(-1,'鸟')");
assert.deepEqual(words(),['城市']);
db.exec("DELETE FROM dirs WHERE parent='测试目录'; INSERT INTO dirs VALUES('测试目录','鸟')");
assert.deepEqual(words(),['城市']);
db.exec("DELETE FROM masks WHERE asset_id=-1; DELETE FROM direct WHERE asset_id=-1 AND source=2");
assert.deepEqual(words(),['城市','鸟']);
db.exec('ROLLBACK');
result.assertions='all passed: query equivalence, deduplication, fallback, mask, source refresh';
result.plan=db.prepare('EXPLAIN QUERY PLAN '+indexed+order+' LIMIT 200').all('稀有','稀有','稀有').map(r=>r.detail);
console.log(JSON.stringify(result,null,2));
db.close();
