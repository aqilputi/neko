// WP-6.3 — WebGL scene and cats. The scene model is derived purely from
// the store (snapshot + tree rows + theme: model.ts) and drawn on demand
// by SceneView (view.ts); Scene is the Preact layer that wires both, with
// the flat 2D fallback (Flat.tsx).
export { Scene, sceneFlat, toggleSceneFlat, type SceneProps, type SceneDebug } from './Scene.tsx';
export { buildScene, poseFor, posesById, modelKey, type SceneModel, type CatModel, type Pose } from './model.ts';
export { FrameLoop } from './loop.ts';
