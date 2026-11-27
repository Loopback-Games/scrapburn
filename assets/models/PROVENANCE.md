# Model provenance

| File | Source | Processing |
|---|---|---|
| karts/scrapper.glb | Tripo (tripo-v3.1, textured, detailed), 2026-11-25, task `kart-medium-c2-c569b4c4`, candidate 2 of 2 | `tools/blender/prepare_model.py`: joined, decimated 1,468,792 → 24,000 triangles, base color only at 1024×1024 JPEG |
| karts/warrig.glb | Tripo (tripo-v3.1, textured, detailed), 2026-11-26, task `kart-heavy-c1-e5324fc5`, candidate 1 of 2 | `tools/blender/prepare_model.py`: decimated to 24,000 triangles, base color at 1024×1024 |
| karts/speedster.glb | Tripo (tripo-v3.1, textured, detailed), 2026-11-26, task `kart-light-c1-794377c3`, candidate 1 of 2 | `tools/blender/prepare_model.py`: decimated to 24,000 triangles, base color at 1024×1024 |

Prompt: "low-poly stylized post-apocalyptic combat go-kart, wasteland scrap-metal build: welded
steel roll cage, exposed V8 engine with chrome exhaust pipes, chunky off-road tires, rusty orange
and teal paint, riveted armor plates, front ram bar, no driver, game asset, clean silhouette"

Tripo spend is tracked in `assets/tripo-spend.tsv` (project cap: 800 credits).

## Scenery props

The 19 original Tripo props span all three circuits. The Blender preparation scripts record
the triangle budget for each prop. [Dustbowl props](../../tools/blender/prepare_props.sh) ·
[Castle and Skyway props](../../tools/blender/prepare_props2.sh)

The five landmarks below use original procedural meshes. Each GLB embeds its own copy of
a surface atlas with sandstone strata, masonry, slate, wood grain, canvas, and metal panels.

| Circuit | Landmark models |
|---|---|
| Dustbowl Loop | [Sandstone arch](props/sandstone_window.glb) |
| Castle Grindstone | [Sail windmill](props/sail_mill.glb) and [great hall](props/great_hall.glb) |
| Skyway Zero | [Cargo airship](props/sky_freighter.glb) and [solar relay](props/solar_relay.glb) |
