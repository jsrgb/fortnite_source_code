#include <metal_stdlib>
using namespace metal;

// uniforms
struct Uniforms {
    float4x4 view_proj;
    float time;
};

struct VertexIn {
    float3 position [[attribute(0)]];
    float3 color    [[attribute(1)]];
};

struct VSOut {
    float4 position [[position]];
    float3 color;
};

vertex VSOut vertex_main(
    constant Uniforms& uniforms [[buffer(0)]],
    VertexIn in [[stage_in]]
) {
    VSOut out;

    //float t = uniforms.time;
    //float c = cos(t);
    //float s = sin(t);

    // Rotate around Y axis
    //float3 p = in.position;
    //float3 rotatedY = float3(
        //c * p.x + s * p.z,
        //p.y,
        //-s * p.x + c * p.z
    //);

  //float cx = cos(t * 0.7);
  //float sx = sin(t * 0.7);

  //float3 scaled = float3(
  //    rotatedY.x,
  //    cx * rotatedY.y - sx * rotatedY.z,
  //    sx * rotatedY.y + cx * rotatedY.z
  //) * 0.6;


  //float3 positioned = scaled + float3(0.0, 0.0, 0.5);
  out.position = uniforms.view_proj * float4(in.position, 1.0);
  out.color = in.color;
  return out;
}

fragment float4 fragment_main(VSOut in [[stage_in]]) {
    return float4(in.color, 1.0);
}
