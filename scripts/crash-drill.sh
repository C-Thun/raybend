#!/usr/bin/env bash
# 崩溃恢复演练（plans/M1-7.md 步骤 2）
#
# 顺序：① 造库 + 造源文件 → ② 真导入并在中途**硬杀**（退出码 137）→
#       ③ 检查崩溃后的库与磁盘是否自洽 → ④ 续跑同一批，必须不重复、不覆盖、半成品续上
#
# 用法：pnpm crash:drill  或  bash scripts/crash-drill.sh [工作目录] [张数] [杀在第几张]
set -uo pipefail

WORK=${1:-/mnt/c/src/tmp/rb-crash}
COUNT=${2:-600}
KILL_AFTER=${3:-100}
BIN=target/debug/examples/crash-drill

echo "══ ① 造数据：$COUNT 张 → $WORK"
rm -rf "$WORK" && mkdir -p "$WORK"
cargo build -q --example crash-drill || exit 1
"$BIN" seed "$WORK" "$COUNT" || exit 1

echo
echo "══ ② 真导入 + 中途硬杀（库里已入库 $KILL_AFTER 张时）"
"$BIN" import "$WORK/repo" "$WORK/src" "$KILL_AFTER"
code=$?
echo "   （导入进程退出码 $code；137 = 被 _exit 硬杀，与 SIGKILL 等价）"
if [ "$code" != "137" ]; then
  echo
  echo "❌ 没能在中途杀掉（退出码 $code）——把张数调大，或把「杀在第几张」调小"
  exit 1
fi

echo
echo "══ ③ 崩溃之后：库还能开吗？账与盘对得上吗？"
"$BIN" inspect "$WORK/repo" || exit 1

echo
echo "══ ④ 续跑同一批：不重复、不覆盖、半成品续上"
"$BIN" rerun "$WORK/repo" "$WORK/src" || exit 1

echo
echo "✅ 崩溃恢复演练全部通过"
echo "   工作目录留着供人工查看：$WORK"
echo "   看完删：rm -rf $WORK"
