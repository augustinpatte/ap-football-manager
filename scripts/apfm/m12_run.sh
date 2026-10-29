#!/bin/zsh
# usage: run.sh <binary> <outdir>
export APFM_WORLD_JSON=$HOME/apfm-migration/output/m7world/apfm-fm24-2026.json
for seed in 21 7; do
  $1 fm-c-498 5 $2/s$seed.json --everything --seed $seed > $2/s$seed.head 2>&1
done
echo done > $2/DONE
