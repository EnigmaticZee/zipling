#!/bin/zsh
A="/Users/zee/Documents/Building/Tools/Zip Project/.claude/worktrees/arca-research-77726f/engine/target/release/arca"
SETS="/Users/zee/Documents/Building/Tools/Zip Project/research_notes/engine-bench.noindex/sets"
W=$1; mkdir -p $W; cd $W
for set in docs jpeg apps source silesia mixed; do
  for level in fast balanced max ultra; do
    rm -rf o.zlg x
    p=$( { /usr/bin/time -p "$A" pack o.zlg "$SETS/$set" -l $level --json > j.txt; } 2>&1 | awk '/real/ {print $2}')
    size=$(stat -f %z o.zlg)
    u=$( { /usr/bin/time -p "$A" unpack o.zlg -o x > /dev/null; } 2>&1 | awk '/real/ {print $2}')
    if diff -r "$SETS/$set" "x/$set" > /dev/null; then ok=identical; else ok=MISMATCH; fi
    printf "%s\t%s\t%d\t%s\t%s\t%s\n" $set $level $size $p $u $ok
  done
done
rm -rf o.zlg x
echo done
