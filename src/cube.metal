#include <metal_stdlib>
using namespace metal;

struct Uniforms {
    float4x4 view_proj;
    float time;
};

struct VertexIn {
    float3 position [[attribute(0)]];
};

struct VSOut {
    float4 position [[position]];
};

vertex VSOut vertex_main(
    constant Uniforms& uniforms [[buffer(0)]],
    VertexIn in [[stage_in]]
) {
    VSOut out;

  out.position = uniforms.view_proj * float4(in.position, 1.0);
  return out;
}

fragment float4 fragment_main(VSOut in [[stage_in]]) {
    return float4(1.0, 0.0, 0.0, 1.0);
}
