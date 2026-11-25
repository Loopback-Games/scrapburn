#!/usr/bin/env bash
# Prepares every downloaded Tripo prop into assets/models/props/<name>.glb with a per-prop
# triangle budget. Usage: tools/blender/prepare_props.sh
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
declare -A budget=(
  [barrel_stack]=6000 [bus_wreck]=10000 [cactus]=3000 [dead_tree]=6000 [fuel_tanks]=10000
  [plane_wreck]=10000 [pumpjack]=8000 [skull_rock]=14000 [water_tower]=6000 [wreck_car]=8000
)
for name in "${!budget[@]}"; do
  src=$(ls "$root"/assets/models/tripo/props/"$name"/tripo-out/*/model.glb | head -1)
  flatpak run org.blender.Blender -b --python "$root/tools/blender/prepare_model.py" -- \
    "$src" "$root/assets/models/props/$name.glb" "${budget[$name]}" 2>&1 | grep PREP
done
