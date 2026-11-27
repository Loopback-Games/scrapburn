#!/usr/bin/env bash
# Prepares the castle and skyway Tripo props into assets/models/props/<name>.glb.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
declare -A budget=(
  [castle_tower]=9000 [gatehouse]=12000 [brazier]=4000 [banner_pole]=3000 [guardian_statue]=12000
  [siege_wagon]=7000 [neon_arch]=6000 [antenna_pylon]=6000
)
for name in "${!budget[@]}"; do
  src=$(ls "$root"/assets/models/tripo/props2/"$name"/tripo-out/*/model.glb | head -1)
  flatpak run org.blender.Blender -b --python "$root/tools/blender/prepare_model.py" -- \
    "$src" "$root/assets/models/props/$name.glb" "${budget[$name]}" 2>&1 | grep PREP
done
