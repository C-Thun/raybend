#!/usr/bin/env bash
# 迁移 / 备份 / 故意损坏 演练（plans/M1-7.md 步骤 3）
set -uo pipefail

WORK=${1:-/mnt/c/src/tmp/rb-migrate}
BIN=target/debug/examples/migration-drill

rm -rf "$WORK" && mkdir -p "$WORK"
cargo build -q --example migration-drill || exit 1

echo "══ ① 造一个有照片、有账的真库"
"$BIN" seed "$WORK" 12 || exit 1

echo
echo "══ ② 做一份快照（与产品同一条机制：VACUUM INTO）"
"$BIN" snapshot "$WORK/lib" "$WORK/backups/seed-copy.db" || exit 1

echo
echo "══ ③ 故意把 catalog.db 的头部写坏 → 必须拒绝打开，且不许静默重建"
"$BIN" corrupt "$WORK/lib" || exit 1

echo
echo "══ ④ 从快照恢复 → 能正常打开，账与损坏前一致"
"$BIN" restore "$WORK/backups/seed-copy.db" "$WORK/lib" || exit 1

echo
echo "══ ⑤ 手工造一个 v1 schema 的旧版库（带身份行 + 一条标记数据）"
"$BIN" old "$WORK" || exit 1

echo
echo "══ ⑥ 打开旧版库 → 自动迁移 + 迁移前留快照 + 数据一条不丢"
"$BIN" upgrade "$WORK" || exit 1

echo
echo "══ ⑦ 快照轮转：每个库只留 7 份，别的库不被牵连"
"$BIN" retention "$WORK" || exit 1

echo
echo "══ ⑧ 未来版本的库 → 拒绝打开并说清怎么办"
"$BIN" future "$WORK/lib" 99 || exit 1

echo
echo "✅ 迁移 / 备份 / 损坏演练全部通过"
echo "   工作目录留着供人工查看：$WORK（看完删：rm -rf $WORK）"
