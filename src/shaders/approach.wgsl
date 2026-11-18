// Camera-relative landing gates share the scene's reverse-Z depth.
@vertex fn vs_approach(@location(0) point: vec3f) -> @builtin(position) vec4f {
  return G.view_proj * vec4f(point, 1.0);
}

@fragment fn fs_approach() -> @location(0) vec4f {
  return vec4f(0.70, 0.24, 0.048, 0.75);
}
